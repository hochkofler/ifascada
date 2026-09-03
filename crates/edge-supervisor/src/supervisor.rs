//! The loop that ties the three pieces together: keep the agent alive, wait for orders
//! from central, carry them out.
//!
//! Exposed as a single observable `step` rather than only an endless `run`, so every
//! scenario in the design's failure-mode table can be driven and asserted on directly.

use crate::child::AgentChild;
use crate::control::{ControlClient, Order};
use anyhow::Result;
use std::time::Duration;
use tracing::{error, info, warn};

/// How often the child is checked for having exited while we wait on central.
pub const DEFAULT_CHILD_WATCH_INTERVAL: Duration = Duration::from_secs(1);

/// What one turn of the loop did. Every variant is something an operator would want to
/// find in a log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Central's wait expired with nothing to do.
    Quiet,
    /// The agent had exited and was launched again.
    ChildRestarted,
    /// An order was carried out. `acked` is false when the restart happened but telling
    /// central about it did not -- central will hand the order back, and a second restart
    /// is better than an edge everyone believes was restarted and was not.
    OrderExecuted { request_id: String, acked: bool },
    /// Central could not be reached or refused us. The caller backs off.
    ControlFailed(String),
}

pub struct Supervisor {
    child: AgentChild,
    control: Option<ControlClient>,
    restart_delay: Duration,
    child_watch_interval: Duration,
}

impl Supervisor {
    pub fn new(
        child: AgentChild,
        control: Option<ControlClient>,
        restart_delay: Duration,
        child_watch_interval: Duration,
    ) -> Self {
        Supervisor {
            child,
            control,
            restart_delay,
            child_watch_interval,
        }
    }

    /// Launches the agent for the first time.
    pub fn start(&mut self) -> Result<()> {
        self.child.spawn()
    }

    pub fn child_pid(&self) -> Option<u32> {
        self.child.pid()
    }

    pub async fn step(&mut self) -> Step {
        // A dead agent is the most urgent thing there is; never park on central first.
        if !self.child.is_running() {
            return self.relaunch().await;
        }

        let wake = {
            // Split the borrow so the child can be watched while the control client is
            // held by the other branch of the select.
            let Supervisor {
                child,
                control,
                child_watch_interval,
                ..
            } = self;
            let interval = *child_watch_interval;
            match control.as_ref() {
                None => {
                    watch_until_exit(child, interval).await;
                    Wake::ChildDied
                }
                Some(control) => tokio::select! {
                    result = control.wait_for_order() => Wake::Order(result),
                    _ = watch_until_exit(child, interval) => Wake::ChildDied,
                },
            }
        };

        match wake {
            Wake::ChildDied => self.relaunch().await,
            Wake::Order(Err(e)) => Step::ControlFailed(e.to_string()),
            Wake::Order(Ok(Order::None)) => Step::Quiet,
            Wake::Order(Ok(Order::Restart { request_id })) => {
                self.execute_restart(request_id).await
            }
        }
    }

    /// Runs until the process is killed. This is what the scheduled task and systemd see.
    pub async fn run(&mut self) -> ! {
        loop {
            match self.step().await {
                Step::Quiet => {}
                Step::ChildRestarted => {}
                Step::OrderExecuted { request_id, acked } => {
                    info!(
                        "restart order {} carried out (central informed: {})",
                        request_id, acked
                    );
                }
                Step::ControlFailed(e) => {
                    warn!("control channel unavailable: {}; retrying", e);
                    // Central being down or restarting is expected. A short pause keeps
                    // this from becoming a hot loop against a refused port.
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            }
        }
    }

    /// Relaunch after the agent exited on its own. Keeps the delay that `run-edge.ps1`
    /// always had, which is what stops a crash-looping agent from spinning the machine.
    async fn relaunch(&mut self) -> Step {
        if !self.restart_delay.is_zero() {
            tokio::time::sleep(self.restart_delay).await;
        }
        match self.child.spawn() {
            Ok(()) => info!("agent relaunched (pid={:?})", self.child.pid()),
            // Not fatal: the next turn sees no running child and tries again. Logged at
            // error level because an agent that will not start is an incident.
            Err(e) => error!("could not relaunch the agent: {:#}", e),
        }
        Step::ChildRestarted
    }

    /// Carry out an order from central. No restart delay here: a person asked for this and
    /// is watching the UI wait for it.
    async fn execute_restart(&mut self, request_id: String) -> Step {
        info!("central ordered a restart (request_id={})", request_id);
        if let Err(e) = self.child.kill() {
            warn!("could not stop the agent cleanly: {:#}", e);
        }
        if let Err(e) = self.child.spawn() {
            error!("could not start the agent after the order: {:#}", e);
        }

        let acked = match self.control.as_ref() {
            Some(control) => match control.ack(&request_id).await {
                Ok(()) => true,
                Err(e) => {
                    // The restart happened. Central will hand the order back and the agent
                    // gets restarted twice, which is a nuisance; believing an edge was
                    // restarted when it was not is a fault.
                    warn!(
                        "agent restarted for {} but central was not told: {:#}",
                        request_id, e
                    );
                    false
                }
            },
            None => false,
        };

        Step::OrderExecuted { request_id, acked }
    }
}

enum Wake {
    ChildDied,
    Order(Result<Order>),
}

/// Returns once the child is no longer running.
async fn watch_until_exit(child: &mut AgentChild, interval: Duration) {
    loop {
        if !child.is_running() {
            return;
        }
        tokio::time::sleep(interval).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::child::ChildSpec;
    use crate::config::ControlConfig;
    use crate::control::fake_central::{self, Reply};

    fn sleeper() -> ChildSpec {
        #[cfg(windows)]
        {
            ChildSpec::new("cmd", ["/c", "ping", "-n", "60", "127.0.0.1"])
        }
        #[cfg(not(windows))]
        {
            ChildSpec::new("sh", ["-c", "sleep 60"])
        }
    }

    fn quitter() -> ChildSpec {
        #[cfg(windows)]
        {
            ChildSpec::new("cmd", ["/c", "exit", "0"])
        }
        #[cfg(not(windows))]
        {
            ChildSpec::new("sh", ["-c", "exit 0"])
        }
    }

    fn client(base_url: &str) -> ControlClient {
        ControlClient::new(ControlConfig {
            base_url: base_url.to_string(),
            enroll_token: "s3cret".to_string(),
            edge_id: "lcc01".to_string(),
            // Short, so a test that waits out central's silence stays quick.
            wait: Duration::from_secs(1),
        })
        .unwrap()
    }

    fn supervisor(spec: ChildSpec, control: Option<ControlClient>) -> Supervisor {
        Supervisor::new(
            AgentChild::new(spec),
            control,
            // No restart delay in tests: the 5s production value only exists to avoid a
            // hot crash loop, and waiting it out would just make the suite slow.
            Duration::ZERO,
            Duration::from_millis(50),
        )
    }

    #[tokio::test]
    async fn an_order_restarts_the_agent_and_is_acknowledged() {
        let central = fake_central::start(Reply::Order {
            request_id: "req-1".to_string(),
        })
        .await;
        let mut sup = supervisor(sleeper(), Some(client(&central.base_url)));
        sup.start().expect("start failed");
        let before = sup.child_pid().expect("no initial pid");

        let step = sup.step().await;

        assert_eq!(
            step,
            Step::OrderExecuted {
                request_id: "req-1".to_string(),
                acked: true
            }
        );
        let after = sup.child_pid().expect("the agent must be running again");
        assert_ne!(before, after, "the agent should have been restarted");

        let seen = central.seen.lock().unwrap();
        assert_eq!(
            seen.ack_bodies.first().map(|b| b["request_id"].clone()),
            Some(serde_json::json!("req-1")),
            "central must be told the order was carried out"
        );
    }

    /// Losing the ack must not lose the restart. Central will hand the order back and the
    /// agent gets restarted twice, which is a nuisance; believing an edge was restarted
    /// when it was not is a fault.
    #[tokio::test]
    async fn a_restart_still_counts_when_the_acknowledgement_fails() {
        // 503 makes the fake reject the ack. The order still has to arrive first, so this
        // scenario needs the pending call to succeed and the ack to fail -- which is what
        // OrderThenFailingAck does.
        let central = fake_central::start(Reply::OrderThenFailingAck {
            request_id: "req-2".to_string(),
        })
        .await;
        let mut sup = supervisor(sleeper(), Some(client(&central.base_url)));
        sup.start().expect("start failed");
        let before = sup.child_pid().unwrap();

        let step = sup.step().await;

        assert_eq!(
            step,
            Step::OrderExecuted {
                request_id: "req-2".to_string(),
                acked: false
            }
        );
        assert_ne!(
            before,
            sup.child_pid().unwrap(),
            "the restart must have happened even though the ack did not"
        );
    }

    #[tokio::test]
    async fn a_quiet_central_leaves_the_agent_alone() {
        let central = fake_central::start(Reply::Empty).await;
        let mut sup = supervisor(sleeper(), Some(client(&central.base_url)));
        sup.start().expect("start failed");
        let before = sup.child_pid().unwrap();

        assert_eq!(sup.step().await, Step::Quiet);
        assert_eq!(
            sup.child_pid(),
            Some(before),
            "a quiet period must not disturb a healthy agent"
        );
    }

    #[tokio::test]
    async fn an_unreachable_central_is_reported_and_the_agent_keeps_running() {
        let mut sup = supervisor(sleeper(), Some(client("http://127.0.0.1:1")));
        sup.start().expect("start failed");
        let before = sup.child_pid().unwrap();

        let step = sup.step().await;

        assert!(
            matches!(step, Step::ControlFailed(_)),
            "expected a control failure, got {:?}",
            step
        );
        assert_eq!(
            sup.child_pid(),
            Some(before),
            "central being down must never cost the agent"
        );
    }

    /// The child dying has to be noticed while the supervisor is parked on a long-poll --
    /// otherwise a crashed agent would wait up to a full 25s wait before being relaunched.
    #[tokio::test]
    async fn an_agent_that_exits_is_relaunched_without_waiting_for_central() {
        // Central holds for far longer than this test is willing to wait, so the only way
        // to pass is to notice the dead child independently.
        let central =
            fake_central::start(Reply::HoldThenEmpty(Duration::from_secs(30))).await;
        let mut sup = supervisor(quitter(), Some(client(&central.base_url)));
        sup.start().expect("start failed");

        let step = tokio::time::timeout(Duration::from_secs(10), sup.step())
            .await
            .expect("the supervisor stayed parked on central while the agent was dead");

        assert_eq!(step, Step::ChildRestarted);
    }

    /// A host with no central configured is still supervised. This is the guarantee that
    /// makes disabling remote control safe.
    #[tokio::test]
    async fn supervision_works_with_no_control_channel_at_all() {
        let mut sup = supervisor(quitter(), None);
        sup.start().expect("start failed");

        let step = tokio::time::timeout(Duration::from_secs(10), sup.step())
            .await
            .expect("step never returned");

        assert_eq!(step, Step::ChildRestarted);
        assert!(
            sup.child_pid().is_some(),
            "the agent must be running again even with no control channel"
        );
    }
}

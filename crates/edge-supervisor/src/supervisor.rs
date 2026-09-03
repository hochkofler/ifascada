//! The loop that ties the three pieces together: keep the agent alive, wait for orders
//! from central, carry them out.
//!
//! Exposed as a single observable `step` rather than only an endless `run`, so every
//! scenario in the design's failure-mode table can be driven and asserted on directly.

use crate::child::AgentChild;
use crate::control::{ControlClient, Order};
use crate::heartbeat::{self, Liveness};
use std::path::PathBuf;
use std::time::{Instant, SystemTime};
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
    /// The agent was alive but had stopped beating, so it was restarted. This is the
    /// case the supervisor was blind to before: a process that is running and doing
    /// nothing looks exactly like a healthy one from the outside.
    ChildWedged,
    /// Central could not be reached or refused us. The caller backs off.
    ControlFailed(String),
}

/// Where the agent's heartbeat lives and how patient to be with it.
#[derive(Debug, Clone)]
pub struct HeartbeatWatch {
    pub path: PathBuf,
    pub stale_after: Duration,
    pub grace: Duration,
}

pub struct Supervisor {
    child: AgentChild,
    control: Option<ControlClient>,
    restart_delay: Duration,
    child_watch_interval: Duration,
    /// `None` disables wedge detection: the supervisor then only notices an agent
    /// that exits, which is what it did before the heartbeat existed.
    heartbeat: Option<HeartbeatWatch>,
    spawned_at: Instant,
}

impl Supervisor {
    pub fn new(
        child: AgentChild,
        control: Option<ControlClient>,
        restart_delay: Duration,
        child_watch_interval: Duration,
        heartbeat: Option<HeartbeatWatch>,
    ) -> Self {
        Supervisor {
            child,
            control,
            restart_delay,
            child_watch_interval,
            heartbeat,
            spawned_at: Instant::now(),
        }
    }

    /// Launches the agent for the first time.
    pub fn start(&mut self) -> Result<()> {
        self.spawn_child()
    }

    /// Every launch restarts the grace window: a freshly started agent has not written
    /// a heartbeat yet, and judging it immediately would kill it in a loop.
    fn spawn_child(&mut self) -> Result<()> {
        let result = self.child.spawn();
        if result.is_ok() {
            self.spawned_at = Instant::now();
        }
        result
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
                heartbeat,
                spawned_at,
                ..
            } = self;
            let interval = *child_watch_interval;
            let hb = heartbeat.as_ref();
            let since = *spawned_at;
            match control.as_ref() {
                None => Wake::Trouble(watch_child(child, hb, since, interval).await),
                Some(control) => tokio::select! {
                    result = control.wait_for_order() => Wake::Order(result),
                    trouble = watch_child(child, hb, since, interval) => Wake::Trouble(trouble),
                },
            }
        };

        match wake {
            Wake::Trouble(Trouble::Exited) => self.relaunch().await,
            Wake::Trouble(Trouble::Wedged) => self.restart_wedged().await,
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
                Step::ChildWedged => {
                    warn!("agent was wedged and has been restarted");
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
        match self.spawn_child() {
            Ok(()) => info!("agent relaunched (pid={:?})", self.child.pid()),
            // Not fatal: the next turn sees no running child and tries again. Logged at
            // error level because an agent that will not start is an incident.
            Err(e) => error!("could not relaunch the agent: {:#}", e),
        }
        Step::ChildRestarted
    }

    /// Replace an agent that is running but no longer beating. No restart delay: this is
    /// already the slow path -- the agent has been useless for at least the staleness
    /// window by the time we get here.
    async fn restart_wedged(&mut self) -> Step {
        if let Err(e) = self.child.kill() {
            warn!("could not stop the wedged agent cleanly: {:#}", e);
        }
        if let Err(e) = self.spawn_child() {
            error!("could not restart the wedged agent: {:#}", e);
        } else {
            info!("wedged agent replaced (pid={:?})", self.child.pid());
        }
        Step::ChildWedged
    }

    /// Carry out an order from central. No restart delay here: a person asked for this and
    /// is watching the UI wait for it.
    async fn execute_restart(&mut self, request_id: String) -> Step {
        info!("central ordered a restart (request_id={})", request_id);
        if let Err(e) = self.child.kill() {
            warn!("could not stop the agent cleanly: {:#}", e);
        }
        if let Err(e) = self.spawn_child() {
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
    Trouble(Trouble),
    Order(Result<Order>),
}

/// Why the child stopped being something to leave alone.
enum Trouble {
    /// The process is gone.
    Exited,
    /// The process is running and has stopped beating.
    Wedged,
}

/// Returns once the child needs attention: it exited, or it is alive and no longer
/// beating. Without a heartbeat configured only the first case can ever be detected,
/// which is where this component started.
async fn watch_child(
    child: &mut AgentChild,
    heartbeat: Option<&HeartbeatWatch>,
    spawned_at: Instant,
    interval: Duration,
) -> Trouble {
    loop {
        if !child.is_running() {
            return Trouble::Exited;
        }
        if let Some(hb) = heartbeat {
            let age = heartbeat::read_age(&hb.path, SystemTime::now());
            match heartbeat::assess(age, spawned_at.elapsed(), hb.stale_after, hb.grace) {
                Liveness::Stale { by } => {
                    error!(
                        "agent is running but its last heartbeat is {}s old; presuming it wedged",
                        by.as_secs()
                    );
                    return Trouble::Wedged;
                }
                Liveness::Missing => {
                    error!(
                        "agent is running but has written no heartbeat at {}; presuming it wedged",
                        hb.path.display()
                    );
                    return Trouble::Wedged;
                }
                Liveness::Alive | Liveness::InGrace => {}
            }
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
            None,
        )
    }

    /// The gap this closes. Before the heartbeat the supervisor only noticed an agent
    /// that EXITED; one that stayed alive doing nothing -- lcc01 on 2026-09-02, 25
    /// minutes of silence with the process Running -- was indistinguishable from a
    /// healthy one.
    #[tokio::test]
    async fn an_agent_that_is_alive_but_not_beating_is_restarted() {
        let hb = HeartbeatWatch {
            // A path nothing ever writes: the child is the `sleeper`, which has no
            // idea what a heartbeat is. That is precisely a wedged agent.
            path: std::env::temp_dir().join("edge-sup-never-written.hb"),
            stale_after: Duration::from_millis(1),
            grace: Duration::ZERO,
        };
        let _ = std::fs::remove_file(&hb.path);

        let mut sup = Supervisor::new(
            AgentChild::new(sleeper()),
            None,
            Duration::ZERO,
            Duration::from_millis(50),
            Some(hb),
        );
        sup.start().expect("start failed");
        let before = sup.child_pid().expect("no initial pid");

        let step = tokio::time::timeout(Duration::from_secs(10), sup.step())
            .await
            .expect("the supervisor never noticed the agent was wedged");

        assert_eq!(step, Step::ChildWedged);
        assert_ne!(
            before,
            sup.child_pid().expect("the agent must be running again"),
            "a wedged agent must actually be replaced, not just reported"
        );
    }

    /// The other half of the guarantee: a healthy agent that IS beating must be left
    /// alone. A wedge detector that restarts working agents is worse than none.
    #[tokio::test]
    async fn an_agent_that_keeps_beating_is_left_alone() {
        let path = std::env::temp_dir().join(format!(
            "edge-sup-beating-{}.hb",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(&path, heartbeat::render(std::time::SystemTime::now())).unwrap();

        let hb = HeartbeatWatch {
            path: path.clone(),
            stale_after: Duration::from_secs(60),
            grace: Duration::ZERO,
        };
        let mut sup = Supervisor::new(
            AgentChild::new(sleeper()),
            None,
            Duration::ZERO,
            Duration::from_millis(50),
            Some(hb),
        );
        sup.start().expect("start failed");
        let before = sup.child_pid().unwrap();

        // Nothing should happen, so the step must simply not return.
        let outcome = tokio::time::timeout(Duration::from_millis(600), sup.step()).await;
        let _ = std::fs::remove_file(&path);

        assert!(
            outcome.is_err(),
            "a beating agent must not be disturbed, got {:?}",
            outcome
        );
        assert_eq!(sup.child_pid(), Some(before));
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

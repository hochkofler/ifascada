//! Application-level watchdog over "when did we last hear *from the broker*".
//!
//! Ported from `crates/central-server/src/mqtt_consumer.rs`, where this shipped on
//! 2026-08-25 as suggested fix #2 of
//! `docs/finding-mqtt-client-stale-session-detection.md`. That finding left the edge side
//! explicitly open -- and the edge is the half that runs in the plant, unattended.
//!
//! Duplicated rather than shared because `central-server` depends only on `domain`, and a
//! broker-session watchdog does not belong in the domain layer. Consolidating the two
//! copies means deciding where a transport-resilience crate lives; until then, **the two
//! implementations must stay in step** -- change one, change the other.

use std::time::{Duration, Instant};

/// Multiplier applied to `keep_alive` to decide when a session is presumed dead.
///
/// `rumqttc` pings the broker every `keep_alive` regardless of traffic, so a live broker
/// must produce *some* inbound packet (at minimum a `PINGRESP`) within roughly that period.
/// Allowing 1.5x gives one whole keep-alive period of slack before declaring it dead.
pub const STALE_KEEP_ALIVE_MULTIPLIER: f64 = 1.5;

/// Floor for the poll timeout so the loop can never busy-spin.
pub const MIN_CHECK_INTERVAL: Duration = Duration::from_secs(1);

/// The failure mode this guards against: a session the broker has dropped without a clean
/// close, where the client keeps believing it is connected. In that state `poll()` can keep
/// succeeding -- it still emits `Event::Outgoing(Outgoing::PingReq)` on every keep-alive
/// tick, because writing into a half-open socket's send buffer does not fail -- while
/// nothing at all arrives from the broker.
///
/// Only *inbound* packets prove the broker is still there, so outgoing events deliberately
/// do not count as activity. On 2026-08-18 this exact shape cost 1 h 13 min of total data
/// loss on `lcc01` with a fully green edge log.
///
/// Known limitation, same as central's: a session where TCP and MQTT keep-alive are both
/// healthy but the *subscription* is gone -- the broker answers every `PINGREQ` yet
/// delivers no publishes -- looks alive here, because a `PINGRESP` is inbound activity.
#[derive(Debug, Clone, Copy)]
pub struct BrokerActivityWatch {
    keep_alive: Duration,
    last_activity: Instant,
}

impl BrokerActivityWatch {
    pub fn new(keep_alive: Duration, now: Instant) -> Self {
        BrokerActivityWatch {
            keep_alive,
            last_activity: now,
        }
    }

    /// How long a silence from the broker we tolerate before presuming the session dead.
    pub fn stale_after(&self) -> Duration {
        self.keep_alive.mul_f64(STALE_KEEP_ALIVE_MULTIPLIER)
    }

    /// Record that something arrived *from the broker*, restarting the staleness window.
    pub fn record_activity(&mut self, now: Instant) {
        self.last_activity = now;
    }

    /// True once the broker has been silent for strictly longer than [`Self::stale_after`].
    pub fn should_force_reconnect(&self, now: Instant) -> bool {
        // `saturating_duration_since` and not `-`: a clock that appears to move backwards
        // yields zero elapsed rather than panicking on Instant underflow.
        now.saturating_duration_since(self.last_activity) > self.stale_after()
    }

    /// How long the poll loop may block before it must re-evaluate staleness.
    ///
    /// This is the timeout wrapped around `event_loop.poll()`, which can otherwise park
    /// indefinitely on a dead socket and keep the check below from ever running. It is
    /// deliberately the *remaining* time until the deadline, never a fixed interval, so the
    /// timeout only expires at a point where the connection is going to be torn down
    /// anyway -- cancelling `poll()` mid-flight can then never leave a still-in-use
    /// connection half-written.
    pub fn next_check_in(&self, now: Instant) -> Duration {
        self.stale_after()
            .saturating_sub(now.saturating_duration_since(self.last_activity))
            .max(MIN_CHECK_INTERVAL)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keep_alive() -> Duration {
        // The edge's own keep-alive, set in mqtt_bridge.rs.
        Duration::from_secs(10)
    }

    fn secs(n: u64) -> Duration {
        Duration::from_secs(n)
    }

    #[test]
    fn a_fresh_watch_is_never_stale() {
        let t0 = Instant::now();
        let watch = BrokerActivityWatch::new(keep_alive(), t0);
        assert!(!watch.should_force_reconnect(t0));
    }

    #[test]
    fn the_staleness_window_is_one_and_a_half_keep_alives() {
        let t0 = Instant::now();
        let watch = BrokerActivityWatch::new(keep_alive(), t0);
        assert_eq!(watch.stale_after(), secs(15));
    }

    #[test]
    fn the_boundary_itself_still_gets_the_benefit_of_the_doubt() {
        let t0 = Instant::now();
        let watch = BrokerActivityWatch::new(keep_alive(), t0);
        assert!(!watch.should_force_reconnect(t0 + secs(15)));
    }

    #[test]
    fn silence_beyond_the_window_forces_a_reconnect() {
        let t0 = Instant::now();
        let watch = BrokerActivityWatch::new(keep_alive(), t0);
        assert!(watch.should_force_reconnect(t0 + secs(16)));
    }

    #[test]
    fn inbound_activity_restarts_the_window() {
        let t0 = Instant::now();
        let mut watch = BrokerActivityWatch::new(keep_alive(), t0);
        // A PingResp arriving 12s in: the session is alive, so the clock restarts.
        watch.record_activity(t0 + secs(12));
        assert!(!watch.should_force_reconnect(t0 + secs(25)));
        assert!(watch.should_force_reconnect(t0 + secs(28)));
    }

    #[test]
    fn the_poll_timeout_shrinks_as_the_deadline_approaches() {
        let t0 = Instant::now();
        let watch = BrokerActivityWatch::new(keep_alive(), t0);
        assert_eq!(watch.next_check_in(t0), secs(15));
        assert_eq!(watch.next_check_in(t0 + secs(10)), secs(5));
    }

    /// Without a floor the loop would spin at full speed once the deadline passed, burning
    /// a core on a machine that is also reading scales.
    #[test]
    fn the_poll_timeout_never_drops_below_the_floor() {
        let t0 = Instant::now();
        let watch = BrokerActivityWatch::new(keep_alive(), t0);
        assert_eq!(watch.next_check_in(t0 + secs(15)), MIN_CHECK_INTERVAL);
        assert_eq!(watch.next_check_in(t0 + secs(600)), MIN_CHECK_INTERVAL);
    }

    /// `Instant` arithmetic panics on underflow, and a watch built from a later timestamp
    /// than the one it is asked about must not take the agent down with it.
    #[test]
    fn a_clock_that_appears_to_move_backwards_never_forces_a_reconnect() {
        let t0 = Instant::now();
        let watch = BrokerActivityWatch::new(keep_alive(), t0 + secs(60));
        assert!(!watch.should_force_reconnect(t0));
        assert_eq!(watch.next_check_in(t0), secs(15));
    }
}

//! Detecting an agent that is alive but wedged.
//!
//! The supervisor already notices an agent that *exits*. It could not notice one that
//! stops doing anything while staying alive -- which is exactly what happened to `lcc01`
//! on 2026-09-02: process running, scheduled task Running, COM ports OK, and 25 minutes of
//! silence that ended only because a person looked.
//!
//! The agent writes a heartbeat from its MQTT bridge loop -- the very loop that wedged --
//! and this module decides what that heartbeat means.

use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// How stale a heartbeat may get before the agent is presumed wedged.
pub const DEFAULT_STALE_AFTER: Duration = Duration::from_secs(60);

/// How long after launching the agent to ignore the heartbeat entirely.
///
/// Without this the supervisor would kill a just-started agent that has not written its
/// first beat yet, and the restart loop would look exactly like an agent that cannot start.
pub const DEFAULT_GRACE: Duration = Duration::from_secs(90);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Liveness {
    /// Too soon after launch to judge.
    InGrace,
    /// Beating recently enough.
    Alive,
    /// Past the grace period with no readable heartbeat at all.
    Missing,
    /// Beating, but too long ago.
    Stale { by: Duration },
}

/// The whole decision, kept pure so every case can be driven directly.
pub fn assess(
    age: Option<Duration>,
    since_spawn: Duration,
    stale_after: Duration,
    grace: Duration,
) -> Liveness {
    if since_spawn < grace {
        return Liveness::InGrace;
    }
    match age {
        None => Liveness::Missing,
        Some(age) if age > stale_after => Liveness::Stale { by: age },
        Some(_) => Liveness::Alive,
    }
}

/// Reads the heartbeat and returns how long ago it was written.
///
/// The file carries the epoch milliseconds rather than relying on its modification time:
/// on Windows the mtime can lag behind the write, and having the number inside lets the
/// supervisor report *how* stale a beat is instead of only that it is.
pub fn read_age(path: &Path, now: SystemTime) -> Option<Duration> {
    let text = std::fs::read_to_string(path).ok()?;
    let millis: u64 = text.trim().parse().ok()?;
    let beat = UNIX_EPOCH + Duration::from_millis(millis);
    // A beat dated in the future (a clock that jumped back) reads as brand new rather than
    // as an error: the agent is clearly writing, which is all this needs to know.
    Some(now.duration_since(beat).unwrap_or(Duration::ZERO))
}

/// Renders a heartbeat value; the agent writes exactly this.
pub fn render(now: SystemTime) -> String {
    now.duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis().to_string())
        .unwrap_or_else(|_| "0".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secs(n: u64) -> Duration {
        Duration::from_secs(n)
    }

    #[test]
    fn nothing_is_judged_during_the_grace_period() {
        // A just-launched agent has not written a beat yet; killing it here would loop.
        assert_eq!(
            assess(None, secs(10), DEFAULT_STALE_AFTER, DEFAULT_GRACE),
            Liveness::InGrace
        );
        assert_eq!(
            assess(Some(secs(3600)), secs(10), DEFAULT_STALE_AFTER, DEFAULT_GRACE),
            Liveness::InGrace,
            "even an ancient beat is ignored inside the grace window"
        );
    }

    #[test]
    fn a_recent_beat_past_the_grace_period_is_alive() {
        assert_eq!(
            assess(Some(secs(5)), secs(300), DEFAULT_STALE_AFTER, DEFAULT_GRACE),
            Liveness::Alive
        );
    }

    #[test]
    fn the_threshold_itself_still_counts_as_alive() {
        assert_eq!(
            assess(Some(secs(60)), secs(300), DEFAULT_STALE_AFTER, DEFAULT_GRACE),
            Liveness::Alive,
            "at the boundary the agent gets the benefit of the doubt"
        );
    }

    #[test]
    fn a_beat_older_than_the_threshold_is_stale() {
        assert_eq!(
            assess(Some(secs(75)), secs(300), DEFAULT_STALE_AFTER, DEFAULT_GRACE),
            Liveness::Stale { by: secs(75) }
        );
    }

    /// An agent that never writes a beat after the grace window is as wedged as one whose
    /// beat went stale -- and this is what an old agent without the feature looks like, so
    /// the caller has to be able to tell the two apart.
    #[test]
    fn no_heartbeat_at_all_past_the_grace_period_is_missing() {
        assert_eq!(
            assess(None, secs(300), DEFAULT_STALE_AFTER, DEFAULT_GRACE),
            Liveness::Missing
        );
    }

    #[test]
    fn a_written_heartbeat_reads_back_as_a_fresh_age() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!(
            "edge-sup-hb-{}.txt",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let now = SystemTime::now();
        std::fs::write(&path, render(now)).unwrap();

        let age = read_age(&path, now + secs(7)).expect("heartbeat should be readable");
        let _ = std::fs::remove_file(&path);

        assert!(
            age >= secs(6) && age <= secs(8),
            "expected roughly 7s of age, got {:?}",
            age
        );
    }

    #[test]
    fn a_missing_or_unreadable_file_has_no_age() {
        let path = std::env::temp_dir().join("edge-sup-hb-does-not-exist.txt");
        let _ = std::fs::remove_file(&path);
        assert_eq!(read_age(&path, SystemTime::now()), None);
    }

    /// A truncated or half-written file must read as "no beat", never panic: the agent
    /// rewrites it constantly and a reader can catch it mid-write.
    #[test]
    fn a_corrupt_heartbeat_reads_as_no_age_rather_than_panicking() {
        let path = std::env::temp_dir().join("edge-sup-hb-corrupt.txt");
        std::fs::write(&path, "no-soy-un-numero").unwrap();
        let got = read_age(&path, SystemTime::now());
        let _ = std::fs::remove_file(&path);
        assert_eq!(got, None);
    }

    /// A clock that jumps backwards must not make a beat look like it came from the future
    /// and certainly must not panic.
    #[test]
    fn a_beat_from_the_future_reads_as_zero_age() {
        let path = std::env::temp_dir().join("edge-sup-hb-future.txt");
        let now = SystemTime::now();
        std::fs::write(&path, render(now + secs(120))).unwrap();
        let got = read_age(&path, now);
        let _ = std::fs::remove_file(&path);
        assert_eq!(
            got,
            Some(Duration::ZERO),
            "a beat dated in the future is treated as brand new, not as an error"
        );
    }
}

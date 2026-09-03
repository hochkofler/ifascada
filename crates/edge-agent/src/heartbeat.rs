//! The agent's proof of life, written from the MQTT bridge loop.
//!
//! Deliberately written by that loop and not by a spawned task: what has to be proven
//! alive is exactly the loop that wedged on 2026-09-02. A beat emitted from anywhere else
//! could keep ticking with the loop dead, which is the failure this exists to catch.
//!
//! Read by `edge-supervisor`'s `heartbeat` module. The format -- epoch milliseconds as
//! text -- is the contract between the two; the reader parses exactly this.

use std::path::Path;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Lower bound on how often the beat is written. Every loop turn would mean a file write
/// per MQTT packet, so this throttles it -- but the beat can only be written WHEN THE LOOP
/// TURNS, so the real cadence is whichever is slower. Idle, that is the MQTT keep-alive
/// (~10s, measured in production on 2026-09-03); worst case it is however long the session
/// watchdog lets  park. Anyone tuning the supervisor's staleness threshold needs
/// this number, not the 5 below.
pub const BEAT_INTERVAL: Duration = Duration::from_secs(5);

/// Whether enough time has passed to write another beat.
pub fn due(last_written: Option<Instant>, now: Instant) -> bool {
    match last_written {
        None => true,
        Some(last) => now.saturating_duration_since(last) >= BEAT_INTERVAL,
    }
}

/// Writes the beat. Failure is never fatal: an agent that cannot write its heartbeat is
/// still an agent doing useful work, and the supervisor restarting it would be worse than
/// leaving it be. The caller logs and carries on.
pub fn write(path: &Path, now: SystemTime) -> std::io::Result<()> {
    let millis = now
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    std::fs::write(path, millis.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_beat_is_always_due() {
        assert!(due(None, Instant::now()));
    }

    #[test]
    fn a_beat_is_not_due_again_immediately() {
        let t0 = Instant::now();
        assert!(!due(Some(t0), t0 + Duration::from_secs(1)));
    }

    #[test]
    fn a_beat_is_due_once_the_interval_has_passed() {
        let t0 = Instant::now();
        assert!(due(Some(t0), t0 + BEAT_INTERVAL));
        assert!(due(Some(t0), t0 + Duration::from_secs(30)));
    }

    /// The format is a contract with edge-supervisor, which parses this back into a
    /// timestamp. If it ever stops being plain epoch milliseconds, the supervisor stops
    /// being able to tell a wedged agent from a healthy one -- silently.
    #[test]
    fn the_beat_is_written_as_plain_epoch_milliseconds() {
        let path = std::env::temp_dir().join(format!(
            "edge-agent-hb-{}.txt",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let now = SystemTime::now();
        write(&path, now).expect("write failed");

        let text = std::fs::read_to_string(&path).unwrap();
        let _ = std::fs::remove_file(&path);

        let parsed: u64 = text
            .trim()
            .parse()
            .unwrap_or_else(|_| panic!("not plain epoch millis: {:?}", text));
        let expected = now.duration_since(UNIX_EPOCH).unwrap().as_millis() as u64;
        assert_eq!(parsed, expected);
    }

    #[test]
    fn writing_into_a_missing_directory_fails_without_panicking() {
        let path = std::env::temp_dir()
            .join("edge-agent-hb-no-such-dir")
            .join("beat.txt");
        assert!(write(&path, SystemTime::now()).is_err());
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConnectionTransition {
    pub event_type: &'static str,
    pub severity: &'static str,
    pub message: &'static str,
}

pub fn normalize_connection_state(state: &str) -> &'static str {
    let s = state.trim();
    if s.eq_ignore_ascii_case("connected") {
        "connected"
    } else if s.eq_ignore_ascii_case("connecting") {
        "connecting"
    } else if s.eq_ignore_ascii_case("disconnected") {
        "disconnected"
    } else if s.eq_ignore_ascii_case("failed") {
        "failed"
    } else {
        "unknown"
    }
}

pub fn classify_connection_transition(
    prev_state: Option<&str>,
    new_state: &str,
) -> ConnectionTransition {
    match new_state {
        "connected" => {
            if matches!(prev_state, Some("failed") | Some("disconnected")) {
                ConnectionTransition {
                    event_type: "connection.reconnected",
                    severity: "info",
                    message: "Connection recovered",
                }
            } else {
                ConnectionTransition {
                    event_type: "connection.connected",
                    severity: "info",
                    message: "Connection established",
                }
            }
        }
        "connecting" => ConnectionTransition {
            event_type: "connection.connecting",
            severity: "info",
            message: "Connection attempting",
        },
        "disconnected" => ConnectionTransition {
            event_type: "connection.disconnected",
            severity: "warn",
            message: "Connection disconnected",
        },
        "failed" => ConnectionTransition {
            event_type: "connection.failed",
            severity: "error",
            message: "Connection failed",
        },
        _ => ConnectionTransition {
            event_type: "connection.changed",
            severity: "info",
            message: "Connection state changed",
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{classify_connection_transition, normalize_connection_state};

    #[test]
    fn normalize_connection_state_case_insensitive() {
        assert_eq!(normalize_connection_state("Connected"), "connected");
        assert_eq!(normalize_connection_state("FAILED"), "failed");
        assert_eq!(normalize_connection_state("x"), "unknown");
    }

    #[test]
    fn classify_connection_transition_reconnected() {
        let t = classify_connection_transition(Some("failed"), "connected");
        assert_eq!(t.event_type, "connection.reconnected");
        assert_eq!(t.severity, "info");
        assert_eq!(t.message, "Connection recovered");
    }

    #[test]
    fn classify_connection_transition_failed() {
        let t = classify_connection_transition(Some("connected"), "failed");
        assert_eq!(t.event_type, "connection.failed");
        assert_eq!(t.severity, "error");
        assert_eq!(t.message, "Connection failed");
    }
}

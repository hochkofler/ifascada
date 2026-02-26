#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagConnectionStatus {
    Connected,
    Stale,
    Disconnected,
}

impl TagConnectionStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            TagConnectionStatus::Connected => "connected",
            TagConnectionStatus::Stale => "stale",
            TagConnectionStatus::Disconnected => "disconnected",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TagStatusInput<'a> {
    pub edge_state: Option<&'a str>,
    pub edge_age_secs: Option<i64>,
    pub edge_stale_after_secs: i64,
    pub connection_state: Option<&'a str>,
    pub sample_age_secs: i64,
    pub expected_interval_ms: Option<i64>,
}

pub fn evaluate_tag_connection_status(input: TagStatusInput<'_>) -> TagConnectionStatus {
    if !is_edge_online(
        input.edge_state,
        input.edge_age_secs,
        input.edge_stale_after_secs,
    ) {
        return TagConnectionStatus::Disconnected;
    }

    if let Some(conn_state) = input.connection_state {
        let state = crate::normalize_connection_state(conn_state);
        if state == "failed" || state == "disconnected" {
            return TagConnectionStatus::Disconnected;
        }
        if state == "connecting" {
            return TagConnectionStatus::Stale;
        }
    }

    let Some(expected_ms) = input.expected_interval_ms else {
        return TagConnectionStatus::Connected;
    };
    if expected_ms <= 0 {
        return TagConnectionStatus::Connected;
    }

    let stale_secs = ((expected_ms * 3) / 1000).clamp(5, 300);
    if input.sample_age_secs <= stale_secs {
        TagConnectionStatus::Connected
    } else {
        TagConnectionStatus::Stale
    }
}

fn is_edge_online(edge_state: Option<&str>, edge_age_secs: Option<i64>, stale_after_secs: i64) -> bool {
    let Some(state) = edge_state else {
        return false;
    };
    let ok_state = state.eq_ignore_ascii_case("ok") || state.eq_ignore_ascii_case("online");
    if !ok_state {
        return false;
    }
    let Some(age) = edge_age_secs else {
        return false;
    };
    age <= stale_after_secs.max(1)
}

#[cfg(test)]
mod tests {
    use super::{TagConnectionStatus, TagStatusInput, evaluate_tag_connection_status};

    #[test]
    fn disconnected_when_edge_not_online() {
        let st = evaluate_tag_connection_status(TagStatusInput {
            edge_state: Some("offline"),
            edge_age_secs: Some(1),
            edge_stale_after_secs: 45,
            connection_state: Some("connected"),
            sample_age_secs: 0,
            expected_interval_ms: None,
        });
        assert_eq!(st, TagConnectionStatus::Disconnected);
    }

    #[test]
    fn stale_when_connection_connecting() {
        let st = evaluate_tag_connection_status(TagStatusInput {
            edge_state: Some("online"),
            edge_age_secs: Some(1),
            edge_stale_after_secs: 45,
            connection_state: Some("connecting"),
            sample_age_secs: 0,
            expected_interval_ms: None,
        });
        assert_eq!(st, TagConnectionStatus::Stale);
    }

    #[test]
    fn stale_when_expected_interval_exceeded() {
        let st = evaluate_tag_connection_status(TagStatusInput {
            edge_state: Some("ok"),
            edge_age_secs: Some(1),
            edge_stale_after_secs: 45,
            connection_state: Some("connected"),
            sample_age_secs: 12,
            expected_interval_ms: Some(1000),
        });
        assert_eq!(st, TagConnectionStatus::Stale);
    }

    #[test]
    fn connected_without_expected_interval_if_edge_and_connection_ok() {
        let st = evaluate_tag_connection_status(TagStatusInput {
            edge_state: Some("ok"),
            edge_age_secs: Some(1),
            edge_stale_after_secs: 45,
            connection_state: Some("connected"),
            sample_age_secs: 5000,
            expected_interval_ms: None,
        });
        assert_eq!(st, TagConnectionStatus::Connected);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceConnectionStatus {
    Connected,
    Stale,
    Disconnected,
}

impl DeviceConnectionStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            DeviceConnectionStatus::Connected => "connected",
            DeviceConnectionStatus::Stale => "stale",
            DeviceConnectionStatus::Disconnected => "disconnected",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct DeviceStatusInput<'a> {
    pub edge_state: Option<&'a str>,
    pub edge_age_secs: Option<i64>,
    pub edge_stale_after_secs: i64,
    pub connection_state: Option<&'a str>,
    pub device_protocol_state: Option<&'a str>,
    pub tags_connected: i64,
    pub tags_stale: i64,
    pub tags_total: i64,
}

pub fn evaluate_device_connection_status(input: DeviceStatusInput<'_>) -> DeviceConnectionStatus {
    if !is_edge_online(
        input.edge_state,
        input.edge_age_secs,
        input.edge_stale_after_secs,
    ) {
        return DeviceConnectionStatus::Disconnected;
    }

    if let Some(conn_state) = input.connection_state {
        let state = crate::normalize_connection_state(conn_state);
        if state == "failed" || state == "disconnected" {
            return DeviceConnectionStatus::Disconnected;
        }
        if state == "connecting" || state == "unknown" {
            return DeviceConnectionStatus::Stale;
        }
    } else {
        return DeviceConnectionStatus::Stale;
    }

    if let Some(pstate) = input.device_protocol_state {
        let ps = pstate.trim().to_ascii_lowercase();
        if ps == "error" {
            return DeviceConnectionStatus::Disconnected;
        }
        if ps == "connected" {
            return DeviceConnectionStatus::Connected;
        }
    }

    if input.tags_total <= 0 {
        return DeviceConnectionStatus::Connected;
    }
    DeviceConnectionStatus::Connected
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
    use super::{DeviceConnectionStatus, DeviceStatusInput, evaluate_device_connection_status};

    #[test]
    fn disconnected_when_edge_offline() {
        let st = evaluate_device_connection_status(DeviceStatusInput {
            edge_state: Some("offline"),
            edge_age_secs: Some(1),
            edge_stale_after_secs: 45,
            connection_state: Some("connected"),
            device_protocol_state: None,
            tags_connected: 1,
            tags_stale: 0,
            tags_total: 1,
        });
        assert_eq!(st, DeviceConnectionStatus::Disconnected);
    }

    #[test]
    fn disconnected_when_connection_failed() {
        let st = evaluate_device_connection_status(DeviceStatusInput {
            edge_state: Some("online"),
            edge_age_secs: Some(1),
            edge_stale_after_secs: 45,
            connection_state: Some("failed"),
            device_protocol_state: None,
            tags_connected: 1,
            tags_stale: 0,
            tags_total: 1,
        });
        assert_eq!(st, DeviceConnectionStatus::Disconnected);
    }

    #[test]
    fn stale_when_connection_connecting() {
        let st = evaluate_device_connection_status(DeviceStatusInput {
            edge_state: Some("ok"),
            edge_age_secs: Some(1),
            edge_stale_after_secs: 45,
            connection_state: Some("connecting"),
            device_protocol_state: None,
            tags_connected: 0,
            tags_stale: 1,
            tags_total: 2,
        });
        assert_eq!(st, DeviceConnectionStatus::Stale);
    }

    #[test]
    fn connected_when_any_tag_connected() {
        let st = evaluate_device_connection_status(DeviceStatusInput {
            edge_state: Some("ok"),
            edge_age_secs: Some(1),
            edge_stale_after_secs: 45,
            connection_state: Some("connected"),
            device_protocol_state: None,
            tags_connected: 1,
            tags_stale: 2,
            tags_total: 3,
        });
        assert_eq!(st, DeviceConnectionStatus::Connected);
    }

    #[test]
    fn disconnected_when_device_protocol_error_even_if_connection_connected() {
        let st = evaluate_device_connection_status(DeviceStatusInput {
            edge_state: Some("ok"),
            edge_age_secs: Some(1),
            edge_stale_after_secs: 45,
            connection_state: Some("connected"),
            device_protocol_state: Some("error"),
            tags_connected: 1,
            tags_stale: 0,
            tags_total: 1,
        });
        assert_eq!(st, DeviceConnectionStatus::Disconnected);
    }
}

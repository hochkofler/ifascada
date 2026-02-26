#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScadaTopic {
    pub site: String,
    pub agent: String,
    pub kind: ScadaTopicKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScadaTopicKind {
    TelemetryTag { tag_id: String },
    CommandActionResult,
    CommandWriteAck,
    AuditAction,
    AuditWrite,
    HealthRuntime,
    AlertsRuntime,
    AlertsRuntimeAck,
    AlertsRuntimeAckResult,
    ConfigApplyResult,
    ControlResetResult,
    ConnectionState,
    DeviceConnectionState,
}

pub fn parse_scada_topic(topic: &str) -> Option<ScadaTopic> {
    let parts: Vec<&str> = topic.split('/').collect();
    if parts.len() < 6 {
        return None;
    }
    if parts[0] != "scada" || parts[2] != "edge" {
        return None;
    }

    let site = parts[1].to_string();
    let agent = parts[3].to_string();
    let suffix = &parts[4..];

    let kind = match suffix {
        ["telemetry", "tag", tag_id] => ScadaTopicKind::TelemetryTag {
            tag_id: (*tag_id).to_string(),
        },
        ["cmd", "action", "result"] => ScadaTopicKind::CommandActionResult,
        ["cmd", "write", "ack"] => ScadaTopicKind::CommandWriteAck,
        ["audit", "action"] => ScadaTopicKind::AuditAction,
        ["audit", "write"] => ScadaTopicKind::AuditWrite,
        ["health", "runtime"] => ScadaTopicKind::HealthRuntime,
        ["alerts", "runtime"] => ScadaTopicKind::AlertsRuntime,
        ["alerts", "runtime", "ack"] => ScadaTopicKind::AlertsRuntimeAck,
        ["alerts", "runtime", "ack", "result"] => ScadaTopicKind::AlertsRuntimeAckResult,
        ["config", "apply", "result"] => ScadaTopicKind::ConfigApplyResult,
        ["control", "reset", "result"] => ScadaTopicKind::ControlResetResult,
        ["conn", "state"] => ScadaTopicKind::ConnectionState,
        ["device", "conn", "state"] => ScadaTopicKind::DeviceConnectionState,
        _ => return None,
    };

    Some(ScadaTopic { site, agent, kind })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_telemetry_topic() {
        let t = parse_scada_topic("scada/plant-a/edge/edge-01/telemetry/tag/tag_1").unwrap();
        assert_eq!(t.site, "plant-a");
        assert_eq!(t.agent, "edge-01");
        assert_eq!(
            t.kind,
            ScadaTopicKind::TelemetryTag {
                tag_id: "tag_1".to_string()
            }
        );
    }

    #[test]
    fn parses_cmd_ack_topic() {
        let t = parse_scada_topic("scada/plant-a/edge/edge-01/cmd/write/ack").unwrap();
        assert_eq!(t.kind, ScadaTopicKind::CommandWriteAck);
    }

    #[test]
    fn parses_cmd_action_result_topic() {
        let t = parse_scada_topic("scada/plant-a/edge/edge-01/cmd/action/result").unwrap();
        assert_eq!(t.kind, ScadaTopicKind::CommandActionResult);
    }

    #[test]
    fn parses_audit_action_topic() {
        let t = parse_scada_topic("scada/plant-a/edge/edge-01/audit/action").unwrap();
        assert_eq!(t.kind, ScadaTopicKind::AuditAction);
    }

    #[test]
    fn parses_control_reset_result_topic() {
        let t = parse_scada_topic("scada/plant-a/edge/edge-01/control/reset/result").unwrap();
        assert_eq!(t.kind, ScadaTopicKind::ControlResetResult);
    }

    #[test]
    fn parses_connection_state_topic() {
        let t = parse_scada_topic("scada/plant-a/edge/edge-01/conn/state").unwrap();
        assert_eq!(t.kind, ScadaTopicKind::ConnectionState);
    }

    #[test]
    fn parses_device_connection_state_topic() {
        let t = parse_scada_topic("scada/plant-a/edge/edge-01/device/conn/state").unwrap();
        assert_eq!(t.kind, ScadaTopicKind::DeviceConnectionState);
    }

    #[test]
    fn rejects_unknown_topic() {
        assert!(parse_scada_topic("scada/x/edge/y/cmd/read").is_none());
        assert!(parse_scada_topic("bad/topic").is_none());
    }
}

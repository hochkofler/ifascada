use super::super::*;

fn is_connection_check_action(cmd: &EdgeActionCommandMessage) -> bool {
    if cmd.action_type.trim().eq_ignore_ascii_case("connection.check") {
        return true;
    }
    if !cmd.action_type.trim().eq_ignore_ascii_case("device.command") {
        return false;
    }
    cmd.payload
        .get("command")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .map(|s| s.eq_ignore_ascii_case("check") || s.eq_ignore_ascii_case("connection.check"))
        .unwrap_or(false)
}

fn extract_action_id_from_payload(
    cmd: &EdgeActionCommandMessage,
    key: &str,
) -> Option<String> {
    let from_root = cmd
        .payload
        .get(key)
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToString::to_string);
    if from_root.is_some() {
        return from_root;
    }
    cmd.payload
        .get("args")
        .and_then(|v| v.get(key))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToString::to_string)
}

pub(crate) async fn handle_action_command_packet(
    config: &MqttBridgeConfig,
    action_runtime_state: &Arc<TokioMutex<ActionRuntimeState>>,
    action_orchestrator: &ActionOrchestrator,
    source_id: &str,
    packet_payload: &[u8],
    client: &AsyncClient,
    outbox: &PersistentMqttOutbox,
    metrics: &BridgeMetrics,
    action_result_topic: &str,
    action_audit_topic: &str,
    device_conn_state_topic: &str,
    flush_batch: usize,
) {
    let cmd = match parse_action_command_message(packet_payload) {
        Ok(v) => v,
        Err(e) => {
            metrics.inc_action_received("invalid_payload");
            metrics.inc_action_failed("invalid_payload");
            let result = EdgeActionResultMessage {
                schema_version: MQTT_SCHEMA_VERSION_V1,
                source: source_id.to_string(),
                request_id: None,
                action_type: "unknown".to_string(),
                accepted: false,
                reason: Some(format!("invalid action payload: {}", e)),
                timestamp: chrono::Utc::now(),
            };
            if let Ok(payload) = serde_json::to_vec(&result) {
                publish_with_outbox(
                    client,
                    outbox,
                    metrics,
                    OutboxMessageKind::Audit,
                    false,
                    OutboxMessageKind::Audit,
                    action_result_topic.to_string(),
                    QoS::AtLeastOnce,
                    false,
                    payload,
                    flush_batch,
                )
                .await;
            }
            return;
        }
    };
    metrics.inc_action_received(&cmd.action_type);
    let req = to_action_request(&cmd);
    let result = match action_orchestrator
        .execute(config, action_runtime_state, &req)
        .await
    {
        Ok(_) => {
            metrics.inc_action_accepted(&cmd.action_type);
            build_action_result(source_id, &cmd, true, None)
        }
        Err(e) => {
            metrics.inc_cmd_failed();
            metrics.inc_action_failed(&cmd.action_type);
            build_action_result(source_id, &cmd, false, Some(e))
        }
    };
    let audit = build_action_audit(
        source_id,
        &cmd,
        if result.accepted { "Applied" } else { "Failed" },
        result.reason.clone(),
    );
    if let Ok(payload) = serde_json::to_vec(&result) {
        publish_with_outbox(
            client,
            outbox,
            metrics,
            OutboxMessageKind::Audit,
            false,
            OutboxMessageKind::Audit,
            action_result_topic.to_string(),
            QoS::AtLeastOnce,
            false,
            payload,
            flush_batch,
        )
        .await;
    }
    if let Ok(payload) = serde_json::to_vec(&audit) {
        publish_with_outbox(
            client,
            outbox,
            metrics,
            OutboxMessageKind::Audit,
            false,
            OutboxMessageKind::Audit,
            action_audit_topic.to_string(),
            QoS::AtLeastOnce,
            false,
            payload,
            flush_batch,
        )
        .await;
    }
    if is_connection_check_action(&cmd) {
        let connection_id = extract_action_id_from_payload(&cmd, "connection_id")
            .or_else(|| config.on_demand_probe_connection_id.clone());
        let device_id =
            extract_action_id_from_payload(&cmd, "device_id").or_else(|| config.on_demand_probe_device_id.clone());
        if let (Some(connection_id), Some(device_id)) = (connection_id, device_id) {
            let state_msg = DeviceConnectionStateMqttMessage {
                schema_version: MQTT_SCHEMA_VERSION_V1,
                source: source_id.to_string(),
                connection_id,
                device_id,
                tag_id: None,
                state: if result.accepted {
                    "Connected".to_string()
                } else {
                    "Error".to_string()
                },
                reason: result
                    .reason
                    .clone()
                    .or_else(|| Some("connection_check_ok".to_string())),
                timestamp: chrono::Utc::now(),
            };
            if let Ok(payload) = serde_json::to_vec(&state_msg) {
                publish_with_outbox(
                    client,
                    outbox,
                    metrics,
                    OutboxMessageKind::Audit,
                    false,
                    OutboxMessageKind::Audit,
                    device_conn_state_topic.to_string(),
                    QoS::AtLeastOnce,
                    false,
                    payload,
                    flush_batch,
                )
                .await;
            }
        } else {
            debug!(
                "connection check action executed without device/connection ids; skipped device/conn/state publish"
            );
        }
    }
}

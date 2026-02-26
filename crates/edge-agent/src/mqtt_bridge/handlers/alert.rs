use super::super::*;

pub(crate) async fn handle_alert_ack_packet(
    shared_alert_state: &Arc<TokioMutex<AlertState>>,
    source_id: &str,
    packet_payload: &[u8],
    client: &AsyncClient,
    outbox: &PersistentMqttOutbox,
    metrics: &BridgeMetrics,
    alert_ack_result_topic: &str,
    flush_batch: usize,
) {
    metrics.inc_alert_ack_received();
    let result_msg = match parse_alert_ack_command_message(packet_payload) {
        Ok(cmd) => {
            let mut st = shared_alert_state.lock().await;
            if cmd.alert_type != "runtime_health_degraded" {
                build_alert_ack_result(
                    source_id,
                    &cmd,
                    false,
                    Some("unsupported alert_type".to_string()),
                )
            } else if st.active {
                st.active = false;
                st.degraded_streak = 0;
                st.recovered_streak = 0;
                metrics.inc_alert_ack_accepted();
                build_alert_ack_result(source_id, &cmd, true, None)
            } else {
                build_alert_ack_result(source_id, &cmd, false, Some("alert not active".to_string()))
            }
        }
        Err(e) => AlertAckResultMessage {
            schema_version: MQTT_SCHEMA_VERSION_V1,
            source: source_id.to_string(),
            alert_type: "runtime_health_degraded".to_string(),
            ack_id: None,
            accepted: false,
            reason: Some(format!("invalid alert ack payload: {}", e)),
            timestamp: chrono::Utc::now(),
        },
    };

    if let Ok(payload) = serde_json::to_vec(&result_msg) {
        publish_with_outbox(
            client,
            outbox,
            metrics,
            OutboxMessageKind::Audit,
            false,
            OutboxMessageKind::Audit,
            alert_ack_result_topic.to_string(),
            QoS::AtLeastOnce,
            false,
            payload,
            flush_batch,
        )
        .await;
    }
}

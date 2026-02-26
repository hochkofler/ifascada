use super::super::*;

pub(crate) async fn handle_control_reset_packet(
    source_id: &str,
    packet_payload: &[u8],
    client: &AsyncClient,
    outbox: &PersistentMqttOutbox,
    metrics: &BridgeMetrics,
    control_reset_result_topic: &str,
    flush_batch: usize,
) -> bool {
    let parsed = parse_control_reset_command_message(packet_payload);
    let mut restart_requested = false;
    let result = match parsed {
        Ok(cmd) => {
            restart_requested = true;
            build_control_reset_result(
                source_id,
                Some(&cmd),
                true,
                Some("edge reset requested".to_string()),
            )
        }
        Err(e) => build_control_reset_result(
            source_id,
            None,
            false,
            Some(format!("invalid reset payload: {}", e)),
        ),
    };
    if let Ok(payload) = serde_json::to_vec(&result) {
        publish_with_outbox(
            client,
            outbox,
            metrics,
            OutboxMessageKind::Audit,
            false,
            OutboxMessageKind::Audit,
            control_reset_result_topic.to_string(),
            QoS::AtLeastOnce,
            false,
            payload,
            flush_batch,
        )
        .await;
    }
    restart_requested
}

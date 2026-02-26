use super::super::*;

pub(crate) async fn handle_write_command_packet(
    engine: &RuntimeEngine,
    source_id: &str,
    packet_payload: &[u8],
    client: &AsyncClient,
    outbox: &PersistentMqttOutbox,
    metrics: &BridgeMetrics,
    ack_topic: &str,
    flush_batch: usize,
) {
    metrics.inc_cmd_received();
    let cmd = match parse_write_command_message(packet_payload) {
        Ok(v) => v,
        Err(e) => {
            warn!("invalid write command payload: {}", e);
            let ack = build_invalid_payload_ack(source_id, format!("invalid payload: {}", e));
            if let Ok(payload) = serde_json::to_vec(&ack) {
                publish_with_outbox(
                    client,
                    outbox,
                    metrics,
                    OutboxMessageKind::Ack,
                    false,
                    OutboxMessageKind::Ack,
                    ack_topic.to_string(),
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
    let result = execute_write_command(engine, cmd.clone()).await;
    if let Err(e) = &result {
        metrics.inc_cmd_failed();
        warn!("failed to execute write command from MQTT: {}", e);
    }
    let ack = build_write_command_ack(source_id, &cmd, &result);
    if let Ok(payload) = serde_json::to_vec(&ack) {
        publish_with_outbox(
            client,
            outbox,
            metrics,
            OutboxMessageKind::Ack,
            false,
            OutboxMessageKind::Ack,
            ack_topic.to_string(),
            QoS::AtLeastOnce,
            false,
            payload,
            flush_batch,
        )
        .await;
    }
}

use super::super::*;

pub(crate) async fn handle_config_apply_packet(
    config_sync: &Arc<TokioMutex<ConfigSyncState>>,
    config_apply_receipt_path: &str,
    source_id: &str,
    packet_payload: &[u8],
    current_config_hash: Option<String>,
    client: &AsyncClient,
    outbox: &PersistentMqttOutbox,
    metrics: &BridgeMetrics,
    config_apply_result_topic: &str,
    flush_batch: usize,
) -> bool {
    let parsed = parse_config_apply_command_message(packet_payload);
    let mut sync = config_sync.lock().await;
    let has_staged_change = sync.sync_state == "changed_staged"
        && sync.target_hash.is_some()
        && sync.target_hash != sync.current_hash;
    let mut restart_requested = false;
    let result = match parsed {
        Ok(cmd) => {
            if has_staged_change {
                sync.sync_state = "apply_requested".to_string();
                restart_requested = true;
                let _ = write_config_apply_receipt(
                    config_apply_receipt_path,
                    &ConfigApplyReceipt {
                        request_id: cmd.request_id.clone(),
                        target_config_hash: sync.target_hash.clone(),
                        requested_at: chrono::Utc::now(),
                    },
                );
                build_config_apply_result(
                    source_id,
                    Some(&cmd),
                    true,
                    Some("staged config accepted; edge restart requested".to_string()),
                    current_config_hash.or(sync.current_hash.clone()),
                    sync.target_hash.clone(),
                )
            } else {
                build_config_apply_result(
                    source_id,
                    Some(&cmd),
                    false,
                    Some("no staged config change to apply".to_string()),
                    current_config_hash.or(sync.current_hash.clone()),
                    sync.target_hash.clone(),
                )
            }
        }
        Err(e) => build_config_apply_result(
            source_id,
            None,
            false,
            Some(format!("invalid config apply payload: {}", e)),
            current_config_hash.or(sync.current_hash.clone()),
            sync.target_hash.clone(),
        ),
    };
    drop(sync);
    if let Ok(payload) = serde_json::to_vec(&result) {
        publish_with_outbox(
            client,
            outbox,
            metrics,
            OutboxMessageKind::Audit,
            false,
            OutboxMessageKind::Audit,
            config_apply_result_topic.to_string(),
            QoS::AtLeastOnce,
            false,
            payload,
            flush_batch,
        )
        .await;
    }
    restart_requested
}

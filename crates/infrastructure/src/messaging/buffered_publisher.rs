use crate::database::SQLiteBuffer;
use crate::messaging::mqtt_client::MqttPublisherClient;
use async_trait::async_trait;
use domain::DomainEvent;
use domain::event::EventPublisher;
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info, warn};

#[derive(Clone)]
pub struct BufferedMqttPublisher {
    client: Arc<dyn MqttPublisherClient>,
    buffer: SQLiteBuffer,
    agent_id: String,
}

impl BufferedMqttPublisher {
    pub fn new(
        client: Arc<dyn MqttPublisherClient>,
        buffer: SQLiteBuffer,
        agent_id: String,
    ) -> Self {
        let publisher = Self {
            client,
            buffer,
            agent_id,
        };
        publisher.start_flusher();
        publisher
    }

    fn start_flusher(&self) {
        let client = self.client.clone();
        let buffer = self.buffer.clone();

        tokio::spawn(async move {
            info!("🔄 Starting buffer flusher...");
            loop {
                // Check every 5 seconds
                tokio::time::sleep(Duration::from_secs(5)).await;

                // Only try if we suspect we might be online
                if !client.is_connected() {
                    continue;
                }

                match buffer.count().await {
                    Ok(count) if count > 0 => {
                        match buffer.dequeue_batch(50).await {
                            Ok(rows) => {
                                if !rows.is_empty() {
                                    info!("📤 Flushing {} buffered events...", rows.len());
                                    for (id, topic, payload) in rows {
                                        // Try publish
                                        match client
                                            .publish_bytes(
                                                &topic,
                                                &payload,
                                                rumqttc::QoS::AtLeastOnce,
                                                false,
                                            )
                                            .await
                                        {
                                            Ok(_) => {
                                                if let Err(e) = buffer.delete(id).await {
                                                    error!(
                                                        "Failed to delete forwarded event {}: {}",
                                                        id, e
                                                    );
                                                }
                                            }
                                            Err(e) => {
                                                warn!("Flusher paused: MQTT publish failed: {}", e);
                                                // Break to wait for next cycle
                                                break;
                                            }
                                        }
                                    }
                                }
                            }
                            Err(e) => error!("Failed to dequeue batch: {}", e),
                        }
                    }
                    Ok(_) => {}
                    Err(e) => error!("Failed to check buffer count: {}", e),
                }
            }
        });
    }

    async fn create_payload(&self, event: &DomainEvent) -> Option<(String, Vec<u8>)> {
        match event {
            DomainEvent::TagValueUpdated {
                tag_id,
                value,
                quality,
                timestamp,
            } => {
                let topic = format!("scada/data/{}", self.agent_id);
                let payload = json!([{
                    "tag_id": tag_id.as_str(),
                    "val": value,
                    "ts": timestamp.timestamp_millis(),
                    "q": quality.as_str()
                }]);
                Some((topic, payload.to_string().into_bytes()))
            }
            DomainEvent::ReportCompleted {
                report_id,
                agent_id: _,
                items,
                timestamp,
            } => {
                let topic = format!("scada/reports/{}", self.agent_id);
                let payload = json!({
                    "report_id": report_id,
                    "timestamp": timestamp,
                    "items": items
                });
                Some((topic, payload.to_string().into_bytes()))
            }
            // We do NOT buffer heartbeats to avoid spamming ephemeral data on recovery
            DomainEvent::AgentHeartbeat { .. } => None,
            _ => None,
        }
    }
}

#[async_trait]
impl EventPublisher for BufferedMqttPublisher {
    async fn publish(
        &self,
        event: DomainEvent,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some((topic, payload)) = self.create_payload(&event).await {
            // 1. Check connection first (Client-side offline detection)
            if !self.client.is_connected() {
                warn!("MQTT Client offline. Buffering event...");
                self.buffer.enqueue(&topic, &payload).await?;
                return Ok(());
            }

            // 2. Try publish immediately
            if let Err(e) = self
                .client
                .publish_bytes(&topic, &payload, rumqttc::QoS::AtLeastOnce, false)
                .await
            {
                // 3. If fail (e.g. timeout or error), buffer it
                warn!("MQTT publish failed ({}). Buffering event...", e);
                self.buffer.enqueue(&topic, &payload).await?;
            }
        } else if let DomainEvent::AgentHeartbeat { .. } = event {
            // For heartbeats, we try best effort but don't buffer
            if let DomainEvent::AgentHeartbeat {
                agent_id,
                config_version, // NEW
                uptime_secs,
                active_tags,
                active_tag_ids,
                timestamp,
            } = event
            {
                let topic = format!("scada/health/{}", agent_id);
                let payload = json!({
                    "uptime": uptime_secs,
                    "version": config_version, // NEW
                    "tags": active_tags,
                    "tag_ids": active_tag_ids,
                    "ts": timestamp.timestamp_millis()
                });
                let _ = self
                    .client
                    .publish_bytes(
                        &topic,
                        &payload.to_string().into_bytes(),
                        rumqttc::QoS::AtMostOnce,
                        false,
                    )
                    .await;
            }
        }
        Ok(())
    }
}

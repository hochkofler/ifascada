use crate::messaging::mqtt_client::MqttClient;
use async_trait::async_trait;
use domain::DomainEvent;
use domain::event::EventPublisher;
use serde_json::json;

pub struct MqttEventPublisher {
    client: MqttClient,
    agent_id: String,
}

impl MqttEventPublisher {
    pub fn new(client: MqttClient, agent_id: String) -> Self {
        Self { client, agent_id }
    }
}

#[async_trait]
impl EventPublisher for MqttEventPublisher {
    async fn publish(
        &self,
        event: DomainEvent,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match event {
            DomainEvent::TagValueUpdated {
                tag_id,
                value,
                quality,
                timestamp,
            } => {
                let topic = format!("scada/data/{}", self.agent_id);

                // Payload format as per architecture
                let payload = json!([{
                    "tag_id": tag_id.as_str(),
                    "val": value,
                    "ts": timestamp.timestamp_millis(),
                    "q": quality.as_str()
                }]);

                if let Err(e) = self
                    .client
                    .publish(&topic, &payload.to_string(), false)
                    .await
                {
                    tracing::error!("Failed to publish MQTT message: {}", e);
                }
            }
            // Handle other events if needed (e.g. Heartbeat to system topic)
            DomainEvent::AgentHeartbeat {
                agent_id,
                uptime_secs,
                active_tags,
                active_tag_ids,
                timestamp,
            } => {
                let topic = format!("scada/health/{}", agent_id);
                let payload = json!({
                    "uptime": uptime_secs,
                    "tags": active_tags,
                    "tag_ids": active_tag_ids,
                    "ts": timestamp.timestamp_millis()
                });
                if let Err(e) = self
                    .client
                    .publish(&topic, &payload.to_string(), false)
                    .await
                {
                    tracing::error!("Failed to publish heartbeat: {}", e);
                }
            }
            _ => {} // Ignore others for now
        }
        Ok(())
    }
}

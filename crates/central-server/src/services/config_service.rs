use infrastructure::repositories::DbConfigRepository;
use infrastructure::{MqttClient, MqttMessage};
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

pub struct ConfigService {
    mqtt_client: MqttClient,
    repo: DbConfigRepository,
    // Track last sync time to debounce frequent status updates
    last_sync: Arc<RwLock<HashMap<String, std::time::Instant>>>,
}

impl ConfigService {
    pub fn new(pool: PgPool, mqtt_client: MqttClient) -> Self {
        Self {
            mqtt_client,
            repo: DbConfigRepository::new(pool),
            last_sync: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn start(&self) {
        info!("🔧 Config Service Started");
        if let Err(e) = self.mqtt_client.subscribe("scada/status/#").await {
            error!("Failed to subscribe to status updates: {}", e);
        }

        let mut rx = self.mqtt_client.subscribe_messages();

        while let Ok(msg) = rx.recv().await {
            if msg.topic.starts_with("scada/status/") {
                self.handle_status_message(msg).await;
            }
        }
    }

    async fn handle_status_message(&self, msg: MqttMessage) {
        let topic = msg.topic.as_str();
        let pkid = msg.pkid;
        let agent_id = topic.trim_start_matches("scada/status/");

        // Parse Payload
        match serde_json::from_slice::<serde_json::Value>(&msg.payload) {
            Ok(payload) => {
                if let Some(status) = payload.get("status").and_then(|s| s.as_str()) {
                    if status == "ONLINE" {
                        // Check Debounce
                        let now = std::time::Instant::now();
                        let should_sync = {
                            let mut map = self.last_sync.write().await;
                            match map.get(agent_id) {
                                Some(last_time) => {
                                    if now.duration_since(*last_time).as_secs() < 10 {
                                        info!(
                                            "Skipping config sync for {} (Debounced - last sync < 10s ago)",
                                            agent_id
                                        );
                                        false
                                    } else {
                                        map.insert(agent_id.to_string(), now);
                                        true
                                    }
                                }
                                None => {
                                    map.insert(agent_id.to_string(), now);
                                    true
                                }
                            }
                        };

                        if should_sync {
                            info!(
                                "Agent {} came ONLINE. Trigger message topic: '{}', payload: '{}'",
                                agent_id,
                                topic,
                                String::from_utf8_lossy(&msg.payload)
                            );
                            info!("Syncing config for agent {}...", agent_id);
                            self.sync_config(agent_id).await;
                        }
                    }
                }
                // Ack valid message
                if let Err(e) = self.mqtt_client.ack(topic, pkid).await {
                    error!("Failed to ack status message: {}", e);
                }
            }
            Err(e) => {
                warn!("Failed to parse status message from {}: {}", agent_id, e);
                // Ack invalid message to clear queue
                let _ = self.mqtt_client.ack(topic, pkid).await;
            }
        }
    }

    async fn sync_config(&self, agent_id: &str) {
        match self.repo.get_agent_config(agent_id).await {
            Ok(config) => {
                let config_topic = format!("scada/config/{}", agent_id);
                match serde_json::to_string(&config) {
                    Ok(payload) => {
                        if let Err(e) = self
                            .mqtt_client
                            .publish(&config_topic, &payload, true)
                            .await
                        {
                            error!("Failed to publish config to {}: {}", config_topic, e);
                        } else {
                            info!("✅ Config synced to {}", agent_id);
                        }
                    }
                    Err(e) => error!("Failed to serialize config for {}: {}", agent_id, e),
                }
            }
            Err(e) => {
                error!("Failed to fetch config for agent {}: {}", agent_id, e);
            }
        }
    }
}

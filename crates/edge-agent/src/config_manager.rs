use application::automation::AutomationEngine;
use application::device::DeviceManager;
use domain::tag::{Tag, TagId, TagRepository, TagUpdateMode, TagValueType};
use infrastructure::config::{AgentConfig, TagConfig};
use infrastructure::{MqttClient, MqttMessage};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::info;

use domain::device::DeviceRepository;

pub struct ConfigManager {
    mqtt_client: MqttClient,
    config_path: PathBuf,
    agent_id: String,
    device_manager: Arc<DeviceManager>,
    // executor_manager: Arc<ExecutorManager>, // Removed
    automation_engine: Arc<AutomationEngine>,
    tag_repository: Arc<dyn TagRepository + Send + Sync>,
    device_repository: Arc<dyn DeviceRepository + Send + Sync>, // Added
    // Store the last processed payload hash/bytes to verify changes
    // Using Mutex because ConfigManager is shared/Send/Sync
    last_config_payload: Arc<tokio::sync::Mutex<Vec<u8>>>,
    // Shared version for heartbeat
    config_version: Arc<std::sync::RwLock<String>>, // NEW
}

impl ConfigManager {
    pub fn new(
        mqtt_client: MqttClient,
        config_path: PathBuf,
        agent_id: String,
        device_manager: Arc<DeviceManager>,
        // executor_manager: Arc<ExecutorManager>, // Removed
        automation_engine: Arc<AutomationEngine>,
        tag_repository: Arc<dyn TagRepository + Send + Sync>,
        device_repository: Arc<dyn DeviceRepository + Send + Sync>, // Added
        config_version: Arc<std::sync::RwLock<String>>,             // NEW
    ) -> Self {
        Self {
            mqtt_client,
            config_path,
            agent_id,
            device_manager,
            // executor_manager,
            automation_engine,
            tag_repository,
            device_repository,
            last_config_payload: Arc::new(tokio::sync::Mutex::new(Vec::new())),
            config_version,
        }
    }

    pub async fn init(&self) -> anyhow::Result<broadcast::Receiver<MqttMessage>> {
        let topic = format!("scada/config/{}", self.agent_id);
        info!("🔧 Config Manager listening on {}", topic);

        // 1. Get internal receiver FIRST (before subscribing to broker)
        // This ensures we catch any retained messages that arrive immediately after SUBACK
        let rx = self.mqtt_client.subscribe_messages();

        // 2. Send SUBSCRIBE packet to Broker
        self.mqtt_client
            .subscribe(&topic)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to subscribe to config topic: {}", e))?;

        Ok(rx)
    }

    pub async fn run_loop(&self, mut rx: broadcast::Receiver<MqttMessage>) {
        let topic = format!("scada/config/{}", self.agent_id);

        info!(
            "👂 ConfigManager internal subscription active. Waiting for messages on {}",
            topic
        );

        while let Ok(msg) = rx.recv().await {
            if msg.topic == topic {
                // Deduplication Check
                {
                    let mut last_payload = self.last_config_payload.lock().await;
                    if *last_payload == msg.payload {
                        info!("🔁 Received identical configuration. Skipping reload.");
                        // Still Ack to be safe
                        if let Err(e) = self.mqtt_client.ack(&msg.topic, msg.pkid).await {
                            tracing::error!("Failed to ack config update: {}", e);
                        }
                        continue;
                    }
                    *last_payload = msg.payload.clone();
                }

                info!("📥 Received remote configuration update");
                tracing::debug!(
                    "Protocol Payload: {}",
                    String::from_utf8_lossy(&msg.payload)
                );

                // Sanitization: If printer is null in payload, remove it to allow default.toml to take precedence
                let mut clean_payload = msg.payload.clone();
                if let Ok(mut json) = serde_json::from_slice::<serde_json::Value>(&msg.payload) {
                    if let Some(obj) = json.as_object_mut() {
                        if let Some(printer) = obj.get("printer") {
                            if printer.is_null() {
                                info!(
                                    "⚠️ Remote config has 'printer: null'. Removing it to preserve local defaults."
                                );
                                obj.remove("printer");
                                if let Ok(new_bytes) = serde_json::to_vec_pretty(&json) {
                                    clean_payload = new_bytes;
                                }
                            }
                        }
                    }
                }

                // 1. Prepare Payload for persistence (Remove MQTT to preserve local config)
                let mut save_payload = clean_payload.clone();
                if let Ok(mut json) = serde_json::from_slice::<serde_json::Value>(&save_payload) {
                    if let Some(obj) = json.as_object_mut() {
                        if obj.contains_key("mqtt") {
                            info!(
                                "🔒 Stripping 'mqtt' section from persisted config to enforce local connection settings."
                            );
                            obj.remove("mqtt");
                            if let Ok(new_bytes) = serde_json::to_vec_pretty(&json) {
                                save_payload = new_bytes;
                            }
                        }
                    }
                }

                // 2. Save to file
                match tokio::fs::write(&self.config_path, &save_payload).await {
                    Ok(_) => info!("✅ Configuration saved to {:?}", self.config_path),
                    Err(e) => tracing::error!("Failed to write config file: {}", e),
                }

                // 3. Hot Reload (Keep 'mqtt' in payload as AgentConfig requires it for deserialization)
                self.handle_reload(&clean_payload).await;

                // 4. Ack the message
                if let Err(e) = self.mqtt_client.ack(&msg.topic, msg.pkid).await {
                    tracing::error!("Failed to ack config update: {}", e);
                }
            }
        }
    }

    async fn handle_reload(&self, payload: &[u8]) {
        info!("🔄 Initiating Hot Reload...");

        // Parse Config
        let config: AgentConfig = match serde_json::from_slice(payload) {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("Failed to parse configuration for reload: {}", e);
                return;
            }
        };

        // Update Shared Version
        {
            let mut v = self.config_version.write().unwrap();
            *v = config.version.clone();
            info!("🔄 Config Version updated to: {}", *v);
        }

        // Reload Automations
        self.automation_engine.reload(config.tags.clone()).await;

        // Persist Devices to DB
        let mut new_device_ids = std::collections::HashSet::new();
        for device in &config.devices {
            new_device_ids.insert(device.id.clone());
            if let Err(e) = self.device_repository.save(device).await {
                tracing::error!("Failed to save device {}: {}", device.id, e);
            }
        }

        // Handle device deletions
        if let Ok(existing_devices) = self.device_repository.find_by_agent(&self.agent_id).await {
            for existing in existing_devices {
                if !new_device_ids.contains(&existing.id) {
                    info!("Removing deleted device: {}", existing.id);
                    if let Err(e) = self.device_repository.delete(&existing.id).await {
                        tracing::error!("Failed to delete device {}: {}", existing.id, e);
                    }
                }
            }
        }

        // Persist Tags to DB
        // TODO: Handle deletions (currently only upserts)
        // Strategy for deletion: get all tags for agent, find diff, delete missing.
        let mut new_tag_ids = std::collections::HashSet::new();

        for tag_cfg in &config.tags {
            let tag = self.convert_config_to_tag(tag_cfg);
            new_tag_ids.insert(tag.id().clone());
            if let Err(e) = self.tag_repository.save(&tag).await {
                tracing::error!("Failed to save tag {}: {}", tag.id(), e);
            }
        }

        // Handle deletions
        if let Ok(existing_tags) = self.tag_repository.find_by_agent(&self.agent_id).await {
            for existing in existing_tags {
                if !new_tag_ids.contains(existing.id()) {
                    info!("Removing deleted tag: {}", existing.id());
                    if let Err(e) = self.tag_repository.delete(existing.id()).await {
                        tracing::error!("Failed to delete tag {}: {}", existing.id(), e);
                    }
                }
            }
        }

        // Load Domain Tags from Repo
        let tags = match self.tag_repository.find_by_agent(&self.agent_id).await {
            Ok(t) => t,
            Err(e) => {
                tracing::error!("Failed to load tags from new config: {}", e);
                return;
            }
        };

        // Load Domain Devices from Repo (ensure we have latest state)
        let devices = match self.device_repository.find_by_agent(&self.agent_id).await {
            Ok(d) => d,
            Err(e) => {
                tracing::error!("Failed to load devices from new config: {}", e);
                return;
            }
        };

        info!("Stopping {} active devices...", "all"); // DeviceManager doesn't expose count yet easily
        self.device_manager.stop_all().await;

        if !devices.is_empty() {
            info!("Starting {} devices...", devices.len());
            self.device_manager.start_devices(devices, tags).await;
        }

        info!("✅ Hot Reload Complete");
    }

    fn convert_config_to_tag(&self, cfg: &TagConfig) -> Tag {
        let pipeline_config = cfg.pipeline.clone().unwrap_or_default();
        let device_id = cfg
            .device_id
            .clone()
            .expect("Device ID required for tags in V2 schema");

        let mut tag = Tag::new(
            TagId::new(&cfg.id).unwrap(),
            device_id,
            cfg.driver_config
                .clone()
                .expect("Source Config (driver_config) required for tags"),
            cfg.update_mode
                .clone()
                .unwrap_or(TagUpdateMode::Polling { interval_ms: 1000 }),
            cfg.value_type.clone().unwrap_or(TagValueType::Simple),
            pipeline_config,
        );

        if let Some(enabled) = cfg.enabled {
            if !enabled {
                tag.disable();
            }
        }

        tag
    }
}

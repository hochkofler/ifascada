use crate::automation::executor::ActionExecutor;
use domain::event::ReportItem;
use domain::tag::TagId;
use infrastructure::MqttClient;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{error, info, warn};

pub struct CommandListener {
    mqtt_client: MqttClient,
    agent_id: String,
    executor: Arc<dyn ActionExecutor>,
}

impl CommandListener {
    pub fn new(
        mqtt_client: MqttClient,
        agent_id: String,
        executor: Arc<dyn ActionExecutor>,
    ) -> Self {
        Self {
            mqtt_client,
            agent_id,
            executor,
        }
    }

    pub async fn start(&self) {
        let topic = format!("scada/cmd/{}", self.agent_id);
        if let Err(e) = self.mqtt_client.subscribe(&topic).await {
            error!(agent_id = %self.agent_id, error = %e, "Failed to subscribe to commands");
            return;
        }

        info!(agent_id = %self.agent_id, topic = %topic, "Listening for commands");

        let mut rx = self.mqtt_client.subscribe_messages();
        let agent_id = self.agent_id.clone();

        // Process commands in loop
        loop {
            match rx.recv().await {
                Ok(msg) => {
                    if msg.topic == topic {
                        let payload_str = String::from_utf8_lossy(&msg.payload);
                        info!(agent_id = %agent_id, command = %payload_str, "Received command");

                        // 1. Parse JSON
                        if let Ok(cmd) = serde_json::from_str::<Value>(&payload_str) {
                            self.handle_command(cmd).await;
                        } else {
                            warn!(agent_id = %agent_id, "Received non-JSON command");
                        }

                        // 3. Ack the command
                        if let Err(e) = self.mqtt_client.ack(&msg.topic, msg.pkid).await {
                            warn!(agent_id = %agent_id, error = %e, "Failed to ack command");
                        }
                    }
                }
                Err(broadcast::error::RecvError::Lagged(count)) => {
                    warn!(agent_id = %agent_id, skipped = count, "Command listener lagged");
                }
                Err(broadcast::error::RecvError::Closed) => {
                    warn!(agent_id = %agent_id, "Command channel closed");
                    break;
                }
            }
        }
    }

    async fn handle_command(&self, cmd: Value) {
        let cmd_type = cmd["type"].as_str().unwrap_or("Unknown");
        match cmd_type {
            "PrintBatchManual" => {
                let tag_id_str = cmd["tag_id"].as_str().unwrap_or("");
                let items_val = &cmd["items"];

                if let (Ok(tag_id), Some(items_array)) =
                    (TagId::new(tag_id_str), items_val.as_array())
                {
                    let items: Vec<ReportItem> = items_array
                        .iter()
                        .filter_map(|i| match serde_json::from_value::<ReportItem>(i.clone()) {
                            Ok(item) => Some(item),
                            Err(e) => {
                                error!(error = %e, item = ?i, "Failed to deserialize ReportItem");
                                None
                            }
                        })
                        .collect();

                    info!(tag_id=%tag_id, count=%items.len(), "Executing manual batch print");
                    self.executor.execute_manual_batch(&tag_id, items).await;
                } else {
                    warn!("Invalid PrintBatchManual command payload");
                }
            }
            _ => {
                warn!(command_type = %cmd_type, "Unhandled command type");
            }
        }
    }
}

use crate::config::{AgentConfig, MqttConfig, TagConfig};
use anyhow::{Result, anyhow};
use domain::driver::DriverType;
use domain::tag::{TagUpdateMode, TagValueType};
use sqlx::{PgPool, Row};

#[derive(Clone)]
pub struct DbConfigRepository {
    pool: PgPool,
}

impl DbConfigRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn get_agent_config(&self, agent_id: &str) -> Result<AgentConfig> {
        // 1. Check if agent exists and get policy
        let agent_row = sqlx::query(
            "SELECT id, heartbeat_interval_secs, printer_config FROM edge_agents WHERE id = $1",
        )
        .bind(agent_id)
        .fetch_optional(&self.pool)
        .await?;

        let agent_row = match agent_row {
            Some(row) => row,
            None => return Err(anyhow!("Agent {} not found", agent_id)),
        };

        let heartbeat_interval_secs: i32 = agent_row.get(1);
        let printer_config_json: Option<serde_json::Value> = agent_row.get(2);

        // 2. Fetch tags
        let rows = sqlx::query(
            r#"
            SELECT 
                id, 
                driver_type, 
                driver_config, 
                update_mode, 
                update_config, 
                value_type, 
                value_schema,
                enabled,
                pipeline_config
            FROM tags 
            WHERE edge_agent_id = $1
            "#,
        )
        .bind(agent_id)
        .fetch_all(&self.pool)
        .await?;

        let total_rows = rows.len();

        let tags: Vec<TagConfig> = rows
            .into_iter()
            .filter_map(|row| {
                let id: String = row.get("id");
                let driver_type_str: String = row.get("driver_type");
                
                // Robust DriverType parsing
                let driver = match serde_json::from_value::<DriverType>(serde_json::json!(driver_type_str)) {
                    Ok(d) => d,
                    Err(_) => {
                        // Attempt Case-Insensitive fallback
                        match driver_type_str.to_lowercase().as_str() {
                            "rs232" => DriverType::RS232,
                            "modbus" => DriverType::Modbus,
                            "opc-ua" | "opcua" => DriverType::OPCUA,
                            "http" => DriverType::HTTP,
                            "simulator" => DriverType::Simulator,
                            _ => {
                                tracing::warn!("⚠️ Invalid DriverType '{}' for tag {}. Skipping tag.", driver_type_str, id);
                                return None;
                            }
                        }
                    }
                };

                let driver_config: serde_json::Value = row.get("driver_config");

                let update_mode_str: String = row.get("update_mode");
                let update_config: serde_json::Value = row.get("update_config");

                // Case-Insensitive Update Mode Parsing
                let update_mode = match update_mode_str.to_lowercase().as_str() {
                    "polling" => {
                        let interval = update_config
                            .get("interval_ms")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(1000);
                        Some(TagUpdateMode::Polling {
                            interval_ms: interval,
                        })
                    }
                    "onchange" => {
                        let debounce = update_config
                            .get("debounce_ms")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0);
                        let timeout = update_config
                            .get("timeout_ms")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0);
                        Some(TagUpdateMode::OnChange {
                            debounce_ms: debounce,
                            timeout_ms: timeout,
                        })
                    }
                    "pollingonchange" => {
                        let interval = update_config
                            .get("interval_ms")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(1000);
                        let threshold = update_config
                            .get("change_threshold")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(0.0);
                        Some(TagUpdateMode::PollingOnChange {
                            interval_ms: interval,
                            change_threshold: threshold,
                        })
                    }
                    _ => {
                        tracing::warn!("⚠️ Unknown UpdateMode '{}' for tag {}. Defaulting to None.", update_mode_str, id);
                        None
                    },
                };

                let value_type_str: String = row.get("value_type");
                let value_type = match value_type_str.to_lowercase().as_str() {
                    "composite" => Some(TagValueType::Composite),
                    "simple" => Some(TagValueType::Simple),
                    _ => Some(TagValueType::Simple),
                };

                let enabled: bool = row.get("enabled");
                let pipeline_config: Option<serde_json::Value> = row.get("pipeline_config");

                let pipeline: Option<domain::tag::PipelineConfig> =
                    if let Some(json) = pipeline_config {
                         match serde_json::from_value(json) {
                            Ok(p) => Some(p),
                            Err(e) => {
                                tracing::warn!("⚠️ Metadata Error: Failed to parse pipeline config for tag {}: {}. Pipeline disabled.", id, e);
                                None
                            }
                        }
                    } else {
                        None
                    };

                let value_schema: Option<serde_json::Value> = row.get("value_schema");

                let automations = pipeline
                    .as_ref()
                    .map(|p| p.automations.clone())
                    .unwrap_or_default();

                Some(TagConfig {
                    id,
                    driver,
                    driver_config,
                    update_mode,
                    value_type,
                    value_schema,
                    enabled: Some(enabled),
                    pipeline,
                    automations,
                })
            })
            .collect();

        // 3. Log summary
        tracing::info!("Loaded {}/{} tags for agent {}", tags.len(), total_rows, agent_id);

        Ok(AgentConfig {
            agent_id: agent_id.to_string(),
            // Mock MQTT config for now, as it's not in DB yet
            mqtt: MqttConfig {
                host: "localhost".to_string(),
                port: 1883,
                status_topic: None,
            },
            printer: printer_config_json.and_then(|v| serde_json::from_value(v).ok()),
            tags,
            heartbeat_interval_secs: heartbeat_interval_secs as u64,
        })
    }
}

use crate::config::{AgentConfig, MqttConfig, TagConfig};
use anyhow::{Result, anyhow};
use domain::device::Device;
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
        // 1. Check if agent exists (V2 edge_agents has no heartbeat_interval_secs or printer_config)
        let agent_row = sqlx::query("SELECT id FROM edge_agents WHERE id = $1")
            .bind(agent_id)
            .fetch_optional(&self.pool)
            .await?;

        let _agent_row = match agent_row {
            Some(row) => row,
            None => return Err(anyhow!("Agent {} not found", agent_id)),
        };

        // Defaults for fields no longer persisted in the DB
        let heartbeat_interval_secs: i64 = 30;
        let printer_config_json: Option<serde_json::Value> = None;

        // 2. Fetch Devices (V2: driver_type column, name required)
        let device_rows = sqlx::query(
            "SELECT id, driver_type, connection_config, enabled FROM devices WHERE edge_agent_id = $1",
        )
        .bind(agent_id)
        .fetch_all(&self.pool)
        .await?;

        let devices: Vec<Device> = device_rows
            .into_iter()
            .map(|row| {
                let driver_str: String = row.get("driver_type");
                let driver = serde_json::from_value(serde_json::json!(driver_str))
                    .unwrap_or(DriverType::Simulator);

                Device {
                    id: row.get("id"),
                    driver,
                    connection_config: row.get("connection_config"),
                    enabled: row.get("enabled"),
                }
            })
            .collect();

        // 3. Fetch Tags (via Join on Devices)
        let tag_rows = sqlx::query(
            r#"
            SELECT 
                t.id, 
                t.device_id,
                t.source_config, 
                t.update_mode, 
                t.update_config, 
                t.value_type, 
                t.value_schema,
                t.enabled,
                t.pipeline_config
            FROM tags t
            JOIN devices d ON t.device_id = d.id
            WHERE d.edge_agent_id = $1
            "#,
        )
        .bind(agent_id)
        .fetch_all(&self.pool)
        .await?;

        let tags: Vec<TagConfig> = tag_rows
            .into_iter()
            .map(|row| {
                let id: String = row.get("id");
                let device_id: String = row.get("device_id");
                let source_config: serde_json::Value = row.get("source_config");

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
                    _ => None,
                };

                TagConfig {
                    id,
                    device_id: Some(device_id),
                    driver: None,
                    driver_config: Some(source_config),
                    update_mode,
                    value_type: Some(match row.get::<String, _>("value_type").as_str() {
                        "Simple" => TagValueType::Simple,
                        "Composite" => TagValueType::Composite,
                        _ => TagValueType::Simple,
                    }),
                    value_schema: row.get("value_schema"),
                    enabled: Some(row.get("enabled")),
                    pipeline: row
                        .get::<Option<serde_json::Value>, _>("pipeline_config")
                        .and_then(|v| serde_json::from_value(v).ok()),
                    automations: vec![],
                }
            })
            .collect();

        Ok(AgentConfig {
            version: uuid::Uuid::new_v4().to_string(),
            agent_id: agent_id.to_string(),
            mqtt: MqttConfig {
                host: "localhost".to_string(),
                port: 1883,
                status_topic: None,
            },
            printer: printer_config_json.and_then(|v| serde_json::from_value(v).ok()),
            devices,
            tags,
            heartbeat_interval_secs: heartbeat_interval_secs as u64,
        })
    }
}

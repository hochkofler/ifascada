use infrastructure::MqttClient;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::{collections::HashMap, sync::RwLock};
use tokio::sync::broadcast;
use tracing::info;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum AgentStatus {
    Online,
    Offline,
    Unknown,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AgentData {
    pub id: String,
    pub status: AgentStatus,
    pub last_seen: chrono::DateTime<chrono::Utc>,
    pub metrics: Option<serde_json::Value>, // uptime, active_tags, etc
    pub is_registered: bool,

    // Monitoring Policy
    pub heartbeat_interval_secs: i32,
    pub missed_threshold: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TagData {
    pub id: String,
    pub agent_id: String,
    pub value: serde_json::Value,
    pub quality: String,
    pub status: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub received_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReportData {
    pub report_id: String,
    #[serde(default)]
    pub agent_id: String,
    pub items: Vec<domain::event::ReportItem>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type", content = "payload")]
pub enum SystemEvent {
    TagChanged(TagData),
    AgentStatusChanged(AgentData),
    ReportCompleted(ReportData),
}

pub struct AppState {
    pub agents: RwLock<HashMap<String, AgentData>>,
    pub tags: RwLock<HashMap<String, TagData>>,
    pub mqtt_client: MqttClient,
    pub pool: sqlx::PgPool,
    pub buffer: infrastructure::database::SQLiteBuffer,
    pub tx: broadcast::Sender<SystemEvent>,
}

impl AppState {
    pub fn new(
        mqtt_client: MqttClient,
        pool: sqlx::PgPool,
        buffer: infrastructure::database::SQLiteBuffer,
    ) -> Self {
        let (tx, _) = broadcast::channel(100);
        Self {
            agents: RwLock::new(HashMap::new()),
            tags: RwLock::new(HashMap::new()),
            mqtt_client,
            pool,
            buffer,
            tx,
        }
    }

    pub fn update_agent_status(&self, agent_id: String, status: AgentStatus) {
        let mut agents = self.agents.write().unwrap();
        let agent = agents.entry(agent_id.clone()).or_insert_with(|| AgentData {
            id: agent_id.clone(),
            status: AgentStatus::Unknown,
            last_seen: chrono::Utc::now(),
            metrics: None,
            is_registered: false,
            heartbeat_interval_secs: 30,
            missed_threshold: 2,
        });

        let old_status = agent.status.clone();
        agent.status = status.clone();
        agent.last_seen = chrono::Utc::now();

        if old_status.to_string() != status.to_string() {
            // Persist transition (Fire and Forget for now, or use a channel)
            let pool = self.pool.clone();
            let aid = agent_id.clone();
            let new_status_str = status.to_string();
            let old_status_str = old_status.to_string();

            tokio::spawn(async move {
                let status_to_lower = new_status_str.to_lowercase();
                let _ = sqlx::query(
                    "INSERT INTO agent_status_history (agent_id, old_status, new_status) VALUES ($1, $2, $3)"
                )
                .bind(aid.clone())
                .bind(old_status_str)
                .bind(new_status_str)
                .execute(&pool)
                .await;

                // Also update the edge_agents table status
                let _ = sqlx::query(
                    "UPDATE edge_agents SET status = $1, updated_at = NOW() WHERE id = $2",
                )
                .bind(status_to_lower)
                .bind(aid)
                .execute(&pool)
                .await;
            });

            // Notify SSE only on change or heartbeat (heartbeat has its own notification)
            let _ = self.tx.send(SystemEvent::AgentStatusChanged(agent.clone()));
        }
    }

    pub fn update_agent_heartbeat(&self, agent_id: String, metrics: serde_json::Value) {
        let mut agents = self.agents.write().unwrap();
        let agent = agents.entry(agent_id.clone()).or_insert_with(|| AgentData {
            id: agent_id.clone(),
            status: AgentStatus::Online,
            last_seen: chrono::Utc::now(),
            metrics: None,
            is_registered: false,
            heartbeat_interval_secs: 30,
            missed_threshold: 2,
        });

        let old_status = agent.status.clone();
        agent.status = AgentStatus::Online;
        agent.last_seen = chrono::Utc::now();
        agent.metrics = Some(metrics.clone());

        if old_status.to_string() != "Online" {
            // Handle transition from Offline/Unknown to Online
            let pool = self.pool.clone();
            let aid = agent_id.clone();
            let old_status_str = old_status.to_string();

            tokio::spawn(async move {
                let aid_update = aid.clone();
                let _ = sqlx::query(
                     "INSERT INTO agent_status_history (agent_id, old_status, new_status, reason) VALUES ($1, $2, 'Online', 'Heartbeat recovered')"
                 )
                 .bind(aid)
                 .bind(old_status_str)
                 .execute(&pool).await;

                let _ = sqlx::query(
                    "UPDATE edge_agents SET status = 'online', updated_at = NOW() WHERE id = $1",
                )
                .bind(aid_update)
                .execute(&pool)
                .await;
            });
        }

        // --- Tag-Level Monitoring ---
        // Update tags status based on heartbeat tag list
        if let Some(tag_ids) = metrics.get("tag_ids").and_then(|v| v.as_array()) {
            let active_tag_ids: std::collections::HashSet<String> = tag_ids
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect();

            let mut tags = self.tags.write().unwrap();
            let pool = self.pool.clone();

            // Get all tags for this agent from memory
            let agent_tags: Vec<String> = tags
                .values()
                .filter(|t| t.agent_id == agent_id)
                .map(|t| t.id.clone())
                .collect();

            for tag_id in agent_tags {
                if let Some(tag) = tags.get_mut(&tag_id) {
                    let new_status = if active_tag_ids.contains(&tag_id) {
                        "online".to_string()
                    } else {
                        "offline".to_string()
                    };

                    if tag.status != new_status {
                        tag.status = new_status.clone();
                        tag.quality = if new_status == "online" {
                            "Good".to_string()
                        } else {
                            "Uncertain".to_string()
                        };

                        // Persist to DB
                        let tid = tag_id.clone();
                        let ns = new_status.clone();
                        let p = pool.clone();
                        tokio::spawn(async move {
                            let quality = if ns == "online" { "good" } else { "uncertain" };
                            let _ = sqlx::query("UPDATE tags SET status = $1, quality = $2, updated_at = NOW() WHERE id = $3")
                                .bind(ns)
                                .bind(quality)
                                .bind(tid)
                                .execute(&p)
                                .await;
                        });
                    }
                }
            }
        }

        // Notify SSE on status change OR heartbeat
        let _ = self.tx.send(SystemEvent::AgentStatusChanged(agent.clone()));
    }

    pub fn update_tag(&self, mut tag_data: TagData) {
        tag_data.received_at = Some(chrono::Utc::now());
        let mut tags = self.tags.write().unwrap();
        tags.insert(tag_data.id.clone(), tag_data.clone());

        // Notify SSE
        let _ = self.tx.send(SystemEvent::TagChanged(tag_data));
    }

    pub async fn load_agents_from_db(&self) -> Result<(), sqlx::Error> {
        // V2: edge_agents has no heartbeat_interval_secs / missed_heartbeat_threshold columns
        let rows = sqlx::query("SELECT id, status FROM edge_agents")
            .fetch_all(&self.pool)
            .await?;

        let mut agents = self.agents.write().unwrap();
        for row in rows {
            let id: String = row.get("id");
            let status_db: Option<String> = row.get("status");

            let status = match status_db.as_deref().unwrap_or("unknown") {
                "online" => AgentStatus::Online,
                "offline" => AgentStatus::Offline,
                _ => AgentStatus::Unknown,
            };

            agents.insert(
                id.clone(),
                AgentData {
                    id,
                    status,
                    last_seen: chrono::Utc::now(),
                    metrics: None,
                    is_registered: true,
                    heartbeat_interval_secs: 30, // Default: not stored in V2 schema
                    missed_threshold: 2,         // Default: not stored in V2 schema
                },
            );
        }
        Ok(())
    }

    pub async fn load_tags_from_db(&self) -> Result<(), sqlx::Error> {
        // V2: edge_agent_id removed from tags — join devices to get agent_id
        let rows = sqlx::query(
            r#"
            SELECT t.id, d.edge_agent_id, t.last_value, t.quality, t.status, t.last_update
            FROM tags t
            JOIN devices d ON t.device_id = d.id
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        let mut tags = self.tags.write().unwrap();
        for row in rows {
            let id: String = row.get("id");
            let agent_id: String = row.get("edge_agent_id");
            let value: serde_json::Value = row
                .get::<Option<serde_json::Value>, _>("last_value")
                .unwrap_or(serde_json::Value::Null);
            let quality: String = row
                .get::<Option<String>, _>("quality")
                .unwrap_or_else(|| "uncertain".to_string());
            let status: String = row
                .get::<Option<String>, _>("status")
                .unwrap_or_else(|| "unknown".to_string());
            let timestamp: chrono::DateTime<chrono::Utc> = row
                .get::<Option<chrono::DateTime<chrono::Utc>>, _>("last_update")
                .unwrap_or_else(|| chrono::Utc::now());

            tags.insert(
                id.clone(),
                TagData {
                    id,
                    agent_id,
                    value,
                    quality,
                    status,
                    timestamp,
                    received_at: None,
                },
            );
        }
        Ok(())
    }

    pub async fn reset_all_tag_statuses(&self) -> Result<(), sqlx::Error> {
        info!("Resetting all tag statuses to offline/unknown...");
        sqlx::query(
            "UPDATE tags SET status = 'offline', quality = 'uncertain', updated_at = NOW()",
        )
        .execute(&self.pool)
        .await?;

        let mut tags = self.tags.write().unwrap();
        for tag in tags.values_mut() {
            tag.status = "offline".to_string();
            tag.quality = "uncertain".to_string();
        }
        Ok(())
    }

    pub fn check_agent_liveness(&self) {
        let mut agents_to_notify = Vec::new();
        {
            let mut agents = self.agents.write().unwrap();
            let now = chrono::Utc::now();

            for agent in agents.values_mut() {
                if matches!(agent.status, AgentStatus::Online) {
                    let timeout_secs =
                        (agent.heartbeat_interval_secs * (agent.missed_threshold + 1)) as i64;
                    let diff = now - agent.last_seen;

                    if diff.num_seconds() > timeout_secs {
                        tracing::warn!(agent_id = %agent.id, "Agent heartbeat timeout ({}s). Marking Offline.", timeout_secs);
                        agent.status = AgentStatus::Offline;
                        agents_to_notify.push(agent.clone());

                        // Persist transition
                        let pool = self.pool.clone();
                        let aid = agent.id.clone();
                        tokio::spawn(async move {
                            let aid_update = aid.clone();
                            let _ = sqlx::query(
                                "INSERT INTO agent_status_history (agent_id, old_status, new_status, reason) VALUES ($1, 'Online', 'Offline', 'Heartbeat timeout')"
                            )
                            .bind(aid)
                            .execute(&pool)
                            .await;

                            let _ = sqlx::query(
                                "UPDATE edge_agents SET status = 'offline', updated_at = NOW() WHERE id = $1"
                            )
                            .bind(aid_update.clone())
                            .execute(&pool)
                            .await;

                            // Also mark all tags of this agent as offline
                            let _ = sqlx::query(
                                "UPDATE tags SET status = 'offline', quality = 'uncertain', updated_at = NOW() WHERE edge_agent_id = $1"
                            )
                            .bind(aid_update)
                            .execute(&pool)
                            .await;
                        });

                        // Update in-memory tags as well
                        {
                            let mut tags = self.tags.write().unwrap();
                            for tag in tags.values_mut() {
                                if tag.agent_id == agent.id {
                                    tag.status = "offline".to_string();
                                    tag.quality = "uncertain".to_string();
                                }
                            }
                        }
                    }
                }
            }
        }

        // Send notifications outside the write lock
        for agent in agents_to_notify {
            let _ = self.tx.send(SystemEvent::AgentStatusChanged(agent));
        }
    }
}

impl std::fmt::Display for AgentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentStatus::Online => write!(f, "Online"),
            AgentStatus::Offline => write!(f, "Offline"),
            AgentStatus::Unknown => write!(f, "Unknown"),
        }
    }
}

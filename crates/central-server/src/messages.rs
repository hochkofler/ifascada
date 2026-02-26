use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TagTelemetryMessage {
    pub schema_version: u16,
    pub source: String,
    pub tag_id: String,
    pub value: Value,
    pub quality: Value,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WriteAckMessage {
    pub schema_version: u16,
    pub source: String,
    pub tag_id: Option<String>,
    pub command_id: Option<String>,
    pub success: bool,
    pub reason: Option<String>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ActionResultMessage {
    pub schema_version: u16,
    pub source: String,
    pub request_id: Option<String>,
    pub action_type: String,
    pub accepted: bool,
    pub reason: Option<String>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ActionAuditMessage {
    pub schema_version: u16,
    pub source: String,
    pub request_id: Option<String>,
    pub action_type: String,
    pub outcome: String,
    pub reason: Option<String>,
    #[serde(default)]
    pub payload: Option<Value>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WriteAuditMessage {
    pub schema_version: u16,
    pub source: String,
    pub connection_id: Option<String>,
    pub tag_id: String,
    pub command_id: Option<String>,
    pub value: Value,
    pub outcome: String,
    pub reason: Option<String>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HealthRuntimeMessage {
    pub schema_version: u16,
    pub source: String,
    pub status: String,
    pub outbox_depth: usize,
    pub outbox_oldest_age_secs: Option<u64>,
    #[serde(default)]
    pub config_hash: Option<String>,
    #[serde(default)]
    pub config_sync_state: Option<String>,
    #[serde(default)]
    pub config_target_hash: Option<String>,
    #[serde(default)]
    pub config_last_check_at: Option<DateTime<Utc>>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConfigApplyResultMessage {
    pub schema_version: u16,
    pub source: String,
    #[serde(default)]
    pub request_id: Option<String>,
    pub accepted: bool,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub current_config_hash: Option<String>,
    #[serde(default)]
    pub target_config_hash: Option<String>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConnectionStateMessage {
    pub schema_version: u16,
    pub source: String,
    pub connection_id: String,
    pub state: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeviceConnectionStateMessage {
    pub schema_version: u16,
    pub source: String,
    pub connection_id: String,
    pub device_id: String,
    #[serde(default)]
    pub tag_id: Option<String>,
    pub state: String,
    #[serde(default)]
    pub reason: Option<String>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ControlResetResultMessage {
    pub schema_version: u16,
    pub source: String,
    #[serde(default)]
    pub request_id: Option<String>,
    pub accepted: bool,
    #[serde(default)]
    pub reason: Option<String>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AlertRuntimeMessage {
    pub schema_version: u16,
    pub source: String,
    pub severity: String,
    pub alert_type: String,
    pub state: String,
    pub message: String,
    pub timestamp: DateTime<Utc>,
}

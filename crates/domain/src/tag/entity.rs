use crate::id::{DeviceId, TagId};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TagUpdateMode {
    Polling { interval_ms: u64 },
    OnChange,
    OnMessage,
    PollingOnChange { interval_ms: u64 },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TagValueType {
    Float,
    Integer,
    Boolean,
    String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    pub id: TagId,
    pub name: String,
    pub device_id: DeviceId,
    pub enabled: bool,
    pub value_type: TagValueType,
    pub update_mode: TagUpdateMode,
    pub source: String, // Protocol-agnostic source string
    // Generic tag metadata used by runtime policies (pipeline/validation/display).
    #[serde(default = "default_tag_metadata")]
    pub metadata: Value,
}

impl Tag {
    pub fn new(id: TagId, name: String, device_id: DeviceId, source: String) -> Self {
        Self {
            id,
            name,
            device_id,
            enabled: true,
            value_type: TagValueType::Float,
            update_mode: TagUpdateMode::Polling { interval_ms: 1000 },
            source,
            metadata: default_tag_metadata(),
        }
    }
}

fn default_tag_metadata() -> Value {
    json!({})
}

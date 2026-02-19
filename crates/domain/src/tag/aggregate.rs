use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::{PipelineConfig, TagId, TagQuality, TagStatus, TagUpdateMode, TagValueType};

/// Tag aggregate root - main entity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    id: TagId,
    // driver_type: DriverType, // Removed in V2
    source_config: serde_json::Value, // Renamed from driver_config
    // edge_agent_id: String, // Removed in V2
    device_id: String, // Promoted from Option
    update_mode: TagUpdateMode,
    value_type: TagValueType,
    value_schema: Option<serde_json::Value>,
    pipeline_config: PipelineConfig,
    enabled: bool,
    metadata: Option<serde_json::Value>,

    // Runtime state
    last_value: Option<serde_json::Value>,
    last_update: Option<DateTime<Utc>>,
    status: TagStatus,
    quality: TagQuality,
    error_message: Option<String>,

    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl Tag {
    /// Create a new tag
    pub fn new(
        id: TagId,
        device_id: String,
        source_config: serde_json::Value,
        update_mode: TagUpdateMode,
        value_type: TagValueType,
        pipeline_config: PipelineConfig,
    ) -> Self {
        let now = Utc::now();

        Self {
            id,
            device_id,
            source_config,
            update_mode,
            value_type,
            value_schema: None,
            pipeline_config,
            enabled: true,
            metadata: None,
            last_value: None,
            last_update: None,
            status: TagStatus::default(),
            quality: TagQuality::default(),
            error_message: None,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn with_device_id(mut self, device_id: String) -> Self {
        self.device_id = device_id;
        self
    }

    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    // Getters
    pub fn id(&self) -> &TagId {
        &self.id
    }

    pub fn source_config(&self) -> &serde_json::Value {
        &self.source_config
    }

    pub fn update_mode(&self) -> &TagUpdateMode {
        &self.update_mode
    }

    pub fn status(&self) -> TagStatus {
        self.status
    }

    pub fn quality(&self) -> TagQuality {
        self.quality
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn is_healthy(&self) -> bool {
        self.enabled && self.status.is_healthy() && self.quality.is_usable()
    }

    /// Check if tag has timed out
    pub fn is_timed_out(&self) -> bool {
        match self.update_mode {
            TagUpdateMode::OnChange { .. } => false,
            TagUpdateMode::Polling { .. } => match self.last_update {
                Some(last_update) => {
                    let timeout_ms = self.update_mode.timeout_ms();
                    let elapsed_ms = (Utc::now() - last_update).num_milliseconds() as u64;
                    elapsed_ms > timeout_ms
                }
                None => true,
            },
            TagUpdateMode::PollingOnChange { .. } => match self.last_update {
                Some(last_update) => {
                    let timeout_ms = self.update_mode.timeout_ms();
                    let elapsed_ms = (Utc::now() - last_update).num_milliseconds() as u64;
                    elapsed_ms > timeout_ms
                }
                None => true,
            },
        }
    }

    /// Update tag value
    pub fn update_value(&mut self, value: serde_json::Value, quality: TagQuality) {
        self.last_value = Some(value);
        self.last_update = Some(Utc::now());
        self.quality = quality;
        self.status = if quality.is_usable() {
            TagStatus::Online
        } else if matches!(quality, TagQuality::Timeout) {
            TagStatus::Offline
        } else {
            TagStatus::Error
        };
        self.updated_at = Utc::now();
    }

    /// Mark tag as offline
    pub fn mark_offline(&mut self) {
        self.status = TagStatus::Offline;
        self.quality = TagQuality::Timeout;
        self.updated_at = Utc::now();
    }

    /// Mark tag as error
    pub fn mark_error(&mut self, message: String) {
        self.status = TagStatus::Error;
        self.quality = TagQuality::Bad;
        self.error_message = Some(message);
        self.updated_at = Utc::now();
    }

    /// Enable tag
    pub fn enable(&mut self) {
        self.enabled = true;
        self.updated_at = Utc::now();
    }

    /// Disable tag
    pub fn disable(&mut self) {
        self.enabled = false;
        self.updated_at = Utc::now();
    }

    /// Reset timeout timer (update last_update to now)
    pub fn reset_timeout(&mut self) {
        self.last_update = Some(Utc::now());
        self.updated_at = Utc::now();
    }

    pub fn pipeline_config(&self) -> &PipelineConfig {
        &self.pipeline_config
    }

    pub fn update_mode_type(&self) -> &str {
        match self.update_mode {
            TagUpdateMode::OnChange { .. } => "OnChange",
            TagUpdateMode::Polling { .. } => "Polling",
            TagUpdateMode::PollingOnChange { .. } => "PollingOnChange",
        }
    }

    pub fn value_type(&self) -> TagValueType {
        self.value_type
    }

    pub fn value_type_str(&self) -> &str {
        match self.value_type {
            TagValueType::Simple => "Simple",
            TagValueType::Composite => "Composite",
        }
    }

    pub fn value_schema(&self) -> Option<serde_json::Value> {
        self.value_schema
            .as_ref()
            .map(|schema| serde_json::to_value(schema).unwrap_or(serde_json::Value::Null))
    }

    pub fn description(&self) -> Option<&str> {
        None // Will implement later
    }

    pub fn metadata(&self) -> Option<&serde_json::Value> {
        self.metadata.as_ref()
    }

    pub fn last_value(&self) -> Option<&serde_json::Value> {
        self.last_value.as_ref()
    }

    pub fn last_update(&self) -> Option<DateTime<Utc>> {
        self.last_update
    }

    pub fn error_message(&self) -> Option<&str> {
        self.error_message.as_deref()
    }

    /// Get the "primary" numeric value for triggers and automation
    pub fn get_primary_value(&self) -> f64 {
        let val = match &self.last_value {
            Some(v) => v,
            None => return 0.0,
        };

        match self.value_type {
            TagValueType::Simple => val.as_f64().unwrap_or(0.0),
            TagValueType::Composite => {
                // Try to find the primary key from schema, or default to "value"
                let primary_key = self
                    .value_schema
                    .as_ref()
                    .and_then(|s| s.get("primary"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("value");

                val.get(primary_key).and_then(|v| v.as_f64()).unwrap_or(0.0)
            }
        }
    }

    /// Get a user-friendly display string
    pub fn get_display_string(&self) -> String {
        let val = match &self.last_value {
            Some(v) => v,
            None => return "---".to_string(),
        };

        match self.value_type {
            TagValueType::Simple => {
                let unit = self
                    .value_schema
                    .as_ref()
                    .and_then(|s| s.get("unit"))
                    .and_then(|u| u.as_str())
                    .unwrap_or("");
                format!("{} {}", val, unit).trim().to_string()
            }
            TagValueType::Composite => {
                // Formatting according to schema if possible
                if let Some(obj) = val.as_object() {
                    let mut parts = Vec::new();
                    for (k, v) in obj {
                        let label = self
                            .value_schema
                            .as_ref()
                            .and_then(|s| s.get("labels"))
                            .and_then(|l| l.get(k.as_str()))
                            .and_then(|v| v.as_str())
                            .unwrap_or(k);
                        parts.push(format!("{}: {}", label, v));
                    }
                    if parts.is_empty() {
                        val.to_string()
                    } else {
                        parts.join(", ")
                    }
                } else {
                    val.to_string()
                }
            }
        }
    }

    /// Get a formatted string for printing
    pub fn get_print_string(&self) -> String {
        self.get_display_string() // Can be customized further if needed
    }

    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }

    // Setters for repository reconstruction
    #[doc(hidden)]
    pub fn set_value_schema(&mut self, schema: serde_json::Value) {
        self.value_schema = Some(schema);
    }

    #[doc(hidden)]
    pub fn set_pipeline_config(&mut self, config: PipelineConfig) {
        self.pipeline_config = config;
    }

    #[doc(hidden)]
    pub fn set_description(&mut self, _description: String) {
        // Will store in metadata for now
    }

    #[doc(hidden)]
    pub fn set_metadata(&mut self, metadata: serde_json::Value) {
        self.metadata = Some(metadata);
    }

    #[doc(hidden)]
    pub fn set_runtime_state(
        &mut self,
        last_value: Option<serde_json::Value>,
        last_update: Option<DateTime<Utc>>,
        status: TagStatus,
        quality: TagQuality,
        error_message: Option<String>,
    ) {
        self.last_value = last_value;
        self.last_update = last_update;
        self.status = status;
        self.quality = quality;
        self.error_message = error_message;
    }

    #[doc(hidden)]
    pub fn set_timestamps(&mut self, created_at: DateTime<Utc>, updated_at: DateTime<Utc>) {
        self.created_at = created_at;
        self.updated_at = updated_at;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn create_test_tag() -> Tag {
        Tag::new(
            TagId::new("TEST_TAG").unwrap(),
            "device-1".to_string(),  // Device ID
            json!({"port": "COM3"}), // Source Config
            TagUpdateMode::OnChange {
                debounce_ms: 100,
                timeout_ms: 30000,
            },
            TagValueType::Simple,
            PipelineConfig::default(),
        )
    }

    #[test]
    fn test_new_tag() {
        let tag = create_test_tag();
        assert_eq!(tag.id().as_str(), "TEST_TAG");
        assert_eq!(tag.status(), TagStatus::Unknown);
        assert_eq!(tag.quality(), TagQuality::Uncertain);
        assert!(tag.is_enabled());
        assert!(!tag.is_healthy()); // Unknown status is not healthy
    }

    #[test]
    fn test_update_value() {
        let mut tag = create_test_tag();
        tag.update_value(json!(25.5), TagQuality::Good);

        assert_eq!(tag.status(), TagStatus::Online);
        assert_eq!(tag.quality(), TagQuality::Good);
        assert!(tag.is_healthy());
    }

    #[test]
    fn test_mark_offline() {
        let mut tag = create_test_tag();
        tag.mark_offline();

        assert_eq!(tag.status(), TagStatus::Offline);
        assert_eq!(tag.quality(), TagQuality::Timeout);
        assert!(!tag.is_healthy());
    }

    #[test]
    fn test_mark_error() {
        let mut tag = create_test_tag();
        tag.mark_error("Serial port disconnected".to_string());

        assert_eq!(tag.status(), TagStatus::Error);
        assert_eq!(tag.quality(), TagQuality::Bad);
        assert!(!tag.is_healthy());
    }

    #[test]
    fn test_enable_disable() {
        let mut tag = create_test_tag();

        assert!(tag.is_enabled());

        tag.disable();
        assert!(!tag.is_enabled());
        assert!(!tag.is_healthy());

        tag.enable();
        assert!(tag.is_enabled());
    }
}

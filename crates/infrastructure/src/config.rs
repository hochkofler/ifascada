use config::{Config, ConfigError, Environment, File};
use domain::automation::AutomationConfig;
use domain::driver::DriverType;
use domain::tag::{TagUpdateMode, TagValueType};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct MqttConfig {
    pub host: String,
    pub port: u16,
    pub status_topic: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PrinterConfig {
    #[serde(default = "default_printer_enabled")]
    pub enabled: bool,
    #[serde(default = "default_printer_host")]
    pub host: String,
    #[serde(default = "default_printer_port")]
    pub port: u16,

    // Extended config for File/Shared printers
    pub r#type: Option<String>, // "Network" (default) or "File"
    pub path: Option<String>,   // Required if type is "File"
}

fn default_printer_enabled() -> bool {
    false
}
fn default_printer_host() -> String {
    "127.0.0.1".to_string()
}
fn default_printer_port() -> u16 {
    9100
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TagConfig {
    pub id: String,
    pub driver: DriverType,
    pub driver_config: serde_json::Value,
    #[serde(default)]
    pub update_mode: Option<TagUpdateMode>,
    #[serde(default)]
    pub value_type: Option<TagValueType>,
    #[serde(default)]
    pub value_schema: Option<serde_json::Value>,
    pub enabled: Option<bool>,
    // For manual mapping
    pub pipeline: Option<domain::tag::PipelineConfig>,
    #[serde(default)]
    pub automations: Vec<AutomationConfig>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AgentConfig {
    pub agent_id: String,
    pub mqtt: MqttConfig,
    #[serde(default)]
    pub printer: Option<PrinterConfig>,
    #[serde(default)]
    pub tags: Vec<TagConfig>,
    #[serde(default = "default_heartbeat_interval")]
    pub heartbeat_interval_secs: u64,
}

fn default_heartbeat_interval() -> u64 {
    30
}

impl AgentConfig {
    pub fn load(config_dir: &str) -> Result<Self, ConfigError> {
        let run_mode = std::env::var("RUN_MODE").unwrap_or_else(|_| "development".into());

        let s = Config::builder()
            // Start with default settings
            .set_default("mqtt.host", "localhost")?
            .set_default("mqtt.port", 1883)?
            // 3. Local config file (Third priority file) - e.g. config/default.toml
            // We make this REQUIRED to avoid starting with a missing configuration
            .add_source(File::with_name(&format!("{}/default", config_dir)).required(true))
            // 2. Persisted config from Central Server (Second priority file)
            .add_source(File::with_name(&format!("{}/last_known", config_dir)).required(false))
            // 1. CLI test config file (First priority file)
            .add_source(File::with_name(&format!("{}/{}", config_dir, run_mode)).required(false))
            // Environment variables (e.g. SCADA__MQTT__HOST=10.0.0.1)
            .add_source(Environment::with_prefix("SCADA").separator("__"))
            // CLI arguments are handled separately or can be merged here if passed as Source
            .build()?;

        s.try_deserialize()
    }
}

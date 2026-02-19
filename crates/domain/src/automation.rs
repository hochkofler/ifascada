use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AutomationConfig {
    pub name: String,
    pub trigger: TriggerConfig,
    pub action: ActionConfig,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub enum Operator {
    Equal,
    LessOrEqual,
    GreaterOrEqual,
    NotEqual,
    Less,
    Greater,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(tag = "type")]
pub enum TriggerConfig {
    /// Fires when a specific value is received 'count' times consecutively
    ConsecutiveValues {
        target_value: f64,
        count: usize,
        /// "Equal", "LessOrEqual", "GreaterOrEqual"
        #[serde(default = "default_operator")]
        operator: Operator,
        /// Reset count if no events within this window (optional)
        within_ms: Option<u64>,
    },
    // Future expansion:
    // StableWeight { duration_ms: u64, variation: f64 },
    // Threshold { value: f64, operator: String, deadband: f64 },
}

fn default_operator() -> Operator {
    Operator::Equal
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(tag = "type")]
pub enum ActionConfig {
    /// Prints a ticket using a specific template
    PrintTicket {
        template: String,
        /// Optional: URL of the print service if decoupled
        service_url: Option<String>,
    },
    /// Publishes a message to an MQTT topic
    PublishMqtt {
        topic: String,
        payload_template: String,
    },
    /// Accumulates data into a session buffer
    AccumulateData {
        session_id: String,
        /// Template for the line item (e.g. "Weight: {value} kg")
        template: String,
    },
    /// Prints the accumulated batch and clears the buffer
    PrintBatch {
        session_id: String,
        header_template: String,
        footer_template: String,
    },
}

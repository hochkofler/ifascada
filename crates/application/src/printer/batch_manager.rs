use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrintItem {
    pub value: serde_json::Value,
    pub timestamp: DateTime<Utc>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug)]
pub struct BatchManager {
    current_batch: Vec<PrintItem>,
    last_update: DateTime<Utc>,
    session_timeout: Duration,
}

impl BatchManager {
    pub fn new() -> Self {
        Self {
            current_batch: Vec::new(),
            last_update: Utc::now(),
            session_timeout: Duration::minutes(30),
        }
    }

    /// Adds an item to the batch, applying business rules for resets.
    pub fn add_item(&mut self, value: serde_json::Value, metadata: Option<serde_json::Value>) {
        let now = Utc::now();

        // Rule 1: Time Window Reset
        if now.signed_duration_since(self.last_update) > self.session_timeout {
            tracing::info!("⏰ Batch session expired (30m). Resetting.");
            self.current_batch.clear();
        }

        // Rule 2: Negative -> Positive Reset (New Weighing Cycle)
        // Only applicable if value is numeric or has a numeric "value" field
        let num_val = match &value {
            serde_json::Value::Number(n) => n.as_f64().unwrap_or(0.0),
            serde_json::Value::Object(map) => {
                map.get("value").and_then(|v| v.as_f64()).unwrap_or(0.0)
            }
            _ => 0.0,
        };

        if let Some(last_item) = self.current_batch.last() {
            let last_num = match &last_item.value {
                serde_json::Value::Number(n) => n.as_f64().unwrap_or(0.0),
                serde_json::Value::Object(map) => {
                    map.get("value").and_then(|v| v.as_f64()).unwrap_or(0.0)
                }
                _ => 0.0,
            };

            if last_num < 0.0 && num_val > 0.0 {
                tracing::info!("🔄 New Weighing Cycle detected (Neg -> Pos). Resetting batch.");
                self.current_batch.clear();
            }
        }

        self.current_batch.push(PrintItem {
            value,
            timestamp: now,
            metadata,
        });
        self.last_update = now;
    }

    /// Returns the current batch items and clears the buffer.
    pub fn take_batch(&mut self) -> Vec<PrintItem> {
        let batch = self.current_batch.clone();
        self.current_batch.clear();
        tracing::info!("📦 Batch taken ({} items). Buffer cleared.", batch.len());
        batch
    }

    pub fn is_empty(&self) -> bool {
        self.current_batch.is_empty()
    }
}

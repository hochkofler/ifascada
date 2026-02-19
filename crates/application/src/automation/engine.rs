use async_trait::async_trait;
use domain::automation::{ActionConfig, AutomationConfig, Operator, TriggerConfig};
use domain::event::DomainEvent;
use domain::event::EventPublisher;
use domain::tag::TagId;
use infrastructure::config::TagConfig;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, info};

/// Tracks the runtime state of a specific trigger (e.g. counters)
#[derive(Debug, Default)]
struct TriggerState {
    consecutive_matches: usize,
    _last_match: Option<chrono::DateTime<chrono::Utc>>,
}

/// Binds a configuration to its runtime state
struct ActiveAutomation {
    config: AutomationConfig,
    state: TriggerState,
    value_type: domain::tag::TagValueType,
    value_schema: Option<serde_json::Value>,
}

use super::executor::{ActionExecutor, LoggingActionExecutor};

pub struct AutomationEngine {
    /// Map of TagId -> List of active automations
    automations: Arc<Mutex<HashMap<TagId, Vec<ActiveAutomation>>>>,
    executor: Arc<dyn ActionExecutor>,
}

impl AutomationEngine {
    pub fn new(tags: Vec<TagConfig>, executor: Arc<dyn ActionExecutor>) -> Self {
        let map = Self::build_map(tags);
        Self {
            automations: Arc::new(Mutex::new(map)),
            executor,
        }
    }

    /// Create with default logging executor
    pub fn default(tags: Vec<TagConfig>) -> Self {
        Self::new(tags, Arc::new(LoggingActionExecutor))
    }

    fn build_map(tags: Vec<TagConfig>) -> HashMap<TagId, Vec<ActiveAutomation>> {
        let mut map = HashMap::new();
        for tag in tags {
            if tag.automations.is_empty() {
                continue;
            }
            let tag_id = match TagId::new(&tag.id) {
                Ok(id) => id,
                Err(_) => continue,
            };

            let value_type = tag.value_type.unwrap_or(domain::tag::TagValueType::Simple);

            let list: Vec<ActiveAutomation> = tag
                .automations
                .into_iter()
                .map(|cfg| ActiveAutomation {
                    config: cfg,
                    state: TriggerState::default(),
                    value_type,
                    value_schema: tag.value_schema.clone(),
                })
                .collect();
            if !list.is_empty() {
                info!(tag_id = %tag_id, count = %list.len(), "⚙️ Automations loaded");
                map.insert(tag_id, list);
            }
        }
        map
    }

    pub async fn reload(&self, tags: Vec<TagConfig>) {
        let new_map = Self::build_map(tags);
        let mut guard = self.automations.lock().await;
        *guard = new_map;
        info!("♻️ Automation Engine Reloaded");
    }

    /// Process an incoming event and fire automations if triggers match
    pub async fn handle_event(&self, event: &DomainEvent) {
        if let DomainEvent::TagValueUpdated { tag_id, value, .. } = event {
            let mut automations = self.automations.lock().await;

            if let Some(list) = automations.get_mut(tag_id) {
                for automation in list {
                    if self.evaluate_trigger(
                        &mut automation.state,
                        &automation.config.trigger,
                        value,
                        automation.value_type,
                        &automation.value_schema,
                    ) {
                        self.execute_action(&automation.config.action, tag_id, value)
                            .await;
                    }
                }
            }
        }
    }

    fn evaluate_trigger(
        &self,
        state: &mut TriggerState,
        trigger: &TriggerConfig,
        value: &serde_json::Value,
        value_type: domain::tag::TagValueType,
        value_schema: &Option<serde_json::Value>,
    ) -> bool {
        match trigger {
            TriggerConfig::ConsecutiveValues {
                target_value,
                count,
                operator,
                ..
            } => {
                // 1. Extract numeric value using logic similar to Tag aggregate
                let num_val = match (value_type, value) {
                    (domain::tag::TagValueType::Simple, serde_json::Value::Number(n)) => {
                        n.as_f64().unwrap_or(0.0)
                    }
                    (domain::tag::TagValueType::Composite, serde_json::Value::Object(_)) => {
                        let primary_key = value_schema
                            .as_ref()
                            .and_then(|s| s.get("primary"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("value");

                        value
                            .get(primary_key)
                            .and_then(|v| v.as_f64())
                            .unwrap_or(0.0)
                    }
                    _ => 0.0,
                };

                // 2. Check condition
                let match_condition = match operator {
                    Operator::Equal => (num_val - target_value).abs() < f64::EPSILON,
                    Operator::NotEqual => (num_val - target_value).abs() >= f64::EPSILON,
                    Operator::LessOrEqual => num_val <= *target_value,
                    Operator::GreaterOrEqual => num_val >= *target_value,
                    Operator::Greater => num_val > *target_value,
                    Operator::Less => num_val < *target_value,
                };

                debug!(
                    val = %num_val,
                    target = %target_value,
                    op = ?operator,
                    matched = %match_condition,
                    "Evaluation"
                );

                if match_condition {
                    state.consecutive_matches += 1;
                    info!(
                        current = %state.consecutive_matches,
                        target = %count,
                        val = %num_val,
                        "🔥 Trigger matching"
                    );
                } else {
                    state.consecutive_matches = 0;
                }

                // 3. Fire if threshold reached
                if state.consecutive_matches >= *count {
                    // Reset to avoid firing continuously? Or fire every time?
                    // User requirements: "2 consecutive zeros... triggers print"
                    // Usually we want to fire ONCE and then wait for reset (value > 0).
                    // But if it stays 0, 0, 0, 0... should it print twice?
                    // Let's assume we reset after firing to prevent spamming,
                    // requiring a "break" in the condition or just reset counter.
                    state.consecutive_matches = 0;
                    return true;
                }

                false
            }
        }
    }

    async fn execute_action(
        &self,
        action: &ActionConfig,
        tag_id: &TagId,
        payload: &serde_json::Value,
    ) {
        self.executor.execute(action, tag_id, payload).await;
    }
}

#[async_trait]
impl EventPublisher for AutomationEngine {
    async fn publish(
        &self,
        event: DomainEvent,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.handle_event(&event).await;
        Ok(())
    }
}

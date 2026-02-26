use crate::tag::TagValue;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AutomationSpec {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    pub trigger: TriggerSpec,
    #[serde(default)]
    pub action: Option<ActionSpec>,
    #[serde(default)]
    pub actions: Vec<ActionSpec>,
}

fn default_enabled() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ActionSpec {
    pub action_type: String,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub scope: Option<ExecutionScope>,
    #[serde(default)]
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionScope {
    Edge,
    Central,
    Auto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TriggerSpec {
    ConsecutiveNumeric {
        tag_id: String,
        threshold: f64,
        count: usize,
        #[serde(default = "default_numeric_operator")]
        operator: NumericOperator,
        #[serde(default)]
        within_ms: Option<u64>,
    },
}

fn default_numeric_operator() -> NumericOperator {
    NumericOperator::Eq
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NumericOperator {
    Eq,
    Ne,
    Lt,
    Lte,
    Gt,
    Gte,
}

#[derive(Debug, Clone)]
pub struct AutomationEvent<'a> {
    pub tag_id: &'a str,
    pub value: &'a TagValue,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct AutomationEvalState {
    pub consecutive_matches: usize,
    pub last_match_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AutomationEvalResult {
    pub matched: bool,
    pub fired: bool,
    pub next_state: AutomationEvalState,
}

impl AutomationSpec {
    pub fn resolved_actions(&self) -> Vec<&ActionSpec> {
        if !self.actions.is_empty() {
            self.actions.iter().collect()
        } else {
            self.action.iter().collect()
        }
    }
}

pub fn evaluate_automation(
    spec: &AutomationSpec,
    state: &AutomationEvalState,
    event: &AutomationEvent<'_>,
) -> AutomationEvalResult {
    if !spec.enabled {
        return AutomationEvalResult {
            matched: false,
            fired: false,
            next_state: state.clone(),
        };
    }

    match &spec.trigger {
        TriggerSpec::ConsecutiveNumeric {
            tag_id,
            threshold,
            count,
            operator,
            within_ms,
        } => {
            if tag_id != "*" && event.tag_id != tag_id {
                return AutomationEvalResult {
                    matched: false,
                    fired: false,
                    next_state: state.clone(),
                };
            }

            let mut next_state = state.clone();
            if let (Some(window_ms), Some(last_ts)) = (within_ms, next_state.last_match_at) {
                let elapsed_ms = event
                    .timestamp
                    .signed_duration_since(last_ts)
                    .num_milliseconds()
                    .max(0) as u64;
                if elapsed_ms > *window_ms {
                    next_state.consecutive_matches = 0;
                    next_state.last_match_at = None;
                }
            }

            let value = numeric_from_tag_value(event.value);
            let matched = value
                .map(|v| compare_numeric(v, *threshold, *operator))
                .unwrap_or(false);
            if matched {
                next_state.consecutive_matches = next_state.consecutive_matches.saturating_add(1);
                next_state.last_match_at = Some(event.timestamp);
            } else {
                next_state.consecutive_matches = 0;
                next_state.last_match_at = None;
            }

            let threshold_count = (*count).max(1);
            let fired = matched && next_state.consecutive_matches >= threshold_count;
            if fired {
                next_state.consecutive_matches = 0;
                next_state.last_match_at = None;
            }

            AutomationEvalResult {
                matched,
                fired,
                next_state,
            }
        }
    }
}

fn compare_numeric(value: f64, threshold: f64, op: NumericOperator) -> bool {
    match op {
        NumericOperator::Eq => (value - threshold).abs() < f64::EPSILON,
        NumericOperator::Ne => (value - threshold).abs() >= f64::EPSILON,
        NumericOperator::Lt => value < threshold,
        NumericOperator::Lte => value <= threshold,
        NumericOperator::Gt => value > threshold,
        NumericOperator::Gte => value >= threshold,
    }
}

fn numeric_from_tag_value(value: &TagValue) -> Option<f64> {
    match value {
        TagValue::Float(v) => Some(*v),
        TagValue::Integer(v) => Some(*v as f64),
        TagValue::Boolean(v) => Some(if *v { 1.0 } else { 0.0 }),
        TagValue::String(v) => {
            if let Ok(parsed) = v.trim().parse::<f64>() {
                return Some(parsed);
            }
            let json = serde_json::from_str::<serde_json::Value>(v).ok()?;
            match json {
                serde_json::Value::Number(n) => n.as_f64(),
                serde_json::Value::Object(map) => map.get("value").and_then(|vv| vv.as_f64()),
                _ => None,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tag::TagValue;

    fn mk_spec(count: usize) -> AutomationSpec {
        AutomationSpec {
            id: "a1".to_string(),
            name: Some("non_positive".to_string()),
            enabled: true,
            trigger: TriggerSpec::ConsecutiveNumeric {
                tag_id: "tag_a".to_string(),
                threshold: 0.0,
                count,
                operator: NumericOperator::Lte,
                within_ms: None,
            },
            action: Some(ActionSpec {
                action_type: "print.escpos".to_string(),
                target: Some("edge".to_string()),
                scope: Some(ExecutionScope::Edge),
                payload: serde_json::json!({"lines":["AUTO"]}),
            }),
            actions: vec![],
        }
    }

    #[test]
    fn fires_after_required_consecutive_matches() {
        let spec = mk_spec(2);
        let t0 = Utc::now();
        let mut state = AutomationEvalState::default();
        let r1 = evaluate_automation(
            &spec,
            &state,
            &AutomationEvent {
                tag_id: "tag_a",
                value: &TagValue::Float(0.0),
                timestamp: t0,
            },
        );
        assert!(r1.matched);
        assert!(!r1.fired);
        state = r1.next_state;

        let r2 = evaluate_automation(
            &spec,
            &state,
            &AutomationEvent {
                tag_id: "tag_a",
                value: &TagValue::Float(-1.0),
                timestamp: t0 + chrono::Duration::milliseconds(100),
            },
        );
        assert!(r2.matched);
        assert!(r2.fired);
    }

    #[test]
    fn resets_when_value_does_not_match() {
        let spec = mk_spec(2);
        let t0 = Utc::now();
        let s0 = AutomationEvalState::default();
        let r1 = evaluate_automation(
            &spec,
            &s0,
            &AutomationEvent {
                tag_id: "tag_a",
                value: &TagValue::Float(0.0),
                timestamp: t0,
            },
        );
        let r2 = evaluate_automation(
            &spec,
            &r1.next_state,
            &AutomationEvent {
                tag_id: "tag_a",
                value: &TagValue::Float(1.0),
                timestamp: t0 + chrono::Duration::milliseconds(100),
            },
        );
        assert!(!r2.matched);
        assert_eq!(r2.next_state.consecutive_matches, 0);
    }

    #[test]
    fn supports_numeric_from_json_string_payload() {
        let spec = mk_spec(1);
        let r = evaluate_automation(
            &spec,
            &AutomationEvalState::default(),
            &AutomationEvent {
                tag_id: "tag_a",
                value: &TagValue::String("{\"value\":-0.5,\"unit\":\"g\"}".to_string()),
                timestamp: Utc::now(),
            },
        );
        assert!(r.fired);
    }
}

use chrono::{DateTime, Utc};
use domain::tag::TagValue;
use domain::{
    AutomationEvalState, AutomationEvent, AutomationSpec, ExecutionScope, evaluate_automation,
};
use std::collections::HashMap;
use tracing::debug;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutomationRuntimeScope {
    Edge,
    Central,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ActionRequest {
    pub automation_id: String,
    pub action_type: String,
    pub target: Option<String>,
    pub request_id: String,
    pub payload: serde_json::Value,
    pub trigger_tag_id: String,
    pub trigger_value: TagValue,
    pub trigger_timestamp: DateTime<Utc>,
}

pub struct AutomationEngine {
    specs_by_tag: HashMap<String, Vec<AutomationSpec>>,
    states: HashMap<String, AutomationEvalState>,
    scope: AutomationRuntimeScope,
}

impl AutomationEngine {
    pub fn new(specs: Vec<AutomationSpec>, scope: AutomationRuntimeScope) -> Self {
        let mut specs_by_tag: HashMap<String, Vec<AutomationSpec>> = HashMap::new();
        for spec in specs {
            let has_scope_match = spec
                .resolved_actions()
                .iter()
                .any(|a| scope_matches(scope, a.scope));
            if !spec.enabled || !has_scope_match {
                continue;
            }
            let tag_id = match &spec.trigger {
                domain::TriggerSpec::ConsecutiveNumeric { tag_id, .. } => tag_id.clone(),
            };
            specs_by_tag.entry(tag_id).or_default().push(spec);
        }
        Self {
            specs_by_tag,
            states: HashMap::new(),
            scope,
        }
    }

    pub fn scope(&self) -> AutomationRuntimeScope {
        self.scope
    }

    pub fn on_tag_changed(
        &mut self,
        tag_id: &str,
        value: &TagValue,
        timestamp: DateTime<Utc>,
    ) -> Vec<ActionRequest> {
        let Some(specs) = self.specs_by_tag.get(tag_id).cloned() else {
            return Vec::new();
        };

        let mut out = Vec::new();
        for spec in specs {
            let state = self.states.get(&spec.id).cloned().unwrap_or_default();
            let eval = evaluate_automation(
                &spec,
                &state,
                &AutomationEvent {
                    tag_id,
                    value,
                    timestamp,
                },
            );
            self.states.insert(spec.id.clone(), eval.next_state);
            debug!(
                automation_id = %spec.id,
                tag_id = tag_id,
                matched = eval.matched,
                fired = eval.fired,
                consecutive_before = state.consecutive_matches,
                consecutive_after = self
                    .states
                    .get(&spec.id)
                    .map(|s| s.consecutive_matches)
                    .unwrap_or_default(),
                "automation evaluation"
            );
            if eval.fired {
                for (idx, action) in spec
                    .resolved_actions()
                    .into_iter()
                    .filter(|a| scope_matches(self.scope, a.scope))
                    .enumerate()
                {
                    let request_id = format!(
                        "auto-{}-{}-{}",
                        spec.id,
                        timestamp.timestamp_millis(),
                        idx
                    );
                    out.push(ActionRequest {
                        automation_id: spec.id.clone(),
                        action_type: action.action_type.clone(),
                        target: action.target.clone(),
                        request_id,
                        payload: action.payload.clone(),
                        trigger_tag_id: tag_id.to_string(),
                        trigger_value: value.clone(),
                        trigger_timestamp: timestamp,
                    });
                }
            }
        }
        out
    }
}

fn scope_matches(runtime_scope: AutomationRuntimeScope, action_scope: Option<ExecutionScope>) -> bool {
    match (runtime_scope, action_scope.unwrap_or(ExecutionScope::Auto)) {
        (AutomationRuntimeScope::Edge, ExecutionScope::Edge) => true,
        // Edge runtime may forward central-intent actions (e.g. print.persist)
        // while still executing locally and publishing durable audit/result.
        (AutomationRuntimeScope::Edge, ExecutionScope::Central) => true,
        (AutomationRuntimeScope::Central, ExecutionScope::Central) => true,
        (_, ExecutionScope::Auto) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::{ActionSpec, AutomationSpec, NumericOperator, TriggerSpec};

    fn mk_spec(id: &str, tag: &str, scope: Option<ExecutionScope>) -> AutomationSpec {
        AutomationSpec {
            id: id.to_string(),
            name: Some(id.to_string()),
            enabled: true,
            trigger: TriggerSpec::ConsecutiveNumeric {
                tag_id: tag.to_string(),
                threshold: 0.0,
                count: 2,
                operator: NumericOperator::Lte,
                within_ms: None,
            },
            action: Some(ActionSpec {
                action_type: "print.escpos".to_string(),
                target: Some("edge".to_string()),
                scope,
                payload: serde_json::json!({"lines":["AUTO"]}),
            }),
            actions: vec![],
        }
    }

    #[test]
    fn emits_action_after_consecutive_trigger() {
        let mut engine = AutomationEngine::new(
            vec![mk_spec("a1", "tag_a", Some(ExecutionScope::Edge))],
            AutomationRuntimeScope::Edge,
        );
        let t0 = Utc::now();
        let a0 = engine.on_tag_changed("tag_a", &TagValue::Float(0.0), t0);
        assert!(a0.is_empty());
        let a1 = engine.on_tag_changed(
            "tag_a",
            &TagValue::Float(-0.1),
            t0 + chrono::Duration::milliseconds(100),
        );
        assert_eq!(a1.len(), 1);
        assert_eq!(a1[0].action_type, "print.escpos");
    }

    #[test]
    fn applies_scope_filter() {
        let specs = vec![
            mk_spec("edge_only", "tag_a", Some(ExecutionScope::Edge)),
            mk_spec("central_only", "tag_a", Some(ExecutionScope::Central)),
        ];
        let mut edge_engine = AutomationEngine::new(specs.clone(), AutomationRuntimeScope::Edge);
        let mut central_engine = AutomationEngine::new(specs, AutomationRuntimeScope::Central);
        let t0 = Utc::now();
        assert!(
            edge_engine
                .on_tag_changed("tag_a", &TagValue::Float(0.0), t0)
                .is_empty()
        );
        let edge_actions = edge_engine.on_tag_changed(
            "tag_a",
            &TagValue::Float(-1.0),
            t0 + chrono::Duration::milliseconds(100),
        );
        assert_eq!(edge_actions.len(), 2);
        assert!(edge_actions.iter().any(|a| a.automation_id == "edge_only"));
        assert!(edge_actions.iter().any(|a| a.automation_id == "central_only"));

        assert!(
            central_engine
                .on_tag_changed("tag_a", &TagValue::Float(0.0), t0)
                .is_empty()
        );
        let central_actions = central_engine.on_tag_changed(
            "tag_a",
            &TagValue::Float(-1.0),
            t0 + chrono::Duration::milliseconds(100),
        );
        assert_eq!(central_actions.len(), 1);
        assert_eq!(central_actions[0].automation_id, "central_only");
    }

    #[test]
    fn emits_multiple_actions_from_single_trigger() {
        let spec = AutomationSpec {
            id: "multi".to_string(),
            name: Some("multi".to_string()),
            enabled: true,
            trigger: TriggerSpec::ConsecutiveNumeric {
                tag_id: "tag_a".to_string(),
                threshold: 0.0,
                count: 2,
                operator: NumericOperator::Lte,
                within_ms: None,
            },
            action: None,
            actions: vec![
                ActionSpec {
                    action_type: "print.escpos".to_string(),
                    target: Some("edge".to_string()),
                    scope: Some(ExecutionScope::Edge),
                    payload: serde_json::json!({"lines":["LOCAL"]}),
                },
                ActionSpec {
                    action_type: "print.persist".to_string(),
                    target: Some("central".to_string()),
                    scope: Some(ExecutionScope::Central),
                    payload: serde_json::json!({"mode":"persist"}),
                },
            ],
        };

        let t0 = Utc::now();
        let mut edge_engine = AutomationEngine::new(vec![spec.clone()], AutomationRuntimeScope::Edge);
        assert!(edge_engine.on_tag_changed("tag_a", &TagValue::Float(-0.1), t0).is_empty());
        let edge_actions = edge_engine.on_tag_changed(
            "tag_a",
            &TagValue::Float(-0.2),
            t0 + chrono::Duration::milliseconds(100),
        );
        assert_eq!(edge_actions.len(), 2);
        assert!(edge_actions.iter().any(|a| a.action_type == "print.escpos"));
        assert!(edge_actions.iter().any(|a| a.action_type == "print.persist"));

        let mut central_engine = AutomationEngine::new(vec![spec], AutomationRuntimeScope::Central);
        assert!(central_engine.on_tag_changed("tag_a", &TagValue::Float(-0.1), t0).is_empty());
        let central_actions = central_engine.on_tag_changed(
            "tag_a",
            &TagValue::Float(-0.2),
            t0 + chrono::Duration::milliseconds(100),
        );
        assert_eq!(central_actions.len(), 1);
        assert_eq!(central_actions[0].action_type, "print.persist");
    }
}

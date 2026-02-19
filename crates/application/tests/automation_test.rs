use application::automation::engine::AutomationEngine;
use application::automation::executor::ActionExecutor;
use async_trait::async_trait;
use domain::automation::{ActionConfig, AutomationConfig, Operator, TriggerConfig};
use domain::event::{DomainEvent, ReportItem};
use domain::tag::TagId;
use infrastructure::config::TagConfig;
use serde_json::json;
use std::sync::Arc;
use tokio::sync::Mutex;

// Mock Executor
struct MockActionExecutor {
    executed_actions: Arc<Mutex<Vec<ActionConfig>>>,
}

impl MockActionExecutor {
    fn new() -> Self {
        Self {
            executed_actions: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

#[async_trait]
impl ActionExecutor for MockActionExecutor {
    async fn execute(&self, action: &ActionConfig, _tag_id: &TagId, _payload: &serde_json::Value) {
        let mut actions = self.executed_actions.lock().await;
        actions.push(action.clone());
    }

    async fn execute_manual_batch(&self, _tag_id: &TagId, _items: Vec<ReportItem>) {
        // Mock implementation
    }
}

#[tokio::test]
async fn test_consecutive_zeros_trigger() {
    // 1. Setup Configuration
    let automation_config = AutomationConfig {
        name: "AutoPrint".to_string(),
        trigger: TriggerConfig::ConsecutiveValues {
            target_value: 0.0,
            count: 2,
            operator: Operator::Equal,
            within_ms: None,
        },
        action: ActionConfig::PrintTicket {
            template: "TEST_TICKET".to_string(),
            service_url: None,
        },
    };

    let tag_config = TagConfig {
        id: "SCALE_TEST".to_string(),
        device_id: None,                                     // NEW
        driver: Some(domain::driver::DriverType::Simulator), // Option
        driver_config: Some(json!({})),                      // Option
        update_mode: None,
        value_type: None,
        value_schema: None,
        enabled: Some(true),
        pipeline: None,
        automations: vec![automation_config],
    };

    // 2. Initialize Engine with Mock Executor
    let mock_executor = MockActionExecutor::new();
    let executed_actions = mock_executor.executed_actions.clone();

    let engine = AutomationEngine::new(vec![tag_config], Arc::new(mock_executor));

    // 3. Simulate Events

    // Event 1: Value 10 (Non-zero) -> Should NOT trigger
    let event1 = DomainEvent::TagValueUpdated {
        tag_id: TagId::new("SCALE_TEST").unwrap(),
        value: json!(10.0),
        quality: domain::tag::TagQuality::Good,
        timestamp: chrono::Utc::now(),
    };
    engine.handle_event(&event1).await;
    assert_eq!(executed_actions.lock().await.len(), 0);

    // Event 2: Value 0 (1st zero) -> Should increment counter (1/2) but NOT fire
    let event2 = DomainEvent::TagValueUpdated {
        tag_id: TagId::new("SCALE_TEST").unwrap(),
        value: json!(0.0),
        quality: domain::tag::TagQuality::Good,
        timestamp: chrono::Utc::now(),
    };
    engine.handle_event(&event2).await;
    assert_eq!(executed_actions.lock().await.len(), 0);

    // Event 3: Value 0 (2nd zero) -> Should increment counter (2/2) AND FIRE
    let event3 = DomainEvent::TagValueUpdated {
        tag_id: TagId::new("SCALE_TEST").unwrap(),
        value: json!(0.0),
        quality: domain::tag::TagQuality::Good,
        timestamp: chrono::Utc::now(),
    };
    engine.handle_event(&event3).await;

    // VERIFICATION: Check if action was executed
    let actions = executed_actions.lock().await;
    assert_eq!(actions.len(), 1, "Action should have been triggered!");

    match &actions[0] {
        ActionConfig::PrintTicket { template, .. } => {
            assert_eq!(template, "TEST_TICKET");
        }
        _ => panic!("Wrong action type"),
    }
}

#[tokio::test]
async fn test_composite_value_trigger() {
    // 1. Setup Configuration
    let automation_config = AutomationConfig {
        name: "AutoPrintComposite".to_string(),
        trigger: TriggerConfig::ConsecutiveValues {
            target_value: 0.0,
            count: 2,
            operator: Operator::Equal,
            within_ms: None,
        },
        action: ActionConfig::PrintTicket {
            template: "TEST_TICKET_COMPOSITE".to_string(),
            service_url: None,
        },
    };

    let tag_config = TagConfig {
        id: "SCALE_COMPOSITE".to_string(),
        device_id: None,                                     // NEW
        driver: Some(domain::driver::DriverType::Simulator), // Option
        driver_config: Some(json!({})),                      // Option
        update_mode: None,
        value_type: Some(domain::tag::TagValueType::Composite),
        value_schema: Some(json!({"primary": "weight"})),
        enabled: Some(true),
        pipeline: None,
        automations: vec![automation_config],
    };

    // 2. Initialize Engine with Mock Executor
    let mock_executor = MockActionExecutor::new();
    let executed_actions = mock_executor.executed_actions.clone();

    let engine = AutomationEngine::new(vec![tag_config], Arc::new(mock_executor));

    // 3. Simulate Events with COMPOSITE values

    // Event 1: Value {weight: 10.0, unit: "kg"} -> Should NOT trigger
    let event1 = DomainEvent::TagValueUpdated {
        tag_id: TagId::new("SCALE_COMPOSITE").unwrap(),
        value: json!({"weight": 10.0, "unit": "kg"}),
        quality: domain::tag::TagQuality::Good,
        timestamp: chrono::Utc::now(),
    };
    engine.handle_event(&event1).await;
    assert_eq!(executed_actions.lock().await.len(), 0);

    // Event 2: Value {weight: 0.0, unit: "kg"} (1st zero)
    let event2 = DomainEvent::TagValueUpdated {
        tag_id: TagId::new("SCALE_COMPOSITE").unwrap(),
        value: json!({"weight": 0.0, "unit": "kg"}),
        quality: domain::tag::TagQuality::Good,
        timestamp: chrono::Utc::now(),
    };
    engine.handle_event(&event2).await;
    assert_eq!(executed_actions.lock().await.len(), 0);

    // Event 3: Value {weight: 0.0, unit: "kg"} (2nd zero) -> Should FIRE
    let event3 = DomainEvent::TagValueUpdated {
        tag_id: TagId::new("SCALE_COMPOSITE").unwrap(),
        value: json!({"weight": 0.0, "unit": "kg"}),
        quality: domain::tag::TagQuality::Good,
        timestamp: chrono::Utc::now(),
    };
    engine.handle_event(&event3).await;

    // VERIFICATION
    let actions = executed_actions.lock().await;
    assert_eq!(
        actions.len(),
        1,
        "Composite value action should have been triggered!"
    );

    match &actions[0] {
        ActionConfig::PrintTicket { template, .. } => {
            assert_eq!(template, "TEST_TICKET_COMPOSITE");
        }
        _ => panic!("Wrong action type"),
    }
}

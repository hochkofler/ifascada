use application::automation::executor::{ActionExecutor, PrintingActionExecutor};
use domain::automation::ActionConfig;
use domain::tag::TagId;
use serde_json::json;
use tokio::sync::mpsc;

#[tokio::test]
async fn test_batch_accumulation_and_print() {
    // Setup
    let (tx, mut rx) = mpsc::channel(32);

    // Simple Mock Publisher
    struct MockPublisher;
    #[async_trait::async_trait]
    impl domain::event::EventPublisher for MockPublisher {
        async fn publish(
            &self,
            _event: domain::DomainEvent,
        ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            Ok(())
        }
    }

    let executor = PrintingActionExecutor::new(
        tx,
        "test_agent".to_string(),
        std::sync::Arc::new(MockPublisher),
    );
    let tag_id = TagId::new("SCALE_01").unwrap();
    let session_id = "test_session".to_string();

    // 1. Accumulate Item 1 (Weight: 10.0)
    let action_acc = ActionConfig::AccumulateData {
        session_id: session_id.clone(),
        template: "ignored".to_string(),
    };
    executor
        .execute(&action_acc, &tag_id, &json!({"value": 10.0}))
        .await;

    // 2. Accumulate Item 2 (Weight: 20.0)
    executor
        .execute(&action_acc, &tag_id, &json!({"value": 20.0}))
        .await;

    // 3. Print Batch
    let action_print = ActionConfig::PrintBatch {
        session_id: session_id.clone(),
        header_template: "BATCH REPORT".to_string(),
        footer_template: "END".to_string(),
    };
    executor.execute(&action_print, &tag_id, &json!({})).await;

    // Verify Output
    // We expect ONE print job containing both items
    let job = rx.recv().await.expect("Should receive print job");
    let job_str = String::from_utf8_lossy(&job);

    println!("Print Output:\n{}", job_str);

    assert!(job_str.contains("BATCH REPORT"));
    assert!(job_str.contains("1.     10.0"));
    assert!(job_str.contains("2.     20.0"));
    assert!(job_str.contains("FIN DEL REPORTE"));
}

#[tokio::test]
async fn test_batch_reset_on_negative_to_positive() {
    // Setup
    let (tx, mut rx) = mpsc::channel(32);

    struct MockPublisher;
    #[async_trait::async_trait]
    impl domain::event::EventPublisher for MockPublisher {
        async fn publish(
            &self,
            _event: domain::DomainEvent,
        ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            Ok(())
        }
    }

    let executor = PrintingActionExecutor::new(
        tx,
        "test_agent".to_string(),
        std::sync::Arc::new(MockPublisher),
    );
    let tag_id = TagId::new("SCALE_01").unwrap();
    let session_id = "session_reset".to_string();

    let action_acc = ActionConfig::AccumulateData {
        session_id: session_id.clone(),
        template: "ignored".to_string(),
    };

    // 1. Accumulate -5.0 (Tare/Negative)
    executor
        .execute(&action_acc, &tag_id, &json!({"value": -5.0}))
        .await;

    // 2. Accumulate 10.0 (Positive) -> Should perform RESET of previous items
    // The -5.0 was the *last item*. So adding 10.0 should clear the -5.0 and add 10.0.
    executor
        .execute(&action_acc, &tag_id, &json!({"value": 10.0}))
        .await;

    // 3. Print Batch
    let action_print = ActionConfig::PrintBatch {
        session_id: session_id.clone(),
        header_template: "RESET TEST".to_string(),
        footer_template: "END".to_string(),
    };
    executor.execute(&action_print, &tag_id, &json!({})).await;

    // Verify Output
    let job = rx.recv().await.expect("Should receive print job");
    let job_str = String::from_utf8_lossy(&job);

    println!("Print Output (Reset Test):\n{}", job_str);

    assert!(job_str.contains("1.     10.0"));
    assert!(
        !job_str.contains("-5"),
        "Negative value should have been cleared"
    );
}

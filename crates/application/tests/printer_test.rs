use application::automation::executor::{ActionExecutor, PrintingActionExecutor};
use application::printer::manager::PrinterManager;
use domain::automation::ActionConfig;
use domain::tag::TagId;
use infrastructure::printer::MockPrinter;
use serde_json::json;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::sleep;

#[tokio::test]
async fn test_printer_flow() {
    // 1. Setup Mock Printer
    let mock_printer = MockPrinter::new();
    let sent_data = mock_printer.sent_data.clone();

    // 2. Setup Printer Manager
    let (tx, rx) = mpsc::channel(32);
    let manager = PrinterManager::new(Box::new(mock_printer.clone()), rx);

    // Spawn Manager
    tokio::spawn(manager.run());

    // Allow manager to "connect"
    sleep(Duration::from_millis(100)).await;

    // 3. Setup Executor
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
        "agent-1".to_string(),
        std::sync::Arc::new(MockPublisher),
    );

    // 4. Trigger Print Action
    let action = ActionConfig::PrintTicket {
        template: "ticket".to_string(),
        service_url: None,
    };
    let tag_id = TagId::new("SCALE_01").unwrap();
    let payload = json!({"value": 123.45, "unit": "kg"});

    executor.execute(&action, &tag_id, &payload).await;

    // Wait for processing
    sleep(Duration::from_millis(200)).await;

    // 5. Verify Data
    let data = sent_data.lock().await;
    assert!(!data.is_empty(), "Printer should have received data");

    // Convert to string to check for keywords (ESC/POS contains binary but also ASCII text)
    // We filter out non-printable to avoid noise
    let printable: String = data
        .iter()
        .map(|&b| if b >= 32 && b <= 126 { b as char } else { '.' })
        .collect();

    println!("Printer Output (ASCII-fied): {}", printable);

    assert!(printable.contains("LABORATORIOS IFA S.A."));
    assert!(printable.contains("SCALE_01"));
    assert!(printable.contains("123.45"));
}

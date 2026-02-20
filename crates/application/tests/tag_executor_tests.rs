use application::tag::TagExecutor;
use async_trait::async_trait;
use dashmap::DashSet;
use domain::driver::{ConnectionState, DriverConnection};
use domain::event::EventPublisher;
use domain::tag::{PipelineConfig, TagUpdateMode, TagValueType};
use domain::{DomainError, DomainEvent, Tag, TagId};
use infrastructure::pipeline::ConcretePipelineFactory;
use serde_json::json;
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};
use tokio_util::sync::CancellationToken;

// --- Infrastructure Mocks (Ports) ---

struct MockDriver {
    rx: Arc<Mutex<mpsc::UnboundedReceiver<Result<Option<serde_json::Value>, String>>>>,
    state: ConnectionState,
}

impl MockDriver {
    fn new() -> (
        Self,
        mpsc::UnboundedSender<Result<Option<serde_json::Value>, String>>,
    ) {
        let (tx, rx) = mpsc::unbounded_channel();
        (
            Self {
                rx: Arc::new(Mutex::new(rx)),
                state: ConnectionState::Disconnected,
            },
            tx,
        )
    }
}

#[async_trait]
impl DriverConnection for MockDriver {
    async fn connect(&mut self) -> Result<(), DomainError> {
        self.state = ConnectionState::Connected;
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<(), DomainError> {
        self.state = ConnectionState::Disconnected;
        Ok(())
    }

    async fn read_value(&mut self) -> Result<Option<serde_json::Value>, DomainError> {
        let mut rx = self.rx.lock().await;
        match rx.recv().await {
            Some(Ok(val)) => Ok(val),
            Some(Err(e)) => Err(DomainError::DriverError(e)),
            None => Ok(None),
        }
    }

    async fn write_value(&mut self, _value: serde_json::Value) -> Result<(), DomainError> {
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.state == ConnectionState::Connected
    }

    fn connection_state(&self) -> ConnectionState {
        self.state
    }

    fn driver_type(&self) -> &str {
        "Mock"
    }
}

struct MockEventPublisher {
    tx: mpsc::UnboundedSender<DomainEvent>,
}

impl MockEventPublisher {
    fn new() -> (Arc<Self>, mpsc::UnboundedReceiver<DomainEvent>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (Arc::new(Self { tx }), rx)
    }
}

#[async_trait]
impl EventPublisher for MockEventPublisher {
    async fn publish(
        &self,
        event: DomainEvent,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let _ = self.tx.send(event);
        Ok(())
    }
}

// --- Use Case Tests (UC-APP-001) ---

#[tokio::test]
async fn uc_app_001_happy_path_successful_read() {
    // ----------------------------------------------------
    // GIVEN: A TagExecutor connected to a Mock Driver
    // ----------------------------------------------------
    let (driver, tx_driver) = MockDriver::new();
    let (publisher, mut rx_events) = MockEventPublisher::new();

    // Create Tag
    let tag = Tag::new(
        TagId::new("HappyTag").unwrap(),
        "mock-device".to_string(),
        json!({}),
        TagUpdateMode::Polling { interval_ms: 100 },
        TagValueType::Simple,
        PipelineConfig::default(),
    );

    let token = CancellationToken::new();
    let registry = Arc::new(DashSet::new());
    let factory = ConcretePipelineFactory;

    // We use Box::new for the driver as it's a trait object
    let mut executor = TagExecutor::new(
        tag,
        Box::new(driver),
        publisher,
        &factory,
        token.clone(),
        registry,
    );

    // Spawn executor in background
    let handle = tokio::spawn(async move { executor.execute().await });

    // Wait for initial connection event (Business Rule 1)
    let event = rx_events
        .recv()
        .await
        .expect("Should receive connected event");
    assert_eq!(event.event_type(), "TagConnected");

    // ----------------------------------------------------
    // WHEN: Driver returns valid data
    // ----------------------------------------------------
    tx_driver
        .send(Ok(Some(json!(123.45))))
        .expect("Send failed");

    // ----------------------------------------------------
    // THEN: TagValueUpdated event is published (Business Rule 4)
    // ----------------------------------------------------
    let event = rx_events.recv().await.expect("Should receive value event");

    assert_eq!(event.event_type(), "TagValueUpdated");
    if let DomainEvent::TagValueUpdated { value, .. } = event {
        assert_eq!(value, 123.45);
    } else {
        panic!("Wrong event type");
    }

    // Cleanup
    token.cancel();
    let _ = handle.await;
}

#[tokio::test]
async fn uc_app_001_error_path_driver_failure() {
    // ----------------------------------------------------
    // GIVEN: A TagExecutor with a driver that will fail
    // ----------------------------------------------------
    let (driver, tx_driver) = MockDriver::new();
    let (publisher, mut rx_events) = MockEventPublisher::new();

    // Create Tag
    let tag = Tag::new(
        TagId::new("ErrorTag").unwrap(),
        "mock-device".to_string(),
        json!({}),
        TagUpdateMode::Polling { interval_ms: 100 },
        TagValueType::Simple,
        PipelineConfig::default(),
    );

    let token = CancellationToken::new();
    let registry = Arc::new(DashSet::new());
    let factory = ConcretePipelineFactory;

    let mut executor = TagExecutor::new(
        tag,
        Box::new(driver),
        publisher,
        &factory,
        token.clone(),
        registry,
    );

    let handle = tokio::spawn(async move { executor.execute().await });

    // Wait for connection
    let _ = rx_events.recv().await;

    // ----------------------------------------------------
    // WHEN: Driver returns an error (timeout/io error)
    // ----------------------------------------------------
    tx_driver
        .send(Err("Device unreachable".to_string()))
        .expect("Send failed");

    // ----------------------------------------------------
    // THEN: TagError event is published (Business Rule 3)
    // ----------------------------------------------------
    let event = rx_events.recv().await.expect("Should receive error event");

    assert_eq!(event.event_type(), "TagExecutorError"); // Use correct event type from DomainEvent
    if let DomainEvent::TagExecutorError { error, .. } = event {
        assert_eq!(error, "Driver error: Device unreachable");
    } else {
        panic!("Wrong event type: {:?}", event);
    }

    // Cleanup
    token.cancel();
    let _ = handle.await;
}

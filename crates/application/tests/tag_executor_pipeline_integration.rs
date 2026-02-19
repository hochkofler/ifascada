use application::tag::TagExecutor;
use async_trait::async_trait;
use domain::driver::{ConnectionState, DriverConnection};
use domain::event::EventPublisher;
use domain::tag::{ParserConfig, PipelineConfig, TagUpdateMode, TagValueType};
use domain::{DomainError, DomainEvent, Tag, TagId};
use serde_json::json;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};
use tokio_util::sync::CancellationToken;

// --- Mock Driver ---

struct ChannelDriver {
    rx: Arc<Mutex<mpsc::UnboundedReceiver<String>>>,
    state: ConnectionState,
}

impl ChannelDriver {
    fn new() -> (Self, mpsc::UnboundedSender<String>) {
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
impl DriverConnection for ChannelDriver {
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
        // Non-blocking try_recv or blocking recv?
        // Real driver blocks with timeout.
        // For test, we can just recv. If channel empty, it waits.
        // But TagExecutor loop wants to run continuously.
        // If we block forever, we might stall thread? No, it's async.
        // But let's use a small timeout to allow loop to check other things if needed,
        // or just return None if empty?
        // virtual_serial_mock in rustscada blocks until newline.
        // Here we receive full strings.

        // Let's use generic recv which awaits.
        // To allow test to finish/shutdown, we might need a way to stop.
        // But for this test, we just want to read one value.

        // Wait, if we await rx.recv(), and no data comes, we wait forever.
        // We can wrap in timeout or just rely on test data being sent.
        match rx.recv().await {
            Some(msg) => Ok(Some(serde_json::Value::String(msg))),
            None => Ok(None), // Channel closed
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
        "ChannelMock"
    }
}

// --- Mock Event Publisher ---

struct ChannelEventPublisher {
    tx: mpsc::UnboundedSender<DomainEvent>,
}

impl ChannelEventPublisher {
    fn new() -> (Arc<Self>, mpsc::UnboundedReceiver<DomainEvent>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (Arc::new(Self { tx }), rx)
    }
}

#[async_trait]
impl EventPublisher for ChannelEventPublisher {
    async fn publish(
        &self,
        event: DomainEvent,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let _ = self.tx.send(event); // Ignore errors (receiver dropped)
        Ok(())
    }
}

// --- Test ---

#[tokio::test]
async fn test_executor_parses_scale_data() {
    // 1. Setup Driver
    let (driver, tx_driver) = ChannelDriver::new();

    // 2. Setup Tag with ScaleParser
    let mut tag = Tag::new(
        TagId::new("SCALE_01").unwrap(),
        "device-scale".to_string(),
        json!({"port": "COM1"}),
        TagUpdateMode::OnChange {
            debounce_ms: 10,
            timeout_ms: 1000,
        }, // Low debounce for fast test
        TagValueType::Composite, // It will be object
        domain::tag::PipelineConfig::default(),
    );

    // Configure pipeline
    let mut pipeline = PipelineConfig::default();
    pipeline.parser = Some(ParserConfig::Custom {
        name: "ScaleParser".to_string(),
        config: Some(json!({})),
    });
    tag.set_pipeline_config(pipeline);

    // 3. Setup Publisher
    let (publisher, mut rx_events) = ChannelEventPublisher::new();

    // 4. Create Executor
    let token = CancellationToken::new();
    let registry = Arc::new(Mutex::new(HashSet::new()));
    let mut executor = TagExecutor::new(tag, Box::new(driver), publisher, token, registry);

    // 5. Spawn Executor
    let handle = tokio::spawn(async move { executor.execute().await });

    // 6. Wait for connection
    // We expect TagConnected event first
    let event = rx_events
        .recv()
        .await
        .expect("Should receive connected event");
    assert_eq!(event.event_type(), "TagConnected");

    // 7. Send Scale Data
    tx_driver
        .send("ST,GS,  5.00kg".to_string())
        .expect("Failed to send data");

    // 8. Wait for ValueUpdated event
    let event = rx_events.recv().await.expect("Should receive value event");

    assert_eq!(event.event_type(), "TagValueUpdated");

    if let DomainEvent::TagValueUpdated { value, .. } = event {
        assert_eq!(value["value"], 5.00);
        assert_eq!(value["unit"], "kg");
    } else {
        panic!("Wrong event type: {:?}", event);
    }

    // Abort executor
    handle.abort();
}

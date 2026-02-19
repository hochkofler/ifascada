use application::tag::TagExecutor;
use async_trait::async_trait;
use domain::driver::{ConnectionState, DriverConnection};
use domain::event::EventPublisher;
use domain::tag::{TagUpdateMode, TagValueType};
use domain::{DomainError, DomainEvent, Tag, TagId};
use serde_json::json;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};
use tokio::time::Duration;
use tokio_util::sync::CancellationToken;

// --- Mock Driver with Fault Injection ---

#[derive(Clone)]
struct ResilienceDriver {
    state: Arc<Mutex<ConnectionState>>,
    // Number of times connect() should fail before succeeding
    connect_fail_count: Arc<Mutex<usize>>,
    // Channel to push values (optional for this test, but needed for read)
    rx: Arc<Mutex<mpsc::UnboundedReceiver<Option<serde_json::Value>>>>,
}

impl ResilienceDriver {
    fn new(initial_fail_count: usize) -> (Self, mpsc::UnboundedSender<Option<serde_json::Value>>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (
            Self {
                state: Arc::new(Mutex::new(ConnectionState::Disconnected)),
                connect_fail_count: Arc::new(Mutex::new(initial_fail_count)),
                rx: Arc::new(Mutex::new(rx)),
            },
            tx,
        )
    }

    async fn set_state(&self, new_state: ConnectionState) {
        let mut state = self.state.lock().await;
        *state = new_state;
    }
}

#[async_trait]
impl DriverConnection for ResilienceDriver {
    async fn connect(&mut self) -> Result<(), DomainError> {
        let mut count = self.connect_fail_count.lock().await;
        if *count > 0 {
            *count -= 1;
            return Err(DomainError::DriverError(
                "Simulated Connection Failure".to_string(),
            ));
        }

        let mut state = self.state.lock().await;
        *state = ConnectionState::Connected;
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<(), DomainError> {
        let mut state = self.state.lock().await;
        *state = ConnectionState::Disconnected;
        Ok(())
    }

    async fn read_value(&mut self) -> Result<Option<serde_json::Value>, DomainError> {
        // If not connected, return error to trigger reconnection logic in Executor
        let state = *self.state.lock().await;
        if state != ConnectionState::Connected {
            return Err(DomainError::DriverError("Driver Disconnected".to_string()));
        }

        let mut rx = self.rx.lock().await;
        // Use timeout to avoid blocking forever if no data
        match tokio::time::timeout(Duration::from_millis(100), rx.recv()).await {
            Ok(Some(val)) => Ok(val),
            Ok(None) => Ok(None), // Channel closed
            Err(_) => Ok(None),   // Timeout, no data
        }
    }

    async fn write_value(&mut self, _value: serde_json::Value) -> Result<(), DomainError> {
        Ok(())
    }

    fn is_connected(&self) -> bool {
        // This method is synchronous, need to use blocking lock or cheat?
        // Since we are in async context, we shouldn't block.
        // But the trait is sync.
        // For testing we can use try_lock or just assume.
        // Let's rely on self.state.try_lock()
        if let Ok(state) = self.state.try_lock() {
            *state == ConnectionState::Connected
        } else {
            false
        }
    }

    fn connection_state(&self) -> ConnectionState {
        if let Ok(state) = self.state.try_lock() {
            *state
        } else {
            ConnectionState::Disconnected
        }
    }

    fn driver_type(&self) -> &str {
        "ResilienceMock"
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
        let _ = self.tx.send(event);
        Ok(())
    }
}

// --- Tests ---

#[tokio::test]
async fn test_infinite_retry_on_startup() {
    // Scenario: Driver fails to connect 3 times, then succeeds.
    // We expect the Executor NOT to return error, but to eventually emit TagConnected.

    let (driver, _tx_data) = ResilienceDriver::new(3);

    let tag = Tag::new(
        TagId::new("RETRY_TAG").unwrap(),
        domain::driver::DriverType::RS232,
        json!({"port": "COM_FAIL"}),
        "test-agent".to_string(),
        TagUpdateMode::OnChange {
            debounce_ms: 10,
            timeout_ms: 5000,
        },
        TagValueType::Simple,
    );

    let (publisher, mut rx_events) = ChannelEventPublisher::new();
    let token = CancellationToken::new();
    let registry = Arc::new(Mutex::new(HashSet::new()));
    let mut executor = TagExecutor::new(tag, Box::new(driver), publisher, token, registry);

    // Spawn executor
    let handle = tokio::spawn(async move {
        // This should run forever (or until we abort)
        let _ = executor.execute().await;
    });

    // We expect:
    // 1. Initial failure (logged, swallowed)
    // 2. Retry 1 (fail)
    // 3. Retry 2 (fail)
    // 4. Retry 3 (fail)
    // 5. Retry 4 (Success!) -> TagConnected event

    // We can monitor events. We should eventually see TagConnected.
    // Timeouts: Backoff starts at 1s. So: 0s (start), 1s (retry1), 2s (retry2), 4s (retry3).
    // Total wait approx 7s.

    tokio::time::pause();

    let mut connected = false;
    // We loop enough times to cover the backoff (1s, 2s, 4s = 7s total wait + execution time)
    // 20 iterations of 1s = 20s simulated time, plenty.
    for i in 0..20 {
        // Advance time by 1s
        tokio::time::advance(Duration::from_secs(1)).await;

        // Consume all available events
        while let Ok(event) = rx_events.try_recv() {
            println!(
                "DEBUG: Received event at sec {}: {}",
                i + 1,
                event.event_type()
            );
            if event.event_type() == "TagConnected" {
                connected = true;
            }
        }

        if connected {
            break;
        }
    }

    assert!(
        connected,
        "Did not receive TagConnected event within 20s (simulated)"
    );

    handle.abort();
}

#[tokio::test]
async fn test_runtime_self_healing() {
    // Scenario: Connects OK, then disconnected, then reconnects.

    let (driver, _tx_data) = ResilienceDriver::new(0); // 0 initial failures

    let tag = Tag::new(
        TagId::new("HEAL_TAG").unwrap(),
        domain::driver::DriverType::RS232,
        json!({"port": "COM_OK"}),
        "test-agent".to_string(),
        TagUpdateMode::OnChange {
            debounce_ms: 10,
            timeout_ms: 1000,
        },
        TagValueType::Simple,
    );

    let (publisher, mut rx_events) = ChannelEventPublisher::new();
    let token = CancellationToken::new();
    let registry = Arc::new(Mutex::new(HashSet::new()));
    let mut executor = TagExecutor::new(tag, Box::new(driver.clone()), publisher, token, registry);

    let handle = tokio::spawn(async move {
        let _ = executor.execute().await;
    });

    // 1. Expect Connected
    let event = rx_events.recv().await.unwrap();
    assert_eq!(event.event_type(), "TagConnected");

    // 2. Force Disconnect
    println!("Simulating disconnection...");
    driver.set_state(ConnectionState::Disconnected).await;
    // Next read loop will fail reading and trigger error handling

    // 3. Expect Error Event
    let event = rx_events.recv().await.unwrap();
    // Might be TagExecutorError
    assert!(event.event_type() == "TagExecutorError" || event.event_type() == "TagDisconnected");

    // 4. Recover
    // The driver logic `read_value` returns error if Disconnected.
    // `tag_executor` calls `disconnect` then `reconnect`.
    // `reconnect` calls `connect`.
    // Our `connect` implementation sets state to `Connected`.
    // So it should heal on next retry (1s later).

    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(2)).await;

    // 5. Expect Re-Connected
    let event = rx_events.recv().await.unwrap();
    assert_eq!(event.event_type(), "TagConnected");

    handle.abort();
}

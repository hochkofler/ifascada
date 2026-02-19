use anyhow::{Result, anyhow};
use async_trait::async_trait;
use domain::{
    DomainEvent,
    event::EventPublisher,
    tag::{TagId, TagQuality},
};
use infrastructure::database::SQLiteBuffer;
use infrastructure::messaging::buffered_publisher::BufferedMqttPublisher;
use infrastructure::messaging::mqtt_client::MqttPublisherClient;
use serde_json::json;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;
use tokio::time::sleep;

// 1. Mock Client
#[derive(Clone)]
struct MockMqttClient {
    pub published_messages: Arc<Mutex<Vec<(String, Vec<u8>)>>>,
    pub connected: Arc<AtomicBool>,
    pub should_fail_publish: Arc<AtomicBool>,
}

impl MockMqttClient {
    fn new() -> Self {
        Self {
            published_messages: Arc::new(Mutex::new(Vec::new())),
            connected: Arc::new(AtomicBool::new(true)),
            should_fail_publish: Arc::new(AtomicBool::new(false)),
        }
    }
}

#[async_trait]
impl MqttPublisherClient for MockMqttClient {
    async fn publish_bytes(
        &self,
        topic: &str,
        payload: &[u8],
        _qos: rumqttc::QoS,
        _retain: bool,
    ) -> Result<()> {
        if self.should_fail_publish.load(Ordering::Relaxed) {
            return Err(anyhow!("Simulated Publish Failure"));
        }

        // Even if publish succeeds technically, if we are "disconnected" logically,
        // the BufferedPublisher checks is_connected() first.
        // But if is_connected() is true, and this returns Ok, it works.

        self.published_messages
            .lock()
            .unwrap()
            .push((topic.to_string(), payload.to_vec()));
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Relaxed)
    }
}

// 2. The Test
#[tokio::test]
async fn test_offline_buffering_and_recovery() -> Result<()> {
    // Setup temp DB
    let db_path = format!("sqlite://test_buffer_{}.db?mode=rwc", uuid::Uuid::new_v4());
    // (In real test run we should clean this up, but for now unique name is fine)

    let buffer = SQLiteBuffer::new(&db_path).await?;
    let mock_client = MockMqttClient::new();
    let client_arc: Arc<dyn MqttPublisherClient> = Arc::new(mock_client.clone());

    let publisher =
        BufferedMqttPublisher::new(client_arc, buffer.clone(), "test-agent".to_string());

    // Scenario 1: Online
    // ------------------
    let tag1 = TagId::new("Tag1").unwrap();
    let event = DomainEvent::tag_value_updated(tag1, json!(10.0), TagQuality::Good);
    publisher.publish(event).await.map_err(|e| anyhow!(e))?;

    {
        let msgs = mock_client.published_messages.lock().unwrap();
        assert_eq!(msgs.len(), 1, "Should publish immediately when online");
    }

    // Scenario 2: Go Offline
    // ----------------------
    mock_client.connected.store(false, Ordering::Relaxed);

    let tag2 = TagId::new("Tag2").unwrap();
    let event_offline = DomainEvent::tag_value_updated(tag2, json!(20.0), TagQuality::Good);
    publisher
        .publish(event_offline)
        .await
        .map_err(|e| anyhow!(e))?;

    // Check it did NOT publish
    {
        let msgs = mock_client.published_messages.lock().unwrap();
        assert_eq!(msgs.len(), 1, "Should NOT publish when offline");
    }

    // Check it buffered
    let count = buffer.count().await?;
    assert_eq!(count, 1, "Should have 1 buffered event");

    // Scenario 3: Recovery
    // --------------------
    mock_client.connected.store(true, Ordering::Relaxed);

    // Wait for flusher (loops every 5s)
    // We need to wait > 5s
    println!("Waiting for flusher...");
    sleep(Duration::from_secs(7)).await;

    // Check buffer empty
    let count_after = buffer.count().await?;
    assert_eq!(count_after, 0, "Buffer should be empty after flush");

    // Check messages received
    {
        let msgs = mock_client.published_messages.lock().unwrap();
        assert_eq!(msgs.len(), 2, "Should have received buffered message");
        assert_eq!(msgs[1].0, "scada/data/test-agent");
        // We could decode payload to verify "val": 20.0
    }

    // Cleanup
    let _ = std::fs::remove_file(db_path.replace("sqlite://", "").replace("?mode=rwc", ""));

    Ok(())
}

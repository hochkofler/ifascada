use edge_agent::config_manager::ConfigManager;
use infrastructure::MqttClient;
use std::fs;
use std::time::Duration;
use tokio::time::timeout;
use uuid::Uuid;

#[tokio::test]
async fn test_config_subscriber_flow() {
    // 1. Setup
    let run_id = Uuid::new_v4().to_string();
    let agent_id = format!("agent-test-{}", &run_id[..8]);
    let config_dir = std::env::temp_dir().join(format!("scada_test_{}", run_id));
    fs::create_dir_all(&config_dir).unwrap();
    let config_path = config_dir.join("last_known.json");

    let mqtt_host = "localhost";
    let mqtt_port = 1883;

    // Agent Client (Simulates the Edge Agent)
    let agent_client_id = format!("agent-client-{}", run_id);
    let agent_client = MqttClient::new(mqtt_host, mqtt_port, &agent_client_id, None)
        .await
        .expect("Failed to create Agent MQTT client");

    // Mock Publisher
    struct MockEventPublisher;
    #[async_trait::async_trait]
    impl domain::event::EventPublisher for MockEventPublisher {
        async fn publish(
            &self,
            _event: domain::event::DomainEvent,
        ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            Ok(())
        }
    }
    use application::tag::ExecutorManager;
    use std::sync::Arc;

    let publisher = Arc::new(MockEventPublisher);
    let executor_manager = Arc::new(ExecutorManager::new(publisher, agent_id.clone()));
    let automation_engine = Arc::new(application::automation::AutomationEngine::default(vec![]));

    // Instantiate ConfigManager (Subject Under Test)
    let tag_repository = Arc::new(infrastructure::repositories::ConfigTagRepository::new(
        &agent_id,
        vec![],
    ));

    let manager = ConfigManager::new(
        agent_client,
        config_path.clone(),
        agent_id.clone(),
        executor_manager,
        automation_engine,
        tag_repository,
    );

    // Spawn manager
    // Initialize subscription
    let rx = manager.init().await.expect("Failed to init config manager");

    // Spawn manager loop
    tokio::spawn(async move {
        manager.run_loop(rx).await;
    });

    // Server Client (Simulates Central Server)
    let server_client_id = format!("server-client-{}", run_id);
    let server_client = MqttClient::new(mqtt_host, mqtt_port, &server_client_id, None)
        .await
        .expect("Failed to create Server MQTT client");

    // 2. Trigger: Publish Config
    let config_topic = format!("scada/config/{}", agent_id);
    let payload = serde_json::json!({
        "agent_id": agent_id,
        "mqtt": {
            "host": "localhost",
            "port": 1883,
            "status_topic": null
        },
        "tags": [
            {
                "id": "TAG_1",
                "driver": "RS232",
                "driver_config": {},
                "enabled": true
            }
        ]
    });

    // Wait for subscription to propagate (flaky but simple for now)
    tokio::time::sleep(Duration::from_millis(500)).await;

    server_client
        .publish(&config_topic, &payload.to_string(), false)
        .await
        .expect("Failed to publish config");

    // 3. Assert: File Created
    // Poll for file existence
    let file_created = timeout(Duration::from_secs(3), async {
        loop {
            if config_path.exists() {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await;

    assert!(
        file_created.is_ok(),
        "Timed out waiting for config file creation"
    );

    // Read content
    let content = fs::read_to_string(&config_path).unwrap();
    let json: serde_json::Value = serde_json::from_str(&content).unwrap();

    assert_eq!(json["agent_id"], agent_id);
    assert_eq!(json["tags"][0]["id"], "TAG_1");

    // Cleanup
    let _ = fs::remove_dir_all(config_dir);
}
#[tokio::test]
async fn test_config_deduplication() {
    use application::ExecutorManager;
    use application::automation::AutomationEngine;
    use infrastructure::repositories::ConfigTagRepository;
    use std::sync::Arc;

    // Setup (Similar to test_config_subscriber_flow)
    let agent_id = "test-agent-dedup";
    let mqtt_host = "localhost";
    let mqtt_port = 1883;

    let config_dir = std::path::PathBuf::from("tests/artifacts");
    fs::create_dir_all(&config_dir).unwrap();
    let config_path = config_dir.join("last_known_dedup.json");
    let _ = fs::remove_file(&config_path);

    let mqtt_client = MqttClient::new(mqtt_host, mqtt_port, "test-client-dedup", None)
        .await
        .expect("Failed to connect to broker");

    // Mock Managers
    // Need a mock publisher for ExecutorManager
    struct MockEventPublisher;
    #[async_trait::async_trait]
    impl domain::event::EventPublisher for MockEventPublisher {
        async fn publish(
            &self,
            _event: domain::event::DomainEvent,
        ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            Ok(())
        }
    }
    let publisher = Arc::new(MockEventPublisher);

    let executor_manager = Arc::new(ExecutorManager::new(publisher, agent_id.to_string()));
    let automation_engine = Arc::new(AutomationEngine::default(vec![]));

    // Use ConfigTagRepository as in the other test
    let tag_repo = Arc::new(ConfigTagRepository::new(agent_id, vec![]));

    let manager = ConfigManager::new(
        mqtt_client.clone(),
        config_path.clone(),
        agent_id.to_string(),
        executor_manager,
        automation_engine,
        tag_repo,
    );

    // Init
    let rx = manager.init().await.unwrap();
    tokio::spawn(async move {
        manager.run_loop(rx).await;
    });

    // Client to publish
    let pub_client = MqttClient::new("localhost", mqtt_port, "pub-client-dedup", None)
        .await
        .unwrap();

    // Connect wait
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Payload
    let payload = r#"{
        "agent_id": "test-agent-dedup",
        "mqtt": {"host": "localhost", "port": 1883, "status_topic": null},
        "tags": [],
        "heartbeat_interval_secs": 30
    }"#;

    // Publish 1st time
    pub_client
        .publish(&format!("scada/config/{}", agent_id), payload, false)
        .await
        .unwrap();

    // Wait for process
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Check file exists
    assert!(config_path.exists());
    let metadata_1 = fs::metadata(&config_path).unwrap();
    let modified_1 = metadata_1.modified().unwrap();

    // Publish 2nd time (Identical)
    pub_client
        .publish(&format!("scada/config/{}", agent_id), payload, false)
        .await
        .unwrap();

    // Wait
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Check file metadata - Should be SAME modification time if write was skipped
    // Wait, ConfigManager writes to file BEFORE reload?
    // In my code:
    // if *last_payload == msg.payload { continue; }
    // So if deduplication works, it skips EVERYTHING including file write.
    // So file mtime should NOT change.

    let metadata_2 = fs::metadata(&config_path).unwrap();
    let modified_2 = metadata_2.modified().unwrap();

    assert_eq!(
        modified_1, modified_2,
        "File should not be modified on duplicate config"
    );

    // Cleanup
    let _ = fs::remove_file(config_path);
}

use central_server::services::ConfigService;
use infrastructure::MqttClient;
use sqlx::PgPool;
use std::time::Duration;
use tokio::time::timeout;
use uuid::Uuid;

#[sqlx::test]
async fn test_config_sync_flow(pool: PgPool) -> sqlx::Result<()> {
    // 0. Run Migrations
    sqlx::migrate!("../../migrations")
        .run(&pool)
        .await
        .expect("Failed to run migrations");

    // 1. Setup Data
    let run_id = Uuid::new_v4().to_string();
    let agent_id = format!("agent-test-{}", &run_id[..8]);
    let tag_id = format!("TAG-{}", &run_id[..8]);

    // Insert Agent
    sqlx::query!(
        "INSERT INTO edge_agents (id, description, status, last_heartbeat) VALUES ($1, 'Test Agent', 'Offline', NOW())",
        agent_id
    )
    .execute(&pool)
    .await?;

    // Insert Tag
    sqlx::query!(
        r#"
        INSERT INTO tags (id, edge_agent_id, driver_type, driver_config, update_mode, update_config, value_type, enabled)
        VALUES ($1, $2, 'RS232', '{"port":"COM1"}', 'Polling', '{"interval_ms":1000}', 'Simple', true)
        "#,
        tag_id,
        agent_id
    )
    .execute(&pool)
    .await?;

    // 2. Setup MQTT Clients
    let mqtt_host = "localhost";
    let mqtt_port = 1883;

    // Service Client (Simulates the Central Server)
    let service_client_id = format!("central-test-{}", run_id);
    let service_client = MqttClient::new(mqtt_host, mqtt_port, &service_client_id, None)
        .await
        .expect("Failed to create Service MQTT client");

    // Instantiate Service (Subject Under Test)
    let service = ConfigService::new(pool.clone(), service_client);
    tokio::spawn(async move { service.start().await });

    // Agent Client (Simulates the Edge Agent)
    let agent_client_id = format!("agent-client-{}", run_id);
    let agent_client = MqttClient::new(mqtt_host, mqtt_port, &agent_client_id, None)
        .await
        .expect("Failed to create Agent MQTT client");

    // Subscribe to config updates
    let config_topic = format!("scada/config/{}", agent_id);
    agent_client
        .subscribe(&config_topic)
        .await
        .expect("Failed to subscribe");

    // 3. Trigger: Publish ONLINE status
    let status_topic = format!("scada/status/{}", agent_id);
    let payload = serde_json::json!({
        "status": "ONLINE",
        "config_hash": "empty"
    });

    agent_client
        .publish(&status_topic, &payload.to_string(), false)
        .await
        .expect("Failed to publish status");

    // 4. Assert: Receive Config
    let mut messages = agent_client.subscribe_messages();

    let received = timeout(Duration::from_secs(3), async {
        while let Ok(msg) = messages.recv().await {
            if msg.topic == config_topic {
                return Some(msg);
            }
        }
        None
    })
    .await;

    // Fails here because service.start() is a stub
    assert!(received.is_ok(), "Timed out waiting for config");

    // If it were to pass:
    let msg = received.unwrap().expect("No message received");
    let config_json: serde_json::Value = serde_json::from_slice(&msg.payload).unwrap();
    assert_eq!(config_json["agent_id"], agent_id);
    assert!(config_json["tags"].as_array().unwrap().len() > 0);
    assert_eq!(config_json["tags"][0]["id"], tag_id);

    Ok(())
}

#[sqlx::test]
async fn test_config_sync_case_insensitive(pool: PgPool) -> sqlx::Result<()> {
    // 0. Run Migrations
    sqlx::migrate!("../../migrations")
        .run(&pool)
        .await
        .expect("Failed to run migrations");

    // 1. Setup Data - Use Lowercase values
    let run_id = Uuid::new_v4().to_string();
    let agent_id = format!("agent-inc-{}", &run_id[..8]);
    let tag_id = format!("TAG-INC-{}", &run_id[..8]);

    // Insert Agent
    sqlx::query!(
        "INSERT INTO edge_agents (id, description, status, last_heartbeat) VALUES ($1, 'Test Agent', 'Offline', NOW())",
        agent_id
    )
    .execute(&pool)
    .await?;

    // Insert Tag with Lowercase 'polling' and 'rs232'
    sqlx::query!(
        r#"
        INSERT INTO tags (id, edge_agent_id, driver_type, driver_config, update_mode, update_config, value_type, enabled)
        VALUES ($1, $2, 'rs232', '{"port":"COM1"}', 'polling', '{"interval_ms":1000}', 'simple', true)
        "#,
        tag_id,
        agent_id
    )
    .execute(&pool)
    .await?;

    // 2. Setup Service
    let mqtt_host = "localhost";
    let mqtt_port = 1883;
    let service_client = MqttClient::new(mqtt_host, mqtt_port, &format!("svc-{}", run_id), None)
        .await
        .expect("MQTT Svc");

    // Config Service Logic Reuse (Avoids full async spawn for unit test speed if possible, but we need the DB query)
    // We can directly test the Repository instead of the full service to avoid MQTT dependency issues in test environment
    // But since we are here, let's use the repo directly which is what we changed.

    use infrastructure::repositories::DbConfigRepository;
    let repo = DbConfigRepository::new(pool.clone());

    let config = repo
        .get_agent_config(&agent_id)
        .await
        .expect("Failed to get config");

    // Assertions
    assert_eq!(config.agent_id, agent_id);
    assert_eq!(config.tags.len(), 1);
    let tag = &config.tags[0];
    assert_eq!(tag.id, tag_id);

    // Check Enum Mapping
    // 'rs232' -> DriverType::RS232
    assert!(matches!(tag.driver, domain::driver::DriverType::RS232));

    // 'polling' -> TagUpdateMode::Polling
    if let Some(domain::tag::TagUpdateMode::Polling { .. }) = tag.update_mode {
        // success
    } else {
        panic!("Expected Polling update mode, got {:?}", tag.update_mode);
    }

    Ok(())
}

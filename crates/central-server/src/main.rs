use anyhow::Result;
use clap::Parser;
use infrastructure::{MqttClient, MqttMessage};
use std::sync::Arc;
use tracing::{info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

// Use modules from the library
use central_server::{api, services, state};
use state::{AgentStatus, AppState, TagData};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// MQTT Broker Host
    #[arg(long, default_value = "localhost")]
    mqtt_host: String,

    /// MQTT Broker Port
    #[arg(long, default_value = "1883")]
    mqtt_port: u16,

    /// API Port
    #[arg(long, default_value = "3000")]
    api_port: u16,

    /// MQTT Client ID
    #[arg(long, default_value = "central-server")]
    mqtt_client_id: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            "info,central_server=debug",
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let args = Args::parse();
    info!("🏢 Central Server API Starting...");

    // 0. Connect to Database
    dotenv::dotenv().ok();
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    info!("Connecting to database...");
    let pool = sqlx::PgPool::connect(&database_url).await?;

    // 0.1 Run Migrations
    info!("Running database migrations...");
    sqlx::migrate!("../../migrations").run(&pool).await?;
    info!("✅ Migrations applied successfully");

    // 0.5 Initialize Local Buffer (Store & Forward)
    let buffer_path = "sqlite://central_buffer.db?mode=rwc";
    let buffer = infrastructure::database::SQLiteBuffer::new(buffer_path).await?;
    info!("✅ Local Buffer Initialized at {}", buffer_path);

    // 1. Initialize MQTT
    let mqtt_client_id = args.mqtt_client_id.clone();
    info!(host = %args.mqtt_host, port = %args.mqtt_port, client_id = %mqtt_client_id, "Connecting to MQTT...");

    let mqtt_client =
        MqttClient::new(&args.mqtt_host, args.mqtt_port, &mqtt_client_id, None).await?;
    mqtt_client.subscribe("scada/data/#").await?;
    mqtt_client.subscribe("scada/status/#").await?;
    mqtt_client.subscribe("scada/reports/#").await?;
    mqtt_client.subscribe("scada/health/#").await?;

    info!("✅ MQTT Connected & Subscribed");

    // 2. Initialize State
    let state = Arc::new(AppState::new(
        mqtt_client.clone(),
        pool.clone(),
        buffer.clone(),
    ));

    // 2.5 Initialize Config Service
    let config_service = services::ConfigService::new(pool.clone(), mqtt_client.clone());
    let config_service_arc = Arc::new(config_service);

    let cs_clone = config_service_arc.clone();
    tokio::spawn(async move {
        cs_clone.start().await;
    });

    // 3. Bridge MQTT -> State
    let mut rx = mqtt_client.subscribe_messages();
    let state_clone = state.clone();

    tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            process_mqtt_message(&state_clone, msg).await;
        }
    });

    // 3.1 Initial Sync from DB
    let s_load = state.clone();
    tokio::spawn(async move {
        // Reset all statuses to offline on startup (central app starting/restarting)
        if let Err(e) = s_load.reset_all_tag_statuses().await {
            warn!("Failed to reset tag statuses: {}", e);
        }

        if let Err(e) = s_load.load_agents_from_db().await {
            warn!("Failed to load agents from DB: {}", e);
        } else {
            info!("✅ Agents loaded from database");
        }

        if let Err(e) = s_load.load_tags_from_db().await {
            warn!("Failed to load tags from DB: {}", e);
        } else {
            info!("✅ Tags loaded from database");
        }
    });

    // 3.2 Start Liveness Monitor
    let s_liveness = state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(15));
        loop {
            interval.tick().await;
            s_liveness.check_agent_liveness();
        }
    });

    // 3.5 Start DB Flusher
    let pool_clone = pool.clone();
    let buffer_clone = buffer.clone();
    tokio::spawn(async move {
        start_db_flusher(pool_clone, buffer_clone).await;
    });

    // 4. Start API Server
    let app = api::create_router(state);
    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], args.api_port));
    info!("🚀 API Listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn start_db_flusher(pool: sqlx::PgPool, buffer: infrastructure::database::SQLiteBuffer) {
    info!("🔄 Starting DB Flusher...");
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

        match buffer.count().await {
            Ok(count) if count > 0 => {
                // Dequeue in batches
                match buffer.dequeue_batch(50).await {
                    Ok(rows) => {
                        if !rows.is_empty() {
                            info!("📤 Flushing {} buffered events to DB...", rows.len());
                            for (id, _topic, payload) in rows {
                                // Payload is the serialized TagData JSON
                                // We need to deserialize it to insert into DB
                                if let Ok(tag_data) = serde_json::from_slice::<TagData>(&payload) {
                                    let query = sqlx::query!(
                                        r#"
                                        INSERT INTO tag_events (tag_id, value, quality, timestamp)
                                        VALUES ($1, $2, $3, $4)
                                        "#,
                                        tag_data.id,
                                        tag_data.value,
                                        tag_data.quality,
                                        to_offset(tag_data.timestamp)
                                    );

                                    match query.execute(&pool).await {
                                        Ok(_) => {
                                            // Delete from buffer on success
                                            if let Err(e) = buffer.delete(id).await {
                                                warn!(
                                                    "Failed to delete buffered event {}: {}",
                                                    id, e
                                                );
                                            }
                                        }
                                        Err(e) => {
                                            warn!("DB Insert failed during flush: {}", e);
                                            // Stop flushing this batch, try again later
                                            break;
                                        }
                                    }
                                } else {
                                    warn!("Failed to deserialize buffered payload for id {}", id);
                                    // Should probably delete it to avoid stuck loop, or move to DLQ
                                    let _ = buffer.delete(id).await;
                                }
                            }
                        }
                    }
                    Err(e) => warn!("Failed to dequeue batch: {}", e),
                }
            }
            Ok(_) => {}
            Err(e) => warn!("Failed to check buffer count: {}", e),
        }
    }
}

// Helper to convert chrono::DateTime<Utc> to time::OffsetDateTime for sqlx
fn to_offset(dt: chrono::DateTime<chrono::Utc>) -> time::OffsetDateTime {
    let timestamp = dt.timestamp();
    let nanos = dt.timestamp_subsec_nanos();
    time::OffsetDateTime::from_unix_timestamp_nanos(
        (timestamp as i128) * 1_000_000_000 + (nanos as i128),
    )
    .unwrap()
}

async fn process_mqtt_message(state: &AppState, msg: MqttMessage) {
    let topic = msg.topic.clone();
    let pkid = msg.pkid;

    if topic.starts_with("scada/status/") {
        // e.g. scada/status/agent-1
        let agent_id = topic.trim_start_matches("scada/status/").to_string();
        let payload_str = String::from_utf8_lossy(&msg.payload);

        let mut status = match payload_str.as_ref() {
            "ONLINE" => AgentStatus::Online,
            "OFFLINE" => AgentStatus::Offline,
            _ => AgentStatus::Unknown,
        };

        // If it was unknown, try parsing as JSON (Edge Agent format)
        if matches!(status, AgentStatus::Unknown) {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&payload_str) {
                if let Some(s) = json.get("status").and_then(|v| v.as_str()) {
                    status = match s {
                        "ONLINE" => AgentStatus::Online,
                        "OFFLINE" => AgentStatus::Offline,
                        _ => AgentStatus::Unknown,
                    };
                }
            }
        }

        // info!(agent_id = %agent_id, status = ?status, "Agent Status Change"); // Removed redundant log
        state.update_agent_status(agent_id, status);

        // Status messages are critical but transient. We Ack them immediately after updating memory.
        // TODO: Persist status to DB if needed.
        if let Err(e) = state.mqtt_client.ack(&topic, pkid).await {
            warn!("Failed to Ack status message: {}", e);
        }
    } else if topic.starts_with("scada/data/") {
        // e.g. scada/data/agent-1
        let agent_id = topic.trim_start_matches("scada/data/").to_string();

        if let Ok(tags) = serde_json::from_slice::<Vec<serde_json::Value>>(&msg.payload) {
            // Start Transaction for Atomicity regarding this Packet
            let mut tx = match state.pool.begin().await {
                Ok(tx) => tx,
                Err(e) => {
                    warn!(
                        "Failed to start transaction: {}. Packet {} will be retried.",
                        e, pkid
                    );
                    return; // Do not Ack -> Broker Retry
                }
            };

            let mut any_error = false;

            for tag_json in tags {
                if let (Some(tag_id), Some(val), Some(q), Some(ts)) = (
                    tag_json.get("tag_id").and_then(|v| v.as_str()),
                    tag_json.get("val"),
                    tag_json.get("q").and_then(|v| v.as_str()),
                    tag_json.get("ts").and_then(|v| v.as_i64()),
                ) {
                    let timestamp =
                        chrono::DateTime::from_timestamp_millis(ts).unwrap_or(chrono::Utc::now());

                    // Update Memory (DashMap)
                    // Note: Memory update happens even if DB fails. Is this okay?
                    // Yes, for monitoring it's better to see live data even if DB is struggling.
                    let tag_data = TagData {
                        id: tag_id.to_string(),
                        agent_id: agent_id.clone(),
                        value: val.clone(),
                        quality: q.to_string(),
                        status: "online".to_string(),
                        timestamp,
                        received_at: None,
                    };
                    state.update_tag(tag_data.clone());

                    // Prepare DB Insert (within Transaction)
                    let timestamp_db = to_offset(tag_data.timestamp);
                    let val_db = val.clone(); // jsonb

                    // Attempt 1: Standard Insert (Assumes tag exists in FK)
                    let query = sqlx::query!(
                        r#"
                        INSERT INTO tag_events (tag_id, value, quality, timestamp)
                        VALUES ($1, $2, $3, $4)
                        "#,
                        tag_id,
                        val_db,
                        q,
                        timestamp_db
                    );

                    // Create a SAVEPOINT to allow recovery from the FK violation within the transaction
                    if let Err(e) = sqlx::query!("SAVEPOINT sp_insert_tag")
                        .execute(&mut *tx)
                        .await
                    {
                        warn!("Failed to create savepoint: {}", e);
                        any_error = true;
                        break;
                    }

                    if let Err(e) = query.execute(&mut *tx).await {
                        // Check for FK violation (Postgres SQLSTATE 23503)
                        let is_fk_violation = if let sqlx::Error::Database(db_err) = &e {
                            db_err.code().as_deref() == Some("23503")
                        } else {
                            false
                        };

                        if is_fk_violation {
                            warn!(tag_id = %tag_id, "Tag not registered. Rolling back to Savepoint and inserting as unregistered.");

                            // ROLLBACK to the savepoint to clear the error state
                            if let Err(e_rb) = sqlx::query!("ROLLBACK TO SAVEPOINT sp_insert_tag")
                                .execute(&mut *tx)
                                .await
                            {
                                warn!("Failed to rollback to savepoint: {}", e_rb);
                                any_error = true;
                                break;
                            }

                            // Attempt 2: Fallback Insert (unregistered tag – NULL FK)
                            let query_fallback = sqlx::query!(
                                r#"
                                INSERT INTO tag_events (tag_id, value, quality, timestamp)
                                VALUES (NULL, $1, $2, $3)
                                "#,
                                val_db,
                                q,
                                timestamp_db
                            );

                            if let Err(e_fallback) = query_fallback.execute(&mut *tx).await {
                                warn!(tag_id = %tag_id, "Fallback DB Insert Error: {}", e_fallback);
                                any_error = true;
                                break;
                            }
                        } else {
                            warn!(tag_id = %tag_id, "DB Insert Error: {}", e);
                            any_error = true;
                            break;
                        }
                    } else {
                        // Release savepoint on success (optional but good practice)
                        let _ = sqlx::query!("RELEASE SAVEPOINT sp_insert_tag")
                            .execute(&mut *tx)
                            .await;
                    }
                }
            }

            if !any_error {
                match tx.commit().await {
                    Ok(_) => {
                        // Success! Ack the message.
                        if let Err(e) = state.mqtt_client.ack(&topic, pkid).await {
                            warn!("Failed to Ack data packet {}: {}", pkid, e);
                            // If Ack fails, broker will redeliver.
                            // Since we have no unique constraint on events, this causes duplicates.
                            // Risk accepted for now.
                        } else {
                            // trace!("Acked packet {}", pkid);
                        }
                    }
                    Err(e) => {
                        warn!(
                            "Transaction Commit Failed: {}. Packet {} will be retried.",
                            e, pkid
                        );
                        // Do not Ack
                    }
                }
            } else {
                warn!(
                    "Packet {} contained DB errors (e.g. FK violation). Rolling back and NOT Acking.",
                    pkid
                );
                let _ = tx.rollback().await;
                // Do not Ack -> Broker ensures retention and retry
            }
        } else {
            warn!(topic = %topic, "Failed to parse telemetry JSON");
            // If we can't parse it, retrying likely won't help unless code changes.
            // Ack it to discard bad data? Or Send to DLQ?
            // For now, Ack to clear queue.
            let _ = state.mqtt_client.ack(&topic, pkid).await;
        }
    } else if topic.starts_with("scada/reports/") {
        process_report_message(state, msg).await;
    } else if topic.starts_with("scada/health/") {
        let agent_id = topic.trim_start_matches("scada/health/").to_string();
        if let Ok(payload) = serde_json::from_slice::<serde_json::Value>(&msg.payload) {
            state.update_agent_heartbeat(agent_id, payload);
            let _ = state.mqtt_client.ack(&topic, pkid).await;
        }
    }
}

async fn process_report_message(state: &AppState, msg: MqttMessage) {
    let topic = msg.topic.clone();
    let pkid = msg.pkid;
    let agent_id = topic.trim_start_matches("scada/reports/").to_string();

    if let Ok(report) = serde_json::from_slice::<state::ReportData>(&msg.payload) {
        let mut report = report;
        report.agent_id = agent_id.clone();

        info!(
            report_id = %report.report_id,
            agent_id = %agent_id,
            items = %report.items.len(),
            "📄 Report Received! Persisting..."
        );

        // 1. Persist to Postgres
        let mut tx = match state.pool.begin().await {
            Ok(tx) => tx,
            Err(e) => {
                warn!("Failed to start transaction for report: {}", e);
                return;
            }
        };

        // Calculate metadata
        let start_time = report
            .items
            .first()
            .map(|i| i.timestamp)
            .unwrap_or(report.timestamp);
        let end_time = report
            .items
            .last()
            .map(|i| i.timestamp)
            .unwrap_or(report.timestamp);

        let total_value: f64 = report
            .items
            .iter()
            .map(|i| match &i.value {
                serde_json::Value::Number(n) => n.as_f64().unwrap_or(0.0),
                serde_json::Value::Object(map) => {
                    map.get("value").and_then(|v| v.as_f64()).unwrap_or(0.0)
                }
                _ => 0.0,
            })
            .sum();

        // 1.1 Insert Report Summary
        // We use DO NOTHING; if report_id is already stored we skip.
        let res = sqlx::query!(
            r#"
            INSERT INTO reports (id, report_id, agent_id, start_time, end_time, total_value)
            VALUES (gen_random_uuid(), $1, $2, $3, $4, $5)
            RETURNING id
            "#,
            report.report_id,
            agent_id,
            to_offset(start_time),
            to_offset(end_time),
            serde_json::json!(total_value)
        )
        .fetch_optional(&mut *tx)
        .await;

        match res {
            Ok(Some(row)) => {
                let db_report_id = row.id;

                // 1.2 Insert Report Items (no tag_id FK – ReportItem has value, timestamp, metadata)
                for item in report.items.iter() {
                    let _ = sqlx::query!(
                        r#"
                        INSERT INTO report_items (id, report_id, tag_id, value, timestamp)
                        VALUES (gen_random_uuid(), $1, NULL, $2, $3)
                        "#,
                        db_report_id,
                        item.value,
                        to_offset(item.timestamp)
                    )
                    .execute(&mut *tx)
                    .await;
                }

                if let Err(e) = tx.commit().await {
                    warn!("Failed to commit report: {}", e);
                    return;
                }

                info!(report_id = %report.report_id, "✅ Report persisted and committed");

                // 2. Broadcast via SSE
                let _ = state.tx.send(state::SystemEvent::ReportCompleted(report));

                // 3. Ack MQTT
                let _ = state.mqtt_client.ack(&topic, pkid).await;
            }
            Ok(None) => {
                info!(report_id = %report.report_id, "⚠️ Report already exists, skipped insertion but acking MQTT");
                let _ = state.mqtt_client.ack(&topic, pkid).await;
                let _ = tx.rollback().await;
            }
            Err(e) => {
                warn!("Failed to insert report summary: {}", e);
                let _ = tx.rollback().await;
            }
        }
    } else {
        warn!(topic = %topic, "Failed to parse report JSON");
        let _ = state.mqtt_client.ack(&topic, pkid).await;
    }
}

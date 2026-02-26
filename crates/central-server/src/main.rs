use anyhow::Result;
use central_server::api::{ApiState, EdgeConfigSettings, connect_read_client, run_api_server};
use central_server::ingestion::IngestionService;
use central_server::mqtt_consumer::{MqttConsumerConfig, run_mqtt_consumer};
use central_server::persistence::cached::CachedCentralPersistence;
use central_server::persistence::postgres::PostgresCentralPersistence;
use central_server::realtime_cache::{CentralRealtimeCache, NoopRealtimeCache, RedisCentralRealtimeCache};
use std::sync::Arc;
use tokio_postgres::NoTls;
use tracing::info;
use rumqttc::{AsyncClient, Event, MqttOptions};

fn default_ingest_topic_filters() -> Vec<String> {
    vec![
        "scada/+/edge/+/telemetry/tag/+".to_string(),
        "scada/+/edge/+/cmd/action/result".to_string(),
        "scada/+/edge/+/cmd/write/ack".to_string(),
        "scada/+/edge/+/audit/action".to_string(),
        "scada/+/edge/+/audit/write".to_string(),
        "scada/+/edge/+/health/runtime".to_string(),
        "scada/+/edge/+/alerts/runtime".to_string(),
        "scada/+/edge/+/alerts/runtime/ack".to_string(),
        "scada/+/edge/+/alerts/runtime/ack/result".to_string(),
        "scada/+/edge/+/config/apply/result".to_string(),
        "scada/+/edge/+/control/reset/result".to_string(),
        "scada/+/edge/+/conn/state".to_string(),
        "scada/+/edge/+/device/conn/state".to_string(),
    ]
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG")
                .unwrap_or_else(|_| "central_server=info,central-server=info,info".to_string()),
        )
        .init();

    let mqtt_enabled = std::env::var("CENTRAL_MQTT_ENABLED")
        .unwrap_or_else(|_| "true".to_string())
        .eq_ignore_ascii_case("true");
    let api_enabled = std::env::var("CENTRAL_API_ENABLED")
        .unwrap_or_else(|_| "true".to_string())
        .eq_ignore_ascii_case("true");
    if !mqtt_enabled && !api_enabled {
        info!("CENTRAL_MQTT_ENABLED=false and CENTRAL_API_ENABLED=false, exiting");
        return Ok(());
    }

    let dsn = std::env::var("CENTRAL_PG_DSN")
        .unwrap_or_else(|_| "host=127.0.0.1 user=postgres password=postgres dbname=ifascada".to_string());
    let pg = PostgresCentralPersistence::connect(&dsn).await?;
    let api_client = connect_read_client(&dsn).await?;

    let redis_enabled = std::env::var("CENTRAL_REDIS_ENABLED")
        .unwrap_or_else(|_| "false".to_string())
        .eq_ignore_ascii_case("true");
    let cache: Box<dyn CentralRealtimeCache> = if redis_enabled {
        let redis_url = std::env::var("CENTRAL_REDIS_URL")
            .unwrap_or_else(|_| "redis://127.0.0.1:6379/".to_string());
        let event_channel = std::env::var("CENTRAL_REDIS_EVENT_CHANNEL")
            .unwrap_or_else(|_| "scada:rt:events".to_string());
        let key_ttl_secs = std::env::var("CENTRAL_REDIS_KEY_TTL_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(300);
        Box::new(RedisCentralRealtimeCache::connect(
            &redis_url,
            event_channel,
            key_ttl_secs,
        )?)
    } else {
        Box::new(NoopRealtimeCache)
    };
    let persistence = CachedCentralPersistence::new(pg, cache);
    let ingestion = IngestionService::new(persistence);
    let topic_filters = std::env::var("CENTRAL_MQTT_TOPIC_FILTERS")
        .ok()
        .map(|raw| {
            raw.split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect::<Vec<String>>()
        })
        .filter(|v| !v.is_empty())
        .unwrap_or_else(default_ingest_topic_filters);

    let mqtt_cfg = MqttConsumerConfig {
        host: std::env::var("MQTT_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
        port: std::env::var("MQTT_PORT")
            .ok()
            .and_then(|v| v.parse::<u16>().ok())
            .unwrap_or(1883),
        client_id: std::env::var("CENTRAL_MQTT_CLIENT_ID")
            .unwrap_or_else(|_| "central-server-01".to_string()),
        topic_filters,
        clean_session: std::env::var("CENTRAL_MQTT_CLEAN_SESSION")
            .ok()
            .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
            .unwrap_or(false),
        manual_acks: std::env::var("CENTRAL_MQTT_MANUAL_ACKS")
            .ok()
            .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
            .unwrap_or(true),
    };
    let mqtt_cmd = if mqtt_enabled {
        let mut cmd_opts = MqttOptions::new(
            std::env::var("CENTRAL_MQTT_CMD_CLIENT_ID")
                .unwrap_or_else(|_| "central-server-cmd-01".to_string()),
            mqtt_cfg.host.clone(),
            mqtt_cfg.port,
        );
        cmd_opts.set_keep_alive(tokio::time::Duration::from_secs(20));
        let (cmd_client, mut cmd_eventloop) = AsyncClient::new(cmd_opts, 100);
        tokio::spawn(async move {
            loop {
                match cmd_eventloop.poll().await {
                    Ok(Event::Incoming(_)) | Ok(Event::Outgoing(_)) => {}
                    Err(e) => {
                        tracing::warn!("central mqtt command publisher event loop error: {}", e);
                        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                    }
                }
            }
        });
        Some(Arc::new(cmd_client))
    } else {
        None
    };
    let api_bind = std::env::var("CENTRAL_API_BIND").unwrap_or_else(|_| "127.0.0.1:8088".to_string());
    let api_state = ApiState {
        client: Arc::new(api_client),
        edge_cfg: EdgeConfigSettings {
            enroll_token: std::env::var("CENTRAL_EDGE_ENROLL_TOKEN")
                .unwrap_or_else(|_| "dev-edge-enroll-token".to_string()),
            signing_secret: std::env::var("CENTRAL_EDGE_CONFIG_SIGNING_SECRET")
                .unwrap_or_else(|_| "dev-edge-config-signing-secret".to_string()),
            signing_key_id: std::env::var("CENTRAL_EDGE_CONFIG_SIGNING_KEY_ID")
                .unwrap_or_else(|_| "v1".to_string()),
            runtime_config_path: std::env::var("CENTRAL_EDGE_RUNTIME_CONFIG_PATH")
                .unwrap_or_else(|_| "crates/edge-agent/config/bootstrap.example.json".to_string()),
        },
        mqtt_cmd,
    };

    let ops_retention_days = std::env::var("CENTRAL_OPS_EVENTS_RETENTION_DAYS")
        .ok()
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(90)
        .max(1);
    let ops_cleanup_interval_secs = std::env::var("CENTRAL_OPS_EVENTS_CLEANUP_INTERVAL_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(3600)
        .max(60);
    let ops_cleanup_enabled = std::env::var("CENTRAL_OPS_EVENTS_CLEANUP_ENABLED")
        .ok()
        .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
        .unwrap_or(true);
    if ops_cleanup_enabled {
        let dsn_cleanup = dsn.clone();
        tokio::spawn(async move {
            let (client, connection) = match tokio_postgres::connect(&dsn_cleanup, NoTls).await {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!("ops events cleanup disabled: failed to connect pg: {}", e);
                    return;
                }
            };
            tokio::spawn(async move {
                if let Err(e) = connection.await {
                    tracing::warn!("ops events cleanup pg connection task error: {}", e);
                }
            });
            loop {
                match client
                    .execute(
                        "DELETE FROM operational_events
                         WHERE ts < NOW() - ($1::text || ' days')::interval",
                        &[&ops_retention_days.to_string()],
                    )
                    .await
                {
                    Ok(n) => {
                        tracing::debug!(
                            "ops events cleanup ran: deleted {} rows older than {} days",
                            n,
                            ops_retention_days
                        );
                    }
                    Err(e) => {
                        tracing::warn!("ops events cleanup failed: {}", e);
                    }
                }
                tokio::time::sleep(tokio::time::Duration::from_secs(ops_cleanup_interval_secs)).await;
            }
        });
    }

    match (mqtt_enabled, api_enabled) {
        (true, true) => {
            tokio::try_join!(
                run_mqtt_consumer(mqtt_cfg, ingestion),
                run_api_server(api_state, &api_bind)
            )?;
        }
        (true, false) => {
            run_mqtt_consumer(mqtt_cfg, ingestion).await?;
        }
        (false, true) => {
            run_api_server(api_state, &api_bind).await?;
        }
        (false, false) => {}
    }
    Ok(())
}

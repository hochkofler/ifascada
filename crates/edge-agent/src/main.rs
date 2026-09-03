use application::runtime::{DriverRegistry, RuntimeEngine};
use domain::{ActionSpec, AutomationSpec, DriverType, ExecutionScope, NumericOperator, TriggerSpec};
use infrastructure::drivers::modbus_rtu::ModbusRtuFactory;
use infrastructure::drivers::modbus_tcp::ModbusTcpFactory;
use infrastructure::drivers::serial_ascii::SerialAsciiFactory;
use infrastructure::drivers::simulator::SimulatorFactory;
mod broker_watch;
mod ticket_sequence;
use mqtt_bridge::{MqttBridgeConfig, MqttBridgeExit, run_mqtt_bridge};
use std::sync::Arc;
use tracing::{info, warn};

mod bootstrap;
mod action_orchestrator;
mod mqtt_bridge;
mod mqtt_outbox;

fn runtime_is_prod() -> bool {
    std::env::var("EDGE_RUNTIME_ENV")
        .or_else(|_| std::env::var("CENTRAL_RUNTIME_ENV"))
        .unwrap_or_else(|_| "dev".to_string())
        .eq_ignore_ascii_case("prod")
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "edge_agent=info,info".to_string()))
        .init();
    info!(
        "Starting Clean Slate Edge Agent version={} git_sha={}",
        env!("CARGO_PKG_VERSION"),
        env!("EDGE_AGENT_GIT_SHA")
    );

    let mut registry = DriverRegistry::new();
    registry.register(DriverType::new("Simulator")?, Box::new(SimulatorFactory));
    registry.register(DriverType::new("ModbusRTU")?, Box::new(ModbusRtuFactory));
    registry.register(DriverType::new("ModbusTCP")?, Box::new(ModbusTcpFactory));
    registry.register(DriverType::new("SerialAscii")?, Box::new(SerialAsciiFactory));

    let mut engine = RuntimeEngine::new(registry);
    let mut runtime_automations: Vec<AutomationSpec> = Vec::new();
    let remote_config_enabled = std::env::var("EDGE_CONFIG_URL")
        .ok()
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false);
    if runtime_is_prod() && remote_config_enabled {
        let has_bearer = std::env::var("EDGE_CONFIG_BEARER_TOKEN")
            .ok()
            .map(|v| !v.trim().is_empty())
            .unwrap_or(false);
        let has_login_pair = std::env::var("EDGE_CONFIG_API_USER")
            .ok()
            .map(|v| !v.trim().is_empty())
            .unwrap_or(false)
            && std::env::var("EDGE_CONFIG_API_PASSWORD")
                .ok()
                .map(|v| !v.trim().is_empty())
                .unwrap_or(false);
        if !has_bearer && !has_login_pair {
            return Err(anyhow::anyhow!(
                "EDGE_CONFIG_BEARER_TOKEN or EDGE_CONFIG_API_USER/EDGE_CONFIG_API_PASSWORD are required in prod when EDGE_CONFIG_URL is enabled"
            ));
        }
        let has_enroll_token = std::env::var("EDGE_ENROLL_TOKEN")
            .ok()
            .map(|v| !v.trim().is_empty())
            .unwrap_or(false);
        if !has_enroll_token {
            return Err(anyhow::anyhow!(
                "EDGE_ENROLL_TOKEN is required in prod when EDGE_CONFIG_URL is enabled"
            ));
        }
    }

    if let Some(loaded) = bootstrap::load_remote_or_cached_and_start(&mut engine).await? {
        runtime_automations = loaded.automations;
        info!(
            "Runtime Engine initialized from central signed config/cache, started {} connection(s), loaded {} automation(s)",
            loaded.started_connections,
            runtime_automations.len()
        );
    } else if !remote_config_enabled {
        if let Some(path) = bootstrap::resolve_bootstrap_path() {
        let loaded = bootstrap::load_and_start(&mut engine, &path).await?;
        runtime_automations = loaded.automations;
        info!(
            "Runtime Engine initialized with bootstrap '{}', started {} connection(s), loaded {} automation(s)",
            path, loaded.started_connections, runtime_automations.len()
        );
        } else {
            info!("Runtime Engine initialized (no bootstrap file found)");
        }
    } else {
        info!(
            "Runtime Engine initialized (EDGE_CONFIG_URL enabled but no remote config/cache available)"
        );
    }

    let mqtt_enabled = std::env::var("EDGE_MQTT_ENABLED")
        .ok()
        .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
        .unwrap_or(false);
    info!("EDGE_MQTT_ENABLED={}", mqtt_enabled);

    if mqtt_enabled {
        let current_config_hash = bootstrap::resolve_cached_runtime_config_hash();
        let auto_print_non_positive_enabled = std::env::var("EDGE_AUTO_PRINT_NONPOS_ENABLED")
            .ok()
            .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
            .unwrap_or(false);
        let auto_print_tag_ids = std::env::var("EDGE_AUTO_PRINT_TAGS")
            .ok()
            .map(|s| {
                s.split(',')
                    .map(str::trim)
                    .filter(|v| !v.is_empty())
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let auto_print_consecutive = std::env::var("EDGE_AUTO_PRINT_CONSECUTIVE")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(2);

        let mut effective_automations = runtime_automations.clone();
        effective_automations.extend(build_env_auto_print_automations(
            auto_print_non_positive_enabled,
            &auto_print_tag_ids,
            auto_print_consecutive,
        ));

        let cfg = MqttBridgeConfig {
            site: std::env::var("EDGE_SITE").unwrap_or_else(|_| "default-site".to_string()),
            agent: std::env::var("EDGE_AGENT").unwrap_or_else(|_| "edge-01".to_string()),
            broker_host: std::env::var("MQTT_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
            broker_port: std::env::var("MQTT_PORT")
                .ok()
                .and_then(|s| s.parse::<u16>().ok())
                .unwrap_or(1883),
            client_id: std::env::var("MQTT_CLIENT_ID")
                .unwrap_or_else(|_| "edge-agent-01".to_string()),
            outbox_path: std::env::var("MQTT_OUTBOX_PATH")
                .unwrap_or_else(|_| "./data/mqtt_outbox.db".to_string()),
            ticket_sequence_path: std::env::var("EDGE_TICKET_SEQUENCE_PATH")
                .unwrap_or_else(|_| "./data/ticket_sequence.db".to_string()),
            outbox_flush_batch: std::env::var("MQTT_OUTBOX_FLUSH_BATCH")
                .ok()
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(50),
            outbox_max_messages: std::env::var("MQTT_OUTBOX_MAX_MESSAGES")
                .ok()
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(10_000),
            outbox_active_key_id: std::env::var("MQTT_OUTBOX_ACTIVE_KEY_ID")
                .unwrap_or_else(|_| "v1".to_string()),
            outbox_prev_key_id: std::env::var("MQTT_OUTBOX_PREV_KEY_ID").ok(),
            outbox_encryption_secret: std::env::var("MQTT_OUTBOX_ENCRYPTION_SECRET").ok(),
            outbox_hmac_secret: std::env::var("MQTT_OUTBOX_HMAC_SECRET").ok(),
            outbox_prev_encryption_secret: std::env::var("MQTT_OUTBOX_PREV_ENCRYPTION_SECRET").ok(),
            outbox_prev_hmac_secret: std::env::var("MQTT_OUTBOX_PREV_HMAC_SECRET").ok(),
            health_publish_interval_secs: std::env::var("MQTT_HEALTH_PUBLISH_INTERVAL_SECS")
                .ok()
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(30),
            health_outbox_depth_warn: std::env::var("MQTT_HEALTH_OUTBOX_DEPTH_WARN")
                .ok()
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(1000),
            health_outbox_oldest_secs_warn: std::env::var("MQTT_HEALTH_OUTBOX_OLDEST_SECS_WARN")
                .ok()
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(300),
            alert_degraded_streak: std::env::var("MQTT_ALERT_DEGRADED_STREAK")
                .ok()
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(3),
            alert_recovered_streak: std::env::var("MQTT_ALERT_RECOVERED_STREAK")
                .ok()
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(3),
            alert_dedup_window_secs: std::env::var("MQTT_ALERT_DEDUP_WINDOW_SECS")
                .ok()
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(300),
            config_hash: current_config_hash,
            config_check_url: std::env::var("EDGE_CONFIG_URL").ok(),
            config_check_enroll_token: std::env::var("EDGE_ENROLL_TOKEN").ok(),
            config_check_hmac_secret: match std::env::var("EDGE_CONFIG_HMAC_SECRET") {
                Ok(v) if !v.trim().is_empty() => Some(v),
                _ => {
                    let remote_enabled = std::env::var("EDGE_CONFIG_URL")
                        .ok()
                        .map(|v| !v.trim().is_empty())
                        .unwrap_or(false);
                    if runtime_is_prod() && remote_enabled {
                        return Err(anyhow::anyhow!(
                            "EDGE_CONFIG_HMAC_SECRET is required in prod when EDGE_CONFIG_URL is enabled"
                        ));
                    }
                    None
                }
            },
            config_check_key_id: std::env::var("EDGE_CONFIG_KEY_ID").ok(),
            config_cache_path: std::env::var("EDGE_RUNTIME_CACHE_PATH")
                .unwrap_or_else(|_| "./data/runtime_config.signed.json".to_string()),
            config_apply_receipt_path: std::env::var("EDGE_CONFIG_APPLY_RECEIPT_PATH")
                .unwrap_or_else(|_| "./data/config_apply_receipt.json".to_string()),
            config_check_interval_secs: std::env::var("EDGE_CONFIG_CHECK_INTERVAL_SECS")
                .ok()
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(120),
            config_check_jitter_secs: std::env::var("EDGE_CONFIG_CHECK_JITTER_SECS")
                .ok()
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(20),
            escpos_output_path: std::env::var("EDGE_ESCPOS_OUTPUT_PATH")
                .unwrap_or_else(|_| "./data/escpos_output.bin".to_string()),
            escpos_tcp_host: std::env::var("EDGE_ESCPOS_TCP_HOST").ok(),
            escpos_tcp_port: std::env::var("EDGE_ESCPOS_TCP_PORT")
                .ok()
                .and_then(|s| s.parse::<u16>().ok())
                .unwrap_or(9100),
            escpos_windows_share: std::env::var("EDGE_ESCPOS_WINDOWS_SHARE").ok(),
            on_demand_tcp_host: std::env::var("EDGE_ON_DEMAND_TCP_HOST").ok(),
            on_demand_tcp_port: std::env::var("EDGE_ON_DEMAND_TCP_PORT")
                .ok()
                .and_then(|s| s.parse::<u16>().ok()),
            on_demand_probe_enabled: std::env::var("EDGE_ON_DEMAND_PROBE_ENABLED")
                .ok()
                .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
                .unwrap_or(false),
            on_demand_probe_timeout_ms: std::env::var("EDGE_ON_DEMAND_PROBE_TIMEOUT_MS")
                .ok()
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(1200),
            on_demand_probe_connection_id: std::env::var("EDGE_ON_DEMAND_PROBE_CONNECTION_ID").ok(),
            on_demand_probe_device_id: std::env::var("EDGE_ON_DEMAND_PROBE_DEVICE_ID").ok(),
            automations: effective_automations,
        };

        let engine = Arc::new(engine);
        let mut backoff_secs = 1u64;
        loop {
            match run_mqtt_bridge(cfg.clone(), engine.clone()).await {
                Ok(MqttBridgeExit::RestartRequested) => {
                    info!("Graceful stop requested by config apply; stopping runtime engine");
                    let mut owned = Arc::try_unwrap(engine)
                        .map_err(|_| anyhow::anyhow!("cannot acquire exclusive runtime engine for graceful stop"))?;
                    owned.stop_all().await;
                    info!("Runtime engine stopped; exiting for supervisor restart");
                    break;
                }
                Err(e) => {
                    warn!(
                        "mqtt bridge stopped with error: {}; reconnect in {}s",
                        e, backoff_secs
                    );
                    tokio::time::sleep(tokio::time::Duration::from_secs(backoff_secs)).await;
                    backoff_secs = (backoff_secs * 2).min(30);
                }
            }
        }
    } else {
        // Keep active for a bit then shut down for demonstration
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        engine.stop_all().await;
        info!("Edge Agent stopped");
    }

    Ok(())
}

fn build_env_auto_print_automations(
    enabled: bool,
    tag_ids: &[String],
    consecutive: usize,
) -> Vec<AutomationSpec> {
    if !enabled {
        return Vec::new();
    }
    let ids: Vec<String> = if tag_ids.is_empty() {
        vec!["*".to_string()]
    } else {
        tag_ids.to_vec()
    };

    ids.into_iter()
        .map(|tag_id| AutomationSpec {
            id: format!("env-auto-non-positive-{}", tag_id.replace('*', "any")),
            name: Some("env_auto_non_positive_print".to_string()),
            enabled: true,
            trigger: TriggerSpec::ConsecutiveNumeric {
                tag_id: tag_id.clone(),
                threshold: 0.0,
                count: consecutive.max(1),
                operator: NumericOperator::Lte,
                within_ms: None,
            },
            action: Some(ActionSpec {
                action_type: "print.escpos".to_string(),
                target: Some("edge".to_string()),
                scope: Some(ExecutionScope::Edge),
                payload: serde_json::json!({
                    "lines": [
                        "AUTO TRIGGER",
                        format!("TAG: {}", tag_id),
                        "RULE: consecutive_non_positive"
                    ]
                }),
            }),
            actions: vec![],
        })
        .collect()
}

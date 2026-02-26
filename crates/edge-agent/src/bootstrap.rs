use anyhow::{Context, Result};
use application::runtime::RuntimeEngine;
use domain::connection::{Connection, ReconnectStrategy, ReconnectionPolicy};
use domain::id::{ConnectionId, DeviceId, TagId};
use domain::tag::{Tag, TagUpdateMode, TagValueType};
use domain::{AutomationSpec, DriverType};
use hmac::{Hmac, Mac};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use std::fs;
use std::path::Path;
use sha2::{Digest, Sha256};
use tracing::warn;

#[derive(Debug, Deserialize)]
pub struct BootstrapConfig {
    pub connections: Vec<BootstrapConnection>,
    #[serde(default)]
    pub automations: Vec<Value>,
}

#[derive(Debug, Deserialize)]
pub struct BootstrapConnection {
    pub id: String,
    pub name: String,
    pub driver_type: String,
    #[serde(default)]
    pub transport: Value,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    #[serde(default)]
    pub reconnect_delay_ms: Option<u64>,
    #[serde(default)]
    pub max_retries: Option<u32>,
    #[serde(default)]
    pub tags: Vec<BootstrapTag>,
}

#[derive(Debug, Deserialize)]
pub struct BootstrapTag {
    pub id: String,
    pub name: String,
    pub device_id: String,
    pub source: String,
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub value_type: Option<String>,
    #[serde(default)]
    pub update_mode: Option<String>,
    #[serde(default)]
    pub interval_ms: Option<u64>,
    #[serde(default)]
    pub metadata_json: Value,
}

#[derive(Debug, Deserialize)]
struct EdgeConfigCheckResponse {
    accepted: bool,
    config_changed: bool,
    target_config_hash: String,
    poll_after_secs: u64,
}

#[derive(Debug, Clone, Default)]
pub struct LoadedRuntimeConfig {
    pub started_connections: usize,
    pub automations: Vec<AutomationSpec>,
}

#[derive(Debug, Deserialize, serde::Serialize)]
struct SignedRuntimeConfigEnvelope {
    edge_id: String,
    issued_at: chrono::DateTime<chrono::Utc>,
    algorithm: String,
    key_id: String,
    payload_json: String,
    config_hash: String,
    signature_hex: String,
}

#[derive(Debug, serde::Serialize)]
struct EdgeConfigCheckRequest<'a> {
    edge_id: &'a str,
    enrollment_token: &'a str,
    current_config_hash: Option<&'a str>,
}

pub async fn load_and_start(engine: &mut RuntimeEngine, path: &str) -> Result<LoadedRuntimeConfig> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read bootstrap file '{}'", path))?;
    let cfg: BootstrapConfig =
        serde_json::from_str(&raw).with_context(|| format!("failed to parse bootstrap JSON '{}'", path))?;
    start_from_config(engine, cfg).await
}

pub async fn load_remote_or_cached_and_start(
    engine: &mut RuntimeEngine,
) -> Result<Option<LoadedRuntimeConfig>> {
    let base_url = match std::env::var("EDGE_CONFIG_URL") {
        Ok(v) if !v.trim().is_empty() => v,
        _ => return Ok(None),
    };
    let edge_id = std::env::var("EDGE_AGENT").unwrap_or_else(|_| "edge-01".to_string());
    let enroll_token = std::env::var("EDGE_ENROLL_TOKEN")
        .unwrap_or_else(|_| "dev-edge-enroll-token".to_string());
    let signing_secret = std::env::var("EDGE_CONFIG_HMAC_SECRET")
        .unwrap_or_else(|_| "dev-edge-config-signing-secret".to_string());
    let expected_key_id = std::env::var("EDGE_CONFIG_KEY_ID").ok();
    let cache_path = std::env::var("EDGE_RUNTIME_CACHE_PATH")
        .unwrap_or_else(|_| "./data/runtime_config.signed.json".to_string());
    let cached = read_verified_cached_envelope(
        &cache_path,
        &edge_id,
        &signing_secret,
        expected_key_id.as_deref(),
    )?;
    let current_hash = cached.as_ref().map(|e| e.config_hash.as_str());

    match fetch_remote_envelope_if_changed(&base_url, &edge_id, &enroll_token, current_hash).await {
        Ok(Some(env)) => {
            verify_envelope(&env, &edge_id, &signing_secret, expected_key_id.as_deref())?;
            write_signed_cache(&cache_path, &env)?;
            let cfg: BootstrapConfig = serde_json::from_str(&env.payload_json)
                .context("failed to parse signed runtime config payload")?;
            let loaded = start_from_config(engine, cfg).await?;
            Ok(Some(loaded))
        }
        Ok(None) => {
            if let Some(env) = cached {
                let cfg: BootstrapConfig = serde_json::from_str(&env.payload_json)
                    .context("failed to parse signed runtime cache payload")?;
                let loaded = start_from_config(engine, cfg).await?;
                Ok(Some(loaded))
            } else {
                Ok(None)
            }
        }
        Err(e) => {
            warn!("remote runtime config check/fetch failed: {}. attempting local cache", e);
            if let Some(env) = read_verified_cached_envelope(
                &cache_path,
                &edge_id,
                &signing_secret,
                expected_key_id.as_deref(),
            )? {
                let cfg: BootstrapConfig = serde_json::from_str(&env.payload_json)
                    .context("failed to parse signed runtime cache payload")?;
                let loaded = start_from_config(engine, cfg).await?;
                Ok(Some(loaded))
            } else {
                Ok(None)
            }
        }
    }
}

pub fn resolve_cached_runtime_config_hash() -> Option<String> {
    let cache_path = std::env::var("EDGE_RUNTIME_CACHE_PATH")
        .unwrap_or_else(|_| "./data/runtime_config.signed.json".to_string());
    let raw = fs::read_to_string(cache_path).ok()?;
    let env: SignedRuntimeConfigEnvelope = serde_json::from_str(&raw).ok()?;
    if env.config_hash.trim().is_empty() {
        None
    } else {
        Some(env.config_hash)
    }
}

pub async fn check_and_stage_remote_config(
    base_url: &str,
    edge_id: &str,
    enrollment_token: &str,
    signing_secret: &str,
    expected_key_id: Option<&str>,
    cache_path: &str,
    current_config_hash: Option<&str>,
) -> Result<Option<String>> {
    match fetch_remote_envelope_if_changed(base_url, edge_id, enrollment_token, current_config_hash).await {
        Ok(Some(env)) => {
            verify_envelope(&env, edge_id, signing_secret, expected_key_id)?;
            let new_hash = env.config_hash.clone();
            write_signed_cache(cache_path, &env)?;
            Ok(Some(new_hash))
        }
        Ok(None) => Ok(None),
        Err(e) => Err(e),
    }
}

async fn start_from_config(
    engine: &mut RuntimeEngine,
    cfg: BootstrapConfig,
) -> Result<LoadedRuntimeConfig> {
    let automations = parse_automations(cfg.automations);
    let mut started_connections = 0usize;
    let connections = cfg.connections;
    for c in connections {
        let driver_type = DriverType::new(c.driver_type.clone()).with_context(|| {
            format!(
                "invalid driver_type '{}' for connection '{}'",
                c.driver_type, c.id
            )
        })?;

        let mut transport = c.transport;
        ensure_protocol_tag_map(&driver_type, &mut transport, &c.tags);

        let mut conn = Connection::new(
            ConnectionId::new(c.id.clone()),
            c.name.clone(),
            driver_type,
            transport,
        );
        if let Some(t) = c.timeout_ms {
            conn.timeout_ms = t;
        }
        if c.reconnect_delay_ms.is_some() || c.max_retries.is_some() {
            conn.reconnection = ReconnectionPolicy {
                strategy: ReconnectStrategy::Fixed {
                    delay_ms: c.reconnect_delay_ms.unwrap_or(1000),
                },
                max_retries: c.max_retries,
            };
        }

        let mut tags = Vec::with_capacity(c.tags.len());
        for t in c.tags {
            let mut tag = Tag::new(
                TagId::new(t.id),
                t.name,
                DeviceId::new(t.device_id),
                t.source,
            );
            if let Some(vt) = t.value_type.as_deref() {
                tag.value_type = parse_value_type(vt)?;
            }
            if let Some(enabled) = t.enabled {
                tag.enabled = enabled;
            }
            tag.update_mode = parse_update_mode(t.update_mode.as_deref(), t.interval_ms)?;
            if t.metadata_json.is_object() {
                tag.metadata = t.metadata_json;
            }
            tags.push(tag);
        }

        engine
            .start_connection(conn, tags)
            .await
            .with_context(|| format!("failed to start bootstrap connection '{}'", c.id))?;
        started_connections += 1;
    }

    Ok(LoadedRuntimeConfig {
        started_connections,
        automations,
    })
}

pub fn resolve_bootstrap_path() -> Option<String> {
    if let Ok(p) = std::env::var("EDGE_BOOTSTRAP_PATH") {
        if !p.trim().is_empty() {
            return Some(p);
        }
    }
    let default = "./config/bootstrap.json";
    if Path::new(default).exists() {
        Some(default.to_string())
    } else {
        None
    }
}

async fn fetch_remote_envelope_if_changed(
    base_url: &str,
    edge_id: &str,
    enrollment_token: &str,
    current_config_hash: Option<&str>,
) -> Result<Option<SignedRuntimeConfigEnvelope>> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()?;
    let base = base_url.trim_end_matches('/');
    let check_url = format!("{}/api/edge/config/check", base);
    let runtime_url = format!("{}/api/edge/config/runtime", base);
    let check_req = EdgeConfigCheckRequest {
        edge_id,
        enrollment_token,
        current_config_hash,
    };
    let check_resp = client.post(&check_url).json(&check_req).send().await?;
    if !check_resp.status().is_success() {
        return Err(anyhow::anyhow!(
            "config check failed with status {}",
            check_resp.status()
        ));
    }
    let check: EdgeConfigCheckResponse = check_resp.json().await?;
    if !check.accepted {
        return Err(anyhow::anyhow!("config check rejected by central"));
    }
    if !check.config_changed {
        return Ok(None);
    }
    let env = client
        .get(&runtime_url)
        .query(&[
            ("edge_id", edge_id),
            ("want_hash", current_config_hash.unwrap_or("")),
        ])
        .send()
        .await?;
    if !env.status().is_success() {
        return Err(anyhow::anyhow!(
            "runtime config fetch failed with status {}",
            env.status()
        ));
    }
    let env = env.json::<SignedRuntimeConfigEnvelope>().await?;
    if !env.config_hash.eq_ignore_ascii_case(&check.target_config_hash) {
        return Err(anyhow::anyhow!(
            "runtime config hash mismatch between check and envelope"
        ));
    }
    let _ = check.poll_after_secs;
    Ok(Some(env))
}

fn verify_envelope(
    env: &SignedRuntimeConfigEnvelope,
    expected_edge_id: &str,
    signing_secret: &str,
    expected_key_id: Option<&str>,
) -> Result<()> {
    if env.edge_id != expected_edge_id {
        return Err(anyhow::anyhow!(
            "signed config edge_id mismatch: expected '{}' got '{}'",
            expected_edge_id,
            env.edge_id
        ));
    }
    if env.algorithm.to_ascii_lowercase() != "hmac-sha256" {
        return Err(anyhow::anyhow!(
            "unsupported signed config algorithm '{}'",
            env.algorithm
        ));
    }
    if let Some(k) = expected_key_id {
        if env.key_id != k {
            return Err(anyhow::anyhow!(
                "signed config key_id mismatch: expected '{}' got '{}'",
                k,
                env.key_id
            ));
        }
    }
    if env.issued_at > chrono::Utc::now() + chrono::Duration::minutes(10) {
        return Err(anyhow::anyhow!(
            "signed config issued_at is too far in the future"
        ));
    }
    let payload_hash = Sha256::digest(env.payload_json.as_bytes());
    let config_hash = to_hex(payload_hash.as_slice());
    if config_hash != env.config_hash.to_ascii_lowercase() {
        return Err(anyhow::anyhow!("signed config hash mismatch"));
    }

    let sig_bytes = from_hex(&env.signature_hex)?;
    let mut mac = Hmac::<Sha256>::new_from_slice(signing_secret.as_bytes())?;
    mac.update(&payload_hash);
    mac.verify_slice(&sig_bytes)
        .map_err(|_| anyhow::anyhow!("signed config signature verification failed"))?;
    Ok(())
}

fn write_signed_cache(cache_path: &str, env: &SignedRuntimeConfigEnvelope) -> Result<()> {
    if let Some(parent) = Path::new(cache_path).parent() {
        let _ = fs::create_dir_all(parent);
    }
    let raw = serde_json::to_string_pretty(env)?;
    fs::write(cache_path, raw).with_context(|| format!("failed to write cache '{}'", cache_path))?;
    Ok(())
}

fn read_verified_cached_envelope(
    cache_path: &str,
    edge_id: &str,
    signing_secret: &str,
    expected_key_id: Option<&str>,
) -> Result<Option<SignedRuntimeConfigEnvelope>> {
    if !Path::new(cache_path).exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(cache_path)
        .with_context(|| format!("failed to read runtime cache '{}'", cache_path))?;
    let env: SignedRuntimeConfigEnvelope =
        serde_json::from_str(&raw).context("failed to parse signed runtime cache envelope")?;
    verify_envelope(&env, edge_id, signing_secret, expected_key_id)?;
    Ok(Some(env))
}

fn from_hex(input: &str) -> Result<Vec<u8>> {
    let bytes = input.trim().as_bytes();
    if bytes.len() % 2 != 0 {
        return Err(anyhow::anyhow!("invalid hex length"));
    }
    let mut out = Vec::with_capacity(bytes.len() / 2);
    for i in (0..bytes.len()).step_by(2) {
        let hi = hex_nibble(bytes[i])?;
        let lo = hex_nibble(bytes[i + 1])?;
        out.push((hi << 4) | lo);
    }
    Ok(out)
}

fn hex_nibble(b: u8) -> Result<u8> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(10 + b - b'a'),
        b'A'..=b'F' => Ok(10 + b - b'A'),
        _ => Err(anyhow::anyhow!("invalid hex character '{}'", b as char)),
    }
}

fn to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

fn ensure_protocol_tag_map(driver_type: &DriverType, transport: &mut Value, tags: &[BootstrapTag]) {
    let driver = driver_type.as_str().to_ascii_lowercase();
    if driver != "modbusrtu" && driver != "modbustcp" && driver != "serialascii" {
        return;
    }

    if !transport.is_object() {
        *transport = json!({});
    }
    let obj = transport.as_object_mut().expect("object ensured above");
    if obj.contains_key("tag_map") {
        return;
    }

    let mut tag_map = Map::new();
    if driver == "modbusrtu" {
        let device_unit_map = obj
            .get("device_unit_map")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        for t in tags {
            let unit_id = device_unit_map
                .get(&t.device_id)
                .and_then(Value::as_u64)
                .and_then(|v| u8::try_from(v).ok());
            let mut entry = Map::new();
            entry.insert("source".to_string(), Value::String(t.source.clone()));
            entry.insert("device_id".to_string(), Value::String(t.device_id.clone()));
            if let Some(unit) = unit_id {
                entry.insert(
                    "unit_id".to_string(),
                    Value::Number(serde_json::Number::from(unit)),
                );
            }
            tag_map.insert(t.id.clone(), Value::Object(entry));
        }
    } else {
        for t in tags {
            tag_map.insert(t.id.clone(), Value::String(t.source.clone()));
        }
    }
    obj.insert("tag_map".to_string(), Value::Object(tag_map));
}

fn parse_value_type(input: &str) -> Result<TagValueType> {
    match input.trim().to_ascii_lowercase().as_str() {
        "float" => Ok(TagValueType::Float),
        "integer" | "int" => Ok(TagValueType::Integer),
        "bool" | "boolean" => Ok(TagValueType::Boolean),
        "string" | "str" => Ok(TagValueType::String),
        other => Err(anyhow::anyhow!("unsupported tag value_type '{}'", other)),
    }
}

fn parse_update_mode(mode: Option<&str>, interval_ms: Option<u64>) -> Result<TagUpdateMode> {
    let m = mode.unwrap_or("polling").trim().to_ascii_lowercase();
    let interval = interval_ms.unwrap_or(1000);
    match m.as_str() {
        "polling" => Ok(TagUpdateMode::Polling { interval_ms: interval }),
        "on_change" | "onchange" => Ok(TagUpdateMode::OnChange),
        "on_message" | "onmessage" => Ok(TagUpdateMode::OnMessage),
        "polling_on_change" | "pollingonchange" => {
            Ok(TagUpdateMode::PollingOnChange { interval_ms: interval })
        }
        other => Err(anyhow::anyhow!("unsupported update_mode '{}'", other)),
    }
}

fn parse_automations(raw_automations: Vec<Value>) -> Vec<AutomationSpec> {
    let mut out = Vec::new();
    for raw in raw_automations {
        match serde_json::from_value::<AutomationSpec>(raw) {
            Ok(spec) => out.push(spec),
            Err(e) => warn!("invalid automation entry ignored: {}", e),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ensure_modbus_tag_map_from_tags() {
        let driver = DriverType::new("ModbusTCP").unwrap();
        let mut transport = json!({"host":"127.0.0.1"});
        let tags = vec![BootstrapTag {
            id: "t1".to_string(),
            name: "T1".to_string(),
            device_id: "d1".to_string(),
            source: "hr:0:u16".to_string(),
            enabled: None,
            value_type: None,
            update_mode: None,
            interval_ms: None,
            metadata_json: json!({}),
        }];
        ensure_protocol_tag_map(&driver, &mut transport, &tags);
        assert_eq!(transport["tag_map"]["t1"], "hr:0:u16");
    }

    #[test]
    fn test_ensure_modbus_rtu_tag_map_uses_device_unit_map() {
        let driver = DriverType::new("ModbusRTU").unwrap();
        let mut transport = json!({
            "serial": {"port":"COM10"},
            "device_unit_map": {
                "dev50": 50,
                "dev100": 100
            }
        });
        let tags = vec![
            BootstrapTag {
                id: "t50".to_string(),
                name: "T50".to_string(),
                device_id: "dev50".to_string(),
                source: "hr:0:u16".to_string(),
                enabled: None,
                value_type: None,
                update_mode: None,
                interval_ms: None,
                metadata_json: json!({}),
            },
            BootstrapTag {
                id: "t100".to_string(),
                name: "T100".to_string(),
                device_id: "dev100".to_string(),
                source: "hr:10:f32".to_string(),
                enabled: None,
                value_type: None,
                update_mode: None,
                interval_ms: None,
                metadata_json: json!({}),
            },
        ];

        ensure_protocol_tag_map(&driver, &mut transport, &tags);
        assert_eq!(transport["tag_map"]["t50"]["source"], "hr:0:u16");
        assert_eq!(transport["tag_map"]["t50"]["device_id"], "dev50");
        assert_eq!(transport["tag_map"]["t50"]["unit_id"], 50);
        assert_eq!(transport["tag_map"]["t100"]["unit_id"], 100);
    }

    #[test]
    fn test_ensure_serial_ascii_tag_map_from_tags() {
        let driver = DriverType::new("SerialAscii").unwrap();
        let mut transport = json!({"serial":{"port":"COM7"}});
        let tags = vec![BootstrapTag {
            id: "tag_scale_manual_compound".to_string(),
            name: "Scale Compound".to_string(),
            device_id: "dev_scale_manual_1".to_string(),
            source: "scale:compound".to_string(),
            enabled: None,
            value_type: None,
            update_mode: None,
            interval_ms: None,
            metadata_json: json!({}),
        }];
        ensure_protocol_tag_map(&driver, &mut transport, &tags);
        assert_eq!(
            transport["tag_map"]["tag_scale_manual_compound"],
            "scale:compound"
        );
    }

    #[test]
    fn test_parse_update_mode_defaults_polling() {
        let m = parse_update_mode(None, None).unwrap();
        match m {
            TagUpdateMode::Polling { interval_ms } => assert_eq!(interval_ms, 1000),
            _ => panic!("expected polling"),
        }
    }

    #[test]
    fn test_parse_update_mode_on_message() {
        let m = parse_update_mode(Some("on_message"), None).unwrap();
        match m {
            TagUpdateMode::OnMessage => {}
            _ => panic!("expected on_message"),
        }
    }

    #[test]
    fn test_bootstrap_example_has_writable_modbus_tag_for_e2e() {
        let raw = include_str!("../config/bootstrap.example.json");
        let cfg: BootstrapConfig = serde_json::from_str(raw).expect("valid bootstrap example");

        let conn = cfg
            .connections
            .iter()
            .find(|c| c.driver_type.eq_ignore_ascii_case("ModbusTCP"))
            .expect("expected ModbusTCP connection in bootstrap example");

        let tag = conn
            .tags
            .iter()
            .find(|t| t.id == "tag_hr_10_cmd")
            .expect("expected writable e2e command tag");

        assert_eq!(tag.value_type.as_deref(), Some("integer"));
        assert!(
            tag.source.starts_with("hr:"),
            "expected holding register source, got {}",
            tag.source
        );
    }

    #[test]
    fn test_verify_envelope_rejects_tampered_payload() {
        let secret = "unit-test-secret";
        let payload = r#"{"connections":[]}"#.to_string();
        let hash = Sha256::digest(payload.as_bytes());
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(&hash);
        let sig = mac.finalize().into_bytes();
        let mut env = SignedRuntimeConfigEnvelope {
            edge_id: "edge-01".to_string(),
            issued_at: chrono::Utc::now(),
            algorithm: "hmac-sha256".to_string(),
            key_id: "v1".to_string(),
            payload_json: payload,
            config_hash: to_hex(hash.as_slice()),
            signature_hex: to_hex(sig.as_slice()),
        };
        verify_envelope(&env, "edge-01", secret, Some("v1")).expect("valid envelope");
        env.payload_json = r#"{"connections":[{"id":"x"}]}"#.to_string();
        let err = verify_envelope(&env, "edge-01", secret, Some("v1")).unwrap_err();
        assert!(
            err.to_string().contains("hash mismatch"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn test_parse_automations_ignores_invalid_entries() {
        let raw = vec![
            json!({
                "id":"a1",
                "name":"auto",
                "enabled":true,
                "trigger":{
                    "type":"consecutive_numeric",
                    "tag_id":"tag_scale_manual_compound",
                    "threshold":0.0,
                    "count":2,
                    "operator":"lte"
                },
                "action":{
                    "action_type":"print.escpos",
                    "target":"edge",
                    "scope":"edge",
                    "payload":{"lines":["AUTO"]}
                }
            }),
            json!({"id":"broken"})
        ];
        let parsed = parse_automations(raw);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].id, "a1");
    }
}

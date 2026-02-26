use crate::mqtt_bridge::MqttBridgeConfig;
use async_trait::async_trait;
use serde_json::json;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::process::Command;
use tokio::sync::Mutex as TokioMutex;
use tokio::time::Duration;
use tracing::debug;

const ACTION_IDEMPOTENCY_TTL_SECS: i64 = 86_400;

#[derive(Debug, Clone)]
pub struct ActionRequest {
    pub request_id: Option<String>,
    pub action_type: String,
    pub target: Option<String>,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct BufferedWeightSample {
    pub value: f64,
    pub unit: String,
    pub raw: String,
    pub ts: chrono::DateTime<chrono::Utc>,
}

#[derive(Default)]
pub struct ActionRuntimeState {
    pub weight_buffers: HashMap<String, Vec<BufferedWeightSample>>,
    pub processed_requests: HashMap<String, chrono::DateTime<chrono::Utc>>,
}

#[async_trait]
pub trait ActionExecutor: Send + Sync {
    async fn execute(
        &self,
        cfg: &MqttBridgeConfig,
        state: &Arc<TokioMutex<ActionRuntimeState>>,
        req: &ActionRequest,
    ) -> Result<(), String>;
}

pub struct ActionOrchestrator {
    executors: HashMap<String, Arc<dyn ActionExecutor>>,
}

impl ActionOrchestrator {
    pub fn new_default() -> Self {
        let mut executors: HashMap<String, Arc<dyn ActionExecutor>> = HashMap::new();
        executors.insert(
            "print.escpos".to_string(),
            Arc::new(PrintEscposExecutor),
        );
        executors.insert(
            "print.escpos.from_buffer".to_string(),
            Arc::new(PrintEscposFromBufferExecutor),
        );
        executors.insert(
            "buffer.weights.accumulate".to_string(),
            Arc::new(BufferWeightsAccumulateExecutor),
        );
        executors.insert(
            "connection.check".to_string(),
            Arc::new(ConnectionCheckExecutor),
        );
        executors.insert("print.persist".to_string(), Arc::new(PrintPersistExecutor));
        executors.insert("device.command".to_string(), Arc::new(DeviceCommandExecutor));
        Self { executors }
    }

    pub async fn execute(
        &self,
        cfg: &MqttBridgeConfig,
        state: &Arc<TokioMutex<ActionRuntimeState>>,
        req: &ActionRequest,
    ) -> Result<(), String> {
        if let Some(target) = req.target.as_deref() {
            if !target.eq_ignore_ascii_case("edge") && !target.eq_ignore_ascii_case("central") {
                return Err(format!("unsupported action target '{}'", target));
            }
        }

        let action_key = req.action_type.trim().to_ascii_lowercase();
        let dedupe_key = if should_dedupe_request(&action_key) {
            req.request_id
                .as_deref()
                .map(|rid| make_request_dedupe_key(&action_key, rid))
        } else {
            None
        };
        if let Some(key) = dedupe_key.as_deref() {
            if is_duplicate_request(state, key).await {
                debug!(
                    request_id = ?req.request_id,
                    action_type = %req.action_type,
                    "duplicate action request ignored"
                );
                return Ok(());
            }
        }

        let executor = self
            .executors
            .get(&action_key)
            .ok_or_else(|| format!("unsupported action_type '{}'", req.action_type))?;
        let result = executor.execute(cfg, state, req).await;

        if result.is_ok() {
            if let Some(key) = dedupe_key {
                mark_request_processed(state, key).await;
            }
        }
        result
    }
}

pub fn action_buffer_id(payload: &serde_json::Value) -> String {
    if let Some(explicit) = payload
        .get("buffer_id")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
    {
        return explicit;
    }

    let trig = payload.get("trigger");
    let auto_id = trig
        .and_then(|t| t.get("automation_id"))
        .and_then(|v| v.as_str())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());
    let tag_id = trig
        .and_then(|t| t.get("tag_id"))
        .and_then(|v| v.as_str())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());

    match (auto_id, tag_id) {
        (Some(a), Some(t)) => format!("auto:{}:tag:{}", a, t),
        (None, Some(t)) => format!("tag:{}", t),
        (Some(a), None) => format!("auto:{}", a),
        _ => "default".to_string(),
    }
}

fn action_buffer_max(payload: &serde_json::Value) -> usize {
    payload
        .get("max_items")
        .and_then(|v| v.as_u64())
        .map(|v| v.clamp(1, 5000) as usize)
        .unwrap_or(500)
}

fn extract_weight_sample(payload: &serde_json::Value) -> Result<BufferedWeightSample, String> {
    let value_json = payload
        .get("trigger")
        .and_then(|t| t.get("value"))
        .cloned()
        .unwrap_or_else(|| payload.get("value").cloned().unwrap_or(serde_json::Value::Null));

    let mut value: Option<f64> = None;
    let mut unit: Option<String> = None;
    let mut raw: Option<String> = None;

    match value_json {
        serde_json::Value::Number(n) => value = n.as_f64(),
        serde_json::Value::Object(map) => {
            value = map.get("value").and_then(|v| v.as_f64());
            unit = map.get("unit").and_then(|v| v.as_str()).map(ToString::to_string);
            raw = map.get("raw").and_then(|v| v.as_str()).map(ToString::to_string);
        }
        serde_json::Value::String(s) => {
            let t = s.trim();
            if let Ok(v) = t.parse::<f64>() {
                value = Some(v);
            } else if t.starts_with('{') && t.ends_with('}') {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(t) {
                    if let serde_json::Value::Object(map) = parsed {
                        value = map.get("value").and_then(|v| v.as_f64());
                        unit = map.get("unit").and_then(|v| v.as_str()).map(ToString::to_string);
                        raw = map.get("raw").and_then(|v| v.as_str()).map(ToString::to_string);
                    }
                }
            } else if !t.is_empty() {
                let compact: String = t.chars().filter(|c| !c.is_whitespace()).collect();
                let mut split_idx: Option<usize> = None;
                for (i, ch) in compact.char_indices() {
                    if !(ch.is_ascii_digit() || ch == '+' || ch == '-' || ch == '.') {
                        split_idx = Some(i);
                        break;
                    }
                }
                if let Some(i) = split_idx {
                    let num = &compact[..i];
                    let unit_txt = compact[i..].trim().to_string();
                    if let Ok(v) = num.parse::<f64>() {
                        value = Some(v);
                        if !unit_txt.is_empty() {
                            unit = Some(unit_txt);
                        }
                    }
                } else if let Ok(v) = compact.parse::<f64>() {
                    value = Some(v);
                }
            }
        }
        _ => {}
    }

    let value = value.ok_or_else(|| "unable to extract numeric value from payload".to_string())?;
    let unit = unit.unwrap_or_default();
    let raw = raw.unwrap_or_else(|| format!("{} {}", value, unit).trim().to_string());
    Ok(BufferedWeightSample {
        value,
        unit,
        raw,
        ts: chrono::Utc::now(),
    })
}

fn render_action_lines(payload: &serde_json::Value) -> Vec<String> {
    if let Some(lines) = payload.get("lines").and_then(|v| v.as_array()) {
        let mut out = Vec::new();
        for entry in lines {
            if let Some(s) = entry.as_str() {
                out.push(s.to_string());
            } else {
                out.push(entry.to_string());
            }
        }
        if !out.is_empty() {
            return out;
        }
    }
    if let Some(text) = payload.get("text").and_then(|v| v.as_str()) {
        return vec![text.to_string()];
    }
    if let Some(data) = payload.get("data") {
        return vec![format!(
            "DATA: {}",
            serde_json::to_string(data).unwrap_or_else(|_| "{}".to_string())
        )];
    }
    vec!["SCADA ACTION PRINT".to_string()]
}

fn pad_right(text: &str, len: usize) -> String {
    let mut s = text.to_string();
    if s.len() < len {
        s.push_str(&" ".repeat(len - s.len()));
    }
    s
}

fn pad_left(text: &str, len: usize) -> String {
    if text.len() >= len {
        return text.to_string();
    }
    format!("{}{}", " ".repeat(len - text.len()), text)
}

fn std_dev_sample(values: &[f64]) -> f64 {
    if values.len() <= 1 {
        return 0.0;
    }
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    let var = values
        .iter()
        .map(|v| {
            let d = *v - mean;
            d * d
        })
        .sum::<f64>()
        / (values.len() as f64 - 1.0);
    var.sqrt()
}

fn escpos_bytes_from_lines(
    lines: &[String],
    cut: bool,
    leading_feed_lines: u8,
    trailing_feed_lines: u8,
) -> Vec<u8> {
    let mut out = vec![0x1B, 0x40];
    for _ in 0..leading_feed_lines {
        out.extend_from_slice(b"\r\n");
    }
    for l in lines {
        out.extend_from_slice(l.as_bytes());
        out.extend_from_slice(b"\r\n");
    }
    for _ in 0..trailing_feed_lines {
        out.extend_from_slice(b"\r\n");
    }
    if cut {
        // Minimal feed cut sequence.
        out.extend_from_slice(&[0x1D, 0x56, 0x41, 0x00]);
    }
    out
}

fn resolve_tcp_target(cfg: &MqttBridgeConfig, payload: &serde_json::Value) -> (Option<String>, u16) {
    let payload_host = payload
        .get("printer")
        .and_then(|p| p.get("host"))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToString::to_string)
        .or_else(|| {
            payload
                .get("printer_host")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(ToString::to_string)
        })
        .or_else(|| {
            payload
                .get("host")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(ToString::to_string)
        });
    let payload_port = payload
        .get("printer")
        .and_then(|p| p.get("port"))
        .and_then(|v| v.as_u64())
        .and_then(|p| u16::try_from(p).ok())
        .or_else(|| {
            payload
                .get("printer_port")
                .and_then(|v| v.as_u64())
                .and_then(|p| u16::try_from(p).ok())
        })
        .or_else(|| {
            payload
                .get("port")
                .and_then(|v| v.as_u64())
                .and_then(|p| u16::try_from(p).ok())
        });
    let tcp_host = payload_host
        .or_else(|| {
            cfg.on_demand_tcp_host
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(ToString::to_string)
        })
        .or_else(|| {
            cfg.escpos_tcp_host
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(ToString::to_string)
        });
    let fallback_port = cfg.on_demand_tcp_port.unwrap_or(cfg.escpos_tcp_port);
    (tcp_host, payload_port.unwrap_or(fallback_port))
}

fn resolve_windows_share_target(
    cfg: &MqttBridgeConfig,
    payload: &serde_json::Value,
) -> Option<String> {
    let raw = payload
        .get("printer")
        .and_then(|p| p.get("share"))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToString::to_string)
        .or_else(|| {
            payload
                .get("printer")
                .and_then(|p| p.get("path"))
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(ToString::to_string)
        })
        .or_else(|| {
            payload
                .get("share")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(ToString::to_string)
        })
        .or_else(|| {
            payload
                .get("unc")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(ToString::to_string)
        })
        .or_else(|| {
            cfg.escpos_windows_share
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(ToString::to_string)
        });
    raw.and_then(|s| normalize_windows_share(&s))
}

fn normalize_windows_share(input: &str) -> Option<String> {
    let mut s = input.trim().trim_matches('"').replace('/', "\\");
    if s.is_empty() {
        return None;
    }
    // Accept values accidentally serialized with extra leading slashes.
    let trimmed_start = s.trim_start_matches('\\');
    let mut parts = trimmed_start
        .split('\\')
        .filter(|p| !p.trim().is_empty())
        .map(str::trim)
        .collect::<Vec<_>>();
    if parts.len() < 2 {
        return None;
    }
    let server = parts.remove(0);
    let share = parts.remove(0);
    let mut out = format!("\\\\{}\\{}", server, share);
    if !parts.is_empty() {
        out.push('\\');
        out.push_str(&parts.join("\\"));
    }
    s.clear();
    Some(out)
}

fn should_dedupe_request(action_type: &str) -> bool {
    action_type.trim().to_ascii_lowercase().starts_with("print.")
}

fn make_request_dedupe_key(action_type: &str, req_id: &str) -> String {
    format!("{}:{}", action_type.trim().to_ascii_lowercase(), req_id.trim())
}

async fn is_duplicate_request(state: &Arc<TokioMutex<ActionRuntimeState>>, key: &str) -> bool {
    let now = chrono::Utc::now();
    let mut st = state.lock().await;
    st.processed_requests.retain(|_, ts| {
        now.signed_duration_since(*ts).num_seconds() <= ACTION_IDEMPOTENCY_TTL_SECS
    });
    st.processed_requests.contains_key(key)
}

async fn mark_request_processed(state: &Arc<TokioMutex<ActionRuntimeState>>, key: String) {
    let now = chrono::Utc::now();
    let mut st = state.lock().await;
    st.processed_requests.insert(key, now);
}

struct BufferWeightsAccumulateExecutor;
struct PrintEscposExecutor;
struct PrintEscposFromBufferExecutor;
struct ConnectionCheckExecutor;
struct PrintPersistExecutor;
struct DeviceCommandExecutor;

#[async_trait]
impl ActionExecutor for BufferWeightsAccumulateExecutor {
    async fn execute(
        &self,
        _cfg: &MqttBridgeConfig,
        state: &Arc<TokioMutex<ActionRuntimeState>>,
        req: &ActionRequest,
    ) -> Result<(), String> {
        let sample = extract_weight_sample(&req.payload)?;
        let only_positive = req
            .payload
            .get("only_positive")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        if only_positive && sample.value <= 0.0 {
            return Ok(());
        }
        let buffer_id = action_buffer_id(&req.payload);
        let max_items = action_buffer_max(&req.payload);
        let mut st = state.lock().await;
        let buf = st.weight_buffers.entry(buffer_id.clone()).or_default();
        buf.push(sample);
        if buf.len() > max_items {
            let drop_n = buf.len() - max_items;
            buf.drain(0..drop_n);
        }
        debug!(buffer_id = %buffer_id, size = buf.len(), "buffer.weights.accumulate applied");
        Ok(())
    }
}

#[async_trait]
impl ActionExecutor for PrintEscposExecutor {
    async fn execute(
        &self,
        cfg: &MqttBridgeConfig,
        state: &Arc<TokioMutex<ActionRuntimeState>>,
        req: &ActionRequest,
    ) -> Result<(), String> {
        let mode = req
            .payload
            .get("mode")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let wants_buffer = req.payload.get("buffer_id").is_some() || mode == "from_buffer";
        if wants_buffer {
            return PrintEscposFromBufferExecutor.execute(cfg, state, req).await;
        }
        execute_print_escpos_lines(
            cfg,
            &req.payload,
            req.request_id.as_deref(),
            &req.action_type,
            &render_action_lines(&req.payload),
        )
        .await
    }
}

#[async_trait]
impl ActionExecutor for PrintEscposFromBufferExecutor {
    async fn execute(
        &self,
        cfg: &MqttBridgeConfig,
        state: &Arc<TokioMutex<ActionRuntimeState>>,
        req: &ActionRequest,
    ) -> Result<(), String> {
        let buffer_id = action_buffer_id(&req.payload);
        let clear_after = req
            .payload
            .get("clear_after_print")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let samples = {
            let mut st = state.lock().await;
            let buf = st.weight_buffers.entry(buffer_id.clone()).or_default();
            let cp = buf.clone();
            if clear_after {
                buf.clear();
            }
            cp
        };
        if samples.is_empty() {
            return Err(format!("buffer '{}' is empty", buffer_id));
        }
        let print_id_num = req
            .payload
            .get("print_id")
            .and_then(|v| v.as_i64())
            .unwrap_or_else(|| chrono::Utc::now().timestamp_millis().rem_euclid(1_000_000));
        let print_id = format!("{:06}", print_id_num);
        let device_name = req
            .payload
            .get("device")
            .and_then(|d| d.get("name"))
            .and_then(|v| v.as_str())
            .or_else(|| req.payload.get("device_name").and_then(|v| v.as_str()))
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(ToString::to_string)
            .or_else(|| {
                req.payload
                    .get("device")
                    .and_then(|d| d.get("id"))
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|v| !v.is_empty())
                    .map(ToString::to_string)
            })
            .or_else(|| {
                req.payload
                    .get("device_id")
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|v| !v.is_empty())
                    .map(ToString::to_string)
            })
            .unwrap_or_else(|| "unknown-device".to_string());
        let description = req
            .payload
            .get("description")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .unwrap_or("___________________");
        let lote = req
            .payload
            .get("batch")
            .and_then(|v| v.as_str())
            .or_else(|| req.payload.get("lote").and_then(|v| v.as_str()))
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .unwrap_or("___________________");
        let decimals = req
            .payload
            .get("decimals")
            .and_then(|v| v.as_u64())
            .map(|v| v.min(6) as usize)
            .unwrap_or(4);
        let count = samples.len();
        let sum: f64 = samples.iter().map(|s| s.value).sum();
        let avg: f64 = if count > 0 { sum / count as f64 } else { 0.0 };
        let min = samples
            .iter()
            .map(|s| s.value)
            .fold(f64::INFINITY, f64::min);
        let max = samples
            .iter()
            .map(|s| s.value)
            .fold(f64::NEG_INFINITY, f64::max);
        let first_ts = samples.first().map(|s| s.ts);
        let last_ts = samples.last().map(|s| s.ts);
        let unit = samples
            .iter()
            .find_map(|s| {
                let u = s.unit.trim();
                if u.is_empty() { None } else { Some(u.to_string()) }
            })
            .unwrap_or_default();
        let values: Vec<f64> = samples.iter().map(|s| s.value).collect();
        let std = std_dev_sample(&values);
        let cv = if avg.abs() > f64::EPSILON {
            std / avg
        } else {
            0.0
        };
        let mut lines = vec![
            format!(
                "{}{}",
                pad_right("Descripcion:", 12),
                pad_left(description, 26)
            ),
            format!("{}{}", pad_right("Lote:", 12), pad_left(lote, 26)),
            format!(
                "{}{}{}{}",
                pad_right("Equipo:", 12),
                device_name,
                pad_left("#", 5),
                print_id
            ),
            format!("BUFFER: {}", buffer_id),
            String::new(),
            "===============< Datos >===============".to_string(),
        ];
        if let (Some(first), Some(last)) = (first_ts, last_ts) {
            lines.push(format!(
                "{}{}",
                pad_right("Inicio:", 12),
                pad_left(&first.format("%d/%m/%Y %H:%M:%S").to_string(), 26)
            ));
            lines.push(format!(
                "{}{}",
                pad_right("Fin:", 12),
                pad_left(&last.format("%d/%m/%Y %H:%M:%S").to_string(), 26)
            ));
        }
        for (i, s) in samples.iter().enumerate() {
            let unit_part = if s.unit.is_empty() {
                "".to_string()
            } else {
                s.unit.clone()
            };
            let value_txt = format!("{:.*}", decimals, s.value);
            lines.push(format!(
                "{}{}{}",
                pad_right(&format!("N{:02}", i + 1), 12),
                pad_left(&value_txt, 20),
                pad_left(&unit_part, 2)
            ));
        }
        lines.push(String::new());
        lines.push("===========< Estadisticas >===========".to_string());
        lines.push(format!("COUNT: {}", count));
        lines.push(format!("{}{}", pad_right("N:", 12), pad_left(&count.to_string(), 26)));
        lines.push(format!(
            "{}{}{}",
            pad_right("Min:", 12),
            pad_left(&format!("{:.*}", decimals, min), 20),
            pad_left(&unit, 2)
        ));
        lines.push(format!(
            "{}{}{}",
            pad_right("Max:", 12),
            pad_left(&format!("{:.*}", decimals, max), 20),
            pad_left(&unit, 2)
        ));
        lines.push(format!(
            "{}{}{}",
            pad_right("Promedio:", 12),
            pad_left(&format!("{:.*}", decimals, avg), 20),
            pad_left(&unit, 2)
        ));
        lines.push(format!(
            "{}{}{}",
            pad_right("Std:", 12),
            pad_left(&format!("{:.*}", decimals, std), 20),
            pad_left(&unit, 2)
        ));
        lines.push(format!(
            "{}{}{}",
            pad_right("CV:", 12),
            pad_left(&format!("{:.*}", decimals, cv), 20),
            pad_left("%", 2)
        ));
        lines.push("--------------------------------------".to_string());
        lines.push(String::new());
        lines.push(String::new());
        lines.push("Firma_________________________________".to_string());
        execute_print_escpos_lines(
            cfg,
            &req.payload,
            req.request_id.as_deref(),
            &req.action_type,
            &lines,
        )
        .await
    }
}

#[async_trait]
impl ActionExecutor for ConnectionCheckExecutor {
    async fn execute(
        &self,
        cfg: &MqttBridgeConfig,
        _state: &Arc<TokioMutex<ActionRuntimeState>>,
        req: &ActionRequest,
    ) -> Result<(), String> {
        let (host_opt, port) = resolve_tcp_target(cfg, &req.payload);
        let timeout_ms = req
            .payload
            .get("timeout_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(cfg.on_demand_probe_timeout_ms)
            .max(100);
        if let Some(host) = host_opt {
            let addr = format!("{}:{}", host, port);
            return match tokio::time::timeout(Duration::from_millis(timeout_ms), TcpStream::connect(addr.clone())).await {
                Ok(Ok(stream)) => {
                    drop(stream);
                    Ok(())
                }
                Ok(Err(e)) => Err(format!("connection.check failed '{}': {}", addr, e)),
                Err(_) => Err(format!("connection.check timeout '{}' after {} ms", addr, timeout_ms)),
            };
        }

        if let Some(share) = resolve_windows_share_target(cfg, &req.payload) {
            return check_windows_share_access(&share, timeout_ms).await;
        }

        Err(
            "connection.check requires host/port or windows printer share (payload.printer.share)"
                .to_string(),
        )
    }
}

#[async_trait]
impl ActionExecutor for PrintPersistExecutor {
    async fn execute(
        &self,
        _cfg: &MqttBridgeConfig,
        _state: &Arc<TokioMutex<ActionRuntimeState>>,
        req: &ActionRequest,
    ) -> Result<(), String> {
        debug!(
            request_id = ?req.request_id,
            payload = %json!(req.payload),
            "print.persist accepted for central pipeline"
        );
        Ok(())
    }
}

#[async_trait]
impl ActionExecutor for DeviceCommandExecutor {
    async fn execute(
        &self,
        cfg: &MqttBridgeConfig,
        state: &Arc<TokioMutex<ActionRuntimeState>>,
        req: &ActionRequest,
    ) -> Result<(), String> {
        let device_id = req
            .payload
            .get("device_id")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .or_else(|| {
                req.payload
                    .get("device")
                    .and_then(|v| v.get("id"))
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|v| !v.is_empty())
            })
            .ok_or_else(|| "device.command requires device_id (or device.id)".to_string())?;

        let command = req
            .payload
            .get("command")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .ok_or_else(|| "device.command requires payload.command".to_string())?
            .to_ascii_lowercase();

        let mut args = req
            .payload
            .get("args")
            .cloned()
            .unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::new()));
        if !args.is_object() {
            args = serde_json::json!({ "value": args });
        }
        let obj = args
            .as_object_mut()
            .ok_or_else(|| "device.command args must be object".to_string())?;
        obj.entry("device_id".to_string())
            .or_insert_with(|| serde_json::Value::String(device_id.to_string()));
        inject_device_transport_defaults(&req.payload, obj);

        let sub_req = if command == "print" || command == "print.escpos" {
            ActionRequest {
                request_id: req.request_id.clone(),
                action_type: "print.escpos".to_string(),
                target: req.target.clone(),
                payload: serde_json::Value::Object(obj.clone()),
            }
        } else if command == "check" || command == "connection.check" {
            ActionRequest {
                request_id: req.request_id.clone(),
                action_type: "connection.check".to_string(),
                target: req.target.clone(),
                payload: serde_json::Value::Object(obj.clone()),
            }
        } else {
            return Err(format!(
                "unsupported device.command '{}'; supported: print, connection.check",
                command
            ));
        };

        if sub_req.action_type == "print.escpos" {
            PrintEscposExecutor.execute(cfg, state, &sub_req).await
        } else {
            ConnectionCheckExecutor.execute(cfg, state, &sub_req).await
        }
    }
}

fn inject_device_transport_defaults(
    root_payload: &serde_json::Value,
    args: &mut serde_json::Map<String, serde_json::Value>,
) {
    if args.contains_key("host")
        || args.contains_key("printer_host")
        || args.get("printer").is_some()
        || args.contains_key("share")
        || args.contains_key("unc")
    {
        return;
    }

    let device = root_payload.get("device").and_then(|v| v.as_object());
    let win_share = device
        .and_then(|d| d.get("transport"))
        .and_then(|v| v.get("windows"))
        .and_then(|v| v.as_object())
        .and_then(|w| w.get("share").and_then(|v| v.as_str()))
        .map(str::trim)
        .filter(|v| !v.is_empty());
    if let Some(share) = win_share {
        let mut printer = serde_json::Map::new();
        printer.insert(
            "share".to_string(),
            serde_json::Value::String(share.to_string()),
        );
        args.insert("printer".to_string(), serde_json::Value::Object(printer));
        return;
    }

    let tcp = device
        .and_then(|d| d.get("transport"))
        .and_then(|v| v.get("tcp"))
        .and_then(|v| v.as_object());
    let Some(tcp) = tcp else {
        return;
    };

    let host = tcp
        .get("host")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty());
    let port = tcp
        .get("port")
        .and_then(|v| v.as_u64())
        .and_then(|p| u16::try_from(p).ok());
    let Some(host) = host else {
        return;
    };

    let mut printer = serde_json::Map::new();
    printer.insert("host".to_string(), serde_json::Value::String(host.to_string()));
    if let Some(port) = port {
        printer.insert(
            "port".to_string(),
            serde_json::Value::Number(serde_json::Number::from(port)),
        );
    }
    args.insert("printer".to_string(), serde_json::Value::Object(printer));
}

async fn execute_print_escpos_lines(
    cfg: &MqttBridgeConfig,
    payload: &serde_json::Value,
    request_id: Option<&str>,
    action_type: &str,
    lines: &[String],
) -> Result<(), String> {
    let windows_share = resolve_windows_share_target(cfg, payload);
    let cut = payload
        .get("printer")
        .and_then(|p| p.get("cut"))
        .and_then(|v| v.as_bool())
        .or_else(|| payload.get("cut").and_then(|v| v.as_bool()))
        .unwrap_or(true);
    let leading_feed_lines = payload
        .get("printer")
        .and_then(|p| p.get("leading_feed_lines"))
        .and_then(|v| v.as_u64())
        .or_else(|| payload.get("leading_feed_lines").and_then(|v| v.as_u64()))
        .map(|v| v.min(10) as u8)
        .unwrap_or(0);
    let trailing_feed_lines = payload
        .get("printer")
        .and_then(|p| p.get("trailing_feed_lines"))
        .and_then(|v| v.as_u64())
        .or_else(|| payload.get("trailing_feed_lines").and_then(|v| v.as_u64()))
        .map(|v| v.min(10) as u8)
        .unwrap_or(1);
    let bytes = escpos_bytes_from_lines(lines, cut, leading_feed_lines, trailing_feed_lines);
    if let Some(share) = windows_share {
        return print_to_windows_share(&bytes, &share).await;
    }
    let (tcp_host, tcp_port) = resolve_tcp_target(cfg, payload);
    if let Some(host) = tcp_host {
        let addr = format!("{}:{}", host, tcp_port);
        let mut sock = TcpStream::connect(addr.clone())
            .await
            .map_err(|e| format!("escpos tcp connect failed '{}': {}", addr, e))?;
        sock.write_all(&bytes)
            .await
            .map_err(|e| format!("escpos tcp write failed: {}", e))?;
        sock.shutdown()
            .await
            .map_err(|e| format!("escpos tcp shutdown failed: {}", e))?;
        return Ok(());
    }

    if let Some(parent) = Path::new(&cfg.escpos_output_path).parent() {
        let _ = tokio::fs::create_dir_all(parent).await;
    }
    let mut f = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&cfg.escpos_output_path)
        .await
        .map_err(|e| format!("escpos output open failed: {}", e))?;
    let header = format!(
        "\n--- {} request_id={} action={} ---\n",
        chrono::Utc::now().to_rfc3339(),
        request_id.unwrap_or("-"),
        action_type
    );
    f.write_all(header.as_bytes())
        .await
        .map_err(|e| format!("escpos output header write failed: {}", e))?;
    f.write_all(&bytes)
        .await
        .map_err(|e| format!("escpos output write failed: {}", e))?;
    f.write_all(b"\n")
        .await
        .map_err(|e| format!("escpos output newline failed: {}", e))?;
    Ok(())
}

async fn print_to_windows_share(bytes: &[u8], share: &str) -> Result<(), String> {
    #[cfg(not(windows))]
    {
        let _ = bytes;
        let _ = share;
        return Err("windows printer share is only supported on Windows edge runtime".to_string());
    }

    #[cfg(windows)]
    {
        let share = normalize_windows_share(share)
            .ok_or_else(|| format!("invalid windows share path '{}'", share))?;
        let job_file = std::env::temp_dir().join(format!(
            "ifascada-escpos-{}.bin",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ));
        tokio::fs::write(&job_file, bytes)
            .await
            .map_err(|e| format!("failed to write temp print job: {}", e))?;
        let output = Command::new("cmd")
            .arg("/C")
            .arg("copy")
            .arg("/B")
            .arg(job_file.to_string_lossy().to_string())
            .arg(share.clone())
            .output()
            .await
            .map_err(|e| format!("failed to execute windows copy to printer share: {}", e))?;
        let _ = tokio::fs::remove_file(&job_file).await;
        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            Err(format!(
                "windows share print failed (status={}) share='{}' stderr='{}' stdout='{}'",
                output.status,
                share,
                stderr.trim(),
                stdout.trim()
            ))
        }
    }
}

async fn check_windows_share_access(share: &str, timeout_ms: u64) -> Result<(), String> {
    #[cfg(not(windows))]
    {
        let _ = share;
        let _ = timeout_ms;
        return Err("windows printer share check is only supported on Windows edge runtime".to_string());
    }

    #[cfg(windows)]
    {
        let share = normalize_windows_share(share)
            .ok_or_else(|| format!("invalid windows share path '{}'", share))?;
        let check_cmd = format!("if exist \"{}\" (exit /b 0) else (exit /b 1)", share);
        let run = Command::new("cmd").arg("/C").arg(check_cmd).output();
        let out = tokio::time::timeout(Duration::from_millis(timeout_ms.max(4000)), run)
            .await
            .map_err(|_| format!("connection.check timeout for windows share '{}' after {} ms", share, timeout_ms))?
            .map_err(|e| format!("failed to run windows share check: {}", e))?;
        if out.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&out.stderr);
            let stdout = String::from_utf8_lossy(&out.stdout);
            Err(format!(
                "windows share check failed '{}' stderr='{}' stdout='{}'",
                share,
                stderr.trim(),
                stdout.trim()
            ))
        }
    }
}

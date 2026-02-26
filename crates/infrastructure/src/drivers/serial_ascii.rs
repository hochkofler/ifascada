use application::runtime::DriverFactory;
use async_trait::async_trait;
use domain::connection::Connection;
use domain::driver::{ConnectionState, DriverConnection};
use domain::error::DomainError;
use domain::id::TagId;
use domain::tag::TagValue;
use regex::Regex;
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use std::time::Instant;
use tokio::io::AsyncReadExt;
use tokio::sync::Mutex;
use tokio::time::{Duration, timeout};
use tokio_serial::{DataBits, Parity, SerialPortBuilderExt, SerialStream, StopBits};
use tracing::{info, warn};

#[derive(Debug, Clone, Deserialize)]
struct SerialAsciiConfig {
    #[serde(default)]
    serial: Option<SerialConfig>,
    #[serde(default)]
    frame: FrameConfig,
    #[serde(default)]
    parser: ParserConfig,
    #[serde(default)]
    mock: Option<MockConfig>,
    tag_map: HashMap<String, String>,
    #[serde(default = "default_read_timeout_ms")]
    read_timeout_ms: u64,
}

#[derive(Debug, Clone, Deserialize)]
struct SerialConfig {
    port: String,
    #[serde(default = "default_baud_rate")]
    baud_rate: u32,
    #[serde(default = "default_data_bits")]
    data_bits: u8,
    #[serde(default = "default_stop_bits")]
    stop_bits: u8,
    #[serde(default = "default_parity")]
    parity: String,
}

#[derive(Debug, Clone, Deserialize)]
struct FrameConfig {
    #[serde(default = "default_frame_mode")]
    mode: String,
    #[serde(default = "default_terminator")]
    terminator: String,
    #[serde(default = "default_max_frame_len")]
    max_len: usize,
}

impl Default for FrameConfig {
    fn default() -> Self {
        Self {
            mode: default_frame_mode(),
            terminator: default_terminator(),
            max_len: default_max_frame_len(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct ParserConfig {
    #[serde(default = "default_scale_regex")]
    regex: String,
    #[serde(default = "default_sign_group")]
    sign_group: usize,
    #[serde(default = "default_value_group")]
    value_group: usize,
    #[serde(default = "default_unit_group")]
    unit_group: usize,
}

#[derive(Debug, Clone, Deserialize)]
struct MockConfig {
    #[serde(default)]
    enabled: bool,
    #[serde(default)]
    frames: Vec<String>,
    #[serde(default = "default_mock_interval_ms")]
    interval_ms: u64,
}

impl Default for ParserConfig {
    fn default() -> Self {
        Self {
            regex: default_scale_regex(),
            sign_group: default_sign_group(),
            value_group: default_value_group(),
            unit_group: default_unit_group(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SerialOutputMode {
    Compound,
    Value,
    Unit,
    Raw,
}

#[derive(Debug, Clone)]
struct ScaleSample {
    value: f64,
    unit: String,
    raw: String,
}

#[derive(Debug, Clone)]
struct MockRuntimeState {
    frames: Vec<String>,
    interval: Duration,
    cursor: usize,
    next_emit_at: Option<Instant>,
}

fn default_baud_rate() -> u32 {
    9_600
}
fn default_data_bits() -> u8 {
    8
}
fn default_stop_bits() -> u8 {
    1
}
fn default_parity() -> String {
    "N".to_string()
}
fn default_frame_mode() -> String {
    "line".to_string()
}
fn default_terminator() -> String {
    "\r\n".to_string()
}
fn default_max_frame_len() -> usize {
    128
}
fn default_scale_regex() -> String {
    r"^\s*([+-])?\s*(\d+(?:\.\d+)?)\s*([A-Za-z]+)\s*$".to_string()
}
fn default_sign_group() -> usize {
    1
}
fn default_value_group() -> usize {
    2
}
fn default_unit_group() -> usize {
    3
}
fn default_read_timeout_ms() -> u64 {
    20
}
fn default_mock_interval_ms() -> u64 {
    500
}

pub struct SerialAsciiDriver {
    state: ConnectionState,
    cfg: Option<SerialAsciiConfig>,
    parser_regex: Option<Regex>,
    outputs: Vec<(TagId, SerialOutputMode)>,
    port: Option<Mutex<SerialStream>>,
    mock_state: Option<MockRuntimeState>,
    read_buffer: Vec<u8>,
    init_error: Option<String>,
}

impl SerialAsciiDriver {
    fn from_connection(connection: &Connection) -> Self {
        let parsed_cfg = serde_json::from_value::<SerialAsciiConfig>(connection.config.transport.clone())
            .map_err(|e| {
                DomainError::ConfigurationError(format!("invalid SerialAscii transport config: {}", e))
            });

        let (cfg, parser_regex, outputs, init_error) = match parsed_cfg {
            Ok(cfg) => {
                let regex = match Regex::new(&cfg.parser.regex) {
                    Ok(r) => r,
                    Err(e) => {
                        return Self {
                            state: ConnectionState::Disconnected,
                            cfg: None,
                            parser_regex: None,
                            outputs: Vec::new(),
                            port: None,
                            mock_state: None,
                            read_buffer: Vec::new(),
                            init_error: Some(format!("invalid serial parser regex: {}", e)),
                        };
                    }
                };
                match build_output_map(&cfg.tag_map) {
                    Ok(outputs) => (Some(cfg), Some(regex), outputs, None),
                    Err(e) => (None, None, Vec::new(), Some(e.to_string())),
                }
            }
            Err(e) => (None, None, Vec::new(), Some(e.to_string())),
        };

        Self {
            state: ConnectionState::Disconnected,
            cfg,
            parser_regex,
            outputs,
            port: None,
            mock_state: None,
            read_buffer: Vec::new(),
            init_error,
        }
    }
}

#[async_trait]
impl DriverConnection for SerialAsciiDriver {
    async fn connect(&mut self) -> Result<(), DomainError> {
        if let Some(msg) = &self.init_error {
            self.state = ConnectionState::Failed;
            return Err(DomainError::ConfigurationError(msg.clone()));
        }
        let cfg = self
            .cfg
            .as_ref()
            .ok_or_else(|| DomainError::ConfigurationError("serial ascii config unavailable".to_string()))?;

        if let Some(mock) = cfg.mock.as_ref() {
            if mock.enabled {
                if mock.frames.is_empty() {
                    self.state = ConnectionState::Failed;
                    return Err(DomainError::ConfigurationError(
                        "serial mock enabled but frames is empty".to_string(),
                    ));
                }
                self.mock_state = Some(MockRuntimeState {
                    frames: mock.frames.clone(),
                    interval: Duration::from_millis(mock.interval_ms.max(1)),
                    cursor: 0,
                    next_emit_at: None,
                });
                info!(
                    "serial-ascii connected in mock mode (frames={}, interval_ms={})",
                    mock.frames.len(),
                    mock.interval_ms.max(1)
                );
                self.state = ConnectionState::Connected;
                return Ok(());
            }
        }

        if !cfg.frame.mode.eq_ignore_ascii_case("line") {
            self.state = ConnectionState::Failed;
            return Err(DomainError::ConfigurationError(format!(
                "unsupported serial frame mode '{}'",
                cfg.frame.mode
            )));
        }

        let serial = cfg.serial.as_ref().ok_or_else(|| {
            DomainError::ConfigurationError(
                "serial config is required when mock mode is disabled".to_string(),
            )
        })?;

        let parity = match serial.parity.to_ascii_uppercase().as_str() {
            "N" => Parity::None,
            "E" => Parity::Even,
            "O" => Parity::Odd,
            other => {
                self.state = ConnectionState::Failed;
                return Err(DomainError::ConfigurationError(format!(
                    "unsupported serial parity '{}'",
                    other
                )));
            }
        };
        let data_bits = match serial.data_bits {
            5 => DataBits::Five,
            6 => DataBits::Six,
            7 => DataBits::Seven,
            8 => DataBits::Eight,
            other => {
                self.state = ConnectionState::Failed;
                return Err(DomainError::ConfigurationError(format!(
                    "unsupported serial data_bits '{}'",
                    other
                )));
            }
        };
        let stop_bits = match serial.stop_bits {
            1 => StopBits::One,
            2 => StopBits::Two,
            other => {
                self.state = ConnectionState::Failed;
                return Err(DomainError::ConfigurationError(format!(
                    "unsupported serial stop_bits '{}'",
                    other
                )));
            }
        };

        self.state = ConnectionState::Connecting;
        let builder = tokio_serial::new(&serial.port, serial.baud_rate)
            .parity(parity)
            .data_bits(data_bits)
            .stop_bits(stop_bits);
        let port = builder.open_native_async().map_err(|e| {
            self.state = ConnectionState::Failed;
            DomainError::DriverError(format!("failed opening serial port: {}", e))
        })?;
        self.port = Some(Mutex::new(port));
        self.mock_state = None;
        self.state = ConnectionState::Connected;
        info!(
            "serial-ascii connected on {} (baud={}, data_bits={}, stop_bits={}, parity={})",
            serial.port, serial.baud_rate, serial.data_bits, serial.stop_bits, serial.parity
        );
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<(), DomainError> {
        self.port = None;
        self.mock_state = None;
        self.state = ConnectionState::Disconnected;
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.state == ConnectionState::Connected
    }

    fn state(&self) -> ConnectionState {
        self.state
    }

    async fn poll(&mut self) -> Result<Vec<(TagId, Result<TagValue, DomainError>)>, DomainError> {
        if !self.is_connected() {
            return Err(DomainError::DriverError("serial ascii not connected".to_string()));
        }
        let cfg = self
            .cfg
            .as_ref()
            .ok_or_else(|| DomainError::ConfigurationError("serial ascii config unavailable".to_string()))?;
        let regex = self
            .parser_regex
            .as_ref()
            .ok_or_else(|| DomainError::ConfigurationError("serial parser regex unavailable".to_string()))?;
        if let Some(mock) = self.mock_state.as_mut() {
            let now = Instant::now();
            if let Some(next) = mock.next_emit_at {
                if now < next {
                    return Ok(Vec::new());
                }
            }
            let frame = mock.frames[mock.cursor % mock.frames.len()].clone();
            mock.cursor = mock.cursor.saturating_add(1);
            mock.next_emit_at = Some(now + mock.interval);
            return Ok(map_line_to_outputs(&self.outputs, regex, &cfg.parser, &frame));
        }

        let port = self
            .port
            .as_ref()
            .ok_or_else(|| DomainError::DriverError("serial port missing".to_string()))?;

        let mut locked = port.lock().await;
        let mut tmp = [0u8; 256];
        match timeout(
            Duration::from_millis(cfg.read_timeout_ms.max(1)),
            locked.read(&mut tmp),
        )
        .await
        {
            Ok(Ok(n)) if n > 0 => {
                self.read_buffer.extend_from_slice(&tmp[..n]);
            }
            Ok(Ok(_)) => {}
            Ok(Err(e)) => {
                return Err(DomainError::DriverError(format!(
                    "serial read failed: {}",
                    e
                )));
            }
            Err(_) => {}
        }
        drop(locked);

        let mut out = Vec::new();
        let term = cfg.frame.terminator.as_bytes();
        if term.is_empty() {
            return Err(DomainError::ConfigurationError(
                "serial frame terminator cannot be empty".to_string(),
            ));
        }

        while let Some((idx, consumed)) = find_line_boundary(&self.read_buffer, term) {
            let mut frame = self
                .read_buffer
                .drain(..idx.saturating_add(consumed))
                .collect::<Vec<u8>>();
            frame.truncate(idx);
            if frame.is_empty() {
                continue;
            }
            if frame.len() > cfg.frame.max_len {
                warn!("serial frame exceeds max_len, dropping frame");
                continue;
            }
            let raw_line = match String::from_utf8(frame) {
                Ok(s) => s.trim().to_string(),
                Err(e) => {
                    warn!("serial frame is not valid utf8: {}", e);
                    continue;
                }
            };
            if raw_line.is_empty() {
                continue;
            }

            out.extend(map_line_to_outputs(
                &self.outputs,
                regex,
                &cfg.parser,
                &raw_line,
            ));
        }

        Ok(out)
    }

    async fn write(&mut self, _tag_id: TagId, _value: TagValue) -> Result<(), DomainError> {
        Err(DomainError::DriverError(
            "serial ascii driver is read-only".to_string(),
        ))
    }
}

fn parse_output_mode(source: &str) -> Result<SerialOutputMode, DomainError> {
    let normalized = source.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "scale:compound" | "compound" => Ok(SerialOutputMode::Compound),
        "scale:value" | "value" => Ok(SerialOutputMode::Value),
        "scale:unit" | "unit" => Ok(SerialOutputMode::Unit),
        "scale:raw" | "raw" => Ok(SerialOutputMode::Raw),
        other => Err(DomainError::ConfigurationError(format!(
            "unsupported serial output mode '{}'",
            other
        ))),
    }
}

fn build_output_map(
    tag_map: &HashMap<String, String>,
) -> Result<Vec<(TagId, SerialOutputMode)>, DomainError> {
    if tag_map.is_empty() {
        return Err(DomainError::ConfigurationError(
            "serial tag_map cannot be empty".to_string(),
        ));
    }
    let mut out = Vec::with_capacity(tag_map.len());
    for (tag_id, source) in tag_map {
        out.push((TagId::new(tag_id), parse_output_mode(source)?));
    }
    Ok(out)
}

fn parse_scale_line(regex: &Regex, parser: &ParserConfig, line: &str) -> Result<ScaleSample, DomainError> {
    let caps = regex.captures(line).ok_or_else(|| {
        DomainError::DriverError("scale line does not match parser regex".to_string())
    })?;
    let sign = caps
        .get(parser.sign_group)
        .map(|m| m.as_str().trim())
        .unwrap_or("");
    let value_str = caps
        .get(parser.value_group)
        .map(|m| m.as_str())
        .ok_or_else(|| DomainError::DriverError("value capture group missing".to_string()))?;
    let unit = caps
        .get(parser.unit_group)
        .map(|m| m.as_str().trim().to_string())
        .ok_or_else(|| DomainError::DriverError("unit capture group missing".to_string()))?;
    let mut value = value_str.parse::<f64>().map_err(|_| {
        DomainError::DriverError(format!("invalid scale numeric value '{}'", value_str))
    })?;
    if sign == "-" {
        value = -value;
    }
    Ok(ScaleSample {
        value,
        unit,
        raw: line.to_string(),
    })
}

fn map_line_to_outputs(
    outputs: &[(TagId, SerialOutputMode)],
    regex: &Regex,
    parser: &ParserConfig,
    raw_line: &str,
) -> Vec<(TagId, Result<TagValue, DomainError>)> {
    let mut out = Vec::new();
    for (tag_id, mode) in outputs {
        if *mode == SerialOutputMode::Raw {
            out.push((tag_id.clone(), Ok(TagValue::String(raw_line.to_string()))));
        }
    }

    let parsed = match parse_scale_line(regex, parser, raw_line) {
        Ok(v) => v,
        Err(e) => {
            warn!("failed to parse scale line '{}': {}", raw_line, e);
            return out;
        }
    };
    for (tag_id, mode) in outputs {
        if *mode == SerialOutputMode::Raw {
            continue;
        }
        out.push((tag_id.clone(), Ok(build_output_value(*mode, &parsed))));
    }
    out
}

fn build_output_value(mode: SerialOutputMode, sample: &ScaleSample) -> TagValue {
    match mode {
        SerialOutputMode::Compound => TagValue::String(
            json!({
                "value": sample.value,
                "unit": sample.unit,
                "raw": sample.raw
            })
            .to_string(),
        ),
        SerialOutputMode::Value => TagValue::Float(sample.value),
        SerialOutputMode::Unit => TagValue::String(sample.unit.clone()),
        SerialOutputMode::Raw => TagValue::String(sample.raw.clone()),
    }
}

fn find_subsequence(buf: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || buf.len() < needle.len() {
        return None;
    }
    buf.windows(needle.len()).position(|w| w == needle)
}

fn find_line_boundary(buf: &[u8], configured_term: &[u8]) -> Option<(usize, usize)> {
    if let Some(idx) = find_subsequence(buf, configured_term) {
        return Some((idx, configured_term.len()));
    }
    if let Some(idx) = find_subsequence(buf, b"\r\n") {
        return Some((idx, 2));
    }
    if let Some(idx) = find_subsequence(buf, b"\n") {
        return Some((idx, 1));
    }
    if let Some(idx) = find_subsequence(buf, b"\r") {
        return Some((idx, 1));
    }
    None
}

pub struct SerialAsciiFactory;

impl DriverFactory for SerialAsciiFactory {
    fn create(&self, connection: &Connection) -> Box<dyn DriverConnection> {
        Box::new(SerialAsciiDriver::from_connection(connection))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_output_mode_compound() {
        assert_eq!(
            parse_output_mode("scale:compound").unwrap(),
            SerialOutputMode::Compound
        );
        assert_eq!(parse_output_mode("unit").unwrap(), SerialOutputMode::Unit);
    }

    #[test]
    fn test_parse_scale_line_plus_value() {
        let re = Regex::new(&default_scale_regex()).unwrap();
        let p = ParserConfig::default();
        let s = parse_scale_line(&re, &p, "+ 12.4354 g").unwrap();
        assert!((s.value - 12.4354).abs() < 1e-9);
        assert_eq!(s.unit, "g");
    }

    #[test]
    fn test_parse_scale_line_negative_value() {
        let re = Regex::new(&default_scale_regex()).unwrap();
        let p = ParserConfig::default();
        let s = parse_scale_line(&re, &p, "- 1.500 kg").unwrap();
        assert!((s.value + 1.5).abs() < 1e-9);
        assert_eq!(s.unit, "kg");
    }

    #[test]
    fn test_parse_scale_line_without_sign_and_many_spaces() {
        let re = Regex::new(&default_scale_regex()).unwrap();
        let p = ParserConfig::default();
        let s = parse_scale_line(&re, &p, "   12.4354      g   ").unwrap();
        assert!((s.value - 12.4354).abs() < 1e-9);
        assert_eq!(s.unit, "g");
    }

    #[test]
    fn test_parse_scale_line_zero_or_more_spaces_between_parts() {
        let re = Regex::new(&default_scale_regex()).unwrap();
        let p = ParserConfig::default();
        let a = parse_scale_line(&re, &p, "+12.0000g").unwrap();
        let b = parse_scale_line(&re, &p, "+   12.0000   g").unwrap();
        assert!((a.value - 12.0).abs() < 1e-9);
        assert!((b.value - 12.0).abs() < 1e-9);
        assert_eq!(a.unit, "g");
        assert_eq!(b.unit, "g");
    }

    #[test]
    fn test_build_output_value_compound_is_json() {
        let sample = ScaleSample {
            value: 10.5,
            unit: "g".to_string(),
            raw: "+ 10.5 g".to_string(),
        };
        let v = build_output_value(SerialOutputMode::Compound, &sample);
        match v {
            TagValue::String(s) => {
                let obj: serde_json::Value = serde_json::from_str(&s).unwrap();
                assert_eq!(obj["value"], 10.5);
                assert_eq!(obj["unit"], "g");
                assert_eq!(obj["raw"], "+ 10.5 g");
            }
            _ => panic!("expected string"),
        }
    }

    #[test]
    fn test_find_line_boundary_fallback_newline() {
        let buf = b"+ 12.4354 g\n";
        let b = find_line_boundary(buf, b"\r\n").expect("boundary");
        assert_eq!(b.0, 11);
        assert_eq!(b.1, 1);
    }

    #[test]
    fn test_find_line_boundary_fallback_carriage_return() {
        let buf = b"+ 12.4354 g\r";
        let b = find_line_boundary(buf, b"\r\n").expect("boundary");
        assert_eq!(b.0, 11);
        assert_eq!(b.1, 1);
    }

    #[test]
    fn test_map_line_to_outputs_emits_raw_even_if_parse_fails() {
        let outputs = vec![
            (TagId::new("tag_scale_raw"), SerialOutputMode::Raw),
            (TagId::new("tag_scale_compound"), SerialOutputMode::Compound),
        ];
        let re = Regex::new(&default_scale_regex()).unwrap();
        let parser = ParserConfig::default();
        let out = map_line_to_outputs(&outputs, &re, &parser, "INVALID FRAME");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].0, TagId::new("tag_scale_raw"));
    }
}

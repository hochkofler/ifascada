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
use tokio::time::{timeout, Duration};
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
    #[serde(default)]
    start_regex: Option<String>,
    #[serde(default)]
    end_regex: Option<String>,
}

impl Default for FrameConfig {
    fn default() -> Self {
        Self {
            mode: default_frame_mode(),
            terminator: default_terminator(),
            max_len: default_max_frame_len(),
            start_regex: None,
            end_regex: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct ParserConfig {
    #[serde(default = "default_parser_version")]
    version: u8,
    #[serde(default = "default_scale_regex")]
    regex: String,
    #[serde(default = "default_sign_group")]
    sign_group: usize,
    #[serde(default = "default_value_group")]
    value_group: usize,
    #[serde(default = "default_unit_group")]
    unit_group: usize,
    #[serde(default)]
    fields: HashMap<String, FieldParserConfig>,
}

#[derive(Debug, Clone, Deserialize)]
struct FieldParserConfig {
    regex: String,
    #[serde(default = "default_value_group")]
    value_group: usize,
    #[serde(default)]
    unit_group: Option<usize>,
    #[serde(default = "default_field_value_type")]
    value_type: String,
    #[serde(default)]
    required: bool,
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
            version: default_parser_version(),
            regex: default_scale_regex(),
            sign_group: default_sign_group(),
            value_group: default_value_group(),
            unit_group: default_unit_group(),
            fields: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SerialOutputMode {
    Compound,
    Value,
    Unit,
    Raw,
    Field(String),
}

#[derive(Debug, Clone)]
struct ScaleSample {
    value: f64,
    unit: String,
    raw: String,
}

#[derive(Debug)]
enum CompiledParser {
    Legacy(Regex),
    Fields(HashMap<String, CompiledField>),
}

#[derive(Debug)]
enum CompiledFrame {
    Line,
    Block { start: Regex, end: Regex },
}

#[derive(Debug)]
struct CompiledField {
    regex: Regex,
    value_group: usize,
    unit_group: Option<usize>,
    value_type: FieldValueType,
    required: bool,
}

#[derive(Debug, Clone, Copy)]
enum FieldValueType {
    Float,
    Integer,
    String,
    Boolean,
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

fn default_parser_version() -> u8 {
    1
}

fn default_field_value_type() -> String {
    "float".to_string()
}

pub struct SerialAsciiDriver {
    state: ConnectionState,
    cfg: Option<SerialAsciiConfig>,
    parser: Option<CompiledParser>,
    frame: Option<CompiledFrame>,
    outputs: Vec<(TagId, SerialOutputMode)>,
    port: Option<Mutex<SerialStream>>,
    mock_state: Option<MockRuntimeState>,
    read_buffer: Vec<u8>,
    init_error: Option<String>,
}

impl SerialAsciiDriver {
    fn from_connection(connection: &Connection) -> Self {
        let parsed_cfg =
            serde_json::from_value::<SerialAsciiConfig>(connection.config.transport.clone())
                .map_err(|e| {
                    DomainError::ConfigurationError(format!(
                        "invalid SerialAscii transport config: {}",
                        e
                    ))
                });

        let (cfg, parser, frame, outputs, init_error) = match parsed_cfg {
            Ok(cfg) => {
                let frame = match compile_frame(&cfg.frame) {
                    Ok(frame) => frame,
                    Err(e) => {
                        return Self {
                            state: ConnectionState::Disconnected,
                            cfg: None,
                            parser: None,
                            frame: None,
                            outputs: Vec::new(),
                            port: None,
                            mock_state: None,
                            read_buffer: Vec::new(),
                            init_error: Some(e.to_string()),
                        };
                    }
                };
                let parser = match compile_parser(&cfg.parser) {
                    Ok(parser) => parser,
                    Err(e) => {
                        return Self {
                            state: ConnectionState::Disconnected,
                            cfg: None,
                            parser: None,
                            frame: None,
                            outputs: Vec::new(),
                            port: None,
                            mock_state: None,
                            read_buffer: Vec::new(),
                            init_error: Some(e.to_string()),
                        };
                    }
                };
                match build_output_map(&cfg.tag_map) {
                    Ok(outputs) => match validate_output_map(&outputs, &parser) {
                        Ok(()) => (Some(cfg), Some(parser), Some(frame), outputs, None),
                        Err(e) => (None, None, None, Vec::new(), Some(e.to_string())),
                    },
                    Err(e) => (None, None, None, Vec::new(), Some(e.to_string())),
                }
            }
            Err(e) => (None, None, None, Vec::new(), Some(e.to_string())),
        };

        Self {
            state: ConnectionState::Disconnected,
            cfg,
            parser,
            frame,
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
        let cfg = self.cfg.as_ref().ok_or_else(|| {
            DomainError::ConfigurationError("serial ascii config unavailable".to_string())
        })?;

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
            return Err(DomainError::DriverError(
                "serial ascii not connected".to_string(),
            ));
        }
        let cfg = self.cfg.as_ref().ok_or_else(|| {
            DomainError::ConfigurationError("serial ascii config unavailable".to_string())
        })?;
        let parser = self.parser.as_ref().ok_or_else(|| {
            DomainError::ConfigurationError("serial parser unavailable".to_string())
        })?;
        let framing = self.frame.as_ref().ok_or_else(|| {
            DomainError::ConfigurationError("serial frame configuration unavailable".to_string())
        })?;
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
            return Ok(map_frame_to_outputs(
                &self.outputs,
                parser,
                &cfg.parser,
                &frame,
            ));
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
        for frame in drain_serial_frames(&mut self.read_buffer, framing, &cfg.frame) {
            let raw_frame = match String::from_utf8(frame) {
                Ok(s) => match framing {
                    CompiledFrame::Line => s.trim().to_string(),
                    CompiledFrame::Block { .. } => s,
                },
                Err(e) => {
                    warn!("serial frame is not valid utf8: {}", e);
                    continue;
                }
            };
            if raw_frame.is_empty() {
                continue;
            }

            out.extend(map_frame_to_outputs(
                &self.outputs,
                parser,
                &cfg.parser,
                &raw_frame,
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
    if let Some(name) = normalized.strip_prefix("field:") {
        if !name.is_empty() {
            return Ok(SerialOutputMode::Field(name.to_string()));
        }
    }
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

fn compile_frame(config: &FrameConfig) -> Result<CompiledFrame, DomainError> {
    match config.mode.trim().to_ascii_lowercase().as_str() {
        "line" => {
            if config.terminator.is_empty() {
                return Err(DomainError::ConfigurationError(
                    "serial frame terminator cannot be empty".to_string(),
                ));
            }
            Ok(CompiledFrame::Line)
        }
        "block" => {
            let start_pattern = config.start_regex.as_deref().ok_or_else(|| {
                DomainError::ConfigurationError(
                    "serial block frame requires start_regex".to_string(),
                )
            })?;
            let end_pattern = config.end_regex.as_deref().ok_or_else(|| {
                DomainError::ConfigurationError("serial block frame requires end_regex".to_string())
            })?;
            if start_pattern.trim().is_empty() {
                return Err(DomainError::ConfigurationError(
                    "serial block start_regex cannot be empty".to_string(),
                ));
            }
            if end_pattern.trim().is_empty() {
                return Err(DomainError::ConfigurationError(
                    "serial block end_regex cannot be empty".to_string(),
                ));
            }
            let start = Regex::new(start_pattern).map_err(|e| {
                DomainError::ConfigurationError(format!("invalid serial block start_regex: {}", e))
            })?;
            let end = Regex::new(end_pattern).map_err(|e| {
                DomainError::ConfigurationError(format!("invalid serial block end_regex: {}", e))
            })?;
            Ok(CompiledFrame::Block { start, end })
        }
        other => Err(DomainError::ConfigurationError(format!(
            "unsupported serial frame mode '{}'",
            other
        ))),
    }
}

fn drain_serial_frames(
    buffer: &mut Vec<u8>,
    frame: &CompiledFrame,
    config: &FrameConfig,
) -> Vec<Vec<u8>> {
    let mut frames = Vec::new();
    match frame {
        CompiledFrame::Line => {
            let term = config.terminator.as_bytes();
            while let Some((idx, consumed)) = find_line_boundary(buffer, term) {
                let mut value = buffer
                    .drain(..idx.saturating_add(consumed))
                    .collect::<Vec<u8>>();
                value.truncate(idx);
                if value.len() <= config.max_len && !value.is_empty() {
                    frames.push(value);
                } else if value.len() > config.max_len {
                    warn!("serial frame exceeds max_len, dropping frame");
                }
            }
        }
        CompiledFrame::Block { start, end } => {
            while let Some(value) = extract_block_frame(buffer, start, end, config.max_len) {
                if !value.is_empty() {
                    frames.push(value);
                }
            }
        }
    }
    frames
}

fn compile_parser(config: &ParserConfig) -> Result<CompiledParser, DomainError> {
    match config.version {
        1 => Regex::new(&config.regex)
            .map(CompiledParser::Legacy)
            .map_err(|e| {
                DomainError::ConfigurationError(format!("invalid serial parser regex: {}", e))
            }),
        2 => {
            if config.fields.is_empty() {
                return Err(DomainError::ConfigurationError(
                    "serial parser version 2 requires at least one field".to_string(),
                ));
            }
            let mut fields = HashMap::with_capacity(config.fields.len());
            for (name, field) in &config.fields {
                let regex = Regex::new(&field.regex).map_err(|e| {
                    DomainError::ConfigurationError(format!(
                        "invalid regex for serial parser field '{}': {}",
                        name, e
                    ))
                })?;
                validate_capture_group(name, "value_group", field.value_group, &regex)?;
                if let Some(group) = field.unit_group {
                    validate_capture_group(name, "unit_group", group, &regex)?;
                }
                let value_type = match field.value_type.trim().to_ascii_lowercase().as_str() {
                    "float" => FieldValueType::Float,
                    "integer" => FieldValueType::Integer,
                    "string" => FieldValueType::String,
                    "boolean" => FieldValueType::Boolean,
                    other => {
                        return Err(DomainError::ConfigurationError(format!(
                            "unsupported value_type '{}' for serial parser field '{}'",
                            other, name
                        )));
                    }
                };
                fields.insert(
                    name.to_ascii_lowercase(),
                    CompiledField {
                        regex,
                        value_group: field.value_group,
                        unit_group: field.unit_group,
                        value_type,
                        required: field.required,
                    },
                );
            }
            Ok(CompiledParser::Fields(fields))
        }
        version => Err(DomainError::ConfigurationError(format!(
            "unsupported serial parser version '{}'",
            version
        ))),
    }
}

fn validate_capture_group(
    field_name: &str,
    setting_name: &str,
    group: usize,
    regex: &Regex,
) -> Result<(), DomainError> {
    if group >= regex.captures_len() {
        return Err(DomainError::ConfigurationError(format!(
            "{} {} does not exist for serial parser field '{}'",
            setting_name, group, field_name
        )));
    }
    Ok(())
}

fn validate_output_map(
    outputs: &[(TagId, SerialOutputMode)],
    parser: &CompiledParser,
) -> Result<(), DomainError> {
    for (_, output) in outputs {
        match (parser, output) {
            (CompiledParser::Legacy(_), SerialOutputMode::Field(name)) => {
                return Err(DomainError::ConfigurationError(format!(
                    "serial parser version 1 does not define field '{}'",
                    name
                )));
            }
            (CompiledParser::Fields(fields), SerialOutputMode::Field(name))
                if !fields.contains_key(name) =>
            {
                return Err(DomainError::ConfigurationError(format!(
                    "serial tag_map references unknown field '{}'",
                    name
                )));
            }
            (CompiledParser::Fields(_), SerialOutputMode::Raw | SerialOutputMode::Field(_)) => {}
            (CompiledParser::Fields(_), _) => {
                return Err(DomainError::ConfigurationError(
                    "serial parser version 2 tag_map supports only 'field:<name>' and 'raw'"
                        .to_string(),
                ));
            }
            (CompiledParser::Legacy(_), _) => {}
        }
    }
    Ok(())
}

fn map_frame_to_outputs(
    outputs: &[(TagId, SerialOutputMode)],
    parser: &CompiledParser,
    config: &ParserConfig,
    raw_frame: &str,
) -> Vec<(TagId, Result<TagValue, DomainError>)> {
    match parser {
        CompiledParser::Legacy(regex) => map_line_to_outputs(outputs, regex, config, raw_frame),
        CompiledParser::Fields(fields) => map_fields_to_outputs(outputs, fields, raw_frame),
    }
}

fn map_fields_to_outputs(
    outputs: &[(TagId, SerialOutputMode)],
    fields: &HashMap<String, CompiledField>,
    raw_frame: &str,
) -> Vec<(TagId, Result<TagValue, DomainError>)> {
    let mut out = Vec::with_capacity(outputs.len());
    let searchable_frame = raw_frame.replace("\r\n", "\n").replace('\r', "\n");
    for (tag_id, output) in outputs {
        match output {
            SerialOutputMode::Raw => {
                out.push((tag_id.clone(), Ok(TagValue::String(raw_frame.to_string()))))
            }
            SerialOutputMode::Field(name) => {
                let field = &fields[name];
                let Some(captures) = field.regex.captures(&searchable_frame) else {
                    if field.required {
                        out.push((
                            tag_id.clone(),
                            Err(DomainError::DriverError(format!(
                                "required serial parser field '{}' was not found",
                                name
                            ))),
                        ));
                    }
                    continue;
                };
                let matched = captures.get(0).map(|m| m.as_str().trim()).unwrap_or("");
                let value = match parse_field_value(name, field, &captures) {
                    Ok(value) => value,
                    Err(error) => {
                        out.push((tag_id.clone(), Err(error)));
                        continue;
                    }
                };
                let unit = field
                    .unit_group
                    .and_then(|group| captures.get(group))
                    .map(|capture| capture.as_str().trim())
                    .unwrap_or("");
                out.push((
                    tag_id.clone(),
                    Ok(TagValue::String(
                        json!({"value": value, "unit": unit, "raw": matched}).to_string(),
                    )),
                ));
            }
            _ => unreachable!("version 2 output map was validated during initialization"),
        }
    }
    out
}

fn parse_field_value(
    name: &str,
    field: &CompiledField,
    captures: &regex::Captures<'_>,
) -> Result<serde_json::Value, DomainError> {
    let raw = captures
        .get(field.value_group)
        .map(|capture| capture.as_str().trim())
        .ok_or_else(|| {
            DomainError::DriverError(format!(
                "value capture group missing for serial parser field '{}'",
                name
            ))
        })?;
    match field.value_type {
        FieldValueType::Float => raw.parse::<f64>().map(|value| json!(value)).map_err(|_| {
            DomainError::DriverError(format!("invalid float '{}' for field '{}'", raw, name))
        }),
        FieldValueType::Integer => raw.parse::<i64>().map(|value| json!(value)).map_err(|_| {
            DomainError::DriverError(format!("invalid integer '{}' for field '{}'", raw, name))
        }),
        FieldValueType::String => Ok(json!(raw)),
        FieldValueType::Boolean => raw.parse::<bool>().map(|value| json!(value)).map_err(|_| {
            DomainError::DriverError(format!("invalid boolean '{}' for field '{}'", raw, name))
        }),
    }
}

fn parse_scale_line(
    regex: &Regex,
    parser: &ParserConfig,
    line: &str,
) -> Result<ScaleSample, DomainError> {
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
        if matches!(mode, SerialOutputMode::Field(_)) {
            out.push((
                tag_id.clone(),
                Err(DomainError::DriverError(
                    "named fields require parser version 2".to_string(),
                )),
            ));
        } else {
            out.push((
                tag_id.clone(),
                Ok(build_output_value(mode.clone(), &parsed)),
            ));
        }
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
        SerialOutputMode::Field(_) => {
            unreachable!("named fields are handled by the version 2 parser")
        }
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

fn extract_block_frame(
    buffer: &mut Vec<u8>,
    start_regex: &Regex,
    end_regex: &Regex,
    max_len: usize,
) -> Option<Vec<u8>> {
    let text = match std::str::from_utf8(buffer) {
        Ok(text) => text,
        Err(_) => {
            if buffer.len() > max_len {
                buffer.clear();
            }
            return None;
        }
    };
    let Some(start) = start_regex.find(text) else {
        if buffer.len() > max_len {
            buffer.clear();
        }
        return None;
    };
    let start_offset = start.start();
    let start_end = start.end().saturating_sub(start_offset);
    if start_offset > 0 {
        buffer.drain(..start_offset);
    }

    let text = std::str::from_utf8(buffer).ok()?;
    let Some(end) = end_regex.find_at(text, start_end) else {
        if buffer.len() > max_len {
            buffer.clear();
        }
        return None;
    };
    let mut frame_end = end.end();
    while frame_end > 0 && matches!(buffer[frame_end - 1], b'\r' | b'\n') {
        frame_end -= 1;
    }
    if frame_end > max_len {
        buffer.drain(..frame_end);
        return None;
    }
    Some(buffer.drain(..frame_end).collect())
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
    use domain::driver::DriverType;
    use domain::id::ConnectionId;

    #[test]
    fn test_parse_output_mode_compound() {
        assert_eq!(
            parse_output_mode("scale:compound").unwrap(),
            SerialOutputMode::Compound
        );
        assert_eq!(parse_output_mode("unit").unwrap(), SerialOutputMode::Unit);
    }

    #[test]
    fn test_build_output_map_accepts_named_parser_field() {
        let tag_map = HashMap::from([("tag_ph".to_string(), "field:ph".to_string())]);

        let outputs = build_output_map(&tag_map).expect("named field should be routable");

        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].0, TagId::new("tag_ph"));
        assert_eq!(outputs[0].1, SerialOutputMode::Field("ph".to_string()));
    }

    #[tokio::test]
    async fn test_configurable_fields_route_phsj5_measurements_to_independent_tags() {
        let raw_frame = "93.79mV\r\n5.25pH\r\n24.2c\r\n98.82%";
        let connection = Connection::new(
            ConnectionId::new("conn_phsj5"),
            "PHSJ-5".to_string(),
            DriverType::new("serial_ascii").unwrap(),
            json!({
                "parser": {
                    "version": 2,
                    "fields": {
                        "potential_mv": {
                            "regex": "(?m)^([+-]?[0-9]+(?:\\.[0-9]+)?)(mV)$",
                            "value_group": 1,
                            "unit_group": 2,
                            "required": true
                        },
                        "ph": {
                            "regex": "(?m)^([+-]?[0-9]+(?:\\.[0-9]+)?)(pH)$",
                            "value_group": 1,
                            "unit_group": 2,
                            "required": true
                        },
                        "temperature_c": {
                            "regex": "(?m)^([+-]?[0-9]+(?:\\.[0-9]+)?)(c)$",
                            "value_group": 1,
                            "unit_group": 2,
                            "required": true
                        },
                        "electrode_efficiency_pct": {
                            "regex": "(?m)^([+-]?[0-9]+(?:\\.[0-9]+)?)(%)$",
                            "value_group": 1,
                            "unit_group": 2,
                            "required": true
                        }
                    }
                },
                "mock": {
                    "enabled": true,
                    "frames": [raw_frame],
                    "interval_ms": 1
                },
                "tag_map": {
                    "tag_potential": "field:potential_mv",
                    "tag_ph": "field:ph",
                    "tag_temperature": "field:temperature_c",
                    "tag_efficiency": "field:electrode_efficiency_pct",
                    "tag_raw": "raw"
                }
            }),
        );
        let mut driver = SerialAsciiDriver::from_connection(&connection);
        driver.connect().await.unwrap();

        let outputs = driver.poll().await.unwrap();
        let values = outputs
            .into_iter()
            .map(|(tag_id, value)| (tag_id.to_string(), value.unwrap()))
            .collect::<HashMap<_, _>>();

        let expected = [
            ("tag_potential", 93.79, "mV", "93.79mV"),
            ("tag_ph", 5.25, "pH", "5.25pH"),
            ("tag_temperature", 24.2, "c", "24.2c"),
            ("tag_efficiency", 98.82, "%", "98.82%"),
        ];
        for (tag_id, expected_value, expected_unit, expected_raw) in expected {
            let TagValue::String(compound) = &values[tag_id] else {
                panic!("expected compound string for {tag_id}");
            };
            let parsed: serde_json::Value = serde_json::from_str(compound).unwrap();
            assert_eq!(parsed["value"], expected_value);
            assert_eq!(parsed["unit"], expected_unit);
            assert_eq!(parsed["raw"], expected_raw);
        }
        assert_eq!(values["tag_raw"], TagValue::String(raw_frame.to_string()));
    }

    #[test]
    fn test_extract_block_frame_waits_for_end_and_discards_prefix_noise() {
        let start = Regex::new(r"(?m)^[+-]?[0-9]+(?:\.[0-9]+)?mV\r?$").unwrap();
        let end = Regex::new(r"(?m)^[+-]?[0-9]+(?:\.[0-9]+)?%\r?$").unwrap();
        let mut buffer = b"noise\r\n93.79mV\r\n5.25pH\r\n24.2c\r\n".to_vec();

        assert_eq!(extract_block_frame(&mut buffer, &start, &end, 256), None);

        buffer.extend_from_slice(b"98.82%\r\nnext");
        let frame = extract_block_frame(&mut buffer, &start, &end, 256).unwrap();

        assert_eq!(
            String::from_utf8(frame).unwrap(),
            "93.79mV\r\n5.25pH\r\n24.2c\r\n98.82%"
        );
        assert_eq!(buffer, b"\r\nnext");
    }

    #[test]
    fn test_extract_block_frame_bounds_invalid_utf8_noise() {
        let start = Regex::new("START").unwrap();
        let end = Regex::new("END").unwrap();
        let mut buffer = vec![0xff; 9];

        assert_eq!(extract_block_frame(&mut buffer, &start, &end, 8), None);
        assert!(buffer.is_empty());
    }

    #[test]
    fn test_block_frame_configuration_drains_complete_phsj5_print() {
        let config: FrameConfig = serde_json::from_value(json!({
            "mode": "block",
            "start_regex": "(?m)^[+-]?[0-9]+(?:\\.[0-9]+)?mV\\r?$",
            "end_regex": "(?m)^[+-]?[0-9]+(?:\\.[0-9]+)?%\\r?$",
            "max_len": 256
        }))
        .unwrap();
        let frame = compile_frame(&config).unwrap();
        let mut buffer = b"93.79mV\r\n5.25pH\r\n24.2c\r\n98.82%\r\n".to_vec();

        let frames = drain_serial_frames(&mut buffer, &frame, &config);

        assert_eq!(
            frames,
            vec![b"93.79mV\r\n5.25pH\r\n24.2c\r\n98.82%".to_vec()]
        );
        assert_eq!(buffer, b"\r\n");
    }

    #[test]
    fn test_block_frame_rejects_empty_boundary_regex() {
        let config: FrameConfig = serde_json::from_value(json!({
            "mode": "block",
            "start_regex": "",
            "end_regex": "END"
        }))
        .unwrap();

        let error = compile_frame(&config).unwrap_err();

        assert!(error.to_string().contains("start_regex cannot be empty"));
    }

    #[test]
    fn test_missing_required_field_errors_only_its_tag_and_keeps_valid_sibling() {
        let config: ParserConfig = serde_json::from_value(json!({
            "version": 2,
            "fields": {
                "ph": {
                    "regex": "(?m)^([0-9]+(?:\\.[0-9]+)?)(pH)$",
                    "value_group": 1,
                    "unit_group": 2,
                    "required": true
                },
                "temperature_c": {
                    "regex": "(?m)^([0-9]+(?:\\.[0-9]+)?)(c)$",
                    "value_group": 1,
                    "unit_group": 2,
                    "required": true
                }
            }
        }))
        .unwrap();
        let parser = compile_parser(&config).unwrap();
        let outputs = vec![
            (
                TagId::new("tag_ph"),
                SerialOutputMode::Field("ph".to_string()),
            ),
            (
                TagId::new("tag_temperature"),
                SerialOutputMode::Field("temperature_c".to_string()),
            ),
        ];

        let values = map_frame_to_outputs(&outputs, &parser, &config, "5.25pH");

        assert_eq!(values.len(), 2);
        let (_, Ok(TagValue::String(compound))) = values
            .iter()
            .find(|(id, _)| id == &TagId::new("tag_ph"))
            .unwrap()
        else {
            panic!("expected valid pH compound");
        };
        let compound: serde_json::Value = serde_json::from_str(compound).unwrap();
        assert_eq!(compound["value"], 5.25);
        assert_eq!(compound["unit"], "pH");
        assert_eq!(compound["raw"], "5.25pH");
        assert!(values.iter().any(|(id, value)| {
            id == &TagId::new("tag_temperature")
                && matches!(value, Err(DomainError::DriverError(message)) if message.contains("required") && message.contains("temperature_c"))
        }));
    }

    #[test]
    fn test_version_two_rejects_unknown_tag_map_field_during_initialization() {
        let config: ParserConfig = serde_json::from_value(json!({
            "version": 2,
            "fields": {
                "ph": {
                    "regex": "([0-9.]+)(pH)",
                    "value_group": 1,
                    "unit_group": 2
                }
            }
        }))
        .unwrap();
        let parser = compile_parser(&config).unwrap();
        let outputs = vec![(
            TagId::new("tag_temperature"),
            SerialOutputMode::Field("temperature_c".to_string()),
        )];

        let error = validate_output_map(&outputs, &parser).unwrap_err();

        assert!(error.to_string().contains("unknown field 'temperature_c'"));
    }

    #[test]
    fn test_configurable_field_raw_is_the_trimmed_complete_match() {
        let config: ParserConfig = serde_json::from_value(json!({
            "version": 2,
            "fields": {
                "ph": {
                    "regex": "(?m)^\\s*([0-9]+(?:\\.[0-9]+)?)\\s*(pH)\\s*$",
                    "value_group": 1,
                    "unit_group": 2
                }
            }
        }))
        .unwrap();
        let parser = compile_parser(&config).unwrap();
        let outputs = vec![(
            TagId::new("tag_ph"),
            SerialOutputMode::Field("ph".to_string()),
        )];

        let values = map_frame_to_outputs(&outputs, &parser, &config, "  5.25 pH  ");
        let TagValue::String(compound) = values[0].1.as_ref().unwrap() else {
            panic!("expected compound string");
        };
        let compound: serde_json::Value = serde_json::from_str(compound).unwrap();

        assert_eq!(compound["raw"], "5.25 pH");
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

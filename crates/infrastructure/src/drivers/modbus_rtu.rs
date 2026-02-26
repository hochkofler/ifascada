use application::runtime::DriverFactory;
use async_trait::async_trait;
use crate::drivers::modbus_shared::{
    ModbusBatchPolicy, ModbusPoint, ModbusRequestPolicy, RetryBackoffStrategy,
    poll_points_batched, write_point,
};
use domain::connection::Connection;
use domain::driver::{ConnectionState, DriverConnection};
use domain::error::DomainError;
use domain::id::TagId;
use domain::tag::TagValue;
use serde::Deserialize;
use std::collections::HashMap;
use tokio_modbus::client::Context;
use tokio_modbus::client::rtu;
use tokio_modbus::prelude::Slave;
use tokio_modbus::slave::SlaveContext;
use tokio::sync::Mutex;
use tokio_serial::{DataBits, Parity, SerialPortBuilderExt, StopBits};

#[derive(Debug, Clone, Deserialize)]
struct ModbusRtuConfig {
    serial: SerialConfig,
    #[serde(default)]
    unit_id: Option<u8>,
    tag_map: HashMap<String, ModbusRtuTagBinding>,
    #[serde(default = "default_request_timeout_ms")]
    request_timeout_ms: u64,
    #[serde(default = "default_request_retries")]
    request_retries: u8,
    #[serde(default = "default_retry_backoff_ms")]
    retry_backoff_ms: u64,
    #[serde(default = "default_retry_backoff_mode")]
    retry_backoff_mode: String,
    #[serde(default = "default_retry_backoff_max_ms")]
    retry_backoff_max_ms: u64,
    #[serde(default = "default_max_batch_registers")]
    max_batch_registers: u16,
    #[serde(default = "default_max_batch_bits")]
    max_batch_bits: u16,
    #[serde(default = "default_max_register_gap")]
    max_register_gap: u16,
    #[serde(default = "default_max_bit_gap")]
    max_bit_gap: u16,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum ModbusRtuTagBinding {
    Source(String),
    Detailed {
        source: String,
        #[serde(default)]
        unit_id: Option<u8>,
        #[serde(default)]
        device_id: Option<String>,
    },
}

#[derive(Debug, Clone)]
struct ModbusRtuPointBinding {
    point: ModbusPoint,
    unit_id: u8,
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
fn default_request_timeout_ms() -> u64 {
    1_500
}
fn default_request_retries() -> u8 {
    1
}
fn default_retry_backoff_ms() -> u64 {
    100
}
fn default_retry_backoff_mode() -> String {
    "fixed".to_string()
}
fn default_retry_backoff_max_ms() -> u64 {
    2_000
}
fn default_max_batch_registers() -> u16 {
    120
}
fn default_max_batch_bits() -> u16 {
    2000
}
fn default_max_register_gap() -> u16 {
    0
}
fn default_max_bit_gap() -> u16 {
    0
}

pub struct ModbusRtuDriver {
    state: ConnectionState,
    cfg: Option<ModbusRtuConfig>,
    points: HashMap<TagId, ModbusRtuPointBinding>,
    ctx: Option<Mutex<Context>>,
    init_error: Option<String>,
}

impl ModbusRtuDriver {
    fn from_connection(connection: &Connection) -> Self {
        let parsed_cfg = serde_json::from_value::<ModbusRtuConfig>(connection.config.transport.clone())
            .map_err(|e| {
                DomainError::ConfigurationError(format!("invalid ModbusRTU transport config: {}", e))
            });

        let (cfg, points, init_error) = match parsed_cfg {
            Ok(cfg) => {
                match build_tag_point_map(&cfg) {
                    Ok(points) => (Some(cfg), points, None),
                    Err(e) => (None, HashMap::new(), Some(e.to_string())),
                }
            }
            Err(e) => (None, HashMap::new(), Some(e.to_string())),
        };

        Self {
            state: ConnectionState::Disconnected,
            cfg,
            points,
            ctx: None,
            init_error,
        }
    }

    fn backoff_strategy(cfg: &ModbusRtuConfig) -> Result<RetryBackoffStrategy, DomainError> {
        match cfg.retry_backoff_mode.trim().to_ascii_lowercase().as_str() {
            "fixed" => Ok(RetryBackoffStrategy::Fixed),
            "exponential" => Ok(RetryBackoffStrategy::Exponential {
                max_ms: cfg.retry_backoff_max_ms.max(cfg.retry_backoff_ms.max(1)),
            }),
            other => Err(DomainError::ConfigurationError(format!(
                "unsupported rtu retry_backoff_mode '{}'",
                other
            ))),
        }
    }

    fn connect_slave_id(&self) -> u8 {
        if let Some(cfg) = &self.cfg {
            if let Some(unit) = cfg.unit_id {
                return unit;
            }
        }
        self.points
            .values()
            .next()
            .map(|p| p.unit_id)
            .unwrap_or(1)
    }
}

#[async_trait]
impl DriverConnection for ModbusRtuDriver {
    async fn connect(&mut self) -> Result<(), DomainError> {
        if let Some(msg) = &self.init_error {
            self.state = ConnectionState::Failed;
            return Err(DomainError::ConfigurationError(msg.clone()));
        }

        let cfg = self
            .cfg
            .as_ref()
            .ok_or_else(|| DomainError::ConfigurationError("modbus config unavailable".to_string()))?;

        self.state = ConnectionState::Connecting;
        let parity = match cfg.serial.parity.to_ascii_uppercase().as_str() {
            "N" => Parity::None,
            "E" => Parity::Even,
            "O" => Parity::Odd,
            other => {
                self.state = ConnectionState::Failed;
                return Err(DomainError::ConfigurationError(format!(
                    "unsupported modbus parity '{}'",
                    other
                )));
            }
        };
        let data_bits = match cfg.serial.data_bits {
            5 => DataBits::Five,
            6 => DataBits::Six,
            7 => DataBits::Seven,
            8 => DataBits::Eight,
            other => {
                self.state = ConnectionState::Failed;
                return Err(DomainError::ConfigurationError(format!(
                    "unsupported data_bits '{}'",
                    other
                )));
            }
        };
        let stop_bits = match cfg.serial.stop_bits {
            1 => StopBits::One,
            2 => StopBits::Two,
            other => {
                self.state = ConnectionState::Failed;
                return Err(DomainError::ConfigurationError(format!(
                    "unsupported stop_bits '{}'",
                    other
                )));
            }
        };

        let builder = tokio_serial::new(&cfg.serial.port, cfg.serial.baud_rate)
            .parity(parity)
            .data_bits(data_bits)
            .stop_bits(stop_bits);
        let port = builder.open_native_async().map_err(|e| {
            self.state = ConnectionState::Failed;
            DomainError::DriverError(format!("failed opening serial port: {}", e))
        })?;
        let ctx = rtu::attach_slave(port, Slave(self.connect_slave_id()));
        self.ctx = Some(Mutex::new(ctx));
        self.state = ConnectionState::Connected;
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<(), DomainError> {
        self.ctx = None;
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
            return Err(DomainError::DriverError("modbus rtu not connected".to_string()));
        }
        let ctx = self
            .ctx
            .as_ref()
            .ok_or_else(|| DomainError::DriverError("modbus context missing".to_string()))?;
        let mut ctx = ctx.lock().await;

        let mut out = Vec::with_capacity(self.points.len());
        let cfg = self
            .cfg
            .as_ref()
            .ok_or_else(|| DomainError::ConfigurationError("modbus config unavailable".to_string()))?;
        let batch = ModbusBatchPolicy {
            max_batch_registers: cfg.max_batch_registers,
            max_batch_bits: cfg.max_batch_bits,
            max_register_gap: cfg.max_register_gap,
            max_bit_gap: cfg.max_bit_gap,
        };
        let req = ModbusRequestPolicy {
            timeout_ms: cfg.request_timeout_ms,
            retries: cfg.request_retries,
            retry_backoff_ms: cfg.retry_backoff_ms,
            retry_backoff_strategy: Self::backoff_strategy(cfg)?,
        };
        let mut points_by_unit: HashMap<u8, HashMap<TagId, ModbusPoint>> = HashMap::new();
        for (tag_id, binding) in &self.points {
            points_by_unit
                .entry(binding.unit_id)
                .or_default()
                .insert(tag_id.clone(), binding.point.clone());
        }

        for (unit_id, points) in points_by_unit {
            ctx.set_slave(Slave(unit_id));
            out.extend(poll_points_batched(&mut ctx, &points, batch, req).await?);
        }
        Ok(out)
    }

    async fn write(&mut self, tag_id: TagId, value: TagValue) -> Result<(), DomainError> {
        if !self.is_connected() {
            return Err(DomainError::DriverError("modbus rtu not connected".to_string()));
        }
        let binding = self.points.get(&tag_id).ok_or_else(|| {
            DomainError::NotFound(format!("modbus point not configured for tag '{}'", tag_id))
        })?;
        let ctx = self
            .ctx
            .as_ref()
            .ok_or_else(|| DomainError::DriverError("modbus context missing".to_string()))?;
        let mut ctx = ctx.lock().await;
        ctx.set_slave(Slave(binding.unit_id));

        let cfg = self
            .cfg
            .as_ref()
            .ok_or_else(|| DomainError::ConfigurationError("modbus config unavailable".to_string()))?;
        let req = ModbusRequestPolicy {
            timeout_ms: cfg.request_timeout_ms,
            retries: cfg.request_retries,
            retry_backoff_ms: cfg.retry_backoff_ms,
            retry_backoff_strategy: Self::backoff_strategy(cfg)?,
        };
        write_point(&mut ctx, &binding.point, value, req).await
    }
}

fn build_tag_point_map(
    cfg: &ModbusRtuConfig,
) -> Result<HashMap<TagId, ModbusRtuPointBinding>, DomainError> {
    if cfg.tag_map.is_empty() {
        return Err(DomainError::ConfigurationError(
            "modbus rtu tag_map cannot be empty".to_string(),
        ));
    }

    let mut out = HashMap::with_capacity(cfg.tag_map.len());
    for (tag_id, binding) in &cfg.tag_map {
        let (source, unit_id, _device_id) = match binding {
            ModbusRtuTagBinding::Source(src) => (src.as_str(), cfg.unit_id, None),
            ModbusRtuTagBinding::Detailed {
                source,
                unit_id,
                device_id,
            } => (source.as_str(), *unit_id, device_id.as_deref()),
        };
        let resolved_unit = unit_id.or(cfg.unit_id).ok_or_else(|| {
            DomainError::ConfigurationError(format!(
                "missing unit_id for tag '{}' (ModbusRTU requires unit_id per device/tag)",
                tag_id
            ))
        })?;
        let point = ModbusPoint::parse(source)?;
        out.insert(
            TagId::new(tag_id.clone()),
            ModbusRtuPointBinding {
                point,
                unit_id: resolved_unit,
            },
        );
    }
    Ok(out)
}

pub struct ModbusRtuFactory;

impl DriverFactory for ModbusRtuFactory {
    fn create(&self, connection: &Connection) -> Box<dyn DriverConnection> {
        Box::new(ModbusRtuDriver::from_connection(connection))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_transport_config_minimal() {
        let cfg: ModbusRtuConfig = serde_json::from_value(serde_json::json!({
            "serial": { "port": "COM3" },
            "tag_map": { "tag1": { "source": "hr:0:u16", "unit_id": 1, "device_id": "d1" } }
        }))
        .unwrap();
        assert_eq!(cfg.serial.port, "COM3");
        assert_eq!(cfg.serial.baud_rate, 9_600);
        assert_eq!(cfg.serial.data_bits, 8);
        assert_eq!(cfg.serial.stop_bits, 1);
        assert_eq!(cfg.serial.parity, "N");
        assert_eq!(cfg.unit_id, None);
        assert_eq!(cfg.request_timeout_ms, 1_500);
        assert_eq!(cfg.request_retries, 1);
        assert_eq!(cfg.retry_backoff_ms, 100);
        assert_eq!(cfg.retry_backoff_mode, "fixed");
        assert_eq!(cfg.retry_backoff_max_ms, 2_000);
        assert_eq!(cfg.max_batch_registers, 120);
        assert_eq!(cfg.max_batch_bits, 2000);
        assert_eq!(cfg.max_register_gap, 0);
        assert_eq!(cfg.max_bit_gap, 0);
    }

    #[test]
    fn test_parse_transport_config_with_exponential_backoff() {
        let cfg: ModbusRtuConfig = serde_json::from_value(serde_json::json!({
            "serial": { "port": "COM3" },
            "unit_id": 1,
            "tag_map": { "tag1": "hr:0:u16" },
            "retry_backoff_mode": "exponential",
            "retry_backoff_ms": 50,
            "retry_backoff_max_ms": 500
        }))
        .unwrap();

        let strategy = ModbusRtuDriver::backoff_strategy(&cfg).unwrap();
        match strategy {
            RetryBackoffStrategy::Exponential { max_ms } => assert_eq!(max_ms, 500),
            _ => panic!("expected exponential strategy"),
        }
    }

    #[test]
    fn test_build_tag_point_map_requires_unit_resolution() {
        let cfg: ModbusRtuConfig = serde_json::from_value(serde_json::json!({
            "serial": { "port": "COM3" },
            "tag_map": { "tag1": "hr:0:u16" }
        }))
        .unwrap();

        let err = build_tag_point_map(&cfg).unwrap_err();
        match err {
            DomainError::ConfigurationError(msg) => {
                assert!(msg.contains("missing unit_id"));
            }
            _ => panic!("expected configuration error"),
        }
    }

    #[test]
    fn test_build_tag_point_map_uses_per_tag_unit_id() {
        let cfg: ModbusRtuConfig = serde_json::from_value(serde_json::json!({
            "serial": { "port": "COM3" },
            "tag_map": {
                "tag50": { "source": "hr:0:u16", "unit_id": 50, "device_id": "dev50" },
                "tag100": { "source": "hr:10:f32", "unit_id": 100, "device_id": "dev100" }
            }
        }))
        .unwrap();

        let map = build_tag_point_map(&cfg).unwrap();
        assert_eq!(map.get(&TagId::new("tag50")).unwrap().unit_id, 50);
        assert_eq!(map.get(&TagId::new("tag100")).unwrap().unit_id, 100);
    }
}

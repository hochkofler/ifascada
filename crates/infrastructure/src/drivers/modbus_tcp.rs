use crate::drivers::modbus_shared::{
    ModbusBatchPolicy, ModbusPoint, ModbusRequestPolicy, RetryBackoffStrategy, build_point_map,
    poll_points_batched, write_point,
};
use application::runtime::DriverFactory;
use async_trait::async_trait;
use domain::connection::Connection;
use domain::driver::{ConnectionState, DriverConnection};
use domain::error::DomainError;
use domain::id::TagId;
use domain::tag::TagValue;
use serde::Deserialize;
use std::collections::HashMap;
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio::time::{Duration, timeout};
use tokio_modbus::client::Context;
use tokio_modbus::client::tcp;
use tokio_modbus::prelude::Slave;

#[derive(Debug, Clone, Deserialize)]
struct ModbusTcpConfig {
    host: String,
    #[serde(default = "default_port")]
    port: u16,
    #[serde(default = "default_unit_id")]
    unit_id: u8,
    tag_map: HashMap<String, String>,
    #[serde(default = "default_connect_timeout_ms")]
    connect_timeout_ms: u64,
    #[serde(default = "default_request_timeout_ms")]
    request_timeout_ms: u64,
    #[serde(default = "default_request_retries")]
    request_retries: u8,
    #[serde(default = "default_retry_backoff_ms")]
    retry_backoff_ms: u64,
    #[serde(default = "default_max_batch_registers")]
    max_batch_registers: u16,
    #[serde(default = "default_max_batch_bits")]
    max_batch_bits: u16,
    #[serde(default = "default_max_register_gap")]
    max_register_gap: u16,
    #[serde(default = "default_max_bit_gap")]
    max_bit_gap: u16,
}

fn default_port() -> u16 {
    502
}
fn default_unit_id() -> u8 {
    1
}
fn default_connect_timeout_ms() -> u64 {
    2_000
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

pub struct ModbusTcpDriver {
    state: ConnectionState,
    cfg: Option<ModbusTcpConfig>,
    points: HashMap<TagId, ModbusPoint>,
    ctx: Option<Mutex<Context>>,
    init_error: Option<String>,
}

impl ModbusTcpDriver {
    fn from_connection(connection: &Connection) -> Self {
        let parsed_cfg =
            serde_json::from_value::<ModbusTcpConfig>(connection.config.transport.clone()).map_err(
                |e| {
                    DomainError::ConfigurationError(format!(
                        "invalid ModbusTCP transport config: {}",
                        e
                    ))
                },
            );

        let (cfg, points, init_error) = match parsed_cfg {
            Ok(cfg) => match build_point_map(&cfg.tag_map) {
                Ok(points) => (Some(cfg), points, None),
                Err(e) => (None, HashMap::new(), Some(e.to_string())),
            },
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
}

#[async_trait]
impl DriverConnection for ModbusTcpDriver {
    async fn connect(&mut self) -> Result<(), DomainError> {
        if let Some(msg) = &self.init_error {
            self.state = ConnectionState::Failed;
            return Err(DomainError::ConfigurationError(msg.clone()));
        }
        let cfg = self
            .cfg
            .as_ref()
            .ok_or_else(|| DomainError::ConfigurationError("modbus tcp config unavailable".to_string()))?;
        self.state = ConnectionState::Connecting;

        let addr = format!("{}:{}", cfg.host, cfg.port);
        let stream = timeout(
            Duration::from_millis(cfg.connect_timeout_ms.max(1)),
            TcpStream::connect(&addr),
        )
        .await
        .map_err(|_| DomainError::DriverError("modbus tcp connect timeout".to_string()))?
        .map_err(|e| DomainError::DriverError(format!("modbus tcp connect failed: {}", e)))?;

        let ctx = tcp::attach_slave(stream, Slave(cfg.unit_id));
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
            return Err(DomainError::DriverError("modbus tcp not connected".to_string()));
        }
        let cfg = self
            .cfg
            .as_ref()
            .ok_or_else(|| DomainError::ConfigurationError("modbus tcp config unavailable".to_string()))?;
        let ctx = self
            .ctx
            .as_ref()
            .ok_or_else(|| DomainError::DriverError("modbus tcp context missing".to_string()))?;
        let mut ctx = ctx.lock().await;

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
            retry_backoff_strategy: RetryBackoffStrategy::Fixed,
        };
        poll_points_batched(&mut ctx, &self.points, batch, req).await
    }

    async fn write(&mut self, tag_id: TagId, value: TagValue) -> Result<(), DomainError> {
        if !self.is_connected() {
            return Err(DomainError::DriverError("modbus tcp not connected".to_string()));
        }
        let cfg = self
            .cfg
            .as_ref()
            .ok_or_else(|| DomainError::ConfigurationError("modbus tcp config unavailable".to_string()))?;
        let point = self.points.get(&tag_id).ok_or_else(|| {
            DomainError::NotFound(format!("modbus point not configured for tag '{}'", tag_id))
        })?;
        let ctx = self
            .ctx
            .as_ref()
            .ok_or_else(|| DomainError::DriverError("modbus tcp context missing".to_string()))?;
        let mut ctx = ctx.lock().await;
        let req = ModbusRequestPolicy {
            timeout_ms: cfg.request_timeout_ms,
            retries: cfg.request_retries,
            retry_backoff_ms: cfg.retry_backoff_ms,
            retry_backoff_strategy: RetryBackoffStrategy::Fixed,
        };
        write_point(&mut ctx, point, value, req).await
    }
}

pub struct ModbusTcpFactory;

impl DriverFactory for ModbusTcpFactory {
    fn create(&self, connection: &Connection) -> Box<dyn DriverConnection> {
        Box::new(ModbusTcpDriver::from_connection(connection))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_transport_config_minimal() {
        let cfg: ModbusTcpConfig = serde_json::from_value(serde_json::json!({
            "host": "127.0.0.1",
            "tag_map": { "tag1": "hr:0:u16" }
        }))
        .unwrap();
        assert_eq!(cfg.host, "127.0.0.1");
        assert_eq!(cfg.port, 502);
        assert_eq!(cfg.unit_id, 1);
        assert_eq!(cfg.connect_timeout_ms, 2000);
    }
}

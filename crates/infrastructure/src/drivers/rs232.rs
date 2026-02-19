use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use domain::DomainError;
use domain::driver::{ConnectionState, DriverConnection};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Mutex;
use tokio_serial::{SerialPortBuilderExt, SerialStream};

use domain::device::Device;
use domain::driver::DeviceDriver;
use domain::tag::{Tag, TagId};

/// RS232 driver configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RS232Config {
    pub port: String,
    #[serde(default = "default_baud_rate")]
    pub baud_rate: u32,
    #[serde(default = "default_data_bits")]
    pub data_bits: u8,
    #[serde(default = "default_parity")]
    pub parity: String, // "None", "Even", "Odd"
    #[serde(default = "default_stop_bits")]
    pub stop_bits: u8,
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
}

fn default_baud_rate() -> u32 {
    9600
}
fn default_data_bits() -> u8 {
    8
}
fn default_parity() -> String {
    "None".to_string()
}
fn default_stop_bits() -> u8 {
    1
}
fn default_timeout_ms() -> u64 {
    1000
}

impl RS232Config {
    pub fn new(port: String) -> Self {
        Self {
            port,
            baud_rate: default_baud_rate(),
            data_bits: default_data_bits(),
            parity: default_parity(),
            stop_bits: default_stop_bits(),
            timeout_ms: default_timeout_ms(),
        }
    }

    fn to_parity(&self) -> Result<tokio_serial::Parity, DomainError> {
        match self.parity.as_str() {
            "None" => Ok(tokio_serial::Parity::None),
            "Even" => Ok(tokio_serial::Parity::Even),
            "Odd" => Ok(tokio_serial::Parity::Odd),
            _ => Err(DomainError::InvalidDriverConfig(format!(
                "Invalid parity: {}",
                self.parity
            ))),
        }
    }

    fn to_stop_bits(&self) -> Result<tokio_serial::StopBits, DomainError> {
        match self.stop_bits {
            1 => Ok(tokio_serial::StopBits::One),
            2 => Ok(tokio_serial::StopBits::Two),
            _ => Err(DomainError::InvalidDriverConfig(format!(
                "Invalid stop bits: {}",
                self.stop_bits
            ))),
        }
    }

    fn to_data_bits(&self) -> Result<tokio_serial::DataBits, DomainError> {
        match self.data_bits {
            5 => Ok(tokio_serial::DataBits::Five),
            6 => Ok(tokio_serial::DataBits::Six),
            7 => Ok(tokio_serial::DataBits::Seven),
            8 => Ok(tokio_serial::DataBits::Eight),
            _ => Err(DomainError::InvalidDriverConfig(format!(
                "Invalid data bits: {}",
                self.data_bits
            ))),
        }
    }
}

/// RS232 driver implementation
/// Uses Arc<Mutex<>> to make it thread-safe (Send + Sync) as required by DriverConnection
pub struct RS232Connection {
    config: RS232Config,
    port: Option<Arc<Mutex<SerialStream>>>,
    state: Arc<Mutex<ConnectionState>>,
}

impl RS232Connection {
    pub fn new(config: RS232Config) -> Self {
        Self {
            config,
            port: None,
            state: Arc::new(Mutex::new(ConnectionState::Disconnected)),
        }
    }
}

#[async_trait]
impl DriverConnection for RS232Connection {
    async fn connect(&mut self) -> Result<(), DomainError> {
        let mut state = self.state.lock().await;

        // Normalize port name for Windows (e.g., COM7 -> \\.\COM7)
        // This is often required for reliable access to serial ports on Windows.
        let port_name = if cfg!(target_os = "windows")
            && !self.config.port.to_uppercase().starts_with(r"\\.\")
        {
            format!(r"\\.\{}", self.config.port)
        } else {
            self.config.port.clone()
        };

        tracing::debug!(
            port = %port_name,
            baud_rate = self.config.baud_rate,
            "Opening serial port"
        );

        // Build serial port configuration
        let port = tokio_serial::new(&port_name, self.config.baud_rate)
            .data_bits(self.config.to_data_bits()?)
            .parity(self.config.to_parity()?)
            .stop_bits(self.config.to_stop_bits()?)
            .timeout(Duration::from_millis(self.config.timeout_ms))
            .open_native_async()
            .map_err(|e| {
                let err_msg = format!(
                    "Failed to open serial port {}: {}. Tip: Ensure the port is not used by another application and that you have sufficient permissions.",
                    port_name, e
                );
                // Downgraded to WARN to avoid spamming error logs during retries
                tracing::warn!(port=%port_name, error=%e, "Failed to open serial port");
                *state = ConnectionState::Failed;
                DomainError::DriverError(err_msg)
            })?;

        self.port = Some(Arc::new(Mutex::new(port)));
        *state = ConnectionState::Connected;

        tracing::debug!(port = %self.config.port, "Serial port opened successfully");
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<(), DomainError> {
        if let Some(port_arc) = self.port.take() {
            let mut port = port_arc.lock().await;
            if let Err(e) = port.shutdown().await {
                tracing::warn!(error = %e, "Error shutting down serial port");
            }
        }

        let mut state = self.state.lock().await;
        *state = ConnectionState::Disconnected;

        tracing::info!(port = %self.config.port, "Serial port disconnected");
        Ok(())
    }

    async fn read_value(&mut self) -> Result<Option<serde_json::Value>, DomainError> {
        let port_arc = self
            .port
            .as_ref()
            .ok_or_else(|| DomainError::DriverError("Port not connected".to_string()))?;

        let mut port = port_arc.lock().await; // Lock ensures exclusive access
        let mut buffer = vec![0u8; 1024];

        // Use configured timeout for read operation
        let timeout_duration = Duration::from_millis(self.config.timeout_ms);

        match tokio::time::timeout(timeout_duration, port.read(&mut buffer)).await {
            Ok(read_result) => match read_result {
                Ok(0) => {
                    // unexpected EOF or empty read
                    Ok(None)
                }
                Ok(n) => {
                    // Data received
                    let data = &buffer[..n];

                    // Try to parse as UTF-8 string first
                    match String::from_utf8(data.to_vec()) {
                        Ok(s) => {
                            let trimmed = s.trim();
                            if trimmed.is_empty() {
                                return Ok(None);
                            }

                            // Try to parse as JSON
                            match serde_json::from_str::<serde_json::Value>(trimmed) {
                                Ok(json) => Ok(Some(json)),
                                Err(_) => {
                                    // If not JSON, return as string value
                                    Ok(Some(serde_json::Value::String(trimmed.to_string())))
                                }
                            }
                        }
                        Err(_) => {
                            // If not valid UTF-8, return as hex string
                            let hex_string = data
                                .iter()
                                .map(|b| format!("{:02X}", b))
                                .collect::<Vec<_>>()
                                .join(" ");
                            Ok(Some(serde_json::Value::String(hex_string)))
                        }
                    }
                }
                Err(e) => {
                    let mut state = self.state.lock().await;
                    *state = ConnectionState::Failed;
                    Err(DomainError::DriverError(format!("Read error: {}", e)))
                }
            },
            Err(_) => {
                // Timeout elapsed, return None to indicate no data (but connection is still valid)
                // This allows the executor to check for logical timeouts on its own schedule
                Ok(None)
            }
        }
    }

    async fn write_value(&mut self, value: serde_json::Value) -> Result<(), DomainError> {
        let port_arc = self
            .port
            .as_ref()
            .ok_or_else(|| DomainError::DriverError("Port not connected".to_string()))?;

        let mut port = port_arc.lock().await;

        // Convert value to bytes
        let data = match value {
            serde_json::Value::String(s) => s.into_bytes(),
            other => serde_json::to_string(&other)
                .map_err(|e| DomainError::InvalidValue(format!("JSON serialization error: {}", e)))?
                .into_bytes(),
        };

        port.write_all(&data)
            .await
            .map_err(|e| DomainError::DriverError(format!("Write error: {}", e)))?;

        port.flush()
            .await
            .map_err(|e| DomainError::DriverError(format!("Flush error: {}", e)))?;

        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.port.is_some()
    }

    fn connection_state(&self) -> ConnectionState {
        // For sync method, we can't await, so we try_lock
        // If locked, assume current state is valid
        match self.state.try_lock() {
            Ok(state) => *state,
            Err(_) => ConnectionState::Connecting, // Conservative guess if locked
        }
    }

    fn driver_type(&self) -> &str {
        "RS232"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rs232_config_defaults() {
        let config = RS232Config::new("COM1".to_string());
        assert_eq!(config.port, "COM1");
        assert_eq!(config.baud_rate, 9600);
        assert_eq!(config.data_bits, 8);
        assert_eq!(config.parity, "None");
        assert_eq!(config.stop_bits, 1);
        assert_eq!(config.timeout_ms, 1000);
    }

    #[test]
    fn test_rs232_config_parity_conversion() {
        let config = RS232Config {
            port: "COM1".to_string(),
            baud_rate: 9600,
            data_bits: 8,
            parity: "Even".to_string(),
            stop_bits: 1,
            timeout_ms: 1000,
        };
        assert!(matches!(
            config.to_parity().unwrap(),
            tokio_serial::Parity::Even
        ));

        let config_odd = RS232Config {
            parity: "Odd".to_string(),
            ..config.clone()
        };
        assert!(matches!(
            config_odd.to_parity().unwrap(),
            tokio_serial::Parity::Odd
        ));
    }

    #[test]
    fn test_rs232_initial_state() {
        let config = RS232Config::new("COM1".to_string());
        let driver = RS232Connection::new(config);
        assert_eq!(driver.connection_state(), ConnectionState::Disconnected);
        assert!(!driver.is_connected());
        assert_eq!(driver.driver_type(), "RS232");
    }

    #[tokio::test]
    async fn test_rs232_disconnect_without_connection() {
        let config = RS232Config::new("COM1".to_string());
        let mut driver = RS232Connection::new(config);

        // Should be able to disconnect even if not connected
        let result = driver.disconnect().await;
        assert!(result.is_ok());
        assert_eq!(driver.connection_state(), ConnectionState::Disconnected);
    }
}

/// Device Driver Implementation for RS232 (Stream/Batch)
pub struct RS232DeviceDriver {
    config: RS232Config,
    tags: Vec<Tag>,
    port: Option<Arc<Mutex<SerialStream>>>,
    state: Arc<Mutex<ConnectionState>>,
}

impl RS232DeviceDriver {
    pub fn new(device: Device, tags: Vec<Tag>) -> Result<Self, DomainError> {
        let config: RS232Config =
            serde_json::from_value(device.connection_config).map_err(|e| {
                DomainError::InvalidDriverConfig(format!("Invalid RS232 Config: {}", e))
            })?;

        Ok(Self {
            config,
            tags,
            port: None,
            state: Arc::new(Mutex::new(ConnectionState::Disconnected)),
        })
    }
}

#[async_trait]
impl DeviceDriver for RS232DeviceDriver {
    async fn connect(&mut self) -> Result<(), DomainError> {
        let mut state = self.state.lock().await;
        let port_name = if cfg!(target_os = "windows")
            && !self.config.port.to_uppercase().starts_with(r"\\.\")
        {
            format!(r"\\.\{}", self.config.port)
        } else {
            self.config.port.clone()
        };

        let port = tokio_serial::new(&port_name, self.config.baud_rate)
            .data_bits(self.config.to_data_bits()?)
            .parity(self.config.to_parity()?)
            .stop_bits(self.config.to_stop_bits()?)
            .timeout(Duration::from_millis(self.config.timeout_ms))
            .open_native_async()
            .map_err(|e| {
                let err_msg = format!("Failed to open serial port {}: {}", port_name, e);
                tracing::warn!("{}", err_msg);
                *state = ConnectionState::Failed;
                DomainError::DriverError(err_msg)
            })?;

        self.port = Some(Arc::new(Mutex::new(port)));
        *state = ConnectionState::Connected;
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<(), DomainError> {
        if let Some(port_arc) = self.port.take() {
            // Just dropping it closes it in most cases, but we can try shutdown
            let mut port = port_arc.lock().await;
            let _ = port.shutdown().await;
        }
        let mut state = self.state.lock().await;
        *state = ConnectionState::Disconnected;
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.port.is_some()
    }

    fn connection_state(&self) -> ConnectionState {
        match self.state.try_lock() {
            Ok(s) => *s,
            Err(_) => ConnectionState::Connecting,
        }
    }

    async fn poll(
        &mut self,
    ) -> Result<Vec<(TagId, Result<serde_json::Value, DomainError>)>, DomainError> {
        let port_arc = self
            .port
            .as_ref()
            .ok_or_else(|| DomainError::DriverError("Port not connected".to_string()))?;

        let mut port = port_arc.lock().await;
        let mut buffer = vec![0u8; 1024];

        match port.read(&mut buffer).await {
            Ok(0) => Ok(vec![]), // EOF or empty
            Ok(n) => {
                let data = &buffer[..n];
                // Simple strategy: Try to parse as String/JSON and assign to ALL tags attached to this device
                // Real usage would require a parser/splitter based on Tag config.

                let value = match String::from_utf8(data.to_vec()) {
                    Ok(s) => {
                        let trimmed = s.trim();
                        if trimmed.is_empty() {
                            return Ok(vec![]);
                        }
                        match serde_json::from_str::<serde_json::Value>(trimmed) {
                            Ok(json) => json,
                            Err(_) => serde_json::Value::String(trimmed.to_string()),
                        }
                    }
                    Err(_) => {
                        let hex = data
                            .iter()
                            .map(|b| format!("{:02X}", b))
                            .collect::<Vec<_>>()
                            .join(" ");
                        serde_json::Value::String(hex)
                    }
                };

                let results = self
                    .tags
                    .iter()
                    .map(|tag| (tag.id().clone(), Ok(value.clone())))
                    .collect();

                Ok(results)
            }
            Err(e) => Err(DomainError::DriverError(format!("Read error: {}", e))),
        }
    }

    async fn write(
        &mut self,
        _tag_id: &TagId,
        value: serde_json::Value,
    ) -> Result<(), DomainError> {
        let port_arc = self
            .port
            .as_ref()
            .ok_or_else(|| DomainError::DriverError("Port not connected".to_string()))?;

        let mut port = port_arc.lock().await;
        let data = match value {
            serde_json::Value::String(s) => s.into_bytes(),
            other => serde_json::to_string(&other).unwrap().into_bytes(),
        };

        port.write_all(&data)
            .await
            .map_err(|e| DomainError::DriverError(format!("Write error: {}", e)))?;
        port.flush()
            .await
            .map_err(|e| DomainError::DriverError(format!("Flush error: {}", e)))?;
        Ok(())
    }
}

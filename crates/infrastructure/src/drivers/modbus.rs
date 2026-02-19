use std::collections::HashMap;
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use async_trait::async_trait;
use domain::DomainError;
use domain::driver::{ConnectionState, DriverConnection};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex as TokioMutex;
use tokio_modbus::client::Context;
use tokio_modbus::prelude::*;
use tokio_serial::SerialStream;

use domain::device::Device;
use domain::driver::DeviceDriver;
use domain::tag::Tag;

// Global registry for shared serial ports
static SHARED_PORTS: std::sync::OnceLock<Mutex<HashMap<String, Weak<TokioMutex<Context>>>>> =
    std::sync::OnceLock::new();

fn get_shared_ports() -> &'static Mutex<HashMap<String, Weak<TokioMutex<Context>>>> {
    SHARED_PORTS.get_or_init(|| Mutex::new(HashMap::new()))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModbusConfig {
    // Port Settings
    pub port: String,
    #[serde(default = "default_baud_rate")]
    pub baud_rate: u32,
    #[serde(default = "default_data_bits")]
    pub data_bits: u8,
    #[serde(default = "default_parity")]
    pub parity: String,
    #[serde(default = "default_stop_bits")]
    pub stop_bits: u8,
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,

    // Device/Tag Settings
    pub slave_id: u8,
    pub address: u16, // Starting address
    #[serde(default = "default_count")]
    pub count: u16, // Number of registers to read
    #[serde(default = "default_register_type")]
    pub register_type: String, // Holding, Input, Coil, Discrete
                      // Optional: Data type interpretation could be added here or handled by upper layer parser
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
fn default_count() -> u16 {
    1
}
fn default_register_type() -> String {
    "Holding".to_string()
}

impl ModbusConfig {
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

pub struct ModbusConnection {
    config: ModbusConfig,
    context: Option<Arc<TokioMutex<Context>>>,
    state: ConnectionState,
}

impl ModbusConnection {
    pub fn new(config: ModbusConfig) -> Self {
        Self {
            config,
            context: None,
            state: ConnectionState::Disconnected,
        }
    }
}

#[async_trait]
impl DriverConnection for ModbusConnection {
    async fn connect(&mut self) -> Result<(), DomainError> {
        self.state = ConnectionState::Connecting;
        let map_mutex = get_shared_ports();

        let port_key = self.config.port.to_lowercase(); // Case-insensitive key

        // 1. Try to get existing context
        let existing_ctx = {
            let map = map_mutex.lock().unwrap();
            if let Some(weak) = map.get(&port_key) {
                weak.upgrade()
            } else {
                None
            }
        };

        if let Some(ctx) = existing_ctx {
            self.context = Some(ctx);
            self.state = ConnectionState::Connected;
            return Ok(());
        }

        // 2. Create new context if not found or dropped
        // Normalize port name for Windows
        let port_name = if cfg!(target_os = "windows") && !self.config.port.starts_with(r"\\.\") {
            format!(r"\\.\{}", self.config.port)
        } else {
            self.config.port.clone()
        };

        let builder = tokio_serial::new(&port_name, self.config.baud_rate)
            .data_bits(self.config.to_data_bits()?)
            .parity(self.config.to_parity()?)
            .stop_bits(self.config.to_stop_bits()?)
            .timeout(Duration::from_millis(self.config.timeout_ms));

        let port = SerialStream::open(&builder).map_err(|e| {
            self.state = ConnectionState::Failed;
            let err_msg = format!("Failed to open serial port {}: {}", port_name, e);
            tracing::error!("{}", err_msg);
            DomainError::DriverError(err_msg)
        })?;

        let ctx = tokio_modbus::client::rtu::attach_slave(port, Slave(self.config.slave_id));
        let ctx = Arc::new(TokioMutex::new(ctx));

        // 3. Store in map
        {
            let mut map = map_mutex.lock().unwrap();
            map.insert(port_key, Arc::downgrade(&ctx));
        }

        self.context = Some(ctx);
        self.state = ConnectionState::Connected;
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<(), DomainError> {
        // Dropping the Arc will decrement the refcount.
        // If it reaches 0, the Context is dropped and port closed.
        self.context = None;
        self.state = ConnectionState::Disconnected;
        Ok(())
    }

    async fn read_value(&mut self) -> Result<Option<serde_json::Value>, DomainError> {
        let ctx_arc = self
            .context
            .as_ref()
            .ok_or(DomainError::DriverError("Not connected".into()))?;

        tracing::debug!(
            "Modbus read_value called. Slave: {}, Addr: {}, Count: {}, Type: {}",
            self.config.slave_id,
            self.config.address,
            self.config.count,
            self.config.register_type
        );

        let mut ctx = ctx_arc.lock().await;

        // Set slave ID for this transaction (in case shared context was used by another slave ID)
        ctx.set_slave(Slave(self.config.slave_id));

        let timeout_duration = Duration::from_millis(self.config.timeout_ms);

        // Define the future that performs the Modbus read operation
        // This future will return a unified type:
        // Result<Result<Option<serde_json::Value>, tokio_modbus::Exception>, tokio_modbus::Error>
        let read_future = async {
            match self.config.register_type.as_str() {
                "Holding" => {
                    let res = ctx
                        .read_holding_registers(self.config.address, self.config.count)
                        .await;
                    res.map(|r| r.map(|v| Some(serde_json::json!(v))))
                }
                "Input" => {
                    let res = ctx
                        .read_input_registers(self.config.address, self.config.count)
                        .await;
                    res.map(|r| r.map(|v| Some(serde_json::json!(v))))
                }
                "Coil" => {
                    let res = ctx.read_coils(self.config.address, self.config.count).await;
                    res.map(|r| {
                        r.map(|v| Some(serde_json::to_value(v).unwrap_or(serde_json::Value::Null)))
                    })
                }
                "Discrete" => {
                    let res = ctx
                        .read_discrete_inputs(self.config.address, self.config.count)
                        .await;
                    res.map(|r| {
                        r.map(|v| Some(serde_json::to_value(v).unwrap_or(serde_json::Value::Null)))
                    })
                }
                _ => {
                    // Return an error for unknown register types, wrapped in the expected Result structure
                    Ok(Err(tokio_modbus::Exception::IllegalFunction))
                }
            }
        };

        let result = tokio::time::timeout(timeout_duration, read_future).await;

        tracing::debug!("Modbus read completed. Result: {:?}", result);

        match result {
            Ok(modbus_res) => {
                match modbus_res {
                    Ok(inner_res) => match inner_res {
                        Ok(val) => {
                            // `val` is already `Option<serde_json::Value>`
                            Ok(val)
                        }
                        Err(e) => Err(DomainError::DriverError(format!("Modbus exception: {}", e))),
                    },
                    Err(e) => Err(DomainError::DriverError(format!(
                        "Modbus transport error: {}",
                        e
                    ))),
                }
            }
            Err(_) => Err(DomainError::DriverError(format!(
                "Modbus request timed out after {}ms",
                self.config.timeout_ms
            ))),
        }
    }

    async fn write_value(&mut self, value: serde_json::Value) -> Result<(), DomainError> {
        let ctx_arc = self
            .context
            .as_ref()
            .ok_or(DomainError::DriverError("Not connected".into()))?;
        let mut ctx = ctx_arc.lock().await;
        ctx.set_slave(Slave(self.config.slave_id));

        // Determine what to write
        match self.config.register_type.as_str() {
            "Holding" => {
                // If value is a array, write multiple. If number, write single.
                if let Some(arr) = value.as_array() {
                    let words: Vec<u16> =
                        arr.iter().map(|v| v.as_u64().unwrap_or(0) as u16).collect();
                    let res = ctx
                        .write_multiple_registers(self.config.address, &words)
                        .await;
                    match res {
                        Ok(inner) => match inner {
                            Ok(_) => {}
                            Err(e) => {
                                return Err(DomainError::DriverError(format!(
                                    "Modbus exception: {}",
                                    e
                                )));
                            }
                        },
                        Err(e) => {
                            return Err(DomainError::DriverError(format!(
                                "Modbus transport error: {}",
                                e
                            )));
                        }
                    }
                } else if let Some(n) = value.as_u64() {
                    let res = ctx
                        .write_single_register(self.config.address, n as u16)
                        .await;
                    match res {
                        Ok(inner) => match inner {
                            Ok(_) => {}
                            Err(e) => {
                                return Err(DomainError::DriverError(format!(
                                    "Modbus exception: {}",
                                    e
                                )));
                            }
                        },
                        Err(e) => {
                            return Err(DomainError::DriverError(format!(
                                "Modbus transport error: {}",
                                e
                            )));
                        }
                    }
                } else {
                    return Err(DomainError::InvalidValue(
                        "Value must be a number or array of numbers for Holding registers".into(),
                    ));
                }
            }
            "Coil" => {
                if let Some(b) = value.as_bool() {
                    let res = ctx.write_single_coil(self.config.address, b).await;
                    match res {
                        Ok(inner) => match inner {
                            Ok(_) => {}
                            Err(e) => {
                                return Err(DomainError::DriverError(format!(
                                    "Modbus exception: {}",
                                    e
                                )));
                            }
                        },
                        Err(e) => {
                            return Err(DomainError::DriverError(format!(
                                "Modbus transport error: {}",
                                e
                            )));
                        }
                    }
                } else {
                    // Try number context (0 = false, >0 = true)
                    if let Some(n) = value.as_i64() {
                        let res = ctx.write_single_coil(self.config.address, n > 0).await;
                        match res {
                            Ok(inner) => match inner {
                                Ok(_) => {}
                                Err(e) => {
                                    return Err(DomainError::DriverError(format!(
                                        "Modbus exception: {}",
                                        e
                                    )));
                                }
                            },
                            Err(e) => {
                                return Err(DomainError::DriverError(format!(
                                    "Modbus transport error: {}",
                                    e
                                )));
                            }
                        }
                    } else {
                        return Err(DomainError::InvalidValue(
                            "Value must be boolean for Coil".into(),
                        ));
                    }
                }
            }
            _ => {
                return Err(DomainError::DriverError(
                    "Write not supported for this register type".into(),
                ));
            }
        }

        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.context.is_some()
    }

    fn connection_state(&self) -> ConnectionState {
        self.state
    }

    fn driver_type(&self) -> &str {
        "Modbus"
    }
}

/// New V2 Device Configuration (Connection Only)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModbusDeviceConfig {
    pub port: String,
    #[serde(default = "default_baud_rate")]
    pub baud_rate: u32,
    #[serde(default = "default_data_bits")]
    pub data_bits: u8,
    #[serde(default = "default_parity")]
    pub parity: String,
    #[serde(default = "default_stop_bits")]
    pub stop_bits: u8,
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
    pub slave_id: u8,
}

impl ModbusDeviceConfig {
    pub fn to_data_bits(&self) -> Result<tokio_serial::DataBits, DomainError> {
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

    pub fn to_parity(&self) -> Result<tokio_serial::Parity, DomainError> {
        match self.parity.to_lowercase().as_str() {
            "n" | "none" => Ok(tokio_serial::Parity::None),
            "o" | "odd" => Ok(tokio_serial::Parity::Odd),
            "e" | "even" => Ok(tokio_serial::Parity::Even),
            _ => Err(DomainError::InvalidDriverConfig(format!(
                "Invalid parity: {}",
                self.parity
            ))),
        }
    }

    pub fn to_stop_bits(&self) -> Result<tokio_serial::StopBits, DomainError> {
        match self.stop_bits {
            1 => Ok(tokio_serial::StopBits::One),
            2 => Ok(tokio_serial::StopBits::Two),
            _ => Err(DomainError::InvalidDriverConfig(format!(
                "Invalid stop bits: {}",
                self.stop_bits
            ))),
        }
    }
}

/// Device Driver Implementation for Modbus (Batch Polling)
pub struct ModbusDeviceDriver {
    config: ModbusDeviceConfig,
    tags: Vec<Tag>,
    context: Option<Arc<TokioMutex<Context>>>,
    state: ConnectionState,
}

impl ModbusDeviceDriver {
    pub fn new(device: Device, tags: Vec<Tag>) -> Result<Self, DomainError> {
        // Parse connection config
        let config: ModbusDeviceConfig =
            serde_json::from_value(device.connection_config).map_err(|e| {
                DomainError::InvalidDriverConfig(format!("Invalid Modbus Config: {}", e))
            })?;

        Ok(Self {
            config,
            tags,
            context: None,
            state: ConnectionState::Disconnected,
        })
    }
}

#[async_trait]
impl DeviceDriver for ModbusDeviceDriver {
    async fn connect(&mut self) -> Result<(), DomainError> {
        self.state = ConnectionState::Connecting;
        let map_mutex = get_shared_ports();
        let port_key = self.config.port.to_lowercase();

        // 1. Try to get existing context
        let existing_ctx = {
            let map = map_mutex.lock().unwrap();
            if let Some(weak) = map.get(&port_key) {
                weak.upgrade()
            } else {
                None
            }
        };

        if let Some(ctx) = existing_ctx {
            self.context = Some(ctx);
            self.state = ConnectionState::Connected;
            return Ok(());
        }

        // 2. Create new context
        let port_name = if cfg!(target_os = "windows") && !self.config.port.starts_with(r"\\.\") {
            format!(r"\\.\{}", self.config.port)
        } else {
            self.config.port.clone()
        };

        let builder = tokio_serial::new(&port_name, self.config.baud_rate)
            .data_bits(self.config.to_data_bits()?)
            .parity(self.config.to_parity()?)
            .stop_bits(self.config.to_stop_bits()?)
            .timeout(Duration::from_millis(self.config.timeout_ms));

        let port = SerialStream::open(&builder).map_err(|e| {
            self.state = ConnectionState::Failed;
            let err_msg = format!("Failed to open serial port {}: {}", port_name, e);
            tracing::error!("{}", err_msg);
            DomainError::DriverError(err_msg)
        })?;

        // We use slave ID 1 initially as default to attach, but will switch per request
        let ctx = tokio_modbus::client::rtu::attach_slave(port, Slave(self.config.slave_id));
        let ctx = Arc::new(TokioMutex::new(ctx));

        // 3. Store in map
        {
            let mut map = map_mutex.lock().unwrap();
            map.insert(port_key, Arc::downgrade(&ctx));
        }

        self.context = Some(ctx);
        self.state = ConnectionState::Connected;
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<(), DomainError> {
        self.context = None;
        self.state = ConnectionState::Disconnected;
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.context.is_some()
    }

    fn connection_state(&self) -> ConnectionState {
        self.state
    }

    async fn poll(
        &mut self,
    ) -> Result<Vec<(domain::tag::TagId, Result<serde_json::Value, DomainError>)>, DomainError>
    {
        let ctx_arc = self
            .context
            .as_ref()
            .ok_or(DomainError::DriverError("Not connected".into()))?;

        let mut results = Vec::new();
        let mut ctx = ctx_arc.lock().await;

        // Ensure we are talking to the correct slave (Device-level)
        ctx.set_slave(Slave(self.config.slave_id));

        // Todo: Group tags by contiguity for optimization?
        // For now, iterate and read individually.
        for tag in &self.tags {
            // Parse Source Config
            // Expected: {"register": u16, "count": u16, "register_type": "Holding"|"Input"|...}
            let source_config = tag.source_config(); // This returns &Value

            let register_addr = source_config
                .get("register")
                .and_then(|v| v.as_u64())
                .map(|v| v as u16);
            let count = source_config
                .get("count")
                .and_then(|v| v.as_u64())
                .map(|v| v as u16)
                .unwrap_or(1);
            let register_type = source_config
                .get("register_type")
                .and_then(|v| v.as_str())
                .unwrap_or("Holding");

            if let Some(addr) = register_addr {
                let read_res: Result<serde_json::Value, DomainError> = match register_type {
                    "Holding" => match ctx.read_holding_registers(addr, count).await {
                        Ok(inner) => match inner {
                            Ok(vals) => Ok(serde_json::json!(vals)),
                            Err(e) => {
                                Err(DomainError::DriverError(format!("Modbus Exception: {}", e)))
                            }
                        },
                        Err(e) => Err(DomainError::DriverError(format!(
                            "Modbus Transport Error: {}",
                            e
                        ))),
                    },
                    "Input" => match ctx.read_input_registers(addr, count).await {
                        Ok(inner) => match inner {
                            Ok(vals) => Ok(serde_json::json!(vals)),
                            Err(e) => {
                                Err(DomainError::DriverError(format!("Modbus Exception: {}", e)))
                            }
                        },
                        Err(e) => Err(DomainError::DriverError(format!(
                            "Modbus Transport Error: {}",
                            e
                        ))),
                    },
                    "Coil" => match ctx.read_coils(addr, count).await {
                        Ok(inner) => match inner {
                            Ok(vals) => Ok(serde_json::json!(vals)),
                            Err(e) => {
                                Err(DomainError::DriverError(format!("Modbus Exception: {}", e)))
                            }
                        },
                        Err(e) => Err(DomainError::DriverError(format!(
                            "Modbus Transport Error: {}",
                            e
                        ))),
                    },
                    "Discrete" => match ctx.read_discrete_inputs(addr, count).await {
                        Ok(inner) => match inner {
                            Ok(vals) => Ok(serde_json::json!(vals)),
                            Err(e) => {
                                Err(DomainError::DriverError(format!("Modbus Exception: {}", e)))
                            }
                        },
                        Err(e) => Err(DomainError::DriverError(format!(
                            "Modbus Transport Error: {}",
                            e
                        ))),
                    },
                    _ => Err(DomainError::DriverError(format!(
                        "Unknown register type: {}",
                        register_type
                    ))),
                };

                results.push((tag.id().clone(), read_res));
            } else {
                results.push((
                    tag.id().clone(),
                    Err(DomainError::InvalidDriverConfig(
                        "Missing 'register' in source_config".into(),
                    )),
                ));
            }
        }

        Ok(results)
    }

    async fn write(
        &mut self,
        _tag_id: &domain::tag::TagId,
        _value: serde_json::Value,
    ) -> Result<(), DomainError> {
        Err(DomainError::DriverError("Write not implemented yet".into()))
    }
}

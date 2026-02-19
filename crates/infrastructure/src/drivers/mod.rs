pub mod device_simulator;
pub mod modbus;
mod rs232;
mod simulator_connection;
pub use device_simulator::SimulatorDeviceDriver;

pub use modbus::{ModbusConfig, ModbusConnection};
pub use rs232::{RS232Config, RS232Connection};
pub use simulator_connection::{SimulatorConfig, SimulatorConnection};

use domain::DomainError;
use domain::driver::{DriverConnection, DriverType};

/// Factory for creating driver connections
pub struct DriverFactory;

impl DriverFactory {
    /// Create a driver connection from type and configuration
    pub fn create_driver(
        driver_type: DriverType,
        config: serde_json::Value,
    ) -> Result<Box<dyn DriverConnection>, DomainError> {
        match driver_type {
            DriverType::RS232 => {
                let rs232_config: RS232Config = serde_json::from_value(config).map_err(|e| {
                    DomainError::InvalidDriverConfig(format!("Invalid RS232 config: {}", e))
                })?;
                Ok(Box::new(RS232Connection::new(rs232_config)) as Box<dyn DriverConnection>)
            }
            DriverType::Simulator => {
                let sim_config: SimulatorConfig = serde_json::from_value(config).map_err(|e| {
                    DomainError::InvalidDriverConfig(format!("Invalid Simulator config: {}", e))
                })?;
                Ok(Box::new(SimulatorConnection::new(sim_config)) as Box<dyn DriverConnection>)
            }
            DriverType::Modbus => {
                let modbus_config: ModbusConfig = serde_json::from_value(config).map_err(|e| {
                    DomainError::InvalidDriverConfig(format!("Invalid Modbus config: {}", e))
                })?;
                Ok(Box::new(ModbusConnection::new(modbus_config)) as Box<dyn DriverConnection>)
            }
            DriverType::OPCUA => Err(DomainError::InvalidDriverConfig(
                "OPCUA driver not yet implemented".to_string(),
            )),
            DriverType::HTTP => Err(DomainError::InvalidDriverConfig(
                "HTTP driver not yet implemented".to_string(),
            )),
        }
    }

    /// Create a device driver (batch/optimized) from device and tags
    pub fn create_device_driver(
        device: domain::device::Device,
        tags: Vec<domain::tag::Tag>,
    ) -> Result<Box<dyn domain::driver::DeviceDriver>, DomainError> {
        match device.driver {
            DriverType::Simulator => {
                // For simulator, we pass the device and tags
                Ok(Box::new(SimulatorDeviceDriver::new(device, tags)))
            }
            DriverType::Modbus => Ok(Box::new(modbus::ModbusDeviceDriver::new(device, tags)?)),
            DriverType::RS232 => Ok(Box::new(rs232::RS232DeviceDriver::new(device, tags)?)),
            _ => Err(DomainError::InvalidDriverConfig(format!(
                "DeviceDriver not yet implemented for {:?}",
                device.driver
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_create_rs232_driver() {
        let config = json!({
            "port": "COM3",
            "baud_rate": 115200
        });

        let driver = DriverFactory::create_driver(DriverType::RS232, config);
        assert!(driver.is_ok());

        let driver = driver.unwrap();
        assert_eq!(driver.driver_type(), "RS232");
    }

    #[test]
    fn test_create_simulator_driver() {
        let config = json!({
            "min_value": 0.0,
            "max_value": 100.0,
            "interval_ms": 1000,
            "unit": "kg",
            "pattern": "sine"
        });

        let driver = DriverFactory::create_driver(DriverType::Simulator, config);
        assert!(driver.is_ok());

        let driver = driver.unwrap();
        assert_eq!(driver.driver_type(), "Simulator");
    }

    #[test]
    fn test_create_rs232_with_minimal_config() {
        let config = json!({"port": "COM1"});

        let driver = DriverFactory::create_driver(DriverType::RS232, config);
        assert!(driver.is_ok());
    }

    #[test]
    fn test_create_rs232_invalid_config() {
        let config = json!({"invalid_field": "value"});

        let driver = DriverFactory::create_driver(DriverType::RS232, config);
        assert!(driver.is_err());
    }

    #[test]
    fn test_unimplemented_drivers() {
        let config = json!({});

        assert!(DriverFactory::create_driver(DriverType::Modbus, config.clone()).is_err());
        assert!(DriverFactory::create_driver(DriverType::OPCUA, config.clone()).is_err());
        assert!(DriverFactory::create_driver(DriverType::HTTP, config).is_err());
    }
}

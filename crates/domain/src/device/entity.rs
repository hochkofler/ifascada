use crate::driver::DriverType;
use serde::{Deserialize, Serialize};

/// Represents a physical connection to a device (PLC, RTU, Simulator).
/// A Device manages the *connection*, not the semantic data.
///
/// # TDD Requirements
/// - Must have a unique `id`
/// - Must specify `driver` type (DriverType)
/// - Must handle specific connection config (IP, Port, etc.) via `serde_json::Value`
/// - Must support enable/disable state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    pub id: String,
    pub driver: DriverType,
    pub connection_config: serde_json::Value,
    pub enabled: bool,
}

impl Device {
    pub fn new(
        id: String,
        driver: DriverType,
        connection_config: serde_json::Value,
        enabled: bool,
    ) -> Self {
        Self {
            id,
            driver,
            connection_config,
            enabled,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_device_creation() {
        let config = json!({
            "ip": "192.168.1.100",
            "port": 502
        });

        // This test ensures the struct has the required fields and constructor
        let device = Device::new(
            "plc-01".to_string(),
            DriverType::Modbus,
            config.clone(),
            true,
        );

        assert_eq!(device.id, "plc-01");
        assert_eq!(device.driver, DriverType::Modbus);
        assert_eq!(device.connection_config, config);
        assert!(device.enabled);
    }
}

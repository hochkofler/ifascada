use serde::{Deserialize, Serialize};

/// Type of driver for communication
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DriverType {
    RS232,
    Modbus,
    #[serde(rename = "OPC-UA")]
    OPCUA,
    HTTP,
    Simulator,
}

impl DriverType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::RS232 => "RS232",
            Self::Modbus => "Modbus",
            Self::OPCUA => "OPC-UA",
            Self::HTTP => "HTTP",
            Self::Simulator => "Simulator",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_driver_type_as_str() {
        assert_eq!(DriverType::RS232.as_str(), "RS232");
        assert_eq!(DriverType::Modbus.as_str(), "Modbus");
        assert_eq!(DriverType::OPCUA.as_str(), "OPC-UA");
        assert_eq!(DriverType::HTTP.as_str(), "HTTP");
        assert_eq!(DriverType::Simulator.as_str(), "Simulator");
    }
}

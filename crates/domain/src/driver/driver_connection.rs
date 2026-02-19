use async_trait::async_trait;

use super::connection_state::ConnectionState;
use crate::error::DomainError;

/// Driver connection trait that infrastructure implementations must provide
#[async_trait]
pub trait DriverConnection: Send + Sync {
    /// Establish connection to the device
    async fn connect(&mut self) -> Result<(), DomainError>;

    /// Disconnect from the device
    async fn disconnect(&mut self) -> Result<(), DomainError>;

    /// Read a value from the device
    /// Returns None if no data is available (non-blocking)
    async fn read_value(&mut self) -> Result<Option<serde_json::Value>, DomainError>;

    /// Write a value to the device (for commands)
    async fn write_value(&mut self, value: serde_json::Value) -> Result<(), DomainError>;

    /// Check if currently connected
    fn is_connected(&self) -> bool;

    /// Get current connection state
    fn connection_state(&self) -> ConnectionState;

    /// Get driver type identifier
    fn driver_type(&self) -> &str;
}

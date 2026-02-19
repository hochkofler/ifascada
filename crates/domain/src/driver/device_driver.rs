use async_trait::async_trait;
use serde_json::Value;

use super::connection_state::ConnectionState;
use crate::error::DomainError;
use crate::tag::TagId;

/// Device Driver trait for batch/optimized acquisition
#[async_trait]
pub trait DeviceDriver: Send + Sync {
    /// Establish connection to the device
    async fn connect(&mut self) -> Result<(), DomainError>;

    /// Disconnect from the device
    async fn disconnect(&mut self) -> Result<(), DomainError>;

    /// Check if currently connected
    fn is_connected(&self) -> bool;

    /// Get current connection state
    fn connection_state(&self) -> ConnectionState;

    /// Polls the device for all configured tags.
    /// Returns a vector of (TagId, Value) tuples for successful reads.
    /// Errors during individual tag reads should be logged/handled internally or returned as Partial results if possible,
    /// but here we simplify to returning a Batch Result.
    /// Actually, to handle partial failures (e.g. one register failed), we might want `Vec<(TagId, Result<Value, DomainError>)>`.
    async fn poll(&mut self) -> Result<Vec<(TagId, Result<Value, DomainError>)>, DomainError>;

    /// Write a value to a specific tag
    async fn write(&mut self, tag_id: &TagId, value: Value) -> Result<(), DomainError>;
}

use crate::error::DomainError;
use crate::id::TagId;
use crate::tag::value::TagValue;
use async_trait::async_trait;

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Failed,
}

/// Opaque driver identifier used by the domain.
/// The domain does not know concrete implementations (e.g. Modbus/OPC/Simulator).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DriverType(String);

impl DriverType {
    pub fn new(value: impl Into<String>) -> Result<Self, DomainError> {
        let value = value.into().trim().to_string();
        if value.is_empty() {
            return Err(DomainError::ConfigurationError(
                "driver_type cannot be empty".to_string(),
            ));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for DriverType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for DriverType {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

#[async_trait]
pub trait DriverConnection: Send + Sync {
    async fn connect(&mut self) -> Result<(), DomainError>;
    async fn disconnect(&mut self) -> Result<(), DomainError>;
    fn is_connected(&self) -> bool;
    fn state(&self) -> ConnectionState;

    /// Polls for multiple tags at once for efficiency.
    /// Returns (TagId, Result) pairs.
    async fn poll(&mut self) -> Result<Vec<(TagId, Result<TagValue, DomainError>)>, DomainError>;

    /// Individual write (for commands)
    async fn write(&mut self, tag_id: TagId, value: TagValue) -> Result<(), DomainError>;
}

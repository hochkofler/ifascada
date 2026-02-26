use crate::id::ConnectionId;
use crate::driver::DriverType;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionConfig {
    pub driver_type: DriverType,
    pub transport: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReconnectStrategy {
    Fixed {
        delay_ms: u64,
    },
    Exponential {
        initial_delay_ms: u64,
        max_delay_ms: u64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconnectionPolicy {
    pub strategy: ReconnectStrategy,
    pub max_retries: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Connection {
    pub id: ConnectionId,
    pub name: String,
    pub enabled: bool,
    pub config: ConnectionConfig,
    pub timeout_ms: u64,
    pub reconnection: ReconnectionPolicy,
}

impl Connection {
    pub fn new(
        id: ConnectionId,
        name: String,
        driver_type: DriverType,
        transport: serde_json::Value,
    ) -> Self {
        Self {
            id,
            name,
            enabled: true,
            config: ConnectionConfig {
                driver_type,
                transport,
            },
            timeout_ms: 5000,
            reconnection: ReconnectionPolicy {
                strategy: ReconnectStrategy::Fixed { delay_ms: 1000 },
                max_retries: None,
            },
        }
    }
}

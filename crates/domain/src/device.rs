use crate::id::{ConnectionId, DeviceId};
use serde::{Deserialize, Serialize};

pub mod status;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    pub id: DeviceId,
    pub connection_id: ConnectionId,
    pub name: String,
    pub enabled: bool,
    pub slave_id: u8, // Common in many industrial protocols
}

impl Device {
    pub fn new(id: DeviceId, connection_id: ConnectionId, name: String) -> Self {
        Self {
            id,
            connection_id,
            name,
            enabled: true,
            slave_id: 1,
        }
    }
}

pub use status::{DeviceConnectionStatus, DeviceStatusInput, evaluate_device_connection_status};

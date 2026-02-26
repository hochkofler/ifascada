use chrono::{DateTime, Utc};
use domain::id::{ConnectionId, DeviceId, TagId};
use domain::tag::quality::TagQuality;
use domain::tag::value::TagValue;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum WriteCommandOutcome {
    Applied,
    Deduplicated,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RuntimeEvent {
    TagChanged {
        tag_id: TagId,
        value: TagValue,
        trigger_value: Option<TagValue>,
        quality: TagQuality,
        timestamp: DateTime<Utc>,
    },
    ConnectionStateChanged {
        connection_id: domain::id::ConnectionId,
        state: domain::driver::ConnectionState,
    },
    DeviceProtocolStateChanged {
        connection_id: ConnectionId,
        device_id: DeviceId,
        tag_id: Option<TagId>,
        state: DeviceProtocolState,
        reason: Option<String>,
        timestamp: DateTime<Utc>,
    },
    TagWriteCommandHandled {
        connection_id: Option<ConnectionId>,
        tag_id: TagId,
        command_id: Option<String>,
        value: TagValue,
        outcome: WriteCommandOutcome,
        reason: Option<String>,
        timestamp: DateTime<Utc>,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum DeviceProtocolState {
    Connected,
    Error,
}

#[derive(Clone)]
pub struct EventBus {
    sender: broadcast::Sender<RuntimeEvent>,
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }

    pub fn publish(&self, event: RuntimeEvent) {
        let _ = self.sender.send(event);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<RuntimeEvent> {
        self.sender.subscribe()
    }
}

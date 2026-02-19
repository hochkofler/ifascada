use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

mod publisher;
pub use publisher::EventPublisher;

use crate::tag::{TagId, TagQuality};

/// Domain events that can occur in the system
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum DomainEvent {
    /// Tag successfully connected to device
    TagConnected {
        tag_id: TagId,
        timestamp: DateTime<Utc>,
    },

    /// Tag disconnected from device
    TagDisconnected {
        tag_id: TagId,
        reason: String,
        timestamp: DateTime<Utc>,
    },

    /// Tag value was updated
    TagValueUpdated {
        tag_id: TagId,
        value: serde_json::Value,
        quality: TagQuality,
        timestamp: DateTime<Utc>,
    },

    /// Edge agent heartbeat
    AgentHeartbeat {
        agent_id: String,
        uptime_secs: u64,
        active_tags: usize,
        active_tag_ids: Vec<String>,
        timestamp: DateTime<Utc>,
    },

    /// Tag executor error occurred
    TagExecutorError {
        tag_id: TagId,
        error: String,
        timestamp: DateTime<Utc>,
    },

    /// A group of readings (report) was completed
    ReportCompleted {
        report_id: String,
        agent_id: String,
        items: Vec<ReportItem>,
        timestamp: DateTime<Utc>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportItem {
    pub value: serde_json::Value,
    pub timestamp: DateTime<Utc>,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
}

impl DomainEvent {
    /// Create a ReportCompleted event
    pub fn report_completed(report_id: String, agent_id: String, items: Vec<ReportItem>) -> Self {
        Self::ReportCompleted {
            report_id,
            agent_id,
            items,
            timestamp: Utc::now(),
        }
    }
    /// Create a TagConnected event
    pub fn tag_connected(tag_id: TagId) -> Self {
        Self::TagConnected {
            tag_id,
            timestamp: Utc::now(),
        }
    }

    /// Create a TagDisconnected event
    pub fn tag_disconnected(tag_id: TagId, reason: impl Into<String>) -> Self {
        Self::TagDisconnected {
            tag_id,
            reason: reason.into(),
            timestamp: Utc::now(),
        }
    }

    /// Create a TagValueUpdated event
    pub fn tag_value_updated(tag_id: TagId, value: serde_json::Value, quality: TagQuality) -> Self {
        Self::TagValueUpdated {
            tag_id,
            value,
            quality,
            timestamp: Utc::now(),
        }
    }

    /// Create an AgentHeartbeat event
    pub fn agent_heartbeat(
        agent_id: impl Into<String>,
        uptime_secs: u64,
        active_tag_ids: Vec<String>,
    ) -> Self {
        let active_tags = active_tag_ids.len();
        Self::AgentHeartbeat {
            agent_id: agent_id.into(),
            uptime_secs,
            active_tags,
            active_tag_ids,
            timestamp: Utc::now(),
        }
    }

    /// Create a TagExecutorError event
    pub fn tag_executor_error(tag_id: TagId, error: impl Into<String>) -> Self {
        Self::TagExecutorError {
            tag_id,
            error: error.into(),
            timestamp: Utc::now(),
        }
    }

    /// Get the timestamp of this event
    pub fn timestamp(&self) -> DateTime<Utc> {
        match self {
            Self::TagConnected { timestamp, .. } => *timestamp,
            Self::TagDisconnected { timestamp, .. } => *timestamp,
            Self::TagValueUpdated { timestamp, .. } => *timestamp,
            Self::AgentHeartbeat { timestamp, .. } => *timestamp,
            Self::TagExecutorError { timestamp, .. } => *timestamp,
            Self::ReportCompleted { timestamp, .. } => *timestamp,
        }
    }

    /// Get the event type as string
    pub fn event_type(&self) -> &str {
        match self {
            Self::TagConnected { .. } => "TagConnected",
            Self::TagDisconnected { .. } => "TagDisconnected",
            Self::TagValueUpdated { .. } => "TagValueUpdated",
            Self::AgentHeartbeat { .. } => "AgentHeartbeat",
            Self::TagExecutorError { .. } => "TagExecutorError",
            Self::ReportCompleted { .. } => "ReportCompleted",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_tag_connected_event() {
        let tag_id = TagId::new("TEST_TAG").unwrap();
        let event = DomainEvent::tag_connected(tag_id.clone());

        assert_eq!(event.event_type(), "TagConnected");
        match event {
            DomainEvent::TagConnected { tag_id: id, .. } => {
                assert_eq!(id, tag_id);
            }
            _ => panic!("Wrong event type"),
        }
    }

    #[test]
    fn test_tag_value_updated_event() {
        let tag_id = TagId::new("TEST_TAG").unwrap();
        let value = json!({"weight": 25.5});
        let event = DomainEvent::tag_value_updated(tag_id.clone(), value.clone(), TagQuality::Good);

        assert_eq!(event.event_type(), "TagValueUpdated");
        match event {
            DomainEvent::TagValueUpdated {
                tag_id: id,
                value: v,
                quality,
                ..
            } => {
                assert_eq!(id, tag_id);
                assert_eq!(v, value);
                assert_eq!(quality, TagQuality::Good);
            }
            _ => panic!("Wrong event type"),
        }
    }

    #[test]
    fn test_agent_heartbeat_event() {
        let event = DomainEvent::agent_heartbeat(
            "agent-1",
            300,
            vec!["tag-1".to_string(), "tag-2".to_string()],
        );

        assert_eq!(event.event_type(), "AgentHeartbeat");
        match event {
            DomainEvent::AgentHeartbeat {
                agent_id,
                uptime_secs,
                active_tags,
                active_tag_ids,
                ..
            } => {
                assert_eq!(agent_id, "agent-1");
                assert_eq!(uptime_secs, 300);
                assert_eq!(active_tags, 2);
                assert_eq!(active_tag_ids, vec!["tag-1", "tag-2"]);
            }
            _ => panic!("Wrong event type"),
        }
    }

    #[test]
    fn test_event_serialization() {
        let tag_id = TagId::new("TEST_TAG").unwrap();
        let event = DomainEvent::tag_connected(tag_id);

        let json_str = serde_json::to_string(&event).unwrap();
        let deserialized: DomainEvent = serde_json::from_str(&json_str).unwrap();

        assert_eq!(deserialized.event_type(), "TagConnected");
    }
}

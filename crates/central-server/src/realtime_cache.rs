use crate::messages::{
    AlertRuntimeMessage, HealthRuntimeMessage, TagTelemetryMessage, WriteAckMessage,
    WriteAuditMessage,
};
use anyhow::Result;
use redis::Commands;
use serde_json::json;
use std::sync::Mutex;

pub trait CentralRealtimeCache: Send + Sync {
    fn on_telemetry(&self, site: &str, agent: &str, msg: &TagTelemetryMessage) -> Result<()>;
    fn on_health(&self, site: &str, agent: &str, msg: &HealthRuntimeMessage) -> Result<()>;
    fn on_alert(&self, site: &str, agent: &str, msg: &AlertRuntimeMessage) -> Result<()>;
    fn on_write_ack(&self, site: &str, agent: &str, msg: &WriteAckMessage) -> Result<()>;
    fn on_write_audit(&self, site: &str, agent: &str, msg: &WriteAuditMessage) -> Result<()>;
}

pub struct NoopRealtimeCache;

impl CentralRealtimeCache for NoopRealtimeCache {
    fn on_telemetry(&self, _site: &str, _agent: &str, _msg: &TagTelemetryMessage) -> Result<()> {
        Ok(())
    }
    fn on_health(&self, _site: &str, _agent: &str, _msg: &HealthRuntimeMessage) -> Result<()> {
        Ok(())
    }
    fn on_alert(&self, _site: &str, _agent: &str, _msg: &AlertRuntimeMessage) -> Result<()> {
        Ok(())
    }
    fn on_write_ack(&self, _site: &str, _agent: &str, _msg: &WriteAckMessage) -> Result<()> {
        Ok(())
    }
    fn on_write_audit(&self, _site: &str, _agent: &str, _msg: &WriteAuditMessage) -> Result<()> {
        Ok(())
    }
}

pub struct RedisCentralRealtimeCache {
    conn: Mutex<redis::Connection>,
    event_channel: String,
    key_ttl_secs: u64,
}

impl RedisCentralRealtimeCache {
    pub fn connect(url: &str, event_channel: String, key_ttl_secs: u64) -> Result<Self> {
        let client = redis::Client::open(url)?;
        let conn = client.get_connection()?;
        Ok(Self {
            conn: Mutex::new(conn),
            event_channel,
            key_ttl_secs,
        })
    }

    fn set_json_with_ttl(&self, key: &str, value: &serde_json::Value) -> Result<()> {
        let payload = serde_json::to_string(value)?;
        let mut conn = self.conn.lock().expect("redis lock");
        let _: () = conn.set_ex(key, payload, self.key_ttl_secs)?;
        Ok(())
    }

    fn publish_event(&self, event_type: &str, site: &str, agent: &str, payload: serde_json::Value) -> Result<()> {
        let evt = json!({
            "event_type": event_type,
            "site": site,
            "agent": agent,
            "payload": payload,
            "published_at": chrono::Utc::now(),
        });
        let encoded = serde_json::to_string(&evt)?;
        let mut conn = self.conn.lock().expect("redis lock");
        let _: i64 = conn.publish(&self.event_channel, encoded)?;
        Ok(())
    }
}

impl CentralRealtimeCache for RedisCentralRealtimeCache {
    fn on_telemetry(&self, site: &str, agent: &str, msg: &TagTelemetryMessage) -> Result<()> {
        let key = format!("scada:tag:{}:{}:{}:current", site, agent, msg.tag_id);
        let payload = json!({
            "tag_id": msg.tag_id,
            "value": msg.value,
            "quality": msg.quality,
            "timestamp": msg.timestamp,
            "source": msg.source
        });
        self.set_json_with_ttl(&key, &payload)?;
        self.publish_event("tag_current", site, agent, payload)
    }

    fn on_health(&self, site: &str, agent: &str, msg: &HealthRuntimeMessage) -> Result<()> {
        let key = format!("scada:edge:{}:{}:status", site, agent);
        let payload = json!({
            "status": msg.status,
            "outbox_depth": msg.outbox_depth,
            "outbox_oldest_age_secs": msg.outbox_oldest_age_secs,
            "timestamp": msg.timestamp,
            "source": msg.source
        });
        self.set_json_with_ttl(&key, &payload)?;
        self.publish_event("edge_status", site, agent, payload)
    }

    fn on_alert(&self, site: &str, agent: &str, msg: &AlertRuntimeMessage) -> Result<()> {
        self.publish_event("runtime_alert", site, agent, serde_json::to_value(msg)?)
    }

    fn on_write_ack(&self, site: &str, agent: &str, msg: &WriteAckMessage) -> Result<()> {
        self.publish_event("write_ack", site, agent, serde_json::to_value(msg)?)
    }

    fn on_write_audit(&self, site: &str, agent: &str, msg: &WriteAuditMessage) -> Result<()> {
        self.publish_event("write_audit", site, agent, serde_json::to_value(msg)?)
    }
}

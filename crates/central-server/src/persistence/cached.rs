use crate::messages::{
    ActionAuditMessage, ActionResultMessage, AlertRuntimeMessage, ConfigApplyResultMessage,
    ConnectionStateMessage, ControlResetResultMessage, DeviceConnectionStateMessage,
    HealthRuntimeMessage, TagTelemetryMessage, WriteAckMessage, WriteAuditMessage,
};
use crate::persistence::CentralPersistence;
use crate::realtime_cache::CentralRealtimeCache;
use async_trait::async_trait;
use anyhow::Result;
use tracing::warn;

pub struct CachedCentralPersistence<P: CentralPersistence> {
    inner: P,
    cache: Box<dyn CentralRealtimeCache>,
}

impl<P: CentralPersistence> CachedCentralPersistence<P> {
    pub fn new(inner: P, cache: Box<dyn CentralRealtimeCache>) -> Self {
        Self { inner, cache }
    }
}

#[async_trait]
impl<P: CentralPersistence> CentralPersistence for CachedCentralPersistence<P> {
    async fn insert_telemetry(
        &self,
        site: &str,
        agent: &str,
        msg: &TagTelemetryMessage,
    ) -> Result<()> {
        self.inner.insert_telemetry(site, agent, msg).await?;
        if let Err(e) = self.cache.on_telemetry(site, agent, msg) {
            warn!("redis cache on_telemetry failed: {}", e);
        }
        Ok(())
    }

    async fn insert_health(
        &self,
        site: &str,
        agent: &str,
        msg: &HealthRuntimeMessage,
    ) -> Result<()> {
        self.inner.insert_health(site, agent, msg).await?;
        if let Err(e) = self.cache.on_health(site, agent, msg) {
            warn!("redis cache on_health failed: {}", e);
        }
        Ok(())
    }

    async fn insert_alert(&self, site: &str, agent: &str, msg: &AlertRuntimeMessage) -> Result<()> {
        self.inner.insert_alert(site, agent, msg).await?;
        if let Err(e) = self.cache.on_alert(site, agent, msg) {
            warn!("redis cache on_alert failed: {}", e);
        }
        Ok(())
    }

    async fn insert_write_ack(
        &self,
        site: &str,
        agent: &str,
        msg: &WriteAckMessage,
    ) -> Result<()> {
        self.inner.insert_write_ack(site, agent, msg).await?;
        if let Err(e) = self.cache.on_write_ack(site, agent, msg) {
            warn!("redis cache on_write_ack failed: {}", e);
        }
        Ok(())
    }

    async fn insert_action_result(
        &self,
        site: &str,
        agent: &str,
        msg: &ActionResultMessage,
    ) -> Result<()> {
        self.inner.insert_action_result(site, agent, msg).await?;
        Ok(())
    }

    async fn insert_action_audit(
        &self,
        site: &str,
        agent: &str,
        msg: &ActionAuditMessage,
    ) -> Result<()> {
        self.inner.insert_action_audit(site, agent, msg).await?;
        Ok(())
    }

    async fn insert_write_audit(
        &self,
        site: &str,
        agent: &str,
        msg: &WriteAuditMessage,
    ) -> Result<()> {
        self.inner.insert_write_audit(site, agent, msg).await?;
        if let Err(e) = self.cache.on_write_audit(site, agent, msg) {
            warn!("redis cache on_write_audit failed: {}", e);
        }
        Ok(())
    }

    async fn insert_config_apply_result(
        &self,
        site: &str,
        agent: &str,
        msg: &ConfigApplyResultMessage,
    ) -> Result<()> {
        self.inner.insert_config_apply_result(site, agent, msg).await?;
        Ok(())
    }

    async fn insert_control_reset_result(
        &self,
        site: &str,
        agent: &str,
        msg: &ControlResetResultMessage,
    ) -> Result<()> {
        self.inner.insert_control_reset_result(site, agent, msg).await?;
        Ok(())
    }

    async fn insert_connection_state(
        &self,
        site: &str,
        agent: &str,
        msg: &ConnectionStateMessage,
    ) -> Result<()> {
        self.inner.insert_connection_state(site, agent, msg).await?;
        Ok(())
    }

    async fn insert_device_connection_state(
        &self,
        site: &str,
        agent: &str,
        msg: &DeviceConnectionStateMessage,
    ) -> Result<()> {
        self.inner
            .insert_device_connection_state(site, agent, msg)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::memory::InMemoryCentralPersistence;
    use crate::realtime_cache::CentralRealtimeCache;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Default)]
    struct CountingCache {
        telemetry: AtomicUsize,
    }

    impl CentralRealtimeCache for CountingCache {
        fn on_telemetry(
            &self,
            _site: &str,
            _agent: &str,
            _msg: &TagTelemetryMessage,
        ) -> Result<()> {
            self.telemetry.fetch_add(1, Ordering::Relaxed);
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
        fn on_write_audit(
            &self,
            _site: &str,
            _agent: &str,
            _msg: &WriteAuditMessage,
        ) -> Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn cache_layer_keeps_inner_contract() {
        let inner = InMemoryCentralPersistence::default();
        let cached = CachedCentralPersistence::new(inner, Box::new(CountingCache::default()));
        let msg = TagTelemetryMessage {
            schema_version: 1,
            source: "edge-agent".to_string(),
            tag_id: "tag-1".to_string(),
            value: serde_json::json!(1.0),
            quality: serde_json::json!({"status":"Good","reason":"None"}),
            timestamp: chrono::Utc::now(),
        };
        cached
            .insert_telemetry("plant-a", "edge-01", &msg)
            .await
            .unwrap();
    }
}

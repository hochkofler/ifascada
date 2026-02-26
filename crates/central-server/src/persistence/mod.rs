use crate::messages::{
    ActionAuditMessage, ActionResultMessage, AlertRuntimeMessage, ConfigApplyResultMessage,
    ConnectionStateMessage, ControlResetResultMessage, DeviceConnectionStateMessage,
    HealthRuntimeMessage, TagTelemetryMessage, WriteAckMessage, WriteAuditMessage,
};
use async_trait::async_trait;
use anyhow::Result;

#[async_trait]
pub trait CentralPersistence: Send + Sync {
    async fn insert_telemetry(
        &self,
        site: &str,
        agent: &str,
        msg: &TagTelemetryMessage,
    ) -> Result<()>;
    async fn insert_health(
        &self,
        site: &str,
        agent: &str,
        msg: &HealthRuntimeMessage,
    ) -> Result<()>;
    async fn insert_alert(&self, site: &str, agent: &str, msg: &AlertRuntimeMessage) -> Result<()>;
    async fn insert_write_ack(
        &self,
        site: &str,
        agent: &str,
        msg: &WriteAckMessage,
    ) -> Result<()>;
    async fn insert_action_result(
        &self,
        site: &str,
        agent: &str,
        msg: &ActionResultMessage,
    ) -> Result<()>;
    async fn insert_action_audit(
        &self,
        site: &str,
        agent: &str,
        msg: &ActionAuditMessage,
    ) -> Result<()>;
    async fn insert_write_audit(
        &self,
        site: &str,
        agent: &str,
        msg: &WriteAuditMessage,
    ) -> Result<()>;
    async fn insert_config_apply_result(
        &self,
        site: &str,
        agent: &str,
        msg: &ConfigApplyResultMessage,
    ) -> Result<()>;
    async fn insert_control_reset_result(
        &self,
        site: &str,
        agent: &str,
        msg: &ControlResetResultMessage,
    ) -> Result<()>;
    async fn insert_connection_state(
        &self,
        site: &str,
        agent: &str,
        msg: &ConnectionStateMessage,
    ) -> Result<()>;
    async fn insert_device_connection_state(
        &self,
        site: &str,
        agent: &str,
        msg: &DeviceConnectionStateMessage,
    ) -> Result<()>;
}

pub mod cached;
pub mod postgres;

#[cfg(test)]
pub mod memory {
    use super::*;
    use async_trait::async_trait;
    use std::sync::Mutex;

    #[derive(Default)]
    pub struct InMemoryCentralPersistence {
        pub telemetry: Mutex<Vec<(String, String, TagTelemetryMessage)>>,
        pub health: Mutex<Vec<(String, String, HealthRuntimeMessage)>>,
        pub alerts: Mutex<Vec<(String, String, AlertRuntimeMessage)>>,
        pub ack: Mutex<Vec<(String, String, WriteAckMessage)>>,
        pub action_result: Mutex<Vec<(String, String, ActionResultMessage)>>,
        pub action_audit: Mutex<Vec<(String, String, ActionAuditMessage)>>,
        pub audit: Mutex<Vec<(String, String, WriteAuditMessage)>>,
        pub config_apply_result: Mutex<Vec<(String, String, ConfigApplyResultMessage)>>,
        pub control_reset_result: Mutex<Vec<(String, String, ControlResetResultMessage)>>,
        pub connection_state: Mutex<Vec<(String, String, ConnectionStateMessage)>>,
        pub device_connection_state: Mutex<Vec<(String, String, DeviceConnectionStateMessage)>>,
    }

    #[async_trait]
    impl CentralPersistence for InMemoryCentralPersistence {
        async fn insert_telemetry(
            &self,
            site: &str,
            agent: &str,
            msg: &TagTelemetryMessage,
        ) -> Result<()> {
            self.telemetry
                .lock()
                .expect("telemetry lock")
                .push((site.to_string(), agent.to_string(), msg.clone()));
            Ok(())
        }

        async fn insert_health(
            &self,
            site: &str,
            agent: &str,
            msg: &HealthRuntimeMessage,
        ) -> Result<()> {
            self.health
                .lock()
                .expect("health lock")
                .push((site.to_string(), agent.to_string(), msg.clone()));
            Ok(())
        }

        async fn insert_alert(
            &self,
            site: &str,
            agent: &str,
            msg: &AlertRuntimeMessage,
        ) -> Result<()> {
            self.alerts
                .lock()
                .expect("alerts lock")
                .push((site.to_string(), agent.to_string(), msg.clone()));
            Ok(())
        }

        async fn insert_write_ack(
            &self,
            site: &str,
            agent: &str,
            msg: &WriteAckMessage,
        ) -> Result<()> {
            self.ack
                .lock()
                .expect("ack lock")
                .push((site.to_string(), agent.to_string(), msg.clone()));
            Ok(())
        }

        async fn insert_action_result(
            &self,
            site: &str,
            agent: &str,
            msg: &ActionResultMessage,
        ) -> Result<()> {
            self.action_result
                .lock()
                .expect("action_result lock")
                .push((site.to_string(), agent.to_string(), msg.clone()));
            Ok(())
        }

        async fn insert_action_audit(
            &self,
            site: &str,
            agent: &str,
            msg: &ActionAuditMessage,
        ) -> Result<()> {
            self.action_audit
                .lock()
                .expect("action_audit lock")
                .push((site.to_string(), agent.to_string(), msg.clone()));
            Ok(())
        }

        async fn insert_write_audit(
            &self,
            site: &str,
            agent: &str,
            msg: &WriteAuditMessage,
        ) -> Result<()> {
            self.audit
                .lock()
                .expect("audit lock")
                .push((site.to_string(), agent.to_string(), msg.clone()));
            Ok(())
        }

        async fn insert_config_apply_result(
            &self,
            site: &str,
            agent: &str,
            msg: &ConfigApplyResultMessage,
        ) -> Result<()> {
            self.config_apply_result
                .lock()
                .expect("config_apply_result lock")
                .push((site.to_string(), agent.to_string(), msg.clone()));
            Ok(())
        }

        async fn insert_control_reset_result(
            &self,
            site: &str,
            agent: &str,
            msg: &ControlResetResultMessage,
        ) -> Result<()> {
            self.control_reset_result
                .lock()
                .expect("control_reset_result lock")
                .push((site.to_string(), agent.to_string(), msg.clone()));
            Ok(())
        }

        async fn insert_connection_state(
            &self,
            site: &str,
            agent: &str,
            msg: &ConnectionStateMessage,
        ) -> Result<()> {
            self.connection_state
                .lock()
                .expect("connection_state lock")
                .push((site.to_string(), agent.to_string(), msg.clone()));
            Ok(())
        }

        async fn insert_device_connection_state(
            &self,
            site: &str,
            agent: &str,
            msg: &DeviceConnectionStateMessage,
        ) -> Result<()> {
            self.device_connection_state
                .lock()
                .expect("device_connection_state lock")
                .push((site.to_string(), agent.to_string(), msg.clone()));
            Ok(())
        }
    }
}

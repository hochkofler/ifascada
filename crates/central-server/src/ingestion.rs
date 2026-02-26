use crate::messages::{
    ActionAuditMessage, ActionResultMessage, AlertRuntimeMessage, ConfigApplyResultMessage,
    ConnectionStateMessage, ControlResetResultMessage, DeviceConnectionStateMessage,
    HealthRuntimeMessage, TagTelemetryMessage, WriteAckMessage, WriteAuditMessage,
};
use crate::persistence::CentralPersistence;
use crate::topic::{ScadaTopicKind, parse_scada_topic};
use anyhow::{Result, anyhow};
use std::fmt::{Display, Formatter};
use tracing::{debug, warn};

const SUPPORTED_SCHEMA_VERSION: u16 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IngestionOutcome {
    Telemetry,
    ActionResult,
    ActionAudit,
    Health,
    Alert,
    WriteAck,
    WriteAudit,
    ConfigApplyResult,
    ControlResetResult,
    ConnectionState,
    DeviceConnectionState,
}

#[derive(Debug)]
pub enum IngestionError {
    NonRetryable(String),
    Retryable(anyhow::Error),
}

impl Display for IngestionError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NonRetryable(msg) => write!(f, "non-retryable ingest error: {}", msg),
            Self::Retryable(err) => write!(f, "retryable ingest error: {}", err),
        }
    }
}

impl std::error::Error for IngestionError {}

pub struct IngestionService<P: CentralPersistence> {
    persistence: P,
}

impl<P: CentralPersistence> IngestionService<P> {
    pub fn new(persistence: P) -> Self {
        Self { persistence }
    }

    pub async fn ingest(&self, topic: &str, payload: &[u8]) -> Result<IngestionOutcome, IngestionError> {
        let parsed_topic = parse_scada_topic(topic).ok_or_else(|| {
            IngestionError::NonRetryable(format!("unsupported topic '{}'", topic))
        })?;
        debug!(
            "ingest parsed topic='{}' kind={:?} site='{}' agent='{}' bytes={}",
            topic,
            parsed_topic.kind,
            parsed_topic.site,
            parsed_topic.agent,
            payload.len()
        );

        match parsed_topic.kind {
            ScadaTopicKind::TelemetryTag { .. } => {
                let msg: TagTelemetryMessage = serde_json::from_slice(payload).map_err(non_retryable)?;
                validate_schema(msg.schema_version).map_err(non_retryable)?;
                self.persistence
                    .insert_telemetry(&parsed_topic.site, &parsed_topic.agent, &msg)
                    .await
                    .map_err(IngestionError::Retryable)?;
                debug!(
                    "ingest telemetry persisted site='{}' agent='{}' tag='{}' ts={}",
                    parsed_topic.site, parsed_topic.agent, msg.tag_id, msg.timestamp
                );
                Ok(IngestionOutcome::Telemetry)
            }
            ScadaTopicKind::CommandActionResult => {
                let msg: ActionResultMessage = serde_json::from_slice(payload).map_err(non_retryable)?;
                validate_schema(msg.schema_version).map_err(non_retryable)?;
                self.persistence
                    .insert_action_result(&parsed_topic.site, &parsed_topic.agent, &msg)
                    .await
                    .map_err(IngestionError::Retryable)?;
                Ok(IngestionOutcome::ActionResult)
            }
            ScadaTopicKind::AuditAction => {
                let msg: ActionAuditMessage = serde_json::from_slice(payload).map_err(non_retryable)?;
                validate_schema(msg.schema_version).map_err(non_retryable)?;
                self.persistence
                    .insert_action_audit(&parsed_topic.site, &parsed_topic.agent, &msg)
                    .await
                    .map_err(IngestionError::Retryable)?;
                Ok(IngestionOutcome::ActionAudit)
            }
            ScadaTopicKind::HealthRuntime => {
                let msg: HealthRuntimeMessage = serde_json::from_slice(payload).map_err(non_retryable)?;
                validate_schema(msg.schema_version).map_err(non_retryable)?;
                self.persistence
                    .insert_health(&parsed_topic.site, &parsed_topic.agent, &msg)
                    .await
                    .map_err(IngestionError::Retryable)?;
                debug!(
                    "ingest health persisted site='{}' agent='{}' status='{}' ts={}",
                    parsed_topic.site, parsed_topic.agent, msg.status, msg.timestamp
                );
                Ok(IngestionOutcome::Health)
            }
            ScadaTopicKind::AlertsRuntime => {
                let msg: AlertRuntimeMessage = serde_json::from_slice(payload).map_err(non_retryable)?;
                validate_schema(msg.schema_version).map_err(non_retryable)?;
                self.persistence
                    .insert_alert(&parsed_topic.site, &parsed_topic.agent, &msg)
                    .await
                    .map_err(IngestionError::Retryable)?;
                Ok(IngestionOutcome::Alert)
            }
            ScadaTopicKind::CommandWriteAck => {
                let msg: WriteAckMessage = serde_json::from_slice(payload).map_err(non_retryable)?;
                validate_schema(msg.schema_version).map_err(non_retryable)?;
                self.persistence
                    .insert_write_ack(&parsed_topic.site, &parsed_topic.agent, &msg)
                    .await
                    .map_err(IngestionError::Retryable)?;
                Ok(IngestionOutcome::WriteAck)
            }
            ScadaTopicKind::AuditWrite => {
                let msg: WriteAuditMessage = serde_json::from_slice(payload).map_err(non_retryable)?;
                validate_schema(msg.schema_version).map_err(non_retryable)?;
                self.persistence
                    .insert_write_audit(&parsed_topic.site, &parsed_topic.agent, &msg)
                    .await
                    .map_err(IngestionError::Retryable)?;
                Ok(IngestionOutcome::WriteAudit)
            }
            ScadaTopicKind::ConfigApplyResult => {
                let msg: ConfigApplyResultMessage =
                    serde_json::from_slice(payload).map_err(non_retryable)?;
                validate_schema(msg.schema_version).map_err(non_retryable)?;
                self.persistence
                    .insert_config_apply_result(&parsed_topic.site, &parsed_topic.agent, &msg)
                    .await
                    .map_err(IngestionError::Retryable)?;
                Ok(IngestionOutcome::ConfigApplyResult)
            }
            ScadaTopicKind::ControlResetResult => {
                let msg: ControlResetResultMessage =
                    serde_json::from_slice(payload).map_err(non_retryable)?;
                validate_schema(msg.schema_version).map_err(non_retryable)?;
                self.persistence
                    .insert_control_reset_result(&parsed_topic.site, &parsed_topic.agent, &msg)
                    .await
                    .map_err(IngestionError::Retryable)?;
                Ok(IngestionOutcome::ControlResetResult)
            }
            ScadaTopicKind::ConnectionState => {
                let msg: ConnectionStateMessage = serde_json::from_slice(payload).map_err(non_retryable)?;
                validate_schema(msg.schema_version).map_err(non_retryable)?;
                self.persistence
                    .insert_connection_state(&parsed_topic.site, &parsed_topic.agent, &msg)
                    .await
                    .map_err(IngestionError::Retryable)?;
                debug!(
                    "ingest connection state persisted site='{}' agent='{}' connection='{}' state='{}' ts={}",
                    parsed_topic.site, parsed_topic.agent, msg.connection_id, msg.state, msg.timestamp
                );
                Ok(IngestionOutcome::ConnectionState)
            }
            ScadaTopicKind::DeviceConnectionState => {
                let msg: DeviceConnectionStateMessage =
                    serde_json::from_slice(payload).map_err(non_retryable)?;
                validate_schema(msg.schema_version).map_err(non_retryable)?;
                self.persistence
                    .insert_device_connection_state(&parsed_topic.site, &parsed_topic.agent, &msg)
                    .await
                    .map_err(IngestionError::Retryable)?;
                debug!(
                    "ingest device connection state persisted site='{}' agent='{}' connection='{}' device='{}' state='{}' ts={}",
                    parsed_topic.site,
                    parsed_topic.agent,
                    msg.connection_id,
                    msg.device_id,
                    msg.state,
                    msg.timestamp
                );
                Ok(IngestionOutcome::DeviceConnectionState)
            }
            ScadaTopicKind::AlertsRuntimeAck | ScadaTopicKind::AlertsRuntimeAckResult => {
                warn!("ack/ack-result alerts are not persisted yet");
                Err(IngestionError::NonRetryable(
                    "topic recognized but not persisted yet".to_string(),
                ))
            }
        }
    }
}

fn non_retryable<E: std::fmt::Display>(err: E) -> IngestionError {
    IngestionError::NonRetryable(err.to_string())
}

fn validate_schema(version: u16) -> Result<()> {
    if version != SUPPORTED_SCHEMA_VERSION {
        return Err(anyhow!(
            "unsupported schema_version '{}', expected '{}'",
            version,
            SUPPORTED_SCHEMA_VERSION
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::memory::InMemoryCentralPersistence;

    #[tokio::test]
    async fn ingests_telemetry_message() {
        let store = InMemoryCentralPersistence::default();
        let svc = IngestionService::new(store);
        let topic = "scada/plant-a/edge/edge-01/telemetry/tag/tag_1";
        let payload = br#"{
            "schema_version":1,
            "source":"edge-agent",
            "tag_id":"tag_1",
            "value":12.3,
            "quality":{"status":"Good","reason":"None"},
            "timestamp":"2026-02-21T14:00:00Z"
        }"#;
        let outcome = svc.ingest(topic, payload).await.unwrap();
        assert_eq!(outcome, IngestionOutcome::Telemetry);
    }

    #[tokio::test]
    async fn ingests_write_audit_message() {
        let store = InMemoryCentralPersistence::default();
        let svc = IngestionService::new(store);
        let topic = "scada/plant-a/edge/edge-01/audit/write";
        let payload = br#"{
            "schema_version":1,
            "source":"edge-agent",
            "connection_id":"conn-1",
            "tag_id":"tag_1",
            "command_id":"cmd-1",
            "value":12.0,
            "outcome":"Applied",
            "reason":null,
            "timestamp":"2026-02-21T14:00:00Z"
        }"#;
        let outcome = svc.ingest(topic, payload).await.unwrap();
        assert_eq!(outcome, IngestionOutcome::WriteAudit);
    }

    #[tokio::test]
    async fn ingests_action_result_message() {
        let store = InMemoryCentralPersistence::default();
        let svc = IngestionService::new(store);
        let topic = "scada/plant-a/edge/edge-01/cmd/action/result";
        let payload = br#"{
            "schema_version":1,
            "source":"edge-agent",
            "request_id":"act-1",
            "action_type":"print.escpos",
            "accepted":true,
            "reason":null,
            "timestamp":"2026-02-24T14:00:00Z"
        }"#;
        let outcome = svc.ingest(topic, payload).await.unwrap();
        assert_eq!(outcome, IngestionOutcome::ActionResult);
    }

    #[tokio::test]
    async fn rejects_unsupported_schema_version() {
        let store = InMemoryCentralPersistence::default();
        let svc = IngestionService::new(store);
        let topic = "scada/plant-a/edge/edge-01/health/runtime";
        let payload = br#"{
            "schema_version":2,
            "source":"edge-agent",
            "status":"ok",
            "outbox_depth":0,
            "outbox_oldest_age_secs":null,
            "timestamp":"2026-02-21T14:00:00Z"
        }"#;
        let err = svc
            .ingest(topic, payload)
            .await
            .expect_err("must reject schema v2");
        assert!(err.to_string().contains("unsupported schema_version"));
    }

    #[tokio::test]
    async fn ingests_device_connection_state_message() {
        let store = InMemoryCentralPersistence::default();
        let svc = IngestionService::new(store);
        let topic = "scada/plant-a/edge/edge-01/device/conn/state";
        let payload = br#"{
            "schema_version":1,
            "source":"edge-agent",
            "connection_id":"conn_modbus_rtu_com10_1",
            "device_id":"dev_modbus_100",
            "tag_id":"tag_airborne_particle_pm1",
            "state":"Error",
            "reason":"modbus read timeout after 300 ms",
            "timestamp":"2026-02-23T15:00:00Z"
        }"#;
        let outcome = svc.ingest(topic, payload).await.unwrap();
        assert_eq!(outcome, IngestionOutcome::DeviceConnectionState);
    }
}

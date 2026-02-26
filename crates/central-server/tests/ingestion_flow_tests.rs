use anyhow::Result;
use async_trait::async_trait;
use central_server::ingestion::{IngestionOutcome, IngestionService};
use central_server::messages::{
    ActionAuditMessage, ActionResultMessage, AlertRuntimeMessage, ConfigApplyResultMessage, ConnectionStateMessage,
    ControlResetResultMessage, DeviceConnectionStateMessage, HealthRuntimeMessage,
    TagTelemetryMessage, WriteAckMessage, WriteAuditMessage,
};
use central_server::persistence::CentralPersistence;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Default)]
struct CountingPersistence {
    telemetry: AtomicUsize,
    health: AtomicUsize,
    alerts: AtomicUsize,
    ack: AtomicUsize,
    audit: AtomicUsize,
    config_apply_result: AtomicUsize,
    control_reset_result: AtomicUsize,
    connection_state: AtomicUsize,
    device_connection_state: AtomicUsize,
    action_result: AtomicUsize,
    action_audit: AtomicUsize,
}

#[async_trait]
impl CentralPersistence for CountingPersistence {
    async fn insert_telemetry(
        &self,
        _site: &str,
        _agent: &str,
        _msg: &TagTelemetryMessage,
    ) -> Result<()> {
        self.telemetry.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    async fn insert_health(
        &self,
        _site: &str,
        _agent: &str,
        _msg: &HealthRuntimeMessage,
    ) -> Result<()> {
        self.health.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    async fn insert_alert(
        &self,
        _site: &str,
        _agent: &str,
        _msg: &AlertRuntimeMessage,
    ) -> Result<()> {
        self.alerts.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    async fn insert_write_ack(
        &self,
        _site: &str,
        _agent: &str,
        _msg: &WriteAckMessage,
    ) -> Result<()> {
        self.ack.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    async fn insert_write_audit(
        &self,
        _site: &str,
        _agent: &str,
        _msg: &WriteAuditMessage,
    ) -> Result<()> {
        self.audit.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    async fn insert_config_apply_result(
        &self,
        _site: &str,
        _agent: &str,
        _msg: &ConfigApplyResultMessage,
    ) -> Result<()> {
        self.config_apply_result.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    async fn insert_control_reset_result(
        &self,
        _site: &str,
        _agent: &str,
        _msg: &ControlResetResultMessage,
    ) -> Result<()> {
        self.control_reset_result.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    async fn insert_connection_state(
        &self,
        _site: &str,
        _agent: &str,
        _msg: &ConnectionStateMessage,
    ) -> Result<()> {
        self.connection_state.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    async fn insert_device_connection_state(
        &self,
        _site: &str,
        _agent: &str,
        _msg: &DeviceConnectionStateMessage,
    ) -> Result<()> {
        self.device_connection_state.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    async fn insert_action_result(
        &self,
        _site: &str,
        _agent: &str,
        _msg: &ActionResultMessage,
    ) -> Result<()> {
        self.action_result.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    async fn insert_action_audit(
        &self,
        _site: &str,
        _agent: &str,
        _msg: &ActionAuditMessage,
    ) -> Result<()> {
        self.action_audit.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
}

#[tokio::test]
async fn routes_write_ack_to_ack_store() {
    let store = CountingPersistence::default();
    let svc = IngestionService::new(store);
    let topic = "scada/plant-a/edge/edge-01/cmd/write/ack";
    let payload = br#"{
      "schema_version":1,
      "source":"edge/edge-01",
      "tag_id":"tag_1",
      "command_id":"cmd-1",
      "success":true,
      "reason":null,
      "timestamp":"2026-02-21T14:00:00Z"
    }"#;

    let outcome = svc.ingest(topic, payload).await.unwrap();
    assert_eq!(outcome, IngestionOutcome::WriteAck);
}

#[tokio::test]
async fn routes_control_reset_result_to_store() {
    let store = CountingPersistence::default();
    let svc = IngestionService::new(store);
    let topic = "scada/plant-a/edge/edge-01/control/reset/result";
    let payload = br#"{
      "schema_version":1,
      "source":"edge/edge-01",
      "request_id":"rst-1",
      "accepted":true,
      "reason":"reset requested",
      "timestamp":"2026-02-21T14:00:00Z"
    }"#;

    let outcome = svc.ingest(topic, payload).await.unwrap();
    assert_eq!(outcome, IngestionOutcome::ControlResetResult);
}

#[tokio::test]
async fn routes_connection_state_to_store() {
    let store = CountingPersistence::default();
    let svc = IngestionService::new(store);
    let topic = "scada/plant-a/edge/edge-01/conn/state";
    let payload = br#"{
      "schema_version":1,
      "source":"edge/edge-01",
      "connection_id":"conn_modbus_tcp_1",
      "state":"Failed",
      "timestamp":"2026-02-21T14:00:00Z"
    }"#;

    let outcome = svc.ingest(topic, payload).await.unwrap();
    assert_eq!(outcome, IngestionOutcome::ConnectionState);
}

#[tokio::test]
async fn routes_device_connection_state_to_store() {
    let store = CountingPersistence::default();
    let svc = IngestionService::new(store);
    let topic = "scada/plant-a/edge/edge-01/device/conn/state";
    let payload = br#"{
      "schema_version":1,
      "source":"edge/edge-01",
      "connection_id":"conn_modbus_rtu_com10_1",
      "device_id":"dev_modbus_100",
      "tag_id":"tag_airborne_particle_pm10",
      "state":"Error",
      "reason":"modbus read timeout after 300 ms",
      "timestamp":"2026-02-21T14:00:00Z"
    }"#;

    let outcome = svc.ingest(topic, payload).await.unwrap();
    assert_eq!(outcome, IngestionOutcome::DeviceConnectionState);
}

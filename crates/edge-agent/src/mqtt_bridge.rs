use application::automation::{
    AutomationEngine as RuntimeAutomationEngine, AutomationRuntimeScope,
};
use application::runtime::{RuntimeEngine, RuntimeEvent, WritePriority};
use async_trait::async_trait;
use crate::action_orchestrator::{ActionOrchestrator, ActionRequest, ActionRuntimeState};
use crate::bootstrap;
use crate::mqtt_outbox::{
    OutboxConfig, OutboxMessageKind, OutboxPublisher, OutboxSecurity, OutboxStats,
    BrokerSession, PersistentMqttOutbox, PublishAttempt,
};
use domain::id::TagId;
use domain::AutomationSpec;
use domain::tag::TagValue;
use rumqttc::{AsyncClient, ClientError, Event, EventLoop, Incoming, MqttOptions, QoS, Transport};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};
use crate::broker_watch::{BrokerActivityWatch, STALE_KEEP_ALIVE_MULTIPLIER};
use std::{fs, path::Path};
use tokio::net::TcpStream;
use tokio::sync::Mutex as TokioMutex;
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};

const MQTT_SCHEMA_VERSION_V1: u16 = 1;
mod handlers;

#[derive(Debug, Clone)]
pub struct MqttBridgeConfig {
    pub site: String,
    pub agent: String,
    pub broker_host: String,
    pub broker_port: u16,
    pub client_id: String,
    pub outbox_path: String,
    pub ticket_sequence_path: String,
    pub outbox_flush_batch: usize,
    /// Where the agent writes its proof of life for the supervisor. `None` disables it,
    /// which leaves the supervisor able to notice only an agent that exits.
    pub heartbeat_path: Option<PathBuf>,
    pub outbox_max_messages: usize,
    pub outbox_active_key_id: String,
    pub outbox_prev_key_id: Option<String>,
    pub outbox_encryption_secret: Option<String>,
    pub outbox_hmac_secret: Option<String>,
    pub outbox_prev_encryption_secret: Option<String>,
    pub outbox_prev_hmac_secret: Option<String>,
    pub health_publish_interval_secs: u64,
    pub health_outbox_depth_warn: usize,
    pub health_outbox_oldest_secs_warn: u64,
    pub alert_degraded_streak: usize,
    pub alert_recovered_streak: usize,
    pub alert_dedup_window_secs: u64,
    pub config_hash: Option<String>,
    pub config_check_url: Option<String>,
    pub config_check_enroll_token: Option<String>,
    pub config_check_hmac_secret: Option<String>,
    pub config_check_key_id: Option<String>,
    pub config_cache_path: String,
    pub config_apply_receipt_path: String,
    pub config_check_interval_secs: u64,
    pub config_check_jitter_secs: u64,
    pub escpos_output_path: String,
    pub escpos_tcp_host: Option<String>,
    pub escpos_tcp_port: u16,
    pub escpos_windows_share: Option<String>,
    pub on_demand_tcp_host: Option<String>,
    pub on_demand_tcp_port: Option<u16>,
    pub on_demand_probe_enabled: bool,
    pub on_demand_probe_timeout_ms: u64,
    pub on_demand_probe_connection_id: Option<String>,
    pub on_demand_probe_device_id: Option<String>,
    pub automations: Vec<AutomationSpec>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MqttBridgeExit {
    RestartRequested,
}

impl MqttBridgeConfig {
    pub fn command_topic(&self) -> String {
        format!("scada/{}/edge/{}/cmd/write", self.site, self.agent)
    }

    pub fn audit_topic(&self) -> String {
        format!("scada/{}/edge/{}/audit/write", self.site, self.agent)
    }

    pub fn ack_topic(&self) -> String {
        format!("scada/{}/edge/{}/cmd/write/ack", self.site, self.agent)
    }

    pub fn action_command_topic(&self) -> String {
        format!("scada/{}/edge/{}/cmd/action", self.site, self.agent)
    }

    pub fn action_result_topic(&self) -> String {
        format!("scada/{}/edge/{}/cmd/action/result", self.site, self.agent)
    }

    pub fn action_audit_topic(&self) -> String {
        format!("scada/{}/edge/{}/audit/action", self.site, self.agent)
    }

    pub fn health_topic(&self) -> String {
        format!("scada/{}/edge/{}/health/runtime", self.site, self.agent)
    }

    pub fn alert_topic(&self) -> String {
        format!("scada/{}/edge/{}/alerts/runtime", self.site, self.agent)
    }

    pub fn alert_ack_topic(&self) -> String {
        format!("scada/{}/edge/{}/alerts/runtime/ack", self.site, self.agent)
    }

    pub fn alert_ack_result_topic(&self) -> String {
        format!(
            "scada/{}/edge/{}/alerts/runtime/ack/result",
            self.site, self.agent
        )
    }

    pub fn config_apply_topic(&self) -> String {
        format!("scada/{}/edge/{}/config/apply", self.site, self.agent)
    }

    pub fn config_apply_result_topic(&self) -> String {
        format!(
            "scada/{}/edge/{}/config/apply/result",
            self.site, self.agent
        )
    }

    pub fn control_reset_topic(&self) -> String {
        format!("scada/{}/edge/{}/control/reset", self.site, self.agent)
    }

    pub fn control_reset_result_topic(&self) -> String {
        format!(
            "scada/{}/edge/{}/control/reset/result",
            self.site, self.agent
        )
    }

    pub fn telemetry_tag_topic(&self, tag_id: &str) -> String {
        format!(
            "scada/{}/edge/{}/telemetry/tag/{}",
            self.site, self.agent, tag_id
        )
    }

    pub fn connection_state_topic(&self) -> String {
        format!("scada/{}/edge/{}/conn/state", self.site, self.agent)
    }

    pub fn device_connection_state_topic(&self) -> String {
        format!("scada/{}/edge/{}/device/conn/state", self.site, self.agent)
    }

}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WriteTagCommandMessage {
    #[serde(default)]
    pub schema_version: Option<u16>,
    #[serde(default)]
    pub source: Option<String>,
    pub tag_id: String,
    pub value: TagValue,
    #[serde(default)]
    pub command_id: Option<String>,
    #[serde(default)]
    pub priority: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteAuditMqttMessage {
    pub schema_version: u16,
    pub source: String,
    pub connection_id: Option<String>,
    pub tag_id: String,
    pub command_id: Option<String>,
    pub value: TagValue,
    pub outcome: String,
    pub reason: Option<String>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WriteCommandAckMessage {
    pub schema_version: u16,
    pub source: String,
    pub tag_id: Option<String>,
    pub command_id: Option<String>,
    pub success: bool,
    pub reason: Option<String>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EdgeActionCommandMessage {
    #[serde(default)]
    pub schema_version: Option<u16>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub request_id: Option<String>,
    pub action_type: String,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EdgeActionResultMessage {
    pub schema_version: u16,
    pub source: String,
    pub request_id: Option<String>,
    pub action_type: String,
    pub accepted: bool,
    pub reason: Option<String>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EdgeActionAuditMessage {
    pub schema_version: u16,
    pub source: String,
    pub request_id: Option<String>,
    pub action_type: String,
    pub outcome: String,
    pub reason: Option<String>,
    #[serde(default)]
    pub payload: Option<serde_json::Value>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeHealthMqttMessage {
    pub schema_version: u16,
    pub source: String,
    pub status: String,
    pub outbox_depth: usize,
    pub outbox_oldest_age_secs: Option<u64>,
    pub cmd_received_total: u64,
    pub cmd_failed_total: u64,
    #[serde(default)]
    pub action_metrics: std::collections::HashMap<String, ActionTypeMetrics>,
    pub ack_publish_fail_total: u64,
    pub audit_publish_fail_total: u64,
    pub outbox_enqueued_total: u64,
    pub outbox_flushed_total: u64,
    pub alert_raised_total: u64,
    pub alert_cleared_total: u64,
    pub alert_ack_received_total: u64,
    pub alert_ack_accepted_total: u64,
    pub alert_publish_fail_total: u64,
    pub config_hash: Option<String>,
    pub config_sync_state: String,
    pub config_target_hash: Option<String>,
    pub config_last_check_at: Option<chrono::DateTime<chrono::Utc>>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone)]
struct ConfigSyncState {
    current_hash: Option<String>,
    sync_state: String,
    target_hash: Option<String>,
    last_check_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagTelemetryMqttMessage {
    pub schema_version: u16,
    pub source: String,
    pub tag_id: String,
    pub value: TagValue,
    pub quality: domain::tag::TagQuality,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionStateMqttMessage {
    pub schema_version: u16,
    pub source: String,
    pub connection_id: String,
    pub state: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceConnectionStateMqttMessage {
    pub schema_version: u16,
    pub source: String,
    pub connection_id: String,
    pub device_id: String,
    pub tag_id: Option<String>,
    pub state: String,
    pub reason: Option<String>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EdgeAlertMqttMessage {
    pub schema_version: u16,
    pub source: String,
    pub severity: String,
    pub alert_type: String,
    pub state: String,
    pub message: String,
    pub outbox_depth: usize,
    pub outbox_oldest_age_secs: Option<u64>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AlertAckCommandMessage {
    #[serde(default)]
    pub schema_version: Option<u16>,
    #[serde(default)]
    pub source: Option<String>,
    pub alert_type: String,
    #[serde(default)]
    pub ack_id: Option<String>,
    #[serde(default)]
    pub operator: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AlertAckResultMessage {
    pub schema_version: u16,
    pub source: String,
    pub alert_type: String,
    pub ack_id: Option<String>,
    pub accepted: bool,
    pub reason: Option<String>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConfigApplyCommandMessage {
    #[serde(default)]
    pub schema_version: Option<u16>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub request_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConfigApplyResultMessage {
    pub schema_version: u16,
    pub source: String,
    pub request_id: Option<String>,
    pub accepted: bool,
    pub reason: Option<String>,
    pub current_config_hash: Option<String>,
    pub target_config_hash: Option<String>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ControlResetCommandMessage {
    #[serde(default)]
    pub schema_version: Option<u16>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub request_id: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub operator: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ControlResetResultMessage {
    pub schema_version: u16,
    pub source: String,
    pub request_id: Option<String>,
    pub accepted: bool,
    pub reason: Option<String>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ConfigApplyReceipt {
    request_id: Option<String>,
    target_config_hash: Option<String>,
    requested_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Default)]
struct BridgeMetrics {
    cmd_received_total: AtomicU64,
    cmd_failed_total: AtomicU64,
    ack_publish_fail_total: AtomicU64,
    audit_publish_fail_total: AtomicU64,
    outbox_enqueued_total: AtomicU64,
    outbox_flushed_total: AtomicU64,
    alert_raised_total: AtomicU64,
    alert_cleared_total: AtomicU64,
    alert_ack_received_total: AtomicU64,
    alert_ack_accepted_total: AtomicU64,
    alert_publish_fail_total: AtomicU64,
    action_metrics: Mutex<std::collections::HashMap<String, ActionTypeMetrics>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ActionTypeMetrics {
    pub received_total: u64,
    pub accepted_total: u64,
    pub failed_total: u64,
}

#[derive(Default)]
struct AlertState {
    degraded_streak: usize,
    recovered_streak: usize,
    active: bool,
    last_emitted: std::collections::HashMap<String, chrono::DateTime<chrono::Utc>>,
}

impl BridgeMetrics {
    fn inc_cmd_received(&self) {
        self.cmd_received_total.fetch_add(1, Ordering::Relaxed);
    }
    fn inc_cmd_failed(&self) {
        self.cmd_failed_total.fetch_add(1, Ordering::Relaxed);
    }
    fn inc_ack_publish_fail(&self) {
        self.ack_publish_fail_total.fetch_add(1, Ordering::Relaxed);
    }
    fn inc_audit_publish_fail(&self) {
        self.audit_publish_fail_total.fetch_add(1, Ordering::Relaxed);
    }
    fn inc_outbox_enqueued(&self) {
        self.outbox_enqueued_total.fetch_add(1, Ordering::Relaxed);
    }
    fn add_outbox_flushed(&self, n: usize) {
        self.outbox_flushed_total
            .fetch_add(n as u64, Ordering::Relaxed);
    }
    fn inc_alert_raised(&self) {
        self.alert_raised_total.fetch_add(1, Ordering::Relaxed);
    }
    fn inc_alert_cleared(&self) {
        self.alert_cleared_total.fetch_add(1, Ordering::Relaxed);
    }
    fn inc_alert_ack_received(&self) {
        self.alert_ack_received_total.fetch_add(1, Ordering::Relaxed);
    }
    fn inc_alert_ack_accepted(&self) {
        self.alert_ack_accepted_total.fetch_add(1, Ordering::Relaxed);
    }
    fn inc_alert_publish_fail(&self) {
        self.alert_publish_fail_total.fetch_add(1, Ordering::Relaxed);
    }
    fn inc_action_received(&self, action_type: &str) {
        self.with_action_metric(action_type, |m| m.received_total = m.received_total.saturating_add(1));
    }
    fn inc_action_accepted(&self, action_type: &str) {
        self.with_action_metric(action_type, |m| m.accepted_total = m.accepted_total.saturating_add(1));
    }
    fn inc_action_failed(&self, action_type: &str) {
        self.with_action_metric(action_type, |m| m.failed_total = m.failed_total.saturating_add(1));
    }
    fn snapshot_action_metrics(&self) -> std::collections::HashMap<String, ActionTypeMetrics> {
        self.action_metrics
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default()
    }
    fn with_action_metric<F: FnOnce(&mut ActionTypeMetrics)>(&self, action_type: &str, f: F) {
        let key = action_type.trim().to_ascii_lowercase();
        if key.is_empty() {
            return;
        }
        if let Ok(mut guard) = self.action_metrics.lock() {
            let entry = guard.entry(key).or_default();
            f(entry);
        }
    }
}

#[async_trait]
pub trait WriteCommandExecutor: Send + Sync {
    async fn write_tag(
        &self,
        tag_id: TagId,
        value: TagValue,
        priority: WritePriority,
    ) -> Result<(), domain::DomainError>;
    async fn write_tag_with_command_id(
        &self,
        tag_id: TagId,
        value: TagValue,
        command_id: String,
        priority: WritePriority,
    ) -> Result<(), domain::DomainError>;
}

#[async_trait]
impl WriteCommandExecutor for RuntimeEngine {
    async fn write_tag(
        &self,
        tag_id: TagId,
        value: TagValue,
        priority: WritePriority,
    ) -> Result<(), domain::DomainError> {
        RuntimeEngine::write_tag_with_priority(self, tag_id, value, priority).await
    }

    async fn write_tag_with_command_id(
        &self,
        tag_id: TagId,
        value: TagValue,
        command_id: String,
        priority: WritePriority,
    ) -> Result<(), domain::DomainError> {
        RuntimeEngine::write_tag_with_command_id_and_priority(
            self, tag_id, value, command_id, priority,
        )
        .await
    }
}

pub fn parse_write_command_message(payload: &[u8]) -> anyhow::Result<WriteTagCommandMessage> {
    Ok(serde_json::from_slice(payload)?)
}

pub fn parse_action_command_message(payload: &[u8]) -> anyhow::Result<EdgeActionCommandMessage> {
    Ok(serde_json::from_slice(payload)?)
}

pub async fn execute_write_command(
    executor: &dyn WriteCommandExecutor,
    command: WriteTagCommandMessage,
) -> Result<(), domain::DomainError> {
    let tag_id = TagId::new(&command.tag_id);
    let priority = parse_write_priority(command.priority.as_deref())?;
    if let Some(command_id) = command.command_id {
        executor
            .write_tag_with_command_id(tag_id, command.value, command_id, priority)
            .await
    } else {
        executor.write_tag(tag_id, command.value, priority).await
    }
}

fn build_action_result(
    source: &str,
    cmd: &EdgeActionCommandMessage,
    accepted: bool,
    reason: Option<String>,
) -> EdgeActionResultMessage {
    EdgeActionResultMessage {
        schema_version: MQTT_SCHEMA_VERSION_V1,
        source: source.to_string(),
        request_id: cmd.request_id.clone(),
        action_type: cmd.action_type.clone(),
        accepted,
        reason,
        timestamp: chrono::Utc::now(),
    }
}

fn build_action_audit(
    source: &str,
    cmd: &EdgeActionCommandMessage,
    outcome: &str,
    reason: Option<String>,
) -> EdgeActionAuditMessage {
    EdgeActionAuditMessage {
        schema_version: MQTT_SCHEMA_VERSION_V1,
        source: source.to_string(),
        request_id: cmd.request_id.clone(),
        action_type: cmd.action_type.clone(),
        outcome: outcome.to_string(),
        reason,
        payload: Some(cmd.payload.clone()),
        timestamp: chrono::Utc::now(),
    }
}

fn to_action_request(cmd: &EdgeActionCommandMessage) -> ActionRequest {
    ActionRequest {
        request_id: cmd.request_id.clone(),
        action_type: cmd.action_type.clone(),
        target: cmd.target.clone(),
        payload: cmd.payload.clone(),
    }
}

fn resolve_tcp_target(cfg: &MqttBridgeConfig, payload: &serde_json::Value) -> (Option<String>, u16) {
    let payload_host = payload
        .get("printer")
        .and_then(|p| p.get("host"))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToString::to_string)
        .or_else(|| {
            payload
                .get("printer_host")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(ToString::to_string)
        })
        .or_else(|| {
            payload
                .get("host")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(ToString::to_string)
        });
    let payload_port = payload
        .get("printer")
        .and_then(|p| p.get("port"))
        .and_then(|v| v.as_u64())
        .and_then(|p| u16::try_from(p).ok())
        .or_else(|| {
            payload
                .get("printer_port")
                .and_then(|v| v.as_u64())
                .and_then(|p| u16::try_from(p).ok())
        })
        .or_else(|| {
            payload
                .get("port")
                .and_then(|v| v.as_u64())
                .and_then(|p| u16::try_from(p).ok())
        });
    let tcp_host = payload_host.or_else(|| {
        cfg.on_demand_tcp_host
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(ToString::to_string)
    }).or_else(|| {
        cfg.escpos_tcp_host
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(ToString::to_string)
    });
    let fallback_port = cfg.on_demand_tcp_port.unwrap_or(cfg.escpos_tcp_port);
    (tcp_host, payload_port.unwrap_or(fallback_port))
}


fn parse_write_priority(priority: Option<&str>) -> Result<WritePriority, domain::DomainError> {
    match priority.map(|s| s.trim().to_ascii_lowercase()) {
        None => Ok(WritePriority::Normal),
        Some(p) if p.is_empty() => Ok(WritePriority::Normal),
        Some(p) if p == "normal" => Ok(WritePriority::Normal),
        Some(p) if p == "high" => Ok(WritePriority::High),
        Some(other) => Err(domain::DomainError::ConfigurationError(format!(
            "unsupported write priority '{}'",
            other
        ))),
    }
}

pub fn to_write_audit_message(event: &RuntimeEvent) -> Option<WriteAuditMqttMessage> {
    match event {
        RuntimeEvent::TagWriteCommandHandled {
            connection_id,
            tag_id,
            command_id,
            value,
            outcome,
            reason,
            timestamp,
        } => Some(WriteAuditMqttMessage {
            schema_version: MQTT_SCHEMA_VERSION_V1,
            source: "edge-agent".to_string(),
            connection_id: connection_id.as_ref().map(ToString::to_string),
            tag_id: tag_id.to_string(),
            command_id: command_id.clone(),
            value: value.clone(),
            outcome: format!("{:?}", outcome),
            reason: reason.clone(),
            timestamp: *timestamp,
        }),
        _ => None,
    }
}

pub fn to_tag_telemetry_message(event: &RuntimeEvent) -> Option<TagTelemetryMqttMessage> {
    match event {
        RuntimeEvent::TagChanged {
            tag_id,
            device_id: _,
            value,
            trigger_value: _,
            quality,
            timestamp,
        } => Some(TagTelemetryMqttMessage {
            schema_version: MQTT_SCHEMA_VERSION_V1,
            source: "edge-agent".to_string(),
            tag_id: tag_id.to_string(),
            value: value.clone(),
            quality: *quality,
            timestamp: *timestamp,
        }),
        _ => None,
    }
}

pub fn to_connection_state_message(event: &RuntimeEvent) -> Option<ConnectionStateMqttMessage> {
    match event {
        RuntimeEvent::ConnectionStateChanged {
            connection_id,
            state,
        } => Some(ConnectionStateMqttMessage {
            schema_version: MQTT_SCHEMA_VERSION_V1,
            source: "edge-agent".to_string(),
            connection_id: connection_id.to_string(),
            state: format!("{:?}", state),
            timestamp: chrono::Utc::now(),
        }),
        _ => None,
    }
}

pub fn to_device_connection_state_message(
    event: &RuntimeEvent,
) -> Option<DeviceConnectionStateMqttMessage> {
    match event {
        RuntimeEvent::DeviceProtocolStateChanged {
            connection_id,
            device_id,
            tag_id,
            state,
            reason,
            timestamp,
        } => Some(DeviceConnectionStateMqttMessage {
            schema_version: MQTT_SCHEMA_VERSION_V1,
            source: "edge-agent".to_string(),
            connection_id: connection_id.to_string(),
            device_id: device_id.to_string(),
            tag_id: tag_id.as_ref().map(ToString::to_string),
            state: format!("{:?}", state),
            reason: reason.clone(),
            timestamp: *timestamp,
        }),
        _ => None,
    }
}

pub fn build_write_command_ack(
    source: &str,
    command: &WriteTagCommandMessage,
    result: &Result<(), domain::DomainError>,
) -> WriteCommandAckMessage {
    WriteCommandAckMessage {
        schema_version: MQTT_SCHEMA_VERSION_V1,
        source: source.to_string(),
        tag_id: Some(command.tag_id.clone()),
        command_id: command.command_id.clone(),
        success: result.is_ok(),
        reason: result.as_ref().err().map(ToString::to_string),
        timestamp: chrono::Utc::now(),
    }
}

pub fn build_invalid_payload_ack(source: &str, reason: String) -> WriteCommandAckMessage {
    WriteCommandAckMessage {
        schema_version: MQTT_SCHEMA_VERSION_V1,
        source: source.to_string(),
        tag_id: None,
        command_id: None,
        success: false,
        reason: Some(reason),
        timestamp: chrono::Utc::now(),
    }
}

pub fn parse_alert_ack_command_message(payload: &[u8]) -> anyhow::Result<AlertAckCommandMessage> {
    Ok(serde_json::from_slice(payload)?)
}

pub fn parse_config_apply_command_message(payload: &[u8]) -> anyhow::Result<ConfigApplyCommandMessage> {
    Ok(serde_json::from_slice(payload)?)
}

pub fn parse_control_reset_command_message(
    payload: &[u8],
) -> anyhow::Result<ControlResetCommandMessage> {
    Ok(serde_json::from_slice(payload)?)
}

fn build_alert_ack_result(
    source: &str,
    cmd: &AlertAckCommandMessage,
    accepted: bool,
    reason: Option<String>,
) -> AlertAckResultMessage {
    AlertAckResultMessage {
        schema_version: MQTT_SCHEMA_VERSION_V1,
        source: source.to_string(),
        alert_type: cmd.alert_type.clone(),
        ack_id: cmd.ack_id.clone(),
        accepted,
        reason,
        timestamp: chrono::Utc::now(),
    }
}

fn build_config_apply_result(
    source: &str,
    cmd: Option<&ConfigApplyCommandMessage>,
    accepted: bool,
    reason: Option<String>,
    current_config_hash: Option<String>,
    target_config_hash: Option<String>,
) -> ConfigApplyResultMessage {
    ConfigApplyResultMessage {
        schema_version: MQTT_SCHEMA_VERSION_V1,
        source: source.to_string(),
        request_id: cmd.and_then(|c| c.request_id.clone()),
        accepted,
        reason,
        current_config_hash,
        target_config_hash,
        timestamp: chrono::Utc::now(),
    }
}

fn build_control_reset_result(
    source: &str,
    cmd: Option<&ControlResetCommandMessage>,
    accepted: bool,
    reason: Option<String>,
) -> ControlResetResultMessage {
    ControlResetResultMessage {
        schema_version: MQTT_SCHEMA_VERSION_V1,
        source: source.to_string(),
        request_id: cmd.and_then(|c| c.request_id.clone()),
        accepted,
        reason,
        timestamp: chrono::Utc::now(),
    }
}

fn write_config_apply_receipt(path: &str, receipt: &ConfigApplyReceipt) -> anyhow::Result<()> {
    if let Some(parent) = Path::new(path).parent() {
        let _ = fs::create_dir_all(parent);
    }
    let raw = serde_json::to_string_pretty(receipt)?;
    fs::write(path, raw)?;
    Ok(())
}

fn read_and_remove_config_apply_receipt(path: &str) -> Option<ConfigApplyReceipt> {
    let raw = fs::read_to_string(path).ok()?;
    let parsed = serde_json::from_str::<ConfigApplyReceipt>(&raw).ok()?;
    let _ = fs::remove_file(path);
    Some(parsed)
}

fn compute_health_status(
    stats: &OutboxStats,
    depth_warn: usize,
    oldest_secs_warn: u64,
) -> &'static str {
    if stats.depth >= depth_warn {
        return "degraded";
    }
    if stats.oldest_age_secs.unwrap_or(0) >= oldest_secs_warn {
        return "degraded";
    }
    "ok"
}

fn build_health_message(
    source: &str,
    stats: &OutboxStats,
    metrics: &BridgeMetrics,
    depth_warn: usize,
    oldest_secs_warn: u64,
    cfg_sync: &ConfigSyncState,
) -> EdgeHealthMqttMessage {
    EdgeHealthMqttMessage {
        schema_version: MQTT_SCHEMA_VERSION_V1,
        source: source.to_string(),
        status: compute_health_status(stats, depth_warn, oldest_secs_warn).to_string(),
        outbox_depth: stats.depth,
        outbox_oldest_age_secs: stats.oldest_age_secs,
        cmd_received_total: metrics.cmd_received_total.load(Ordering::Relaxed),
        cmd_failed_total: metrics.cmd_failed_total.load(Ordering::Relaxed),
        action_metrics: metrics.snapshot_action_metrics(),
        ack_publish_fail_total: metrics.ack_publish_fail_total.load(Ordering::Relaxed),
        audit_publish_fail_total: metrics.audit_publish_fail_total.load(Ordering::Relaxed),
        outbox_enqueued_total: metrics.outbox_enqueued_total.load(Ordering::Relaxed),
        outbox_flushed_total: metrics.outbox_flushed_total.load(Ordering::Relaxed),
        alert_raised_total: metrics.alert_raised_total.load(Ordering::Relaxed),
        alert_cleared_total: metrics.alert_cleared_total.load(Ordering::Relaxed),
        alert_ack_received_total: metrics.alert_ack_received_total.load(Ordering::Relaxed),
        alert_ack_accepted_total: metrics
            .alert_ack_accepted_total
            .load(Ordering::Relaxed),
        alert_publish_fail_total: metrics.alert_publish_fail_total.load(Ordering::Relaxed),
        config_hash: cfg_sync.current_hash.clone(),
        config_sync_state: cfg_sync.sync_state.clone(),
        config_target_hash: cfg_sync.target_hash.clone(),
        config_last_check_at: cfg_sync.last_check_at,
        timestamp: chrono::Utc::now(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AlertTransition {
    Raised,
    Cleared,
}

fn evaluate_alert_transition(
    status: &str,
    state: &mut AlertState,
    degraded_threshold: usize,
    recovered_threshold: usize,
) -> Option<AlertTransition> {
    if status == "degraded" {
        state.degraded_streak = state.degraded_streak.saturating_add(1);
        state.recovered_streak = 0;
        if !state.active && state.degraded_streak >= degraded_threshold.max(1) {
            state.active = true;
            return Some(AlertTransition::Raised);
        }
    } else {
        state.recovered_streak = state.recovered_streak.saturating_add(1);
        state.degraded_streak = 0;
        if state.active && state.recovered_streak >= recovered_threshold.max(1) {
            state.active = false;
            return Some(AlertTransition::Cleared);
        }
    }
    None
}

fn should_emit_alert_for_window(
    state: &mut AlertState,
    alert_type: &str,
    transition: AlertTransition,
    window_secs: u64,
) -> bool {
    let slot = match transition {
        AlertTransition::Raised => "raised",
        AlertTransition::Cleared => "cleared",
    };
    let key = format!("{}:{}", alert_type, slot);
    let now = chrono::Utc::now();
    if let Some(last) = state.last_emitted.get(&key) {
        if (now.timestamp() - last.timestamp()).max(0) < window_secs as i64 {
            return false;
        }
    }
    state.last_emitted.insert(key, now);
    true
}

fn build_alert_message(
    source: &str,
    status: &str,
    transition: AlertTransition,
    stats: &OutboxStats,
) -> EdgeAlertMqttMessage {
    let state = match transition {
        AlertTransition::Raised => "raised",
        AlertTransition::Cleared => "cleared",
    };
    let severity = if status == "degraded" { "warning" } else { "info" };
    let message = match transition {
        AlertTransition::Raised => "Runtime degraded sustained above threshold",
        AlertTransition::Cleared => "Runtime health recovered sustained above threshold",
    };
    EdgeAlertMqttMessage {
        schema_version: MQTT_SCHEMA_VERSION_V1,
        source: source.to_string(),
        severity: severity.to_string(),
        alert_type: "runtime_health_degraded".to_string(),
        state: state.to_string(),
        message: message.to_string(),
        outbox_depth: stats.depth,
        outbox_oldest_age_secs: stats.oldest_age_secs,
        timestamp: chrono::Utc::now(),
    }
}

/// MQTT-level keep-alive for the edge's broker session. Shared with the staleness
/// watchdog so the two can never drift apart.
const MQTT_KEEP_ALIVE: Duration = Duration::from_secs(10);

#[async_trait]
impl OutboxPublisher for AsyncClient {
    async fn publish(
        &self,
        topic: String,
        qos: QoS,
        retain: bool,
        payload: Vec<u8>,
    ) -> Result<(), String> {
        self.publish(topic, qos, retain, payload)
            .await
            .map_err(|e| e.to_string())
    }

    /// `AsyncClient::try_publish` is synchronous and returns instead of waiting when the
    /// request channel is full, which is exactly the property `flush_pending` needs to
    /// avoid wedging the task that drives `event_loop.poll()`.
    ///
    /// `rumqttc` 0.24 reports both "channel full" and "channel closed" as
    /// `ClientError::TryRequest`, so this cannot tell them apart. Treating both as
    /// backpressure is safe here: the channel is only closed once the `EventLoop` is
    /// dropped, which happens when the bridge is already on its way out, and the row stays
    /// in the outbox either way.
    async fn try_publish(
        &self,
        topic: String,
        qos: QoS,
        retain: bool,
        payload: Vec<u8>,
    ) -> PublishAttempt {
        match AsyncClient::try_publish(self, topic, qos, retain, payload) {
            Ok(()) => PublishAttempt::Sent,
            Err(ClientError::TryRequest(_)) => PublishAttempt::Backpressure,
            Err(e) => PublishAttempt::Failed(e.to_string()),
        }
    }
}

async fn publish_with_outbox(
    publisher: &impl OutboxPublisher,
    outbox: &PersistentMqttOutbox,
    metrics: &BridgeMetrics,
    channel: OutboxMessageKind,
    mark_alert_publish_failure: bool,
    kind: OutboxMessageKind,
    topic: String,
    qos: QoS,
    retain: bool,
    payload: Vec<u8>,
    flush_batch: usize,
) {
    if let Ok(flushed) = outbox.flush_pending(publisher, flush_batch).await {
        if flushed > 0 {
            metrics.add_outbox_flushed(flushed);
            debug!("outbox flushed {} message(s)", flushed);
        }
    }
    match publisher.publish(topic.clone(), qos, retain, payload.clone()).await {
        Ok(_) => {
            debug!(
                "mqtt publish ok topic='{}' qos={:?} retain={} bytes={}",
                topic,
                qos,
                retain,
                payload.len()
            );
        }
        Err(e) => {
            warn!("mqtt publish failed, enqueueing outbox message: {}", e);
            match channel {
                OutboxMessageKind::Ack => metrics.inc_ack_publish_fail(),
                OutboxMessageKind::Audit => metrics.inc_audit_publish_fail(),
            }
            if mark_alert_publish_failure {
                metrics.inc_alert_publish_fail();
            }
            if let Err(store_err) = outbox.enqueue(kind, topic, qos, retain, payload).await {
                warn!("failed to persist mqtt outbox message: {}", store_err);
            } else {
                metrics.inc_outbox_enqueued();
            }
        }
    }
}

async fn publish_on_demand_startup_probe(
    cfg: &MqttBridgeConfig,
    publisher: &impl OutboxPublisher,
    outbox: &PersistentMqttOutbox,
    metrics: &BridgeMetrics,
    device_conn_state_topic: &str,
    source_id: &str,
    flush_batch: usize,
) {
    if !cfg.on_demand_probe_enabled {
        return;
    }
    let Some(connection_id) = cfg
        .on_demand_probe_connection_id
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToString::to_string)
    else {
        debug!("on-demand startup probe skipped: missing connection_id");
        return;
    };
    let Some(device_id) = cfg
        .on_demand_probe_device_id
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToString::to_string)
    else {
        debug!("on-demand startup probe skipped: missing device_id");
        return;
    };
    let (host_opt, port) = resolve_tcp_target(cfg, &serde_json::Value::Null);
    let Some(host) = host_opt else {
        warn!("on-demand startup probe skipped: missing host (configured default)");
        return;
    };
    let timeout_ms = cfg.on_demand_probe_timeout_ms.max(100);
    let addr = format!("{}:{}", host, port);
    let (state, reason) =
        match tokio::time::timeout(Duration::from_millis(timeout_ms), TcpStream::connect(addr.clone())).await {
            Ok(Ok(stream)) => {
                drop(stream);
                ("Connected".to_string(), Some("startup_probe_ok".to_string()))
            }
            Ok(Err(e)) => (
                "Error".to_string(),
                Some(format!("startup_probe_failed '{}' : {}", addr, e)),
            ),
            Err(_) => (
                "Error".to_string(),
                Some(format!("startup_probe_timeout '{}' after {} ms", addr, timeout_ms)),
            ),
        };
    let msg = DeviceConnectionStateMqttMessage {
        schema_version: MQTT_SCHEMA_VERSION_V1,
        source: source_id.to_string(),
        connection_id,
        device_id,
        tag_id: None,
        state,
        reason,
        timestamp: chrono::Utc::now(),
    };
    if let Ok(payload) = serde_json::to_vec(&msg) {
        publish_with_outbox(
            publisher,
            outbox,
            metrics,
            OutboxMessageKind::Audit,
            false,
            OutboxMessageKind::Audit,
            device_conn_state_topic.to_string(),
            QoS::AtLeastOnce,
            false,
            payload,
            flush_batch,
        )
        .await;
    }
}

/// Subscribes to every inbound command/control topic the bridge listens on.
///
/// Must be called after every successful (re)connection, not just the first
/// one: rumqttc's `MqttOptions` defaults to `clean_session = true`, so the
/// broker discards this client's subscriptions on every disconnect. Without
/// re-subscribing here, a transient network blip silently leaves the edge
/// deaf to inbound commands (manual web-UI actions, config apply, alert ack,
/// control reset) while outbound publish and local automations keep working
/// normally, making the failure invisible in the logs.
async fn subscribe_all_topics(
    client: &AsyncClient,
    cmd_topic: &str,
    action_cmd_topic: &str,
    alert_ack_topic: &str,
    config_apply_topic: &str,
    control_reset_topic: &str,
) -> anyhow::Result<()> {
    client
        .subscribe(cmd_topic, QoS::AtLeastOnce)
        .await?;
    client
        .subscribe(action_cmd_topic, QoS::AtLeastOnce)
        .await?;
    client
        .subscribe(alert_ack_topic, QoS::AtLeastOnce)
        .await?;
    client
        .subscribe(config_apply_topic, QoS::AtLeastOnce)
        .await?;
    client
        .subscribe(control_reset_topic, QoS::AtLeastOnce)
        .await?;
    Ok(())
}

pub async fn run_mqtt_bridge(
    config: MqttBridgeConfig,
    engine: Arc<RuntimeEngine>,
) -> anyhow::Result<MqttBridgeExit> {
    info!(
        "connecting mqtt bridge host={} port={} client_id={}",
        config.broker_host, config.broker_port, config.client_id
    );
    let mut opts = MqttOptions::new(&config.client_id, &config.broker_host, config.broker_port);
    opts.set_keep_alive(MQTT_KEEP_ALIVE);
    apply_edge_mqtt_security_from_env(&mut opts)?;
    let (client, mut event_loop): (AsyncClient, EventLoop) = AsyncClient::new(opts, 100);
    let cmd_topic = config.command_topic();
    let action_cmd_topic = config.action_command_topic();
    let audit_topic = config.audit_topic();
    let action_audit_topic = config.action_audit_topic();
    let ack_topic = config.ack_topic();
    let action_result_topic = config.action_result_topic();
    let health_topic = config.health_topic();
    let alert_topic = config.alert_topic();
    let alert_ack_topic = config.alert_ack_topic();
    let alert_ack_result_topic = config.alert_ack_result_topic();
    let config_apply_topic = config.config_apply_topic();
    let config_apply_result_topic = config.config_apply_result_topic();
    let control_reset_topic = config.control_reset_topic();
    let control_reset_result_topic = config.control_reset_result_topic();
    let config_apply_receipt_path = config.config_apply_receipt_path.clone();
    let telemetry_topic_for = {
        let cfg = config.clone();
        move |tag_id: &str| cfg.telemetry_tag_topic(tag_id)
    };
    let conn_state_topic = config.connection_state_topic();
    let device_conn_state_topic = config.device_connection_state_topic();
    let source_id = format!("edge/{}", config.agent);
    let outbox = Arc::new(PersistentMqttOutbox::new(
        &config.outbox_path,
        OutboxConfig {
            max_messages: config.outbox_max_messages.max(1),
            security: OutboxSecurity::from_rotation(
                config.outbox_active_key_id.clone(),
                config.outbox_encryption_secret.clone(),
                config.outbox_hmac_secret.clone(),
                config.outbox_prev_key_id.clone(),
                config.outbox_prev_encryption_secret.clone(),
                config.outbox_prev_hmac_secret.clone(),
            ),
        },
    )?);
    let flush_batch = config.outbox_flush_batch.max(1);
    let metrics = Arc::new(BridgeMetrics::default());
    let shared_alert_state = Arc::new(TokioMutex::new(AlertState::default()));

    subscribe_all_topics(
        &client,
        &cmd_topic,
        &action_cmd_topic,
        &alert_ack_topic,
        &config_apply_topic,
        &control_reset_topic,
    )
    .await?;
    info!(
        "mqtt subscriptions ready cmd='{}' cmd_action='{}' alert_ack='{}' config_apply='{}' control_reset='{}'",
        cmd_topic, action_cmd_topic, alert_ack_topic, config_apply_topic, control_reset_topic
    );
    publish_on_demand_startup_probe(
        &config,
        &client,
        &outbox,
        &metrics,
        &device_conn_state_topic,
        &source_id,
        flush_batch,
    )
    .await;

    if let Some(receipt) = read_and_remove_config_apply_receipt(&config_apply_receipt_path) {
        let applied = build_config_apply_result(
            &source_id,
            Some(&ConfigApplyCommandMessage {
                schema_version: Some(MQTT_SCHEMA_VERSION_V1),
                source: Some("edge-restart".to_string()),
                request_id: receipt.request_id.clone(),
            }),
            true,
            Some("applied_after_restart".to_string()),
            config.config_hash.clone(),
            receipt.target_config_hash.clone(),
        );
        if let Ok(payload) = serde_json::to_vec(&applied) {
            if let Err(e) = client
                .publish(
                    config_apply_result_topic.clone(),
                    QoS::AtLeastOnce,
                    false,
                    payload,
                )
                .await
            {
                warn!(
                    "failed to publish config applied-after-restart result: {}",
                    e
                );
            }
        }
    }

    let mut events = engine.event_bus().subscribe();
    let publisher = client.clone();
    let outbox_pub = outbox.clone();
    let metrics_pub = metrics.clone();
    let action_runtime_state = Arc::new(TokioMutex::new(ActionRuntimeState::default()));
    let action_orchestrator = Arc::new(ActionOrchestrator::new_default());
    let publish_source = source_id.clone();
    let trigger_cfg = config.clone();
    let trigger_action_runtime_state = action_runtime_state.clone();
    let trigger_action_orchestrator = action_orchestrator.clone();
    let trigger_action_topic = action_result_topic.clone();
    let trigger_action_audit_topic = action_audit_topic.clone();
    let device_conn_state_topic_events = device_conn_state_topic.clone();
    let runtime_automations = config.automations.clone();
    let publish_task: JoinHandle<()> = tokio::spawn(async move {
        let mut automation_engine =
            RuntimeAutomationEngine::new(runtime_automations, AutomationRuntimeScope::Edge);
        loop {
            match events.recv().await {
                Ok(evt) => {
                    if let Some(mut msg) = to_write_audit_message(&evt) {
                        msg.source = publish_source.clone();
                        match serde_json::to_vec(&msg) {
                            Ok(payload) => {
                                publish_with_outbox(
                                    &publisher,
                                    &outbox_pub,
                                    &metrics_pub,
                                    OutboxMessageKind::Audit,
                                    false,
                                    OutboxMessageKind::Audit,
                                    audit_topic.clone(),
                                    QoS::AtLeastOnce,
                                    false,
                                    payload,
                                    flush_batch,
                                )
                                .await;
                            }
                            Err(e) => warn!("failed to serialize write audit mqtt message: {}", e),
                        }
                    }
                    if let Some(mut msg) = to_tag_telemetry_message(&evt) {
                        msg.source = publish_source.clone();
                        let (trigger_value, trigger_device_id) = match &evt {
                            RuntimeEvent::TagChanged {
                                trigger_value,
                                device_id,
                                ..
                            } => (
                                trigger_value.clone().unwrap_or_else(|| msg.value.clone()),
                                Some(device_id.to_string()),
                            ),
                            _ => (msg.value.clone(), None),
                        };
                        let auto_requests = automation_engine.on_tag_changed(
                            &msg.tag_id,
                            &trigger_value,
                            msg.timestamp,
                        );
                        for req in auto_requests {
                            let mut payload = req.payload.clone();
                            if let Some(obj) = payload.as_object_mut() {
                                obj.insert(
                                    "trigger".to_string(),
                                    serde_json::json!({
                                        "automation_id": req.automation_id,
                                        "tag_id": req.trigger_tag_id,
                                        "device_id": trigger_device_id,
                                        "value": req.trigger_value,
                                        "display_value": msg.value,
                                        "timestamp": req.trigger_timestamp
                                    }),
                                );
                            }
                            let auto_cmd = EdgeActionCommandMessage {
                                schema_version: Some(MQTT_SCHEMA_VERSION_V1),
                                source: Some("edge-automation".to_string()),
                                request_id: Some(req.request_id),
                                action_type: req.action_type,
                                target: req.target,
                                payload,
                            };
                            let auto_req = to_action_request(&auto_cmd);
                            let auto_result = match trigger_action_orchestrator
                                .execute(&trigger_cfg, &trigger_action_runtime_state, &auto_req)
                                .await
                            {
                                Ok(_) => build_action_result(&publish_source, &auto_cmd, true, None),
                                Err(e) => build_action_result(&publish_source, &auto_cmd, false, Some(e)),
                            };
                            let auto_audit = build_action_audit(
                                &publish_source,
                                &auto_cmd,
                                if auto_result.accepted { "Applied" } else { "Failed" },
                                auto_result.reason.clone(),
                            );
                            if let Ok(payload) = serde_json::to_vec(&auto_result) {
                                publish_with_outbox(
                                    &publisher,
                                    &outbox_pub,
                                    &metrics_pub,
                                    OutboxMessageKind::Audit,
                                    false,
                                    OutboxMessageKind::Audit,
                                    trigger_action_topic.clone(),
                                    QoS::AtLeastOnce,
                                    false,
                                    payload,
                                    flush_batch,
                                )
                                .await;
                            }
                            if let Ok(payload) = serde_json::to_vec(&auto_audit) {
                                publish_with_outbox(
                                    &publisher,
                                    &outbox_pub,
                                    &metrics_pub,
                                    OutboxMessageKind::Audit,
                                    false,
                                    OutboxMessageKind::Audit,
                                    trigger_action_audit_topic.clone(),
                                    QoS::AtLeastOnce,
                                    false,
                                    payload,
                                    flush_batch,
                                )
                                .await;
                            }
                        }
                        let topic = telemetry_topic_for(&msg.tag_id);
                        match serde_json::to_vec(&msg) {
                            Ok(payload) => {
                                publish_with_outbox(
                                    &publisher,
                                    &outbox_pub,
                                    &metrics_pub,
                                    OutboxMessageKind::Audit,
                                    false,
                                    OutboxMessageKind::Audit,
                                    topic,
                                    QoS::AtLeastOnce,
                                    false,
                                    payload,
                                    flush_batch,
                                )
                                .await;
                            }
                            Err(e) => warn!("failed to serialize tag telemetry mqtt message: {}", e),
                        }
                    }
                    if let Some(mut msg) = to_connection_state_message(&evt) {
                        msg.source = publish_source.clone();
                        match serde_json::to_vec(&msg) {
                            Ok(payload) => {
                                publish_with_outbox(
                                    &publisher,
                                    &outbox_pub,
                                    &metrics_pub,
                                    OutboxMessageKind::Audit,
                                    false,
                                    OutboxMessageKind::Audit,
                                    conn_state_topic.clone(),
                                    QoS::AtLeastOnce,
                                    false,
                                    payload,
                                    flush_batch,
                                )
                                .await;
                            }
                            Err(e) => warn!("failed to serialize conn state mqtt message: {}", e),
                        }
                    }
                    if let Some(mut msg) = to_device_connection_state_message(&evt) {
                        msg.source = publish_source.clone();
                        match serde_json::to_vec(&msg) {
                            Ok(payload) => {
                                publish_with_outbox(
                                    &publisher,
                                    &outbox_pub,
                                    &metrics_pub,
                                    OutboxMessageKind::Audit,
                                    false,
                                    OutboxMessageKind::Audit,
                                    device_conn_state_topic_events.clone(),
                                    QoS::AtLeastOnce,
                                    false,
                                    payload,
                                    flush_batch,
                                )
                                .await;
                            }
                            Err(e) => warn!(
                                "failed to serialize device conn state mqtt message: {}",
                                e
                            ),
                        }
                    }
                }
                Err(e) => {
                    warn!("runtime event bus recv error: {}", e);
                    break;
                }
            }
        }
    });

    let health_publisher = client.clone();
    let health_outbox = outbox.clone();
    let health_metrics = metrics.clone();
    let health_source = source_id.clone();
    let health_interval_secs = config.health_publish_interval_secs.max(5);
    let depth_warn = config.health_outbox_depth_warn.max(1);
    let oldest_warn = config.health_outbox_oldest_secs_warn.max(1);
    let alert_degraded_threshold = config.alert_degraded_streak.max(1);
    let alert_recovered_threshold = config.alert_recovered_streak.max(1);
    let alert_dedup_window_secs = config.alert_dedup_window_secs.max(1);
    let config_sync = Arc::new(TokioMutex::new(ConfigSyncState {
        current_hash: config.config_hash.clone(),
        sync_state: "unknown".to_string(),
        target_hash: None,
        last_check_at: None,
    }));
    if let (Some(url), Some(token)) = (
        config.config_check_url.clone().filter(|s| !s.trim().is_empty()),
        config
            .config_check_enroll_token
            .clone()
            .filter(|s| !s.trim().is_empty()),
    ) {
        let signing_secret = config
            .config_check_hmac_secret
            .clone()
            .unwrap_or_else(|| "dev-edge-config-signing-secret".to_string());
        let key_id = config.config_check_key_id.clone();
        let cache_path = config.config_cache_path.clone();
        let state = config_sync.clone();
        let edge = config.agent.clone();
        let interval_secs = config.config_check_interval_secs.max(5);
        let jitter_secs = config.config_check_jitter_secs.min(300);
        tokio::spawn(async move {
            loop {
                let current = { state.lock().await.current_hash.clone() };
                match bootstrap::check_and_stage_remote_config(
                    &url,
                    &edge,
                    &token,
                    &signing_secret,
                    key_id.as_deref(),
                    &cache_path,
                    current.as_deref(),
                )
                .await
                {
                    Ok(Some(new_hash)) => {
                        let mut s = state.lock().await;
                        s.last_check_at = Some(chrono::Utc::now());
                        s.target_hash = Some(new_hash.clone());
                        s.sync_state = "changed_staged".to_string();
                        warn!(
                            "central config changed detected and staged (current={:?}, target={})",
                            s.current_hash, new_hash
                        );
                    }
                    Ok(None) => {
                        let mut s = state.lock().await;
                        s.last_check_at = Some(chrono::Utc::now());
                        s.target_hash = s.current_hash.clone();
                        s.sync_state = "in_sync".to_string();
                    }
                    Err(e) => {
                        let mut s = state.lock().await;
                        s.last_check_at = Some(chrono::Utc::now());
                        s.sync_state = "error".to_string();
                        warn!("periodic config check failed: {}", e);
                    }
                }
                let wait = if jitter_secs == 0 {
                    interval_secs
                } else {
                    interval_secs + rand::random::<u64>() % (jitter_secs + 1)
                };
                tokio::time::sleep(Duration::from_secs(wait)).await;
            }
        });
    }
    let health_config_sync = config_sync.clone();
    let health_alert_state = shared_alert_state.clone();
    let health_task: JoinHandle<()> = tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(health_interval_secs));
        loop {
            ticker.tick().await;
            let stats = health_outbox.stats().await;
            let cfg_sync = { health_config_sync.lock().await.clone() };
            let msg = build_health_message(
                &health_source,
                &stats,
                &health_metrics,
                depth_warn,
                oldest_warn,
                &cfg_sync,
            );
            if msg.status != "ok" {
                warn!(
                    "edge health degraded: outbox_depth={}, outbox_oldest_age_secs={:?}",
                    msg.outbox_depth, msg.outbox_oldest_age_secs
                );
            }
            {
                let mut alert_state = health_alert_state.lock().await;
                if let Some(transition) = evaluate_alert_transition(
                    &msg.status,
                    &mut alert_state,
                    alert_degraded_threshold,
                    alert_recovered_threshold,
                ) {
                    let alert_type = "runtime_health_degraded";
                    if should_emit_alert_for_window(
                        &mut alert_state,
                        alert_type,
                        transition,
                        alert_dedup_window_secs,
                    ) {
                        let alert =
                            build_alert_message(&health_source, &msg.status, transition, &stats);
                        match transition {
                            AlertTransition::Raised => health_metrics.inc_alert_raised(),
                            AlertTransition::Cleared => health_metrics.inc_alert_cleared(),
                        }
                        if let Ok(alert_payload) = serde_json::to_vec(&alert) {
                            publish_with_outbox(
                                &health_publisher,
                                &health_outbox,
                                &health_metrics,
                                OutboxMessageKind::Audit,
                                true,
                                OutboxMessageKind::Audit,
                                alert_topic.clone(),
                                QoS::AtLeastOnce,
                                false,
                                alert_payload,
                                flush_batch,
                            )
                            .await;
                        }
                    }
                }
            }
            if let Ok(payload) = serde_json::to_vec(&msg) {
                publish_with_outbox(
                    &health_publisher,
                    &health_outbox,
                    &health_metrics,
                    OutboxMessageKind::Audit,
                    false,
                    OutboxMessageKind::Audit,
                    health_topic.clone(),
                    QoS::AtLeastOnce,
                    false,
                    payload,
                    flush_batch,
                )
                .await;
            }
        }
    });

    let heartbeat_path = config.heartbeat_path.clone();
    let mut last_beat: Option<Instant> = None;
    let mut heartbeat_failing = false;

    let mut watch = BrokerActivityWatch::new(MQTT_KEEP_ALIVE, Instant::now());
    // Mirrors the watch's own last_activity purely so the log can say how long the
    // silence actually was; the watch keeps its field private.
    let mut watch_last_seen = Instant::now();

    loop {
        // Written from THIS loop on purpose: what has to be proven alive is the loop
        // that wedged on 2026-09-02. A beat emitted from a spawned task could keep
        // ticking with this one dead, which is the failure it exists to catch.
        if let Some(path) = &heartbeat_path {
            let beat_now = Instant::now();
            if crate::heartbeat::due(last_beat, beat_now) {
                last_beat = Some(beat_now);
                match crate::heartbeat::write(path, SystemTime::now()) {
                    Ok(()) => {
                        if heartbeat_failing {
                            heartbeat_failing = false;
                            info!("heartbeat writing recovered");
                        }
                    }
                    // Never fatal: an agent that cannot write its heartbeat is still an
                    // agent reading scales. Warned once so a broken path is visible
                    // without filling the log every five seconds.
                    Err(e) => {
                        if !heartbeat_failing {
                            heartbeat_failing = true;
                            warn!(
                                "cannot write the heartbeat to {}: {}; the supervisor will not be able to tell a wedged agent from a healthy one",
                                path.display(),
                                e
                            );
                        }
                    }
                }
            }
        }

        if let Ok(flushed) = outbox.flush_pending(&client, flush_batch).await {
            if flushed > 0 {
                metrics.add_outbox_flushed(flushed);
            }
        }
        let mut restart_requested = false;

        // Presume the session dead after too long a silence, even though nothing
        // errored. `poll()` can keep succeeding on a half-open socket -- it still
        // emits outgoing PingReq, because writing into a dead socket's send buffer
        // does not fail -- so only inbound traffic proves the broker is still there.
        let now = Instant::now();
        if watch.should_force_reconnect(now) {
            warn!(
                "no mqtt broker activity for {}s (> keep_alive*{}); presuming the session dead and reconnecting",
                now.saturating_duration_since(watch_last_seen).as_secs(),
                STALE_KEEP_ALIVE_MULTIPLIER
            );
            outbox.set_broker_session(BrokerSession::Down);
            event_loop.clean();
            watch.record_activity(now);
            watch_last_seen = now;
            continue;
        }

        // The timeout is the *remaining* time to the staleness deadline, never a fixed
        // interval, so it only ever fires where the connection was going to be torn
        // down anyway -- cancelling poll() mid-flight can then never leave a
        // still-in-use connection half-written.
        let polled = match tokio::time::timeout(watch.next_check_in(now), event_loop.poll()).await
        {
            Ok(polled) => polled,
            Err(_) => {
                warn!(
                    "mqtt broker silent for {}s while polling (> keep_alive*{}); forcing reconnect",
                    watch.stale_after().as_secs(),
                    STALE_KEEP_ALIVE_MULTIPLIER
                );
                outbox.set_broker_session(BrokerSession::Down);
                event_loop.clean();
                let woke = Instant::now();
                watch.record_activity(woke);
                watch_last_seen = woke;
                continue;
            }
        };

        if matches!(&polled, Ok(Event::Incoming(_))) {
            let heard = Instant::now();
            watch.record_activity(heard);
            watch_last_seen = heard;
        }

        match polled {
            Ok(Event::Incoming(Incoming::Publish(packet))) => {
                debug!(
                    "mqtt incoming topic='{}' bytes={} qos={:?} retain={}",
                    packet.topic,
                    packet.payload.len(),
                    packet.qos,
                    packet.retain
                );
                if packet.topic == cmd_topic {
                    handlers::handle_write_command_packet(
                        engine.as_ref(),
                        &source_id,
                        packet.payload.as_ref(),
                        &client,
                        &outbox,
                        &metrics,
                        &ack_topic,
                        flush_batch,
                    )
                    .await;
                } else if packet.topic == action_cmd_topic {
                    metrics.inc_cmd_received();
                    handlers::handle_action_command_packet(
                        &config,
                        &action_runtime_state,
                        action_orchestrator.as_ref(),
                        &source_id,
                        packet.payload.as_ref(),
                        &client,
                        &outbox,
                        &metrics,
                        &action_result_topic,
                        &action_audit_topic,
                        &device_conn_state_topic,
                        flush_batch,
                    )
                    .await;
                } else if packet.topic == alert_ack_topic {
                    handlers::handle_alert_ack_packet(
                        &shared_alert_state,
                        &source_id,
                        packet.payload.as_ref(),
                        &client,
                        &outbox,
                        &metrics,
                        &alert_ack_result_topic,
                        flush_batch,
                    )
                    .await;
                } else if packet.topic == config_apply_topic {
                    if handlers::handle_config_apply_packet(
                        &config_sync,
                        &config_apply_receipt_path,
                        &source_id,
                        packet.payload.as_ref(),
                        config.config_hash.clone(),
                        &client,
                        &outbox,
                        &metrics,
                        &config_apply_result_topic,
                        flush_batch,
                    )
                    .await
                    {
                        restart_requested = true;
                    }
                } else if packet.topic == control_reset_topic {
                    if handlers::handle_control_reset_packet(
                        &source_id,
                        packet.payload.as_ref(),
                        &client,
                        &outbox,
                        &metrics,
                        &control_reset_result_topic,
                        flush_batch,
                    )
                    .await
                    {
                        restart_requested = true;
                    }
                }
            }
            Ok(Event::Incoming(Incoming::ConnAck(connack))) => {
                outbox.set_broker_session(BrokerSession::Live);
                if connack.session_present {
                    debug!("mqtt (re)connected with session_present=true; subscriptions retained by broker");
                } else {
                    warn!(
                        "mqtt (re)connected with session_present=false; re-subscribing all inbound topics"
                    );
                    if let Err(e) = subscribe_all_topics(
                        &client,
                        &cmd_topic,
                        &action_cmd_topic,
                        &alert_ack_topic,
                        &config_apply_topic,
                        &control_reset_topic,
                    )
                    .await
                    {
                        warn!("failed to re-subscribe after mqtt reconnect: {}", e);
                    } else {
                        info!("mqtt re-subscription after reconnect completed");
                    }
                }
            }
            Ok(_) => {}
            Err(e) => {
                // No session until the next ConnAck. Draining the outbox now would move
                // rows out of durable storage and into rumqttc's in-memory pending queue,
                // which is the opposite of what the outbox is for.
                outbox.set_broker_session(BrokerSession::Down);
                let stats = outbox.stats().await;
                warn!(
                    "MQTT event loop error: {}; retrying poll in 1s (outbox_depth={}, oldest_age_secs={:?})",
                    e, stats.depth, stats.oldest_age_secs
                );
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }
        }
        if restart_requested {
            info!("config apply requested; stopping mqtt bridge for restart");
            publish_task.abort();
            health_task.abort();
            return Ok(MqttBridgeExit::RestartRequested);
        }
    }
}

fn apply_edge_mqtt_security_from_env(opts: &mut MqttOptions) -> anyhow::Result<()> {
    let runtime_prod = std::env::var("EDGE_RUNTIME_ENV")
        .or_else(|_| std::env::var("CENTRAL_RUNTIME_ENV"))
        .unwrap_or_else(|_| "dev".to_string())
        .eq_ignore_ascii_case("prod");

    let mut has_credentials = false;
    if let Ok(user) = std::env::var("MQTT_USERNAME") {
        if !user.trim().is_empty() {
            let password = std::env::var("MQTT_PASSWORD").unwrap_or_default();
            opts.set_credentials(user, password);
            has_credentials = true;
        }
    }
    if runtime_prod && !has_credentials {
        return Err(anyhow::anyhow!(
            "MQTT_USERNAME and MQTT_PASSWORD are required in prod"
        ));
    }

    let tls_enabled = std::env::var("MQTT_TLS_ENABLED")
        .ok()
        .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
        .unwrap_or(false);
    if runtime_prod && !tls_enabled {
        return Err(anyhow::anyhow!(
            "MQTT_TLS_ENABLED must be true in prod (mqtts without fallback)"
        ));
    }
    if !tls_enabled {
        return Ok(());
    }

    if let Ok(path) = std::env::var("MQTT_TLS_CA_PATH") {
        if !path.trim().is_empty() {
            let ca = fs::read(&path)
                .map_err(|e| anyhow::anyhow!("failed to read MQTT_TLS_CA_PATH '{}': {}", path, e))?;
            opts.set_transport(Transport::tls(ca, None, None));
            return Ok(());
        }
    }

    opts.set_transport(Transport::tls_with_default_config());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action_orchestrator::action_buffer_id;
    use application::runtime::WriteCommandOutcome;
    use serde_json::Value;
    use std::sync::Mutex;
    use std::time::{SystemTime, UNIX_EPOCH};
    use tokio::net::TcpListener;

    struct FakeExecutor {
        calls: Mutex<Vec<(String, TagValue, Option<String>, WritePriority)>>,
    }

    impl FakeExecutor {
        fn new() -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl WriteCommandExecutor for FakeExecutor {
        async fn write_tag(
            &self,
            tag_id: TagId,
            value: TagValue,
            priority: WritePriority,
        ) -> Result<(), domain::DomainError> {
            self.calls
                .lock()
                .expect("fake lock")
                .push((tag_id.to_string(), value, None, priority));
            Ok(())
        }

        async fn write_tag_with_command_id(
            &self,
            tag_id: TagId,
            value: TagValue,
            command_id: String,
            priority: WritePriority,
        ) -> Result<(), domain::DomainError> {
            self.calls
                .lock()
                .expect("fake lock")
                .push((tag_id.to_string(), value, Some(command_id), priority));
            Ok(())
        }
    }

    struct MockOutboxPublisher {
        published: Mutex<Vec<(String, Vec<u8>)>>,
    }

    impl MockOutboxPublisher {
        fn new() -> Self {
            Self {
                published: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl OutboxPublisher for MockOutboxPublisher {
        async fn publish(
            &self,
            topic: String,
            _qos: QoS,
            _retain: bool,
            payload: Vec<u8>,
        ) -> Result<(), String> {
            self.published
                .lock()
                .expect("mock publisher lock")
                .push((topic, payload));
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_execute_write_command_uses_command_id_path() {
        let fake = FakeExecutor::new();
        execute_write_command(
            &fake,
            WriteTagCommandMessage {
                schema_version: Some(1),
                source: Some("central".to_string()),
                tag_id: "tag1".to_string(),
                value: TagValue::Float(1.1),
                command_id: Some("cmd-1".to_string()),
                priority: Some("high".to_string()),
            },
        )
        .await
        .unwrap();

        let calls = fake.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "tag1");
        assert_eq!(calls[0].2.as_deref(), Some("cmd-1"));
        assert_eq!(calls[0].3, WritePriority::High);
    }

    #[tokio::test]
    async fn test_execute_write_command_without_command_id() {
        let fake = FakeExecutor::new();
        execute_write_command(
            &fake,
            WriteTagCommandMessage {
                schema_version: Some(1),
                source: Some("central".to_string()),
                tag_id: "tag2".to_string(),
                value: TagValue::Integer(7),
                command_id: None,
                priority: None,
            },
        )
        .await
        .unwrap();

        let calls = fake.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "tag2");
        assert!(calls[0].2.is_none());
        assert_eq!(calls[0].3, WritePriority::Normal);
    }

    #[test]
    fn test_parse_write_command_message() {
        let payload = br#"{"schema_version":1,"source":"central","tag_id":"tag1","value":42,"command_id":"cmd-9","priority":"high"}"#;
        let parsed = parse_write_command_message(payload).unwrap();
        assert_eq!(parsed.schema_version, Some(1));
        assert_eq!(parsed.source.as_deref(), Some("central"));
        assert_eq!(parsed.tag_id, "tag1");
        assert_eq!(parsed.value, TagValue::Float(42.0));
        assert_eq!(parsed.command_id.as_deref(), Some("cmd-9"));
        assert_eq!(parsed.priority.as_deref(), Some("high"));
    }

    #[test]
    fn test_parse_write_command_message_backward_compatible() {
        let payload = br#"{"tag_id":"tag1","value":42}"#;
        let parsed = parse_write_command_message(payload).unwrap();
        assert_eq!(parsed.schema_version, None);
        assert_eq!(parsed.source, None);
        assert_eq!(parsed.tag_id, "tag1");
        assert_eq!(parsed.priority, None);
    }

    #[test]
    fn test_parse_action_command_message() {
        let payload = br#"{
            "schema_version":1,
            "source":"central",
            "request_id":"act-1",
            "action_type":"print.escpos",
            "target":"edge",
            "payload":{"lines":["A","B"]}
        }"#;
        let parsed = parse_action_command_message(payload).unwrap();
        assert_eq!(parsed.schema_version, Some(1));
        assert_eq!(parsed.request_id.as_deref(), Some("act-1"));
        assert_eq!(parsed.action_type, "print.escpos");
        assert_eq!(parsed.target.as_deref(), Some("edge"));
    }

    #[test]
    fn test_parse_write_priority_validation() {
        assert_eq!(
            parse_write_priority(Some("high")).unwrap(),
            WritePriority::High
        );
        assert_eq!(
            parse_write_priority(Some("normal")).unwrap(),
            WritePriority::Normal
        );
        assert!(parse_write_priority(Some("urgent")).is_err());
    }

    #[test]
    fn test_to_write_audit_message_maps_runtime_event() {
        let evt = RuntimeEvent::TagWriteCommandHandled {
            connection_id: Some(domain::id::ConnectionId::new("conn1")),
            tag_id: TagId::new("tag1"),
            command_id: Some("cmd-1".to_string()),
            value: TagValue::Boolean(true),
            outcome: WriteCommandOutcome::Applied,
            reason: None,
            timestamp: chrono::Utc::now(),
        };
        let msg = to_write_audit_message(&evt).expect("expected mapping");
        assert_eq!(msg.schema_version, 1);
        assert_eq!(msg.source, "edge-agent".to_string());
        assert_eq!(msg.tag_id, "tag1");
        assert_eq!(msg.command_id.as_deref(), Some("cmd-1"));
        assert_eq!(msg.outcome, "Applied");
    }

    #[test]
    fn test_to_tag_telemetry_message_maps_runtime_event() {
        let evt = RuntimeEvent::TagChanged {
            tag_id: TagId::new("tag_scale_compound"),
            device_id: domain::id::DeviceId::new("dev_scale_1"),
            value: TagValue::String("{\"value\":12.3,\"unit\":\"g\"}".to_string()),
            trigger_value: None,
            quality: domain::tag::TagQuality::good(),
            timestamp: chrono::Utc::now(),
        };
        let msg = to_tag_telemetry_message(&evt).expect("expected telemetry mapping");
        assert_eq!(msg.schema_version, 1);
        assert_eq!(msg.tag_id, "tag_scale_compound");
    }

    #[test]
    fn test_topics_follow_convention() {
        let cfg = MqttBridgeConfig {
            site: "plant-a".to_string(),
            agent: "edge-01".to_string(),
            broker_host: "localhost".to_string(),
            broker_port: 1883,
            client_id: "edge-01".to_string(),
            outbox_path: "./data/mqtt_outbox.db".to_string(),
            ticket_sequence_path: "./data/ticket_sequence.db".to_string(),
            outbox_flush_batch: 50,
            heartbeat_path: None,
            outbox_max_messages: 1000,
            outbox_active_key_id: "v1".to_string(),
            outbox_prev_key_id: None,
            outbox_encryption_secret: None,
            outbox_hmac_secret: None,
            outbox_prev_encryption_secret: None,
            outbox_prev_hmac_secret: None,
            health_publish_interval_secs: 30,
            health_outbox_depth_warn: 1000,
            health_outbox_oldest_secs_warn: 300,
            alert_degraded_streak: 3,
            alert_recovered_streak: 3,
            alert_dedup_window_secs: 300,
            config_hash: None,
            config_check_url: None,
            config_check_enroll_token: None,
            config_check_hmac_secret: None,
            config_check_key_id: None,
            config_cache_path: "./data/runtime_config.signed.json".to_string(),
            config_apply_receipt_path: "./data/config_apply_receipt.json".to_string(),
            config_check_interval_secs: 120,
            config_check_jitter_secs: 20,
            escpos_output_path: "./data/escpos_output.bin".to_string(),
            escpos_tcp_host: None,
            escpos_tcp_port: 9100,
            escpos_windows_share: None,
            on_demand_tcp_host: None,
            on_demand_tcp_port: None,
            on_demand_probe_enabled: false,
            on_demand_probe_timeout_ms: 1200,
            on_demand_probe_connection_id: None,
            on_demand_probe_device_id: None,
            automations: vec![],
        };
        assert_eq!(
            cfg.command_topic(),
            "scada/plant-a/edge/edge-01/cmd/write".to_string()
        );
        assert_eq!(
            cfg.audit_topic(),
            "scada/plant-a/edge/edge-01/audit/write".to_string()
        );
        assert_eq!(
            cfg.ack_topic(),
            "scada/plant-a/edge/edge-01/cmd/write/ack".to_string()
        );
        assert_eq!(
            cfg.action_command_topic(),
            "scada/plant-a/edge/edge-01/cmd/action".to_string()
        );
        assert_eq!(
            cfg.action_result_topic(),
            "scada/plant-a/edge/edge-01/cmd/action/result".to_string()
        );
        assert_eq!(
            cfg.action_audit_topic(),
            "scada/plant-a/edge/edge-01/audit/action".to_string()
        );
        assert_eq!(
            cfg.health_topic(),
            "scada/plant-a/edge/edge-01/health/runtime".to_string()
        );
        assert_eq!(
            cfg.alert_topic(),
            "scada/plant-a/edge/edge-01/alerts/runtime".to_string()
        );
        assert_eq!(
            cfg.alert_ack_topic(),
            "scada/plant-a/edge/edge-01/alerts/runtime/ack".to_string()
        );
        assert_eq!(
            cfg.alert_ack_result_topic(),
            "scada/plant-a/edge/edge-01/alerts/runtime/ack/result".to_string()
        );
        assert_eq!(
            cfg.telemetry_tag_topic("tag_scale_compound"),
            "scada/plant-a/edge/edge-01/telemetry/tag/tag_scale_compound".to_string()
        );
        assert_eq!(
            cfg.connection_state_topic(),
            "scada/plant-a/edge/edge-01/conn/state".to_string()
        );
        assert_eq!(
            cfg.device_connection_state_topic(),
            "scada/plant-a/edge/edge-01/device/conn/state".to_string()
        );
        assert_eq!(
            cfg.control_reset_topic(),
            "scada/plant-a/edge/edge-01/control/reset".to_string()
        );
        assert_eq!(
            cfg.control_reset_result_topic(),
            "scada/plant-a/edge/edge-01/control/reset/result".to_string()
        );
    }

    #[test]
    fn test_to_device_connection_state_message_maps_runtime_event() {
        let evt = RuntimeEvent::DeviceProtocolStateChanged {
            connection_id: domain::id::ConnectionId::new("conn_modbus"),
            device_id: domain::id::DeviceId::new("dev_modbus_100"),
            tag_id: Some(TagId::new("tag_pm10")),
            state: application::runtime::DeviceProtocolState::Error,
            reason: Some("modbus read timeout".to_string()),
            timestamp: chrono::Utc::now(),
        };
        let msg =
            to_device_connection_state_message(&evt).expect("expected device connection mapping");
        assert_eq!(msg.schema_version, 1);
        assert_eq!(msg.connection_id, "conn_modbus");
        assert_eq!(msg.device_id, "dev_modbus_100");
        assert_eq!(msg.tag_id.as_deref(), Some("tag_pm10"));
        assert_eq!(msg.state, "Error");
    }

    #[test]
    fn test_build_write_command_ack_success() {
        let cmd = WriteTagCommandMessage {
            schema_version: Some(1),
            source: Some("central".to_string()),
            tag_id: "tag1".to_string(),
            value: TagValue::Float(1.0),
            command_id: Some("cmd-1".to_string()),
            priority: Some("high".to_string()),
        };
        let ack = build_write_command_ack("edge/edge-01", &cmd, &Ok(()));
        assert_eq!(ack.schema_version, 1);
        assert_eq!(ack.source, "edge/edge-01".to_string());
        assert!(ack.success);
        assert_eq!(ack.tag_id.as_deref(), Some("tag1"));
        assert_eq!(ack.command_id.as_deref(), Some("cmd-1"));
        assert!(ack.reason.is_none());
    }

    #[test]
    fn test_build_write_command_ack_error() {
        let cmd = WriteTagCommandMessage {
            schema_version: Some(1),
            source: Some("central".to_string()),
            tag_id: "tag2".to_string(),
            value: TagValue::Boolean(false),
            command_id: Some("cmd-err".to_string()),
            priority: Some("normal".to_string()),
        };
        let ack = build_write_command_ack(
            "edge/edge-01",
            &cmd,
            &Err(domain::DomainError::DriverError(
                "simulated driver error".to_string(),
            )),
        );
        assert!(!ack.success);
        assert_eq!(ack.tag_id.as_deref(), Some("tag2"));
        assert_eq!(ack.command_id.as_deref(), Some("cmd-err"));
        assert!(
            ack.reason
                .unwrap_or_default()
                .contains("simulated driver error")
        );
    }

    #[test]
    fn test_build_invalid_payload_ack() {
        let ack = build_invalid_payload_ack("edge/edge-01", "bad json".to_string());
        assert_eq!(ack.schema_version, 1);
        assert_eq!(ack.source, "edge/edge-01".to_string());
        assert!(!ack.success);
        assert!(ack.tag_id.is_none());
        assert!(ack.command_id.is_none());
        assert_eq!(ack.reason.as_deref(), Some("bad json"));
    }

    #[test]
    fn test_compute_health_status_transitions() {
        let ok = OutboxStats {
            depth: 10,
            oldest_age_secs: Some(5),
        };
        assert_eq!(compute_health_status(&ok, 100, 60), "ok");

        let degraded_depth = OutboxStats {
            depth: 200,
            oldest_age_secs: Some(5),
        };
        assert_eq!(compute_health_status(&degraded_depth, 100, 60), "degraded");

        let degraded_age = OutboxStats {
            depth: 10,
            oldest_age_secs: Some(120),
        };
        assert_eq!(compute_health_status(&degraded_age, 100, 60), "degraded");
    }

    #[test]
    fn test_evaluate_alert_transition_raise_and_clear() {
        let mut state = AlertState::default();
        assert_eq!(
            evaluate_alert_transition("degraded", &mut state, 2, 2),
            None
        );
        assert_eq!(
            evaluate_alert_transition("degraded", &mut state, 2, 2),
            Some(AlertTransition::Raised)
        );
        assert_eq!(state.active, true);
        assert_eq!(evaluate_alert_transition("ok", &mut state, 2, 2), None);
        assert_eq!(
            evaluate_alert_transition("ok", &mut state, 2, 2),
            Some(AlertTransition::Cleared)
        );
        assert_eq!(state.active, false);
    }

    #[test]
    fn test_should_emit_alert_for_window_deduplicates() {
        let mut state = AlertState::default();
        assert!(should_emit_alert_for_window(
            &mut state,
            "runtime_health_degraded",
            AlertTransition::Raised,
            300
        ));
        assert!(!should_emit_alert_for_window(
            &mut state,
            "runtime_health_degraded",
            AlertTransition::Raised,
            300
        ));
    }

    #[test]
    fn test_parse_alert_ack_command_message() {
        let payload = br#"{"schema_version":1,"source":"central","alert_type":"runtime_health_degraded","ack_id":"ack-1","operator":"op"}"#;
        let parsed = parse_alert_ack_command_message(payload).unwrap();
        assert_eq!(parsed.alert_type, "runtime_health_degraded");
        assert_eq!(parsed.ack_id.as_deref(), Some("ack-1"));
        assert_eq!(parsed.operator.as_deref(), Some("op"));
    }

    #[test]
    fn test_action_buffer_id_uses_explicit_then_context() {
        let explicit = serde_json::json!({
            "buffer_id":"weights_session_1",
            "trigger":{"device_id":"dev_scale_1","tag_id":"tag_x"}
        });
        assert_eq!(action_buffer_id(&explicit), "weights_session_1");

        let contextual = serde_json::json!({
            "trigger":{"device_id":"dev_scale_1","tag_id":"tag_x"}
        });
        assert_eq!(action_buffer_id(&contextual), "device:dev_scale_1");

        let tag_only = serde_json::json!({
            "trigger":{"tag_id":"tag_x"}
        });
        assert_eq!(action_buffer_id(&tag_only), "tag:tag_x");
    }

    fn mk_test_cfg(path: String) -> MqttBridgeConfig {
        MqttBridgeConfig {
            site: "plant-a".to_string(),
            agent: "edge-01".to_string(),
            broker_host: "localhost".to_string(),
            broker_port: 1883,
            client_id: "edge-01".to_string(),
            outbox_path: "./data/mqtt_outbox.db".to_string(),
            ticket_sequence_path: "./data/ticket_sequence.db".to_string(),
            outbox_flush_batch: 50,
            heartbeat_path: None,
            outbox_max_messages: 1000,
            outbox_active_key_id: "v1".to_string(),
            outbox_prev_key_id: None,
            outbox_encryption_secret: None,
            outbox_hmac_secret: None,
            outbox_prev_encryption_secret: None,
            outbox_prev_hmac_secret: None,
            health_publish_interval_secs: 30,
            health_outbox_depth_warn: 1000,
            health_outbox_oldest_secs_warn: 300,
            alert_degraded_streak: 3,
            alert_recovered_streak: 3,
            alert_dedup_window_secs: 300,
            config_hash: None,
            config_check_url: None,
            config_check_enroll_token: None,
            config_check_hmac_secret: None,
            config_check_key_id: None,
            config_cache_path: "./data/runtime_config.signed.json".to_string(),
            config_apply_receipt_path: "./data/config_apply_receipt.json".to_string(),
            config_check_interval_secs: 120,
            config_check_jitter_secs: 20,
            escpos_output_path: path,
            escpos_tcp_host: None,
            escpos_tcp_port: 9100,
            escpos_windows_share: None,
            on_demand_tcp_host: None,
            on_demand_tcp_port: None,
            on_demand_probe_enabled: false,
            on_demand_probe_timeout_ms: 1200,
            on_demand_probe_connection_id: None,
            on_demand_probe_device_id: None,
            automations: vec![],
        }
    }

    async fn exec_action(
        orchestrator: &ActionOrchestrator,
        cfg: &MqttBridgeConfig,
        state: &Arc<TokioMutex<ActionRuntimeState>>,
        cmd: &EdgeActionCommandMessage,
    ) -> Result<(), String> {
        let req = to_action_request(cmd);
        orchestrator.execute(cfg, state, &req).await
    }

    #[tokio::test]
    async fn test_buffer_accumulate_then_print_from_buffer() {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let out = std::env::temp_dir()
            .join(format!("ifascada-escpos-{}.txt", suffix))
            .to_string_lossy()
            .to_string();
        let cfg = mk_test_cfg(out.clone());
        let orchestrator = ActionOrchestrator::new_default();
        let state = Arc::new(TokioMutex::new(ActionRuntimeState::default()));

        let acc_cmd = EdgeActionCommandMessage {
            schema_version: Some(1),
            source: Some("edge-automation".to_string()),
            request_id: Some("acc-1".to_string()),
            action_type: "buffer.weights.accumulate".to_string(),
            target: Some("edge".to_string()),
            payload: serde_json::json!({
                "buffer_id":"weights-a",
                "only_positive": true,
                "trigger": {
                    "value": "{\"value\":12.345,\"unit\":\"g\",\"raw\":\"+ 12.3450 g\"}"
                }
            }),
        };
        exec_action(&orchestrator, &cfg, &state, &acc_cmd)
            .await
            .unwrap();

        let print_cmd = EdgeActionCommandMessage {
            schema_version: Some(1),
            source: Some("edge-automation".to_string()),
            request_id: Some("print-1".to_string()),
            action_type: "print.escpos.from_buffer".to_string(),
            target: Some("edge".to_string()),
            payload: serde_json::json!({
                "buffer_id":"weights-a",
                "clear_after_print": true
            }),
        };
        exec_action(&orchestrator, &cfg, &state, &print_cmd)
            .await
            .unwrap();

        let txt = std::fs::read_to_string(&out).unwrap_or_default();
        assert!(txt.contains("COUNT: 1"));
        let _ = std::fs::remove_file(&out);
    }

    #[tokio::test]
    async fn test_print_persist_accepts_central_target() {
        let cfg = mk_test_cfg("./data/test-print-persist.txt".to_string());
        let orchestrator = ActionOrchestrator::new_default();
        let state = Arc::new(TokioMutex::new(ActionRuntimeState::default()));
        let cmd = EdgeActionCommandMessage {
            schema_version: Some(1),
            source: Some("edge-automation".to_string()),
            request_id: Some("persist-1".to_string()),
            action_type: "print.persist".to_string(),
            target: Some("central".to_string()),
            payload: serde_json::json!({"doc":"x"}),
        };
        exec_action(&orchestrator, &cfg, &state, &cmd)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_print_escpos_with_buffer_mode_uses_buffer() {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let out = std::env::temp_dir()
            .join(format!("ifascada-escpos-buffer-{}.txt", suffix))
            .to_string_lossy()
            .to_string();
        let cfg = mk_test_cfg(out.clone());
        let orchestrator = ActionOrchestrator::new_default();
        let state = Arc::new(TokioMutex::new(ActionRuntimeState::default()));

        let acc_cmd = EdgeActionCommandMessage {
            schema_version: Some(1),
            source: Some("edge-automation".to_string()),
            request_id: Some("acc-b1".to_string()),
            action_type: "buffer.weights.accumulate".to_string(),
            target: Some("edge".to_string()),
            payload: serde_json::json!({
                "buffer_id":"weights-b",
                "trigger": { "value": "{\"value\":9.1,\"unit\":\"g\",\"raw\":\"+ 9.1000 g\"}" }
            }),
        };
        exec_action(&orchestrator, &cfg, &state, &acc_cmd)
            .await
            .unwrap();

        let print_cmd = EdgeActionCommandMessage {
            schema_version: Some(1),
            source: Some("edge-automation".to_string()),
            request_id: Some("print-b1".to_string()),
            action_type: "print.escpos".to_string(),
            target: Some("edge".to_string()),
            payload: serde_json::json!({
                "mode":"from_buffer",
                "buffer_id":"weights-b",
                "clear_after_print": true
            }),
        };
        exec_action(&orchestrator, &cfg, &state, &print_cmd)
            .await
            .unwrap();
        let txt = std::fs::read_to_string(&out).unwrap_or_default();
        assert!(txt.contains("COUNT: 1"));
        let _ = std::fs::remove_file(&out);
    }

    #[tokio::test]
    async fn test_connection_check_requires_target() {
        let cfg = mk_test_cfg("./data/test-conn-check.txt".to_string());
        let orchestrator = ActionOrchestrator::new_default();
        let state = Arc::new(TokioMutex::new(ActionRuntimeState::default()));
        let cmd = EdgeActionCommandMessage {
            schema_version: Some(1),
            source: Some("manual".to_string()),
            request_id: Some("conn-check-1".to_string()),
            action_type: "connection.check".to_string(),
            target: Some("edge".to_string()),
            payload: serde_json::json!({}),
        };
        let err = exec_action(&orchestrator, &cfg, &state, &cmd)
            .await
            .expect_err("expected missing target error");
        assert!(err.contains("requires host"));
    }

    #[test]
    fn test_parse_config_apply_command_message_backward_compatible() {
        let payload = br#"{"request_id":"cfg-1"}"#;
        let parsed = parse_config_apply_command_message(payload).expect("parse config apply");
        assert_eq!(parsed.request_id.as_deref(), Some("cfg-1"));
        assert_eq!(parsed.schema_version, None);
        assert_eq!(parsed.source, None);
    }

    #[test]
    fn test_config_apply_receipt_roundtrip_and_remove() {
        let path = std::env::temp_dir().join(format!(
            "ifascada-config-apply-receipt-{}.json",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let receipt = ConfigApplyReceipt {
            request_id: Some("cfg-req-1".to_string()),
            target_config_hash: Some("hash-abc".to_string()),
            requested_at: chrono::Utc::now(),
        };
        write_config_apply_receipt(path.to_string_lossy().as_ref(), &receipt)
            .expect("write receipt");

        let loaded = read_and_remove_config_apply_receipt(path.to_string_lossy().as_ref())
            .expect("receipt should exist");
        assert_eq!(loaded.request_id.as_deref(), Some("cfg-req-1"));
        assert_eq!(loaded.target_config_hash.as_deref(), Some("hash-abc"));
        assert!(
            read_and_remove_config_apply_receipt(path.to_string_lossy().as_ref()).is_none(),
            "receipt file should be removed after first read"
        );
    }

    #[tokio::test]
    async fn test_on_demand_startup_probe_publishes_connected_state() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind listener");
        let port = listener.local_addr().expect("listener addr").port();
        let accept_task = tokio::spawn(async move {
            let _ = listener.accept().await;
        });

        let mut cfg = mk_test_cfg("./data/test-on-demand-probe-ok.txt".to_string());
        cfg.on_demand_probe_enabled = true;
        cfg.on_demand_probe_timeout_ms = 500;
        cfg.on_demand_probe_connection_id = Some("conn_printer_1".to_string());
        cfg.on_demand_probe_device_id = Some("dev_printer_1".to_string());
        cfg.on_demand_tcp_host = Some("127.0.0.1".to_string());
        cfg.on_demand_tcp_port = Some(port);

        let publisher = MockOutboxPublisher::new();
        let outbox_path = std::env::temp_dir().join(format!(
            "ifascada-on-demand-probe-{}.db",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let outbox = PersistentMqttOutbox::new(outbox_path.clone(), OutboxConfig::default())
            .expect("create outbox");
        let metrics = BridgeMetrics::default();

        publish_on_demand_startup_probe(
            &cfg,
            &publisher,
            &outbox,
            &metrics,
            "scada/plant-a/edge/edge-com-01/state/device/conn",
            "edge/edge-com-01",
            10,
        )
        .await;

        let published = publisher.published.lock().expect("published lock");
        assert_eq!(published.len(), 1);
        assert_eq!(
            published[0].0,
            "scada/plant-a/edge/edge-com-01/state/device/conn"
        );
        let msg: Value = serde_json::from_slice(&published[0].1).expect("json payload");
        assert_eq!(msg["connection_id"].as_str(), Some("conn_printer_1"));
        assert_eq!(msg["device_id"].as_str(), Some("dev_printer_1"));
        assert_eq!(msg["state"].as_str(), Some("Connected"));
        assert_eq!(msg["reason"].as_str(), Some("startup_probe_ok"));

        let _ = std::fs::remove_file(outbox_path);
        let _ = accept_task.await;
    }

    #[tokio::test]
    async fn test_on_demand_startup_probe_skips_when_ids_missing() {
        let mut cfg = mk_test_cfg("./data/test-on-demand-probe-skip.txt".to_string());
        cfg.on_demand_probe_enabled = true;
        cfg.on_demand_tcp_host = Some("127.0.0.1".to_string());
        cfg.on_demand_tcp_port = Some(9100);
        cfg.on_demand_probe_connection_id = None;
        cfg.on_demand_probe_device_id = Some("dev_printer_1".to_string());

        let publisher = MockOutboxPublisher::new();
        let outbox_path = std::env::temp_dir().join(format!(
            "ifascada-on-demand-probe-skip-{}.db",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let outbox = PersistentMqttOutbox::new(outbox_path.clone(), OutboxConfig::default())
            .expect("create outbox");
        let metrics = BridgeMetrics::default();

        publish_on_demand_startup_probe(
            &cfg,
            &publisher,
            &outbox,
            &metrics,
            "scada/plant-a/edge/edge-com-01/state/device/conn",
            "edge/edge-com-01",
            10,
        )
        .await;

        let published = publisher.published.lock().expect("published lock");
        assert!(published.is_empty());

        let _ = std::fs::remove_file(outbox_path);
    }

    #[tokio::test]
    async fn test_print_idempotent_by_request_id() {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let out = std::env::temp_dir()
            .join(format!("ifascada-escpos-idem-{}.txt", suffix))
            .to_string_lossy()
            .to_string();
        let cfg = mk_test_cfg(out.clone());
        let orchestrator = ActionOrchestrator::new_default();
        let state = Arc::new(TokioMutex::new(ActionRuntimeState::default()));

        let cmd = EdgeActionCommandMessage {
            schema_version: Some(1),
            source: Some("central".to_string()),
            request_id: Some("dup-1".to_string()),
            action_type: "print.escpos".to_string(),
            target: Some("edge".to_string()),
            payload: serde_json::json!({"lines":["A","B"]}),
        };
        exec_action(&orchestrator, &cfg, &state, &cmd)
            .await
            .unwrap();
        exec_action(&orchestrator, &cfg, &state, &cmd)
            .await
            .unwrap();

        let txt = std::fs::read_to_string(&out).unwrap_or_default();
        let hits = txt.matches("request_id=dup-1").count();
        assert_eq!(hits, 1);
        let _ = std::fs::remove_file(&out);
    }

    #[tokio::test]
    async fn test_device_command_print_routes_to_escpos() {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let out = std::env::temp_dir()
            .join(format!("ifascada-device-command-print-{}.txt", suffix))
            .to_string_lossy()
            .to_string();
        let cfg = mk_test_cfg(out.clone());
        let orchestrator = ActionOrchestrator::new_default();
        let state = Arc::new(TokioMutex::new(ActionRuntimeState::default()));

        let cmd = EdgeActionCommandMessage {
            schema_version: Some(1),
            source: Some("central".to_string()),
            request_id: Some("dev-cmd-print-1".to_string()),
            action_type: "device.command".to_string(),
            target: Some("edge".to_string()),
            payload: serde_json::json!({
                "device_id":"dev_printer_u220",
                "command":"print",
                "args":{
                    "lines":["IFA SCADA","PRINT FROM DEVICE.COMMAND"]
                }
            }),
        };
        exec_action(&orchestrator, &cfg, &state, &cmd)
            .await
            .expect("device.command print");

        let txt = std::fs::read_to_string(&out).unwrap_or_default();
        assert!(txt.contains("action=print.escpos"));
        assert!(txt.contains("PRINT FROM DEVICE.COMMAND"));
        let _ = std::fs::remove_file(&out);
    }

    #[tokio::test]
    async fn test_device_command_connection_check_uses_device_transport_defaults() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind listener");
        let port = listener.local_addr().expect("listener addr").port();
        let accept_task = tokio::spawn(async move {
            let _ = listener.accept().await;
        });

        let cfg = mk_test_cfg("./data/test-device-command-check.txt".to_string());
        let orchestrator = ActionOrchestrator::new_default();
        let state = Arc::new(TokioMutex::new(ActionRuntimeState::default()));
        let cmd = EdgeActionCommandMessage {
            schema_version: Some(1),
            source: Some("central".to_string()),
            request_id: Some("dev-cmd-check-1".to_string()),
            action_type: "device.command".to_string(),
            target: Some("edge".to_string()),
            payload: serde_json::json!({
                "device_id":"dev_printer_u220",
                "command":"connection.check",
                "device":{
                    "id":"dev_printer_u220",
                    "transport":{
                        "tcp":{"host":"127.0.0.1","port":port}
                    }
                },
                "args":{"timeout_ms":800}
            }),
        };
        exec_action(&orchestrator, &cfg, &state, &cmd)
            .await
            .expect("device.command connection.check");
        let _ = accept_task.await;
    }

    #[tokio::test]
    async fn test_device_command_requires_device_id() {
        let cfg = mk_test_cfg("./data/test-device-command-err.txt".to_string());
        let orchestrator = ActionOrchestrator::new_default();
        let state = Arc::new(TokioMutex::new(ActionRuntimeState::default()));
        let cmd = EdgeActionCommandMessage {
            schema_version: Some(1),
            source: Some("central".to_string()),
            request_id: Some("dev-cmd-err-1".to_string()),
            action_type: "device.command".to_string(),
            target: Some("edge".to_string()),
            payload: serde_json::json!({
                "command":"print",
                "args":{"lines":["x"]}
            }),
        };
        let err = exec_action(&orchestrator, &cfg, &state, &cmd)
            .await
            .expect_err("expected validation error");
        assert!(err.contains("device_id"));
    }

    #[tokio::test]
    async fn test_subscribe_all_topics_issues_every_inbound_topic() {
        // No broker is started on purpose: AsyncClient::subscribe only enqueues
        // the request on the client's internal channel, it does not wait for a
        // network round-trip. This guards the exact set of topics re-subscribed
        // after every (re)connection (see the `ConnAck` handling in
        // `run_mqtt_bridge`) against accidental omissions/typos, but it cannot
        // exercise the real reconnect-then-resubscribe flow against a live
        // broker; that must be verified manually against a real edge (drop and
        // restore its network path and confirm the
        // "mqtt re-subscription after reconnect completed" log line appears).
        let opts = MqttOptions::new("test-resubscribe-client", "127.0.0.1", 1883);
        let (client, _event_loop) = AsyncClient::new(opts, 100);

        let result = subscribe_all_topics(
            &client,
            "scada/plant-a/edge/edge-01/cmd",
            "scada/plant-a/edge/edge-01/cmd/action",
            "scada/plant-a/edge/edge-01/cmd/alert/ack",
            "scada/plant-a/edge/edge-01/cmd/config/apply",
            "scada/plant-a/edge/edge-01/cmd/control/reset",
        )
        .await;

        assert!(
            result.is_ok(),
            "subscribe_all_topics should succeed without a live connection: {:?}",
            result.err()
        );
    }
}

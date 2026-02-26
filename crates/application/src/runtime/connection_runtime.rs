use crate::runtime::event_bus::{DeviceProtocolState, EventBus, RuntimeEvent, WriteCommandOutcome};
use crate::runtime::live_state::LiveState;
use crate::runtime::tag_runtime::TagRuntime;
use crate::runtime::write_audit::{WriteAuditRecord, WriteAuditRepository};
use domain::connection::ReconnectStrategy;
use domain::driver::DriverConnection;
use domain::id::{DeviceId, TagId};
use domain::tag::{QualityReason, TagQuality, TagUpdateMode, TagValue, TagValueType};
use domain::Connection;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TryRecvError;
use tokio::sync::oneshot;
use tokio::time::{Duration, Instant, interval};
use tracing::{error, info, warn};

const DEFAULT_COMMAND_DEDUP_MS: u64 = 30_000;
const DEFAULT_WRITE_CIRCUIT_FAIL_THRESHOLD: u64 = 0;
const DEFAULT_WRITE_CIRCUIT_COOLDOWN_MS: u64 = 0;

pub enum ConnectionCommand {
    Stop,
    Reload(Connection),
    WriteTag {
        tag_id: TagId,
        value: TagValue,
        command_id: Option<String>,
        priority: WritePriority,
        respond_to: oneshot::Sender<Result<(), domain::DomainError>>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WritePriority {
    Normal,
    High,
}

pub struct ConnectionRuntime {
    connection: Connection,
    driver: Box<dyn DriverConnection>,
    tags: HashMap<TagId, TagRuntimeState>,
    event_bus: EventBus,
    live_state: Arc<LiveState>,
    write_audit_repository: Arc<dyn WriteAuditRepository>,
    command_rx: mpsc::Receiver<ConnectionCommand>,
    retry_attempts: u32,
    next_retry_at: Option<Instant>,
    retries_exhausted: bool,
    seen_write_commands: HashMap<(TagId, String), Instant>,
    last_write_applied_at: HashMap<TagId, Instant>,
    write_fail_streak: u32,
    write_circuit_open_until: Option<Instant>,
    pending_writes_high: VecDeque<PendingWriteCommand>,
    pending_writes_normal: VecDeque<PendingWriteCommand>,
    device_protocol_state: HashMap<DeviceId, DeviceProtocolState>,
}

struct TagRuntimeState {
    runtime: TagRuntime,
    last_published_at: Option<Instant>,
    last_published_value: Option<TagValue>,
    last_received_at: Option<Instant>,
    current_quality: TagQuality,
    timeout_reported: bool,
}

enum WriteExecutionResult {
    Applied,
    Deduplicated,
}

struct PendingWriteCommand {
    tag_id: TagId,
    value: TagValue,
    command_id: Option<String>,
    respond_to: oneshot::Sender<Result<(), domain::DomainError>>,
}

impl ConnectionRuntime {
    fn publish_connection_state(&self, state: domain::driver::ConnectionState) {
        self.event_bus
            .publish(RuntimeEvent::ConnectionStateChanged {
                connection_id: self.connection.id.clone(),
                state,
            });
    }

    pub fn new(
        connection: Connection,
        driver: Box<dyn DriverConnection>,
        tags: Vec<TagRuntime>,
        event_bus: EventBus,
        live_state: Arc<LiveState>,
        write_audit_repository: Arc<dyn WriteAuditRepository>,
        command_rx: mpsc::Receiver<ConnectionCommand>,
    ) -> Self {
        let dedup_missing = connection
            .config
            .transport
            .get("command_dedup_ms")
            .is_none();
        if dedup_missing {
            warn!(
                "connection '{}' uses default command_dedup_ms={} (set transport.command_dedup_ms to tune)",
                connection.id,
                DEFAULT_COMMAND_DEDUP_MS
            );
        }
        let threshold = connection
            .config
            .transport
            .get("write_circuit_fail_threshold")
            .and_then(|v| v.as_u64())
            .unwrap_or(DEFAULT_WRITE_CIRCUIT_FAIL_THRESHOLD);
        let cooldown_ms = connection
            .config
            .transport
            .get("write_circuit_cooldown_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(DEFAULT_WRITE_CIRCUIT_COOLDOWN_MS);
        if threshold == 0 || cooldown_ms == 0 {
            warn!(
                "connection '{}' write circuit breaker disabled (threshold={}, cooldown_ms={})",
                connection.id,
                threshold,
                cooldown_ms
            );
        }

        let tags_map = tags
            .into_iter()
            .map(|t| {
                (
                    t.definition.id.clone(),
                    TagRuntimeState {
                        runtime: t,
                        last_published_at: None,
                        last_published_value: None,
                        last_received_at: None,
                        current_quality: TagQuality::good(),
                        timeout_reported: false,
                    },
                )
            })
            .collect();
        Self {
            connection,
            driver,
            tags: tags_map,
            event_bus,
            live_state,
            write_audit_repository,
            command_rx,
            retry_attempts: 0,
            next_retry_at: None,
            retries_exhausted: false,
            seen_write_commands: HashMap::new(),
            last_write_applied_at: HashMap::new(),
            write_fail_streak: 0,
            write_circuit_open_until: None,
            pending_writes_high: VecDeque::new(),
            pending_writes_normal: VecDeque::new(),
            device_protocol_state: HashMap::new(),
        }
    }

    pub async fn run(mut self) {
        info!("Starting ConnectionRuntime for {}", self.connection.id);

        let mut poll_interval =
            interval(Duration::from_millis(self.connection.timeout_ms.max(100)));

        loop {
            tokio::select! {
                cmd = self.command_rx.recv() => {
                    match cmd {
                        Some(ConnectionCommand::Stop) => break,
                        Some(ConnectionCommand::Reload(new_conn)) => self.handle_reload(new_conn).await,
                        Some(ConnectionCommand::WriteTag { tag_id, value, command_id, priority, respond_to }) => {
                            self.enqueue_write(
                                PendingWriteCommand { tag_id, value, command_id, respond_to },
                                priority,
                            );
                            if self.drain_write_commands_nonblocking() {
                                break;
                            }
                        }
                        None => break,
                    }
                }
                _ = poll_interval.tick() => {
                    if self.process_pending_writes().await {
                        continue;
                    }
                    if !self.driver.is_connected() {
                        let connected = self.attempt_connect_if_due().await;
                        if !connected {
                            continue;
                        }
                    }

                    match self.driver.poll().await {
                        Ok(results) => {
                            let now = Instant::now();
                            for (tag_id, value_res) in results {
                                let mut protocol_state_update: Option<(DeviceProtocolState, Option<String>)> =
                                    None;
                                if let Some(tag_state) = self.tags.get_mut(&tag_id) {
                                    match value_res {
                                        Ok(value) => {
                                            let was_bad = tag_state.current_quality.status
                                                == domain::tag::QualityStatus::Bad;
                                            tag_state.last_received_at = Some(now);
                                            tag_state.timeout_reported = false;
                                            tag_state.current_quality = TagQuality::good();
                                            protocol_state_update =
                                                Some((DeviceProtocolState::Connected, None));

                                            if was_bad
                                                || Self::should_publish(tag_state, &value, now)
                                            {
                                                let state = tag_state.runtime.process_raw_value(value.clone());
                                                self.live_state.update_tag(tag_id.clone(), state.value.clone(), state.quality);
                                                self.event_bus.publish(RuntimeEvent::TagChanged {
                                                    tag_id: tag_id.clone(),
                                                    value: state.value.clone(),
                                                    trigger_value: state.trigger_value.clone(),
                                                    quality: state.quality,
                                                    timestamp: state.timestamp,
                                                });
                                                tag_state.last_published_at = Some(now);
                                                tag_state.last_published_value = Some(value);
                                            }
                                        }
                                        Err(e) => {
                                            warn!("Error polling tag {}: {}", tag_id, e);
                                            self.mark_tag_bad_communication(&tag_id, now);
                                            protocol_state_update = Some((
                                                DeviceProtocolState::Error,
                                                Some(e.to_string()),
                                            ));
                                        }
                                    }
                                }
                                if let Some((state, reason)) = protocol_state_update {
                                    self.publish_device_protocol_state_for_tag(
                                        &tag_id,
                                        state,
                                        reason,
                                    );
                                }
                            }
                            self.evaluate_timeouts(now);
                        }
                        Err(e) => {
                            error!("Poll error on {}: {}", self.connection.id, e);
                            self.mark_all_tags_bad_communication(Instant::now());
                            self.publish_all_devices_protocol_state(
                                DeviceProtocolState::Error,
                                Some(format!("poll_error: {}", e)),
                            );
                            let _ = self.driver.disconnect().await;
                            self.publish_connection_state(domain::driver::ConnectionState::Disconnected);
                            self.record_connection_failure();
                        }
                    }
                }
            }
        }

        let _ = self.driver.disconnect().await;
        self.publish_connection_state(domain::driver::ConnectionState::Disconnected);
        info!("Stopped ConnectionRuntime for {}", self.connection.id);
    }

    fn should_publish(state: &TagRuntimeState, new_value: &TagValue, now: Instant) -> bool {
        let mode = &state.runtime.definition.update_mode;
        let changed = state
            .last_published_value
            .as_ref()
            .map(|v| v != new_value)
            .unwrap_or(true);

        match mode {
            TagUpdateMode::OnChange => changed,
            TagUpdateMode::OnMessage => true,
            TagUpdateMode::Polling { interval_ms } => match state.last_published_at {
                None => true,
                Some(last) => now.duration_since(last) >= Duration::from_millis(*interval_ms),
            },
            TagUpdateMode::PollingOnChange { interval_ms } => {
                let elapsed = match state.last_published_at {
                    None => true,
                    Some(last) => now.duration_since(last) >= Duration::from_millis(*interval_ms),
                };
                changed && elapsed
            }
        }
    }

    fn enqueue_write(&mut self, cmd: PendingWriteCommand, priority: WritePriority) {
        match priority {
            WritePriority::High => self.pending_writes_high.push_back(cmd),
            WritePriority::Normal => self.pending_writes_normal.push_back(cmd),
        }
    }

    fn drain_write_commands_nonblocking(&mut self) -> bool {
        loop {
            match self.command_rx.try_recv() {
                Ok(ConnectionCommand::Stop) => return true,
                Ok(ConnectionCommand::Reload(new_conn)) => {
                    self.connection = new_conn;
                    self.retry_attempts = 0;
                    self.next_retry_at = None;
                    self.retries_exhausted = false;
                    self.pending_writes_high.clear();
                    self.pending_writes_normal.clear();
                }
                Ok(ConnectionCommand::WriteTag {
                    tag_id,
                    value,
                    command_id,
                    priority,
                    respond_to,
                }) => {
                    self.enqueue_write(
                        PendingWriteCommand {
                            tag_id,
                            value,
                            command_id,
                            respond_to,
                        },
                        priority,
                    );
                }
                Err(TryRecvError::Empty) => return false,
                Err(TryRecvError::Disconnected) => return true,
            }
        }
    }

    fn pop_next_pending_write(&mut self) -> Option<PendingWriteCommand> {
        if let Some(cmd) = self.pending_writes_high.pop_front() {
            return Some(cmd);
        }
        self.pending_writes_normal.pop_front()
    }

    async fn process_pending_writes(&mut self) -> bool {
        let mut processed_any = false;
        while let Some(cmd) = self.pop_next_pending_write() {
            processed_any = true;
            let tag_id = cmd.tag_id.clone();
            let value = cmd.value.clone();
            let command_id = cmd.command_id.clone();
            let res = self
                .handle_write_command(tag_id.clone(), value.clone(), command_id.clone())
                .await;
            match &res {
                Ok(WriteExecutionResult::Applied) => {
                    self.publish_write_audit(
                        tag_id,
                        command_id,
                        value,
                        WriteCommandOutcome::Applied,
                        None,
                    );
                }
                Ok(WriteExecutionResult::Deduplicated) => {
                    self.publish_write_audit(
                        tag_id,
                        command_id,
                        value,
                        WriteCommandOutcome::Deduplicated,
                        None,
                    );
                }
                Err(err) => {
                    self.publish_write_audit(
                        tag_id,
                        command_id,
                        value,
                        WriteCommandOutcome::Rejected,
                        Some(err.to_string()),
                    );
                }
            }
            let _ = cmd.respond_to.send(res.map(|_| ()));
        }
        processed_any
    }

    async fn handle_reload(&mut self, new_conn: Connection) {
        self.connection = new_conn;
        self.retry_attempts = 0;
        self.next_retry_at = None;
        self.retries_exhausted = false;
        self.pending_writes_high.clear();
        self.pending_writes_normal.clear();
        let _ = self.driver.disconnect().await;
        self.publish_connection_state(domain::driver::ConnectionState::Disconnected);
    }

    async fn attempt_connect_if_due(&mut self) -> bool {
        if self.retries_exhausted {
            return false;
        }

        if let Some(max) = self.connection.reconnection.max_retries {
            if self.retry_attempts >= max {
                self.retries_exhausted = true;
                return false;
            }
        }

        if let Some(next) = self.next_retry_at {
            if Instant::now() < next {
                return false;
            }
        }

        self.publish_connection_state(domain::driver::ConnectionState::Connecting);
        match self.driver.connect().await {
            Ok(_) => {
                self.retry_attempts = 0;
                self.next_retry_at = None;
                self.retries_exhausted = false;
                self.publish_connection_state(domain::driver::ConnectionState::Connected);
                true
            }
            Err(e) => {
                error!("Failed to connect {}: {}", self.connection.id, e);
                self.record_connection_failure();
                false
            }
        }
    }

    fn record_connection_failure(&mut self) {
        self.retry_attempts = self.retry_attempts.saturating_add(1);

        self.publish_connection_state(domain::driver::ConnectionState::Failed);

        if let Some(max) = self.connection.reconnection.max_retries {
            if self.retry_attempts >= max {
                self.retries_exhausted = true;
                self.next_retry_at = None;
                return;
            }
        }

        let delay = self.compute_retry_delay(self.retry_attempts);
        self.next_retry_at = Some(Instant::now() + delay);
    }

    fn compute_retry_delay(&self, attempt: u32) -> Duration {
        match self.connection.reconnection.strategy {
            ReconnectStrategy::Fixed { delay_ms } => Duration::from_millis(delay_ms.max(1)),
            ReconnectStrategy::Exponential {
                initial_delay_ms,
                max_delay_ms,
            } => {
                let base = initial_delay_ms.max(1);
                let multiplier = 2u64.saturating_pow(attempt.saturating_sub(1));
                let delay = base.saturating_mul(multiplier).min(max_delay_ms.max(1));
                Duration::from_millis(delay)
            }
        }
    }

    fn evaluate_timeouts(&mut self, now: Instant) {
        let timeout_window_ms = self.connection.timeout_ms.max(100) * 2;
        let timeout_window = Duration::from_millis(timeout_window_ms);
        let ids: Vec<TagId> = self.tags.keys().cloned().collect();
        for tag_id in ids {
            if let Some(tag_state) = self.tags.get_mut(&tag_id) {
                // Event-driven tags (OnChange/PollingOnChange) may legitimately stay silent
                // for long periods; do not degrade quality by timeout in that mode.
                match tag_state.runtime.definition.update_mode {
                    TagUpdateMode::Polling { .. } => {}
                    TagUpdateMode::OnChange
                    | TagUpdateMode::OnMessage
                    | TagUpdateMode::PollingOnChange { .. } => {
                        continue;
                    }
                }
                if tag_state.timeout_reported {
                    continue;
                }
                if let Some(last_rx) = tag_state.last_received_at {
                    if now.duration_since(last_rx) >= timeout_window {
                        let value = tag_state
                            .last_published_value
                            .clone()
                            .unwrap_or_else(|| Self::default_value_for(&tag_state.runtime));
                        let quality = TagQuality::bad(QualityReason::Timeout);
                        self.live_state
                            .update_tag(tag_id.clone(), value.clone(), quality);
                        self.event_bus.publish(RuntimeEvent::TagChanged {
                            tag_id: tag_id.clone(),
                            value,
                            trigger_value: None,
                            quality,
                            timestamp: chrono::Utc::now(),
                        });
                        tag_state.current_quality = quality;
                        tag_state.timeout_reported = true;
                    }
                }
            }
        }
    }

    fn mark_tag_bad_communication(&mut self, tag_id: &TagId, now: Instant) {
        if let Some(tag_state) = self.tags.get_mut(tag_id) {
            let value = tag_state
                .last_published_value
                .clone()
                .unwrap_or_else(|| Self::default_value_for(&tag_state.runtime));
            let quality = TagQuality::bad(QualityReason::CommunicationFailure);
            self.live_state
                .update_tag(tag_id.clone(), value.clone(), quality);
            self.event_bus.publish(RuntimeEvent::TagChanged {
                tag_id: tag_id.clone(),
                value,
                trigger_value: None,
                quality,
                timestamp: chrono::Utc::now(),
            });
            tag_state.current_quality = quality;
            tag_state.last_published_at = Some(now);
        }
    }

    fn mark_all_tags_bad_communication(&mut self, now: Instant) {
        let ids: Vec<TagId> = self.tags.keys().cloned().collect();
        for tag_id in ids {
            self.mark_tag_bad_communication(&tag_id, now);
        }
    }

    fn publish_device_protocol_state_for_tag(
        &mut self,
        tag_id: &TagId,
        state: DeviceProtocolState,
        reason: Option<String>,
    ) {
        let Some(device_id) = self
            .tags
            .get(tag_id)
            .map(|t| t.runtime.definition.device_id.clone())
        else {
            return;
        };

        let changed = self
            .device_protocol_state
            .get(&device_id)
            .map(|prev| *prev != state)
            .unwrap_or(true);
        if !changed {
            return;
        }
        self.device_protocol_state.insert(device_id.clone(), state);
        self.event_bus
            .publish(RuntimeEvent::DeviceProtocolStateChanged {
                connection_id: self.connection.id.clone(),
                device_id,
                tag_id: Some(tag_id.clone()),
                state,
                reason,
                timestamp: chrono::Utc::now(),
            });
    }

    fn publish_all_devices_protocol_state(
        &mut self,
        state: DeviceProtocolState,
        reason: Option<String>,
    ) {
        let mut devices: Vec<DeviceId> = self
            .tags
            .values()
            .map(|t| t.runtime.definition.device_id.clone())
            .collect();
        devices.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        devices.dedup();
        for device_id in devices {
            let changed = self
                .device_protocol_state
                .get(&device_id)
                .map(|prev| *prev != state)
                .unwrap_or(true);
            if !changed {
                continue;
            }
            self.device_protocol_state.insert(device_id.clone(), state);
            self.event_bus
                .publish(RuntimeEvent::DeviceProtocolStateChanged {
                    connection_id: self.connection.id.clone(),
                    device_id,
                    tag_id: None,
                    state,
                    reason: reason.clone(),
                    timestamp: chrono::Utc::now(),
                });
        }
    }

    fn default_value_for(tag_state: &TagRuntime) -> TagValue {
        match tag_state.definition.value_type {
            TagValueType::Float => TagValue::Float(0.0),
            TagValueType::Integer => TagValue::Integer(0),
            TagValueType::Boolean => TagValue::Boolean(false),
            TagValueType::String => TagValue::String(String::new()),
        }
    }

    async fn handle_write_command(
        &mut self,
        tag_id: TagId,
        value: TagValue,
        command_id: Option<String>,
    ) -> Result<WriteExecutionResult, domain::DomainError> {
        let now = Instant::now();
        if !self.tags.contains_key(&tag_id) {
            return Err(domain::DomainError::NotFound(format!(
                "tag not attached to connection: {}",
                tag_id
            )));
        }

        self.prune_seen_write_commands(now);
        if let Some(cmd) = command_id.as_ref() {
            let key = (tag_id.clone(), cmd.clone());
            if self.seen_write_commands.contains_key(&key) {
                return Ok(WriteExecutionResult::Deduplicated);
            }
        }
        if self.is_write_rate_limited(&tag_id, now) {
            let min_ms = self.write_rate_limit_window().as_millis();
            return Err(domain::DomainError::TagError(format!(
                "write rate limited for tag '{}' (min {} ms between writes)",
                tag_id, min_ms
            )));
        }
        if self.is_write_circuit_open(now) {
            return Err(domain::DomainError::DriverError(
                "write circuit breaker open".to_string(),
            ));
        }

        if !self.driver.is_connected() {
            let connected = self.attempt_connect_if_due().await;
            if !connected {
                self.mark_tag_bad_communication(&tag_id, now);
                return Err(domain::DomainError::DriverError(
                    "connection unavailable for write".to_string(),
                ));
            }
        }

        match self.driver.write(tag_id.clone(), value.clone()).await {
            Ok(_) => {
                let tag_state = self
                    .tags
                    .get_mut(&tag_id)
                    .expect("tag existence validated above");
                let state = tag_state.runtime.process_raw_value(value.clone());
                self.live_state
                    .update_tag(tag_id.clone(), state.value.clone(), state.quality);
                self.event_bus.publish(RuntimeEvent::TagChanged {
                    tag_id: tag_id.clone(),
                    value: state.value.clone(),
                    trigger_value: state.trigger_value.clone(),
                    quality: state.quality,
                    timestamp: state.timestamp,
                });
                tag_state.last_published_at = Some(now);
                tag_state.last_published_value = Some(state.value);
                tag_state.current_quality = TagQuality::good();
                tag_state.timeout_reported = false;

                if let Some(cmd) = command_id {
                    self.seen_write_commands
                        .insert((tag_id.clone(), cmd), now);
                }
                self.last_write_applied_at.insert(tag_id, now);
                self.write_fail_streak = 0;
                self.write_circuit_open_until = None;
                Ok(WriteExecutionResult::Applied)
            }
            Err(e) => {
                self.record_write_failure(now);
                self.mark_tag_bad_communication(&tag_id, now);
                Err(e)
            }
        }
    }

    fn command_dedup_window(&self) -> Duration {
        let ms = self
            .connection
            .config
            .transport
            .get("command_dedup_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(DEFAULT_COMMAND_DEDUP_MS);
        Duration::from_millis(ms.max(1))
    }

    fn write_rate_limit_window(&self) -> Duration {
        let ms = self
            .connection
            .config
            .transport
            .get("write_rate_limit_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        Duration::from_millis(ms)
    }

    fn is_write_rate_limited(&self, tag_id: &TagId, now: Instant) -> bool {
        let window = self.write_rate_limit_window();
        if window.is_zero() {
            return false;
        }
        if let Some(last) = self.last_write_applied_at.get(tag_id) {
            return now.duration_since(*last) < window;
        }
        false
    }

    fn write_circuit_fail_threshold(&self) -> u32 {
        self.connection
            .config
            .transport
            .get("write_circuit_fail_threshold")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32)
            .unwrap_or(DEFAULT_WRITE_CIRCUIT_FAIL_THRESHOLD as u32)
    }

    fn write_circuit_cooldown(&self) -> Duration {
        let ms = self
            .connection
            .config
            .transport
            .get("write_circuit_cooldown_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(DEFAULT_WRITE_CIRCUIT_COOLDOWN_MS);
        Duration::from_millis(ms)
    }

    fn is_write_circuit_open(&mut self, now: Instant) -> bool {
        if let Some(until) = self.write_circuit_open_until {
            if now < until {
                return true;
            }
            self.write_circuit_open_until = None;
            self.write_fail_streak = 0;
        }
        false
    }

    fn record_write_failure(&mut self, now: Instant) {
        let threshold = self.write_circuit_fail_threshold();
        let cooldown = self.write_circuit_cooldown();
        if threshold == 0 || cooldown.is_zero() {
            return;
        }
        self.write_fail_streak = self.write_fail_streak.saturating_add(1);
        if self.write_fail_streak >= threshold {
            self.write_circuit_open_until = Some(now + cooldown);
        }
    }

    fn prune_seen_write_commands(&mut self, now: Instant) {
        let window = self.command_dedup_window();
        self.seen_write_commands
            .retain(|_, ts| now.duration_since(*ts) <= window);
    }

    fn publish_write_audit(
        &self,
        tag_id: TagId,
        command_id: Option<String>,
        value: TagValue,
        outcome: WriteCommandOutcome,
        reason: Option<String>,
    ) {
        let timestamp = chrono::Utc::now();
        self.event_bus.publish(RuntimeEvent::TagWriteCommandHandled {
            connection_id: Some(self.connection.id.clone()),
            tag_id: tag_id.clone(),
            command_id: command_id.clone(),
            value: value.clone(),
            outcome,
            reason: reason.clone(),
            timestamp,
        });
        if let Err(e) = self.write_audit_repository.append(WriteAuditRecord {
            connection_id: Some(self.connection.id.clone()),
            tag_id,
            command_id,
            value,
            outcome,
            reason,
            timestamp,
        }) {
            warn!("failed to persist write audit record: {}", e);
        }
    }
}

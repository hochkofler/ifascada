use crate::messages::{
    ActionAuditMessage, ActionResultMessage, AlertRuntimeMessage, ConfigApplyResultMessage,
    ConnectionStateMessage, ControlResetResultMessage, DeviceConnectionStateMessage,
    HealthRuntimeMessage, TagTelemetryMessage, WriteAckMessage, WriteAuditMessage,
};
use crate::persistence::CentralPersistence;
use anyhow::Result;
use async_trait::async_trait;
use domain::{
    DeviceConnectionStatus, DeviceStatusInput, TagConnectionStatus, TagStatusInput,
    classify_connection_transition, evaluate_device_connection_status,
    evaluate_tag_connection_status, normalize_connection_state,
};
use redis::Commands;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio_postgres::{Client, NoTls};
use tracing::{debug, error, warn};

pub struct PostgresCentralPersistence {
    client: Client,
    device_trackers: Mutex<HashMap<i64, DeviceTransitionTracker>>,
    device_metrics: DeviceStateMetrics,
    device_rt: Option<RedisDevicePublisher>,
}

#[derive(Debug, Clone, Copy)]
struct HistorianPolicy {
    deadband: f64,
    max_interval_secs: i64,
}

#[derive(Debug, Clone)]
struct LastSample {
    ts: chrono::DateTime<chrono::Utc>,
    quality_status: String,
    value_json: serde_json::Value,
}

#[derive(Debug, Clone)]
struct DeviceContextRow {
    site_code: String,
    edge_code: String,
    device_code: String,
    connection_pk: Option<i64>,
    connection_code: Option<String>,
    edge_state: Option<String>,
    edge_age_secs: Option<i64>,
    edge_stale_after_secs: i64,
    connection_state: Option<String>,
    device_status_mode: String,
    device_on_demand_stale_after_secs: Option<i64>,
}

#[derive(Debug, Clone, Copy, Default)]
struct DeviceTagCounts {
    connected: i64,
    stale: i64,
    disconnected: i64,
    total: i64,
}

#[derive(Debug, Clone)]
struct DeviceTransitionTracker {
    pending_candidate: Option<String>,
    pending_count: u32,
}

#[derive(Debug, Clone)]
struct DeviceCurrentStateRow {
    state: String,
    last_change_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone)]
struct DeviceProtocolSnapshot {
    state: String,
    age_secs: Option<i64>,
}

#[derive(Debug, Default)]
struct DeviceStateMetrics {
    recompute_total: AtomicU64,
    nochange_total: AtomicU64,
    debounce_dropped_total: AtomicU64,
    transition_total: AtomicU64,
}

struct RedisDevicePublisher {
    conn: Mutex<redis::Connection>,
    event_channel: String,
    key_ttl_secs: u64,
}

impl RedisDevicePublisher {
    fn connect(url: &str, event_channel: String, key_ttl_secs: u64) -> Result<Self> {
        let client = redis::Client::open(url)?;
        let conn = client.get_connection()?;
        Ok(Self {
            conn: Mutex::new(conn),
            event_channel,
            key_ttl_secs,
        })
    }

    fn publish_snapshot(
        &self,
        site: &str,
        edge: &str,
        device: &str,
        payload: &serde_json::Value,
    ) -> Result<()> {
        let key = format!("scada:device:{}:{}:{}:status", site, edge, device);
        let encoded = serde_json::to_string(payload)?;
        let mut conn = self.conn.lock().expect("redis lock");
        let _: () = conn.set_ex(key, encoded.clone(), self.key_ttl_secs)?;
        let evt = json!({
            "event_type": "device_status",
            "site": site,
            "agent": edge,
            "payload": payload,
            "published_at": chrono::Utc::now(),
        });
        let _: i64 = conn.publish(&self.event_channel, serde_json::to_string(&evt)?)?;
        Ok(())
    }
}

impl PostgresCentralPersistence {
    fn default_edge_stale_after_secs() -> i64 {
        std::env::var("CENTRAL_EDGE_STALE_AFTER_SECS_DEFAULT")
            .ok()
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(45)
            .max(1)
    }

    pub async fn connect(dsn: &str) -> Result<Self> {
        let (client, connection) = tokio_postgres::connect(dsn, NoTls).await?;
        tokio::spawn(async move {
            if let Err(e) = connection.await {
                error!("postgres connection task error: {}", e);
            }
        });
        let redis_enabled = std::env::var("CENTRAL_REDIS_ENABLED")
            .unwrap_or_else(|_| "false".to_string())
            .eq_ignore_ascii_case("true");
        let device_rt = if redis_enabled {
            let redis_url = std::env::var("CENTRAL_REDIS_URL")
                .unwrap_or_else(|_| "redis://127.0.0.1:6379/".to_string());
            let event_channel = std::env::var("CENTRAL_REDIS_EVENT_CHANNEL")
                .unwrap_or_else(|_| "scada:rt:events".to_string());
            let key_ttl_secs = std::env::var("CENTRAL_REDIS_KEY_TTL_SECS")
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(120);
            match RedisDevicePublisher::connect(&redis_url, event_channel, key_ttl_secs) {
                Ok(v) => Some(v),
                Err(e) => {
                    warn!("redis device publisher disabled: {}", e);
                    None
                }
            }
        } else {
            None
        };
        Ok(Self {
            client,
            device_trackers: Mutex::new(HashMap::new()),
            device_metrics: DeviceStateMetrics::default(),
            device_rt,
        })
    }
}

impl PostgresCentralPersistence {
    async fn refresh_devices_for_connection(
        &self,
        connection_pk: i64,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<()> {
        let rows = self
            .client
            .query(
                "SELECT id
                 FROM devices
                 WHERE connection_id = $1",
                &[&connection_pk],
            )
            .await?;
        for row in rows {
            self.refresh_device_state(row.get::<_, i64>(0), now).await?;
        }
        Ok(())
    }

    async fn refresh_devices_for_edge(
        &self,
        edge_id: i64,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<()> {
        let rows = self
            .client
            .query(
                "SELECT id
                 FROM devices
                 WHERE edge_id = $1",
                &[&edge_id],
            )
            .await?;
        for row in rows {
            self.refresh_device_state(row.get::<_, i64>(0), now).await?;
        }
        Ok(())
    }

    async fn refresh_device_state(
        &self,
        device_id: i64,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<()> {
        self.device_metrics
            .recompute_total
            .fetch_add(1, Ordering::Relaxed);

        let Some(ctx) = load_device_context(
            &self.client,
            device_id,
            now,
            Self::default_edge_stale_after_secs(),
        )
        .await? else {
            return Ok(());
        };

        let device_protocol = load_last_device_connection_event_snapshot(
            &self.client,
            &ctx.site_code,
            &ctx.edge_code,
            &ctx.device_code,
            now,
        )
        .await?;
        let (counts, candidate) = if ctx.device_status_mode.eq_ignore_ascii_case("on_demand") {
            (
                DeviceTagCounts::default(),
                evaluate_on_demand_device_status(
                    device_protocol.as_ref(),
                    ctx.device_on_demand_stale_after_secs,
                ),
            )
        } else {
            let counts = load_device_tag_counts(
                &self.client,
                device_id,
                now,
                ctx.edge_state.as_deref(),
                ctx.edge_age_secs,
                ctx.edge_stale_after_secs,
                ctx.connection_state.as_deref(),
            )
            .await?;
            let candidate = evaluate_device_connection_status(DeviceStatusInput {
                edge_state: ctx.edge_state.as_deref(),
                edge_age_secs: ctx.edge_age_secs,
                edge_stale_after_secs: ctx.edge_stale_after_secs,
                connection_state: ctx.connection_state.as_deref(),
                device_protocol_state: device_protocol.as_ref().map(|v| v.state.as_str()),
                tags_connected: counts.connected,
                tags_stale: counts.stale,
                tags_total: counts.total,
            });
            (counts, candidate)
        };
        let candidate_state = candidate.as_str();
        let prev = load_current_device_state(&self.client, device_id).await?;
        let prev_state = prev.as_ref().map(|v| v.state.as_str());
        let prev_last_change_at = prev.as_ref().map(|v| v.last_change_at);

        let apply = self.should_apply_device_transition(
            device_id,
            prev_state,
            prev_last_change_at,
            candidate_state,
            now,
        );
        if !apply {
            self.device_metrics
                .nochange_total
                .fetch_add(1, Ordering::Relaxed);
            return Ok(());
        }

        let reason = device_state_reason(candidate, &ctx, counts, device_protocol.as_ref());
        let severity = device_status_severity(candidate);
        let payload = json!({
            "state": candidate_state,
            "reason": reason,
            "status_mode": ctx.device_status_mode,
            "device_code": ctx.device_code,
            "edge_code": ctx.edge_code,
            "connection_id": ctx.connection_code,
            "tags_connected": counts.connected,
            "tags_stale": counts.stale,
            "tags_disconnected": counts.disconnected,
            "edge_state": ctx.edge_state,
            "connection_state": ctx.connection_state,
            "device_protocol_state": device_protocol.as_ref().map(|v| v.state.clone()),
            "device_protocol_age_secs": device_protocol.as_ref().and_then(|v| v.age_secs),
            "evaluated_at": now,
        });

        self.client
            .execute(
                "INSERT INTO device_current_state
                 (device_id, state, severity, reason, connection_id, tags_connected, tags_stale, tags_disconnected, last_change_at, last_seen_at, payload_json, updated_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $9, $10, NOW())
                 ON CONFLICT (device_id) DO UPDATE
                 SET state = EXCLUDED.state,
                     severity = EXCLUDED.severity,
                     reason = EXCLUDED.reason,
                     connection_id = EXCLUDED.connection_id,
                     tags_connected = EXCLUDED.tags_connected,
                     tags_stale = EXCLUDED.tags_stale,
                     tags_disconnected = EXCLUDED.tags_disconnected,
                     last_change_at = EXCLUDED.last_change_at,
                     last_seen_at = EXCLUDED.last_seen_at,
                     payload_json = EXCLUDED.payload_json,
                     updated_at = NOW()",
                &[
                    &device_id,
                    &candidate_state,
                    &severity,
                    &reason,
                    &ctx.connection_pk,
                    &(counts.connected as i32),
                    &(counts.stale as i32),
                    &(counts.disconnected as i32),
                    &now,
                    &payload,
                ],
            )
            .await?;

        let event_type = device_status_event_type(candidate);
        insert_operational_event(
            &self.client,
            OperationalEvent {
                ts: now,
                severity,
                event_type,
                site_code: &ctx.site_code,
                edge_code: Some(&ctx.edge_code),
                connection_id: ctx.connection_code.as_deref(),
                device_code: Some(&ctx.device_code),
                tag_code: None,
                config_hash: None,
                op_id: None,
                message: reason,
                payload_json: payload.clone(),
            },
        )
        .await?;

        self.device_metrics
            .transition_total
            .fetch_add(1, Ordering::Relaxed);

        if let Some(rt) = &self.device_rt {
            if let Err(e) = rt.publish_snapshot(
                &ctx.site_code,
                &ctx.edge_code,
                &ctx.device_code,
                &payload,
            ) {
                warn!("redis device_status publish failed: {}", e);
            }
        }

        debug!(
            "device_status transition: site={} edge={} device={} state={} (c={} s={} d={})",
            ctx.site_code,
            ctx.edge_code,
            ctx.device_code,
            candidate_state,
            counts.connected,
            counts.stale,
            counts.disconnected
        );

        Ok(())
    }

    fn should_apply_device_transition(
        &self,
        device_id: i64,
        current_state: Option<&str>,
        current_last_change_at: Option<chrono::DateTime<chrono::Utc>>,
        candidate_state: &str,
        now: chrono::DateTime<chrono::Utc>,
    ) -> bool {
        if current_state == Some(candidate_state) {
            let mut trackers = self.device_trackers.lock().expect("device trackers lock");
            if let Some(t) = trackers.get_mut(&device_id) {
                t.pending_candidate = None;
                t.pending_count = 0;
            }
            return false;
        }

        if current_state.is_none() {
            let mut trackers = self.device_trackers.lock().expect("device trackers lock");
            trackers.insert(
                device_id,
                DeviceTransitionTracker {
                pending_candidate: None,
                pending_count: 0,
            },
            );
            return true;
        }

        let mut trackers = self.device_trackers.lock().expect("device trackers lock");
        let tracker = trackers
            .entry(device_id)
            .or_insert_with(|| DeviceTransitionTracker {
                pending_candidate: None,
                pending_count: 0,
            });

        if tracker.pending_candidate.as_deref() == Some(candidate_state) {
            tracker.pending_count = tracker.pending_count.saturating_add(1);
        } else {
            tracker.pending_candidate = Some(candidate_state.to_string());
            tracker.pending_count = 1;
        }

        let threshold = match (current_state, candidate_state) {
            (Some("stale"), "disconnected") => 2,
            (Some("disconnected"), "connected") => 2,
            _ => 1,
        };
        if tracker.pending_count < threshold {
            return false;
        }

        let is_recovery = matches!((current_state, candidate_state), (Some("disconnected"), "connected"));
        if let Some(last_change_at) = current_last_change_at {
            let elapsed = now.signed_duration_since(last_change_at).num_seconds().max(0);
            if elapsed < 5 && !is_recovery {
                self.device_metrics
                    .debounce_dropped_total
                    .fetch_add(1, Ordering::Relaxed);
                return false;
            }
        }

        tracker.pending_candidate = None;
        tracker.pending_count = 0;
        true
    }
}

#[derive(Debug, Clone)]
struct OperationalEvent<'a> {
    ts: chrono::DateTime<chrono::Utc>,
    severity: &'a str,
    event_type: &'a str,
    site_code: &'a str,
    edge_code: Option<&'a str>,
    connection_id: Option<&'a str>,
    device_code: Option<&'a str>,
    tag_code: Option<&'a str>,
    config_hash: Option<&'a str>,
    op_id: Option<&'a str>,
    message: &'a str,
    payload_json: serde_json::Value,
}

async fn insert_operational_event(client: &Client, evt: OperationalEvent<'_>) -> Result<()> {
    client
        .execute(
            "INSERT INTO operational_events
             (ts, severity, event_type, site_code, edge_code, connection_id, device_code, tag_code, config_hash, op_id, message, payload_json)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)",
            &[
                &evt.ts,
                &evt.severity,
                &evt.event_type,
                &evt.site_code,
                &evt.edge_code,
                &evt.connection_id,
                &evt.device_code,
                &evt.tag_code,
                &evt.config_hash,
                &evt.op_id,
                &evt.message,
                &evt.payload_json,
            ],
        )
        .await?;
    Ok(())
}

#[async_trait]
impl CentralPersistence for PostgresCentralPersistence {
    async fn insert_telemetry(
        &self,
        site: &str,
        agent: &str,
        msg: &TagTelemetryMessage,
    ) -> Result<()> {
        let received_at = chrono::Utc::now();
        let quality_status = extract_quality_status(&msg.quality);
        let connectivity_failure = is_connectivity_failure_quality(&msg.quality);
        let payload = json!({
            "schema_version": msg.schema_version,
            "source": msg.source,
            "tag_id": msg.tag_id,
            "value": msg.value,
            "quality": msg.quality,
            "timestamp": msg.timestamp,
            "received_at": received_at,
        });
        if !connectivity_failure {
            self.client
                .execute(
                    "INSERT INTO telemetry_ingest_events (site_code, edge_code, tag_code, quality_status, value_json, payload_json, ts, received_at)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
                    &[
                        &site,
                        &agent,
                        &msg.tag_id,
                        &quality_status,
                        &msg.value,
                        &payload,
                        &msg.timestamp,
                        &received_at,
                    ],
                )
                .await?;
        }

        if let Some((site_id, edge_id, device_id, tag_id)) =
            resolve_tag_ref(&self.client, site, agent, msg.tag_id.as_str()).await?
        {
            let prev_quality_status = load_current_tag_quality_status(&self.client, tag_id).await?;
            if !connectivity_failure {
                let policy = load_historian_policy(&self.client, tag_id).await?;
                let prev = load_last_sample(&self.client, tag_id).await?;
                if should_persist_historian_sample(
                    prev.as_ref(),
                    &msg.timestamp,
                    &msg.value,
                    &quality_status,
                    policy,
                ) {
                    let (value_num, value_bool, value_text, value_json) =
                        split_value_channels(&msg.value);
                    self.client
                        .execute(
                            "INSERT INTO telemetry_samples (ts, site_id, edge_id, tag_id, quality_status, value_num, value_bool, value_text, value_json, source, received_at)
                             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
                            &[
                                &msg.timestamp,
                                &site_id,
                                &edge_id,
                                &tag_id,
                                &quality_status,
                                &value_num,
                                &value_bool,
                                &value_text,
                                &value_json,
                                &msg.source,
                                &received_at,
                            ],
                        )
                        .await?;
                }
            }

            self.client
                .execute(
                    "INSERT INTO tag_current_state (tag_id, ts, value_json, quality_json, source, updated_at)
                     VALUES ($1, $2, $3, $4, $5, NOW())
                     ON CONFLICT (tag_id) DO UPDATE
                     SET ts = CASE WHEN $6 THEN tag_current_state.ts ELSE EXCLUDED.ts END,
                         value_json = CASE WHEN $6 THEN tag_current_state.value_json ELSE EXCLUDED.value_json END,
                         quality_json = EXCLUDED.quality_json,
                         source = EXCLUDED.source,
                         updated_at = NOW()",
                    &[
                        &tag_id,
                        &msg.timestamp,
                        &msg.value,
                        &msg.quality,
                        &msg.source,
                        &connectivity_failure,
                    ],
                )
                .await?;

            if prev_quality_status.as_deref() != Some(quality_status.as_str()) {
                let is_good = is_good_status(&quality_status);
                let event_type = if is_good {
                    "tag.quality.recovered"
                } else {
                    "tag.quality.bad"
                };
                let severity = if is_good { "info" } else { "warn" };
                let msg_text = if is_good {
                    "Tag quality recovered to Good"
                } else {
                    "Tag quality changed to non-Good"
                };
                insert_operational_event(
                    &self.client,
                    OperationalEvent {
                        ts: msg.timestamp,
                        severity,
                        event_type,
                        site_code: site,
                        edge_code: Some(agent),
                        connection_id: None,
                        device_code: None,
                        tag_code: Some(&msg.tag_id),
                        config_hash: None,
                        op_id: None,
                        message: msg_text,
                        payload_json: json!({
                            "quality_status": quality_status,
                            "quality": msg.quality,
                            "source": msg.source,
                        }),
                    },
                )
                .await?;
            }

            self.client
                .execute(
                    "INSERT INTO edge_current_state (edge_id, status, last_seen_at, outbox_depth, outbox_oldest_secs, updated_at)
                     VALUES ($1, 'online', $2, 0, NULL, NOW())
                     ON CONFLICT (edge_id) DO UPDATE
                     SET status = 'online',
                         last_seen_at = EXCLUDED.last_seen_at,
                         updated_at = NOW()",
                    &[&edge_id, &msg.timestamp],
                )
                .await?;
            self.refresh_device_state(device_id, msg.timestamp).await?;
        } else {
            if !connectivity_failure {
                warn!(
                    "telemetry mapping not found for site='{}' edge='{}' tag='{}'; only raw ingest event persisted",
                    site, agent, msg.tag_id
                );
            }
        }
        Ok(())
    }

    async fn insert_health(&self, site: &str, agent: &str, msg: &HealthRuntimeMessage) -> Result<()> {
        let payload = serde_json::to_value(msg)?;
        let edge_id = resolve_edge_id(&self.client, site, agent).await?;
        let prev_status = if let Some(edge_id) = edge_id {
            load_current_edge_status(&self.client, edge_id).await?
        } else {
            None
        };
        self.client
            .execute(
                "INSERT INTO edge_health_events (edge_id, status, payload_json, ts)
                 VALUES ($1, $2, $3, $4)",
                &[&edge_id, &msg.status, &payload, &msg.timestamp],
            )
            .await?;

        if let Some(edge_id) = edge_id {
            self.client
                .execute(
                    "INSERT INTO edge_current_state (edge_id, status, last_seen_at, outbox_depth, outbox_oldest_secs, updated_at)
                     VALUES ($1, $2, $3, $4, $5, NOW())
                     ON CONFLICT (edge_id) DO UPDATE
                     SET status = EXCLUDED.status,
                         last_seen_at = EXCLUDED.last_seen_at,
                         outbox_depth = EXCLUDED.outbox_depth,
                         outbox_oldest_secs = EXCLUDED.outbox_oldest_secs,
                         updated_at = NOW()",
                    &[
                        &edge_id,
                        &msg.status,
                        &msg.timestamp,
                        &(msg.outbox_depth as i64),
                        &msg.outbox_oldest_age_secs.map(|v| v as i64),
                    ],
                )
                .await?;
        } else {
            warn!(
                "health mapping not found for site='{}' edge='{}'; event stored without edge_id",
                site, agent
            );
        }
        if prev_status.as_deref() != Some(msg.status.as_str()) {
            let (severity, event_type, message) = if msg.status.eq_ignore_ascii_case("ok")
                || msg.status.eq_ignore_ascii_case("online")
            {
                ("info", "edge.status.recovered", "Edge status recovered")
            } else {
                ("warn", "edge.status.changed", "Edge status changed to non-OK")
            };
            insert_operational_event(
                &self.client,
                OperationalEvent {
                    ts: msg.timestamp,
                    severity,
                    event_type,
                    site_code: site,
                    edge_code: Some(agent),
                    connection_id: None,
                    device_code: None,
                    tag_code: None,
                    config_hash: msg.config_hash.as_deref(),
                    op_id: None,
                    message,
                    payload_json: payload.clone(),
                },
            )
            .await?;
        }
        if let Some(edge_id) = edge_id {
            self.refresh_devices_for_edge(edge_id, msg.timestamp).await?;
        }
        Ok(())
    }

    async fn insert_alert(&self, site: &str, agent: &str, msg: &AlertRuntimeMessage) -> Result<()> {
        let payload = serde_json::to_value(msg)?;
        let edge_id = resolve_edge_id(&self.client, site, agent).await?;
        self.client
            .execute(
                "INSERT INTO runtime_alert_events (edge_id, alert_type, state, severity, payload_json, ts)
                 VALUES ($1, $2, $3, $4, $5, $6)",
                &[
                    &edge_id,
                    &msg.alert_type,
                    &msg.state,
                    &msg.severity,
                    &payload,
                    &msg.timestamp,
                ],
            )
            .await?;
        let severity = if msg.severity.eq_ignore_ascii_case("critical")
            || msg.severity.eq_ignore_ascii_case("error")
        {
            "error"
        } else if msg.severity.eq_ignore_ascii_case("warning")
            || msg.severity.eq_ignore_ascii_case("warn")
        {
            "warn"
        } else {
            "info"
        };
        let event_type = if msg.state.eq_ignore_ascii_case("raised") {
            "runtime.alert.raised"
        } else {
            "runtime.alert.cleared"
        };
        insert_operational_event(
            &self.client,
            OperationalEvent {
                ts: msg.timestamp,
                severity,
                event_type,
                site_code: site,
                edge_code: Some(agent),
                connection_id: None,
                device_code: None,
                tag_code: None,
                config_hash: None,
                op_id: None,
                message: &msg.message,
                payload_json: payload,
            },
        )
        .await?;
        Ok(())
    }

    async fn insert_write_ack(
        &self,
        site: &str,
        agent: &str,
        msg: &WriteAckMessage,
    ) -> Result<()> {
        let payload = serde_json::to_value(msg)?;
        let edge_id = resolve_edge_id(&self.client, site, agent).await?;
        let tag_id = if let Some(tag_code) = msg.tag_id.as_deref() {
            resolve_tag_id(&self.client, site, agent, tag_code).await?
        } else {
            None
        };
        self.client
            .execute(
                "INSERT INTO command_ack_events (command_id, edge_id, tag_id, success, reason, payload_json, ts)
                 VALUES ($1, $2, $3, $4, $5, $6, $7)",
                &[
                    &msg.command_id,
                    &edge_id,
                    &tag_id,
                    &msg.success,
                    &msg.reason,
                    &payload,
                    &msg.timestamp,
                ],
            )
            .await?;
        if !msg.success {
            insert_operational_event(
                &self.client,
                OperationalEvent {
                    ts: msg.timestamp,
                    severity: "warn",
                    event_type: "command.write.failed",
                    site_code: site,
                    edge_code: Some(agent),
                    connection_id: None,
                    device_code: None,
                    tag_code: msg.tag_id.as_deref(),
                    config_hash: None,
                    op_id: msg.command_id.as_deref(),
                    message: msg.reason.as_deref().unwrap_or("Write command rejected"),
                    payload_json: payload,
                },
            )
            .await?;
        }
        Ok(())
    }

    async fn insert_action_result(
        &self,
        site: &str,
        agent: &str,
        msg: &ActionResultMessage,
    ) -> Result<()> {
        let payload = serde_json::to_value(msg)?;
        insert_operational_event(
            &self.client,
            OperationalEvent {
                ts: msg.timestamp,
                severity: if msg.accepted { "info" } else { "warn" },
                event_type: if msg.accepted {
                    "action.command.accepted"
                } else {
                    "action.command.rejected"
                },
                site_code: site,
                edge_code: Some(agent),
                connection_id: None,
                device_code: None,
                tag_code: None,
                config_hash: None,
                op_id: msg.request_id.as_deref(),
                message: msg
                    .reason
                    .as_deref()
                    .unwrap_or("Edge action command result"),
                payload_json: payload,
            },
        )
        .await?;
        Ok(())
    }

    async fn insert_action_audit(
        &self,
        site: &str,
        agent: &str,
        msg: &ActionAuditMessage,
    ) -> Result<()> {
        let payload = serde_json::to_value(msg)?;
        let severity = if msg.outcome.eq_ignore_ascii_case("failed") {
            "warn"
        } else {
            "info"
        };
        insert_operational_event(
            &self.client,
            OperationalEvent {
                ts: msg.timestamp,
                severity,
                event_type: "action.audit",
                site_code: site,
                edge_code: Some(agent),
                connection_id: None,
                device_code: None,
                tag_code: None,
                config_hash: None,
                op_id: msg.request_id.as_deref(),
                message: msg.reason.as_deref().unwrap_or("Edge action audit"),
                payload_json: payload,
            },
        )
        .await?;
        Ok(())
    }

    async fn insert_write_audit(
        &self,
        site: &str,
        agent: &str,
        msg: &WriteAuditMessage,
    ) -> Result<()> {
        let edge_id = resolve_edge_id(&self.client, site, agent).await?;
        let tag_id = resolve_tag_id(&self.client, site, agent, msg.tag_id.as_str()).await?;
        self.client
            .execute(
                "INSERT INTO command_audit_events (command_id, edge_id, tag_id, outcome, reason, value_json, ts)
                 VALUES ($1, $2, $3, $4, $5, $6, $7)",
                &[
                    &msg.command_id,
                    &edge_id,
                    &tag_id,
                    &msg.outcome,
                    &msg.reason,
                    &msg.value,
                    &msg.timestamp,
                ],
            )
            .await?;
        Ok(())
    }

    async fn insert_config_apply_result(
        &self,
        site: &str,
        agent: &str,
        msg: &ConfigApplyResultMessage,
    ) -> Result<()> {
        let payload = serde_json::to_value(msg)?;
        insert_operational_event(
            &self.client,
            OperationalEvent {
                ts: msg.timestamp,
                severity: if msg.accepted { "info" } else { "warn" },
                event_type: if msg.accepted {
                    "config.apply.accepted"
                } else {
                    "config.apply.rejected"
                },
                site_code: site,
                edge_code: Some(agent),
                connection_id: None,
                device_code: None,
                tag_code: None,
                config_hash: msg.target_config_hash.as_deref(),
                op_id: msg.request_id.as_deref(),
                message: msg
                    .reason
                    .as_deref()
                    .unwrap_or("Config apply result from edge"),
                payload_json: payload,
            },
        )
        .await?;
        Ok(())
    }

    async fn insert_control_reset_result(
        &self,
        site: &str,
        agent: &str,
        msg: &ControlResetResultMessage,
    ) -> Result<()> {
        let payload = serde_json::to_value(msg)?;
        insert_operational_event(
            &self.client,
            OperationalEvent {
                ts: msg.timestamp,
                severity: if msg.accepted { "info" } else { "warn" },
                event_type: if msg.accepted {
                    "edge.reset.accepted"
                } else {
                    "edge.reset.rejected"
                },
                site_code: site,
                edge_code: Some(agent),
                connection_id: None,
                device_code: None,
                tag_code: None,
                config_hash: None,
                op_id: msg.request_id.as_deref(),
                message: msg.reason.as_deref().unwrap_or("Edge reset result"),
                payload_json: payload,
            },
        )
        .await?;
        Ok(())
    }

    async fn insert_connection_state(
        &self,
        site: &str,
        agent: &str,
        msg: &ConnectionStateMessage,
    ) -> Result<()> {
        let Some(edge_id) = resolve_edge_id(&self.client, site, agent).await? else {
            warn!(
                "connection state mapping not found for site='{}' edge='{}'; event skipped",
                site, agent
            );
            return Ok(());
        };
        let connection_pk =
            ensure_connection_row(&self.client, edge_id, msg.connection_id.as_str()).await?;
        let prev_state = load_current_connection_state(&self.client, connection_pk).await?;
        let state_norm = normalize_connection_state(msg.state.as_str());
        let transition = classify_connection_transition(prev_state.as_deref(), state_norm);

        self.client
            .execute(
                "INSERT INTO connection_current_state
                 (connection_id, state, severity, reason, payload_json, last_change_at, last_seen_at, updated_at)
                 VALUES ($1, $2, $3, NULL, $4, $5, $5, NOW())
                 ON CONFLICT (connection_id) DO UPDATE
                 SET state = EXCLUDED.state,
                     severity = EXCLUDED.severity,
                     payload_json = EXCLUDED.payload_json,
                     last_seen_at = EXCLUDED.last_seen_at,
                     last_change_at = CASE
                       WHEN connection_current_state.state IS DISTINCT FROM EXCLUDED.state
                       THEN EXCLUDED.last_change_at
                       ELSE connection_current_state.last_change_at
                     END,
                     updated_at = NOW()",
                &[
                    &connection_pk,
                    &state_norm,
                    &transition.severity,
                    &serde_json::to_value(msg)?,
                    &msg.timestamp,
                ],
            )
            .await?;

        if prev_state.as_deref() != Some(state_norm) {
            insert_operational_event(
                &self.client,
                OperationalEvent {
                    ts: msg.timestamp,
                    severity: transition.severity,
                    event_type: transition.event_type,
                    site_code: site,
                    edge_code: Some(agent),
                    connection_id: Some(&msg.connection_id),
                    device_code: None,
                    tag_code: None,
                    config_hash: None,
                    op_id: None,
                    message: transition.message,
                    payload_json: serde_json::to_value(msg)?,
                },
            )
            .await?;
        }
        self.refresh_devices_for_connection(connection_pk, msg.timestamp).await?;
        Ok(())
    }

    async fn insert_device_connection_state(
        &self,
        site: &str,
        agent: &str,
        msg: &DeviceConnectionStateMessage,
    ) -> Result<()> {
        let Some(edge_id) = resolve_edge_id(&self.client, site, agent).await? else {
            warn!(
                "device connection mapping not found for site='{}' edge='{}'; event skipped",
                site, agent
            );
            return Ok(());
        };
        let Some(device_pk) = resolve_device_id(&self.client, edge_id, msg.device_id.as_str()).await? else {
            warn!(
                "device connection mapping not found for site='{}' edge='{}' device='{}'; event skipped",
                site, agent, msg.device_id
            );
            return Ok(());
        };
        let state_norm = msg.state.trim().to_ascii_lowercase();
        let event_type = if state_norm == "connected" {
            "device.connection.connected"
        } else {
            "device.connection.error"
        };
        let severity = if state_norm == "connected" {
            "info"
        } else {
            "warn"
        };
        let prev = load_last_device_connection_event_snapshot(
            &self.client,
            site,
            agent,
            msg.device_id.as_str(),
            msg.timestamp,
        )
        .await?;
        if prev.as_ref().map(|v| v.state.as_str()) == Some(state_norm.as_str()) {
            return Ok(());
        }
        let message = msg
            .reason
            .as_deref()
            .unwrap_or(if state_norm == "connected" {
                "device_protocol_connected"
            } else {
                "device_protocol_error"
            });
        insert_operational_event(
            &self.client,
            OperationalEvent {
                ts: msg.timestamp,
                severity,
                event_type,
                site_code: site,
                edge_code: Some(agent),
                connection_id: Some(&msg.connection_id),
                device_code: Some(&msg.device_id),
                tag_code: msg.tag_id.as_deref(),
                config_hash: None,
                op_id: None,
                message,
                payload_json: serde_json::to_value(msg)?,
            },
        )
        .await?;
        self.refresh_device_state(device_pk, msg.timestamp).await?;
        Ok(())
    }
}

fn extract_quality_status(quality: &serde_json::Value) -> String {
    quality
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("Unknown")
        .to_string()
}

fn is_connectivity_failure_quality(quality: &serde_json::Value) -> bool {
    let status_bad = quality
        .get("status")
        .and_then(|v| v.as_str())
        .map(|s| s.eq_ignore_ascii_case("Bad"))
        .unwrap_or(false);
    if !status_bad {
        return false;
    }
    let reason = quality
        .get("reason")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    matches!(
        reason.as_str(),
        "communicationfailure" | "timeout" | "notconnected"
    )
}

fn split_value_channels(
    value: &serde_json::Value,
) -> (
    Option<f64>,
    Option<bool>,
    Option<String>,
    Option<serde_json::Value>,
) {
    match value {
        serde_json::Value::Null => (None, None, None, None),
        serde_json::Value::Bool(b) => (None, Some(*b), None, Some(value.clone())),
        serde_json::Value::Number(n) => (n.as_f64(), None, None, Some(value.clone())),
        serde_json::Value::String(s) => (None, None, Some(s.clone()), Some(value.clone())),
        serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
            (None, None, None, Some(value.clone()))
        }
    }
}

fn should_persist_historian_sample(
    prev: Option<&LastSample>,
    current_ts: &chrono::DateTime<chrono::Utc>,
    current_value: &serde_json::Value,
    current_quality: &str,
    policy: HistorianPolicy,
) -> bool {
    let Some(prev) = prev else {
        return true;
    };

    if prev.quality_status != current_quality {
        return true;
    }

    let elapsed_secs = current_ts
        .signed_duration_since(prev.ts)
        .num_seconds()
        .max(0);
    if elapsed_secs >= policy.max_interval_secs.max(1) {
        return true;
    }

    value_changed(prev, current_value, policy.deadband)
}

fn value_changed(prev: &LastSample, current_value: &serde_json::Value, deadband: f64) -> bool {
    let prev_num = as_f64_value(&prev.value_json);
    let cur_num = as_f64_value(current_value);
    if let (Some(p), Some(c)) = (prev_num, cur_num) {
        if deadband > 0.0 {
            return (c - p).abs() >= deadband;
        }
        return (c - p).abs() > 0.0;
    }
    prev.value_json != *current_value
}

fn as_f64_value(v: &serde_json::Value) -> Option<f64> {
    match v {
        serde_json::Value::Number(n) => n.as_f64(),
        serde_json::Value::String(s) => s.parse::<f64>().ok(),
        serde_json::Value::Object(map) => map.get("value").and_then(as_f64_value),
        _ => None,
    }
}

async fn load_historian_policy(client: &Client, tag_id: i64) -> Result<HistorianPolicy> {
    let row = client
        .query_opt(
            "SELECT metadata_json
             FROM tags
             WHERE id = $1",
            &[&tag_id],
        )
        .await?;

    let default = HistorianPolicy {
        deadband: 0.0,
        max_interval_secs: 300,
    };
    let Some(row) = row else {
        return Ok(default);
    };
    let meta: serde_json::Value = row.get(0);
    let deadband = meta
        .get("historian_deadband")
        .and_then(|v| v.as_f64())
        .unwrap_or(default.deadband);
    let max_interval_secs = meta
        .get("historian_max_interval_secs")
        .and_then(|v| v.as_i64())
        .unwrap_or(default.max_interval_secs);
    Ok(HistorianPolicy {
        deadband: deadband.max(0.0),
        max_interval_secs: max_interval_secs.max(1),
    })
}

async fn load_last_sample(client: &Client, tag_id: i64) -> Result<Option<LastSample>> {
    let row = client
        .query_opt(
            "SELECT ts, quality_status, value_json
             FROM telemetry_samples
             WHERE tag_id = $1
             ORDER BY ts DESC
             LIMIT 1",
            &[&tag_id],
        )
        .await?;
    Ok(row.map(|r| LastSample {
        ts: r.get(0),
        quality_status: r.get(1),
        value_json: r.get(2),
    }))
}

async fn load_current_edge_status(client: &Client, edge_id: i64) -> Result<Option<String>> {
    let row = client
        .query_opt(
            "SELECT status
             FROM edge_current_state
             WHERE edge_id = $1",
            &[&edge_id],
        )
        .await?;
    Ok(row.map(|r| r.get(0)))
}

async fn load_current_tag_quality_status(client: &Client, tag_id: i64) -> Result<Option<String>> {
    let row = client
        .query_opt(
            "SELECT quality_json->>'status'
             FROM tag_current_state
             WHERE tag_id = $1",
            &[&tag_id],
        )
        .await?;
    Ok(row.map(|r| r.get::<_, Option<String>>(0)).flatten())
}

async fn load_current_connection_state(client: &Client, connection_id: i64) -> Result<Option<String>> {
    let row = client
        .query_opt(
            "SELECT state
             FROM connection_current_state
             WHERE connection_id = $1",
            &[&connection_id],
        )
        .await?;
    Ok(row.map(|r| r.get::<_, String>(0)))
}

async fn load_current_device_state(
    client: &Client,
    device_id: i64,
) -> Result<Option<DeviceCurrentStateRow>> {
    let row = client
        .query_opt(
            "SELECT state, last_change_at
             FROM device_current_state
             WHERE device_id = $1",
            &[&device_id],
        )
        .await?;
    Ok(row.map(|r| DeviceCurrentStateRow {
        state: r.get(0),
        last_change_at: r.get(1),
    }))
}

async fn load_device_context(
    client: &Client,
    device_id: i64,
    now: chrono::DateTime<chrono::Utc>,
    edge_stale_after_secs_default: i64,
) -> Result<Option<DeviceContextRow>> {
    let row = client
        .query_opt(
            "SELECT
                s.code,
                e.edge_code,
                d.device_code,
                d.connection_id,
                cn.connection_code,
                ecs.status,
                CASE
                  WHEN ecs.last_seen_at IS NULL THEN NULL
                  ELSE GREATEST(0, EXTRACT(EPOCH FROM ($2 - ecs.last_seen_at))::bigint)
                END AS edge_age_secs,
                $3::bigint AS edge_stale_after_secs,
                ccs.state,
                COALESCE(d.metadata_json, '{}'::jsonb) AS device_metadata
             FROM devices d
             JOIN edges e ON e.id = d.edge_id
             JOIN sites s ON s.id = e.site_id
             LEFT JOIN edge_current_state ecs ON ecs.edge_id = e.id
             LEFT JOIN connections cn ON cn.id = d.connection_id
             LEFT JOIN connection_current_state ccs ON ccs.connection_id = d.connection_id
             WHERE d.id = $1
             LIMIT 1",
            &[&device_id, &now, &edge_stale_after_secs_default],
        )
        .await?;
    Ok(row.map(|r| {
        let metadata: serde_json::Value = r.get(9);
        let (device_status_mode, device_on_demand_stale_after_secs) =
            parse_device_status_policy(&metadata);
        DeviceContextRow {
        site_code: r.get(0),
        edge_code: r.get(1),
        device_code: r.get(2),
        connection_pk: r.get(3),
        connection_code: r.get(4),
        edge_state: r.get(5),
        edge_age_secs: r.get(6),
        edge_stale_after_secs: r.get::<_, i64>(7),
        connection_state: r.get(8),
            device_status_mode,
            device_on_demand_stale_after_secs,
        }
    }))
}

async fn load_device_tag_counts(
    client: &Client,
    device_id: i64,
    now: chrono::DateTime<chrono::Utc>,
    edge_state: Option<&str>,
    edge_age_secs: Option<i64>,
    edge_stale_after_secs: i64,
    connection_state: Option<&str>,
) -> Result<DeviceTagCounts> {
    let rows = client
        .query(
            "SELECT
                CASE
                  WHEN (t.metadata_json->>'expected_interval_ms') ~ '^[0-9]+$'
                  THEN (t.metadata_json->>'expected_interval_ms')::bigint
                  ELSE NULL
                END AS expected_interval_ms,
                CASE
                  WHEN tcs.ts IS NULL THEN NULL
                  ELSE GREATEST(0, EXTRACT(EPOCH FROM ($2 - tcs.ts))::bigint)
                END AS sample_age_secs
             FROM tags t
             LEFT JOIN tag_current_state tcs ON tcs.tag_id = t.id
             WHERE t.device_id = $1
               AND t.tag_code NOT LIKE '%_raw'",
            &[&device_id, &now],
        )
        .await?;

    let mut counts = DeviceTagCounts::default();
    for r in rows {
        let expected_interval_ms: Option<i64> = r.get(0);
        let sample_age_secs: i64 = r.get::<_, Option<i64>>(1).unwrap_or(1_000_000_000);
        let st = evaluate_tag_connection_status(TagStatusInput {
            edge_state,
            edge_age_secs,
            edge_stale_after_secs,
            connection_state,
            sample_age_secs,
            expected_interval_ms,
        });
        counts.total += 1;
        match st {
            TagConnectionStatus::Connected => counts.connected += 1,
            TagConnectionStatus::Stale => counts.stale += 1,
            TagConnectionStatus::Disconnected => counts.disconnected += 1,
        }
    }
    Ok(counts)
}

fn device_status_severity(st: DeviceConnectionStatus) -> &'static str {
    match st {
        DeviceConnectionStatus::Connected => "info",
        DeviceConnectionStatus::Stale => "warn",
        DeviceConnectionStatus::Disconnected => "error",
    }
}

fn device_status_event_type(st: DeviceConnectionStatus) -> &'static str {
    match st {
        DeviceConnectionStatus::Connected => "device.status.connected",
        DeviceConnectionStatus::Stale => "device.status.stale",
        DeviceConnectionStatus::Disconnected => "device.status.disconnected",
    }
}

fn device_state_reason(
    st: DeviceConnectionStatus,
    ctx: &DeviceContextRow,
    _counts: DeviceTagCounts,
    device_protocol: Option<&DeviceProtocolSnapshot>,
) -> &'static str {
    if ctx.device_status_mode.eq_ignore_ascii_case("on_demand") {
        if let Some(s) = device_protocol.map(|v| v.state.as_str()) {
            if s == "connected" {
                if let Some(limit) = ctx.device_on_demand_stale_after_secs {
                    if limit > 0 {
                        if let Some(age) = device_protocol.and_then(|v| v.age_secs) {
                            if age > limit {
                                return "on_demand_stale";
                            }
                        }
                    }
                }
                return "device_protocol_connected";
            }
            if s == "error" || s == "failed" || s == "disconnected" {
                return "device_protocol_error";
            }
            return "on_demand_unknown";
        }
        return "on_demand_unknown";
    }

    if !is_edge_online_for_device(ctx.edge_state.as_deref(), ctx.edge_age_secs, ctx.edge_stale_after_secs) {
        return "edge_offline_or_stale";
    }

    if let Some(conn_state) = ctx.connection_state.as_deref() {
        let c = normalize_connection_state(conn_state);
        if c == "failed" || c == "disconnected" {
            return "connection_disconnected";
        }
        if c == "connecting" || c == "unknown" {
            return "connection_connecting";
        }
    } else {
        return "connection_unknown";
    }

    if let Some(ps) = device_protocol.map(|v| v.state.as_str()) {
        if ps == "error" {
            return "device_protocol_error";
        }
        if ps == "connected" {
            return "device_protocol_connected";
        }
    }

    match st {
        DeviceConnectionStatus::Connected => "connection_connected",
        DeviceConnectionStatus::Stale => "connection_stale",
        DeviceConnectionStatus::Disconnected => "connection_disconnected",
    }
}

fn is_edge_online_for_device(
    edge_state: Option<&str>,
    edge_age_secs: Option<i64>,
    edge_stale_after_secs: i64,
) -> bool {
    let Some(state) = edge_state else {
        return false;
    };
    let ok_state = state.eq_ignore_ascii_case("ok") || state.eq_ignore_ascii_case("online");
    if !ok_state {
        return false;
    }
    let Some(age) = edge_age_secs else {
        return false;
    };
    age <= edge_stale_after_secs.max(1)
}

fn is_good_status(status: &str) -> bool {
    status.eq_ignore_ascii_case("good")
}

async fn resolve_edge_id(client: &Client, site: &str, agent: &str) -> Result<Option<i64>> {
    let row = client
        .query_opt(
            "SELECT e.id
             FROM edges e
             JOIN sites s ON s.id = e.site_id
             WHERE s.code = $1 AND e.edge_code = $2
             LIMIT 1",
            &[&site, &agent],
        )
        .await?;
    Ok(row.map(|r| r.get::<_, i64>(0)))
}

async fn ensure_connection_row(
    client: &Client,
    edge_id: i64,
    connection_code: &str,
) -> Result<i64> {
    let row = client
        .query_one(
            "INSERT INTO connections
             (edge_id, connection_code, name, driver_type, metadata_json, updated_at)
             VALUES ($1, $2, $2, 'Unknown', '{}'::jsonb, NOW())
             ON CONFLICT (edge_id, connection_code) DO UPDATE
             SET updated_at = NOW()
             RETURNING id",
            &[&edge_id, &connection_code],
        )
        .await?;
    Ok(row.get::<_, i64>(0))
}

async fn resolve_device_id(client: &Client, edge_id: i64, device_code: &str) -> Result<Option<i64>> {
    let row = client
        .query_opt(
            "SELECT id
             FROM devices
             WHERE edge_id = $1 AND device_code = $2
             LIMIT 1",
            &[&edge_id, &device_code],
        )
        .await?;
    Ok(row.map(|r| r.get::<_, i64>(0)))
}

fn parse_device_status_policy(metadata: &serde_json::Value) -> (String, Option<i64>) {
    let mode = metadata
        .get("status_policy")
        .and_then(|v| v.get("mode"))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .unwrap_or("derived")
        .to_ascii_lowercase();
    let stale_after_secs = metadata
        .get("status_policy")
        .and_then(|v| v.get("stale_after_secs"))
        .and_then(|v| v.as_i64())
        .filter(|v| *v > 0);
    (mode, stale_after_secs)
}

fn evaluate_on_demand_device_status(
    protocol: Option<&DeviceProtocolSnapshot>,
    stale_after_secs: Option<i64>,
) -> DeviceConnectionStatus {
    let Some(p) = protocol else {
        return DeviceConnectionStatus::Stale;
    };
    match p.state.as_str() {
        "connected" => {
            if let (Some(limit), Some(age)) = (stale_after_secs, p.age_secs) {
                if age > limit {
                    return DeviceConnectionStatus::Stale;
                }
            }
            DeviceConnectionStatus::Connected
        }
        "error" | "failed" | "disconnected" => DeviceConnectionStatus::Disconnected,
        _ => DeviceConnectionStatus::Stale,
    }
}

async fn load_last_device_connection_event_snapshot(
    client: &Client,
    site: &str,
    edge: &str,
    device: &str,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<Option<DeviceProtocolSnapshot>> {
    let row = client
        .query_opt(
            "SELECT
                payload_json->>'state' AS state,
                CASE
                  WHEN ts IS NULL THEN NULL
                  ELSE GREATEST(0, EXTRACT(EPOCH FROM ($4 - ts))::bigint)
                END AS age_secs
             FROM operational_events
             WHERE site_code = $1
               AND edge_code = $2
               AND device_code = $3
               AND event_type IN ('device.connection.connected', 'device.connection.error', 'device.connection.disconnected')
             ORDER BY ts DESC
             LIMIT 1",
            &[&site, &edge, &device, &now],
        )
        .await?;
    Ok(row.and_then(|r| {
        let state = r
            .get::<_, Option<String>>(0)
            .map(|v| v.trim().to_ascii_lowercase())
            .filter(|v| !v.is_empty())?;
        Some(DeviceProtocolSnapshot {
            state,
            age_secs: r.get(1),
        })
    }))
}

async fn resolve_tag_id(
    client: &Client,
    site: &str,
    agent: &str,
    tag_code: &str,
) -> Result<Option<i64>> {
    Ok(resolve_tag_ref(client, site, agent, tag_code)
        .await?
        .map(|(_, _, _, tag_id)| tag_id))
}

async fn resolve_tag_ref(
    client: &Client,
    site: &str,
    agent: &str,
    tag_code: &str,
) -> Result<Option<(i64, i64, i64, i64)>> {
    let row = client
        .query_opt(
            "SELECT s.id, e.id, d.id, t.id
             FROM sites s
             JOIN edges e ON e.site_id = s.id
             JOIN devices d ON d.edge_id = e.id
             JOIN tags t ON t.device_id = d.id
             WHERE s.code = $1
               AND e.edge_code = $2
               AND (
                    t.tag_code = $3
                    OR t.tag_code_canonical = $3
                    OR t.aliases_json ? $3
               )
             ORDER BY
               CASE
                 WHEN t.tag_code = $3 THEN 0
                 WHEN t.tag_code_canonical = $3 THEN 1
                 ELSE 2
               END
             LIMIT 1",
            &[&site, &agent, &tag_code],
        )
        .await?;

    Ok(row.map(|r| {
        (
            r.get::<_, i64>(0),
            r.get::<_, i64>(1),
            r.get::<_, i64>(2),
            r.get::<_, i64>(3),
        )
    }))
}

#[cfg(test)]
mod tests {
    use super::{
        as_f64_value, is_connectivity_failure_quality, should_persist_historian_sample,
        split_value_channels, HistorianPolicy, LastSample,
    };

    #[test]
    fn split_value_channels_numeric() {
        let v = serde_json::json!(12.5);
        let (num, b, s, j) = split_value_channels(&v);
        assert_eq!(num, Some(12.5));
        assert_eq!(b, None);
        assert_eq!(s, None);
        assert_eq!(j, Some(v));
    }

    #[test]
    fn split_value_channels_bool() {
        let v = serde_json::json!(true);
        let (num, b, s, j) = split_value_channels(&v);
        assert_eq!(num, None);
        assert_eq!(b, Some(true));
        assert_eq!(s, None);
        assert_eq!(j, Some(v));
    }

    #[test]
    fn as_f64_value_extracts_compound_value() {
        let v = serde_json::json!({"value": 12.5, "unit":"g"});
        assert_eq!(as_f64_value(&v), Some(12.5));
    }

    #[test]
    fn persist_historian_when_no_previous() {
        let now = chrono::Utc::now();
        let ok = should_persist_historian_sample(
            None,
            &now,
            &serde_json::json!(1.0),
            "Good",
            HistorianPolicy {
                deadband: 0.5,
                max_interval_secs: 300,
            },
        );
        assert!(ok);
    }

    #[test]
    fn skip_historian_when_value_unchanged_before_interval() {
        let now = chrono::Utc::now();
        let prev = LastSample {
            ts: now - chrono::Duration::seconds(10),
            quality_status: "Good".to_string(),
            value_json: serde_json::json!(10.0),
        };
        let ok = should_persist_historian_sample(
            Some(&prev),
            &now,
            &serde_json::json!(10.0),
            "Good",
            HistorianPolicy {
                deadband: 0.1,
                max_interval_secs: 300,
            },
        );
        assert!(!ok);
    }

    #[test]
    fn persist_historian_when_quality_changes() {
        let now = chrono::Utc::now();
        let prev = LastSample {
            ts: now - chrono::Duration::seconds(10),
            quality_status: "Good".to_string(),
            value_json: serde_json::json!(10.0),
        };
        let ok = should_persist_historian_sample(
            Some(&prev),
            &now,
            &serde_json::json!(10.0),
            "Bad",
            HistorianPolicy {
                deadband: 0.1,
                max_interval_secs: 300,
            },
        );
        assert!(ok);
    }

    #[test]
    fn persist_historian_when_deadband_crossed() {
        let now = chrono::Utc::now();
        let prev = LastSample {
            ts: now - chrono::Duration::seconds(10),
            quality_status: "Good".to_string(),
            value_json: serde_json::json!(10.0),
        };
        let ok = should_persist_historian_sample(
            Some(&prev),
            &now,
            &serde_json::json!(10.6),
            "Good",
            HistorianPolicy {
                deadband: 0.5,
                max_interval_secs: 300,
            },
        );
        assert!(ok);
    }

    #[test]
    fn connectivity_failure_quality_detected() {
        assert!(is_connectivity_failure_quality(
            &serde_json::json!({"status":"Bad","reason":"CommunicationFailure"})
        ));
        assert!(is_connectivity_failure_quality(
            &serde_json::json!({"status":"Bad","reason":"Timeout"})
        ));
        assert!(is_connectivity_failure_quality(
            &serde_json::json!({"status":"Bad","reason":"NotConnected"})
        ));
        assert!(!is_connectivity_failure_quality(
            &serde_json::json!({"status":"Good","reason":null})
        ));
        assert!(!is_connectivity_failure_quality(
            &serde_json::json!({"status":"Bad","reason":"ValidationFailed"})
        ));
    }

}

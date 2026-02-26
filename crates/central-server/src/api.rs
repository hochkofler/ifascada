use anyhow::Result;
use axum::extract::{Path, Query, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::{get, post};
use axum::{Json, Router};
use hmac::{Hmac, Mac};
use domain::{TagStatusInput, evaluate_tag_connection_status};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::convert::Infallible;
use std::fs;
use std::sync::Arc;
use tokio_postgres::{Client, NoTls};
use rumqttc::{AsyncClient, QoS};
use tokio::time::Duration;
use tower_http::cors::{Any, CorsLayer};
use tracing::{error, info};

#[derive(Clone)]
pub struct ApiState {
    pub client: Arc<Client>,
    pub edge_cfg: EdgeConfigSettings,
    pub mqtt_cmd: Option<Arc<AsyncClient>>,
}

#[derive(Clone)]
pub struct EdgeConfigSettings {
    pub enroll_token: String,
    pub signing_secret: String,
    pub signing_key_id: String,
    pub runtime_config_path: String,
}

#[derive(Debug, Serialize)]
struct LiveHealthResponse {
    status: &'static str,
}

#[derive(Debug, Serialize)]
struct EdgeCurrentDto {
    site_code: String,
    line_code: Option<String>,
    area_code: Option<String>,
    cell_code: Option<String>,
    edge_code: String,
    status: String,
    last_seen_at: chrono::DateTime<chrono::Utc>,
    outbox_depth: i64,
    outbox_oldest_secs: Option<i64>,
    action_metrics: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct ConnectionCurrentDto {
    site_code: String,
    line_code: Option<String>,
    area_code: Option<String>,
    cell_code: Option<String>,
    edge_code: String,
    connection_id: String,
    state: String,
    severity: String,
    last_change_at: chrono::DateTime<chrono::Utc>,
    message: String,
}

#[derive(Debug, Serialize)]
struct DeviceCurrentDto {
    site_code: String,
    line_code: Option<String>,
    area_code: Option<String>,
    cell_code: Option<String>,
    edge_code: String,
    device_code: String,
    connection_id: Option<String>,
    state: String,
    severity: String,
    reason: Option<String>,
    tags_connected: i64,
    tags_stale: i64,
    tags_disconnected: i64,
    last_change_at: chrono::DateTime<chrono::Utc>,
    last_seen_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
struct TagCurrentDto {
    tag_code: String,
    device_code: String,
    site_code: String,
    line_code: Option<String>,
    area_code: Option<String>,
    cell_code: Option<String>,
    edge_code: String,
    ts: chrono::DateTime<chrono::Utc>,
    value: serde_json::Value,
    quality: serde_json::Value,
    source: String,
    metadata_json: serde_json::Value,
    expected_interval_ms: Option<i64>,
    tag_status: String,
}

#[derive(Debug, Serialize)]
struct TagHistoryDto {
    ts: chrono::DateTime<chrono::Utc>,
    site_code: String,
    edge_code: String,
    tag_code: String,
    value: serde_json::Value,
    quality_status: String,
}

#[derive(Debug, Serialize)]
struct OperationalEventDto {
    id: i64,
    ts: chrono::DateTime<chrono::Utc>,
    severity: String,
    event_type: String,
    site_code: String,
    edge_code: Option<String>,
    connection_id: Option<String>,
    device_code: Option<String>,
    tag_code: Option<String>,
    config_hash: Option<String>,
    op_id: Option<String>,
    message: String,
    payload_json: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct RtEventDto {
    event_type: &'static str,
    site: String,
    agent: String,
    payload: serde_json::Value,
    published_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
struct ListQuery {
    limit: Option<i64>,
    offset: Option<i64>,
    site: Option<String>,
    line: Option<String>,
    area: Option<String>,
    cell: Option<String>,
    edge: Option<String>,
}

#[derive(Debug, Serialize)]
struct ContextOptionDto {
    code: String,
    name: String,
}

#[derive(Debug, Deserialize)]
struct ContextQuery {
    site: Option<String>,
    line: Option<String>,
    area: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpsQuery {
    limit: Option<i64>,
    site: Option<String>,
    edge: Option<String>,
    device: Option<String>,
    tag: Option<String>,
    severity: Option<String>,
    event_type: Option<String>,
    q: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
struct OpsStreamQuery {
    site: Option<String>,
    edge: Option<String>,
    device: Option<String>,
    tag: Option<String>,
    severity: Option<String>,
    event_type: Option<String>,
    q: Option<String>,
    replay: Option<bool>,
}

#[derive(Debug, Deserialize, Clone)]
struct StreamQuery {
    site: Option<String>,
    line: Option<String>,
    area: Option<String>,
    cell: Option<String>,
    edge: Option<String>,
    tag: Option<String>,
    exclude_raw: Option<bool>,
    replay: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct EdgeEnrollRequest {
    edge_id: String,
    enrollment_token: String,
}

#[derive(Debug, Deserialize)]
struct EdgeConfigCheckRequest {
    edge_id: String,
    enrollment_token: String,
    current_config_hash: Option<String>,
}

#[derive(Debug, Serialize)]
struct EdgeEnrollResponse {
    accepted: bool,
    edge_id: String,
    issued_at: chrono::DateTime<chrono::Utc>,
    config_hash: String,
}

#[derive(Debug, Serialize)]
struct EdgeConfigCheckResponse {
    accepted: bool,
    edge_id: String,
    config_changed: bool,
    target_config_hash: String,
    poll_after_secs: u64,
}

#[derive(Debug, Deserialize)]
struct EdgeRuntimeConfigQuery {
    edge_id: String,
    want_hash: Option<String>,
}

#[derive(Debug, Deserialize)]
struct EdgeResetRequest {
    site_code: String,
    edge_code: String,
    #[serde(default)]
    request_id: Option<String>,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    operator: Option<String>,
}

#[derive(Debug, Deserialize)]
struct EdgeActionRequest {
    site_code: String,
    edge_code: String,
    action_type: String,
    #[serde(default)]
    request_id: Option<String>,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    target: Option<String>,
    #[serde(default)]
    payload: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct EdgeActionResponse {
    accepted: bool,
    topic: String,
    request_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct EdgeActionCommandMessage {
    schema_version: u16,
    source: String,
    request_id: Option<String>,
    action_type: String,
    target: String,
    payload: serde_json::Value,
    timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
struct EdgeResetResponse {
    accepted: bool,
    topic: String,
    request_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct EdgeResetCommandMessage {
    schema_version: u16,
    source: String,
    request_id: Option<String>,
    reason: Option<String>,
    operator: Option<String>,
    timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SignedRuntimeConfigEnvelope {
    edge_id: String,
    issued_at: chrono::DateTime<chrono::Utc>,
    algorithm: String,
    key_id: String,
    payload_json: String,
    config_hash: String,
    signature_hex: String,
}

fn default_edge_stale_after_secs() -> i64 {
    std::env::var("CENTRAL_EDGE_STALE_AFTER_SECS_DEFAULT")
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(45)
        .max(1)
}

pub async fn connect_read_client(dsn: &str) -> Result<Client> {
    let (client, connection) = tokio_postgres::connect(dsn, NoTls).await?;
    tokio::spawn(async move {
        if let Err(e) = connection.await {
            error!("api postgres connection task error: {}", e);
        }
    });
    Ok(client)
}

pub async fn run_api_server(state: ApiState, bind: &str) -> Result<()> {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/health/live", get(health_live))
        .route("/api/edges/current", get(list_edges_current))
        .route("/api/devices/current", get(list_devices_current))
        .route("/api/connections/current", get(list_connections_current))
        .route("/api/tags/current", get(list_tags_current))
        .route("/api/tags/:tag_code/history", get(get_tag_history))
        .route("/api/context/lines", get(list_lines))
        .route("/api/context/areas", get(list_areas))
        .route("/api/context/cells", get(list_cells))
        .route("/api/ops/events", get(list_operational_events))
        .route("/api/ops/events/stream", get(stream_operational_events))
        .route("/api/edges/reset", post(edge_reset))
        .route("/api/edges/action", post(edge_action))
        .route("/api/edge/config/enroll", post(edge_enroll))
        .route("/api/edge/config/check", post(edge_config_check))
        .route("/api/edge/config/runtime", get(get_edge_runtime_config))
        .route("/api/stream/events", get(stream_events))
        .layer(cors)
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(bind).await?;
    info!("central-server API listening on {}", bind);
    axum::serve(listener, app).await?;
    Ok(())
}

async fn health_live() -> Json<LiveHealthResponse> {
    Json(LiveHealthResponse { status: "ok" })
}

async fn list_edges_current(
    State(state): State<ApiState>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<EdgeCurrentDto>>, axum::http::StatusCode> {
    let edge_stale_after_secs_default = default_edge_stale_after_secs();
    let limit = q.limit.unwrap_or(200).clamp(1, 2000);
    let rows = state
            .client
            .query(
            "SELECT s.code, l.code, a.code, c.code, e.edge_code,
                    CASE
                        WHEN ecs.last_seen_at IS NULL THEN 'disconnected'
                        WHEN GREATEST(0, EXTRACT(EPOCH FROM (NOW() - ecs.last_seen_at))::bigint) >
                             $7
                        THEN 'disconnected'
                        ELSE ecs.status
                    END AS effective_status,
                    ecs.last_seen_at, ecs.outbox_depth, ecs.outbox_oldest_secs,
                    COALESCE(eh.payload_json->'action_metrics', '{}'::jsonb) AS action_metrics
             FROM edge_current_state ecs
             JOIN edges e ON e.id = ecs.edge_id
             JOIN sites s ON s.id = e.site_id
             LEFT JOIN cells c ON c.id = e.cell_id
             LEFT JOIN areas a ON a.id = c.area_id
             LEFT JOIN lines l ON l.id = a.line_id
             LEFT JOIN LATERAL (
                 SELECT payload_json
                 FROM edge_health_events ehe
                 WHERE ehe.edge_id = ecs.edge_id
                 ORDER BY ehe.ts DESC
                 LIMIT 1
             ) eh ON TRUE
             WHERE ($2::text IS NULL OR s.code = $2)
               AND ($3::text IS NULL OR l.code = $3)
               AND ($4::text IS NULL OR a.code = $4)
               AND ($5::text IS NULL OR c.code = $5)
               AND ($6::text IS NULL OR e.edge_code = $6)
             ORDER BY ecs.last_seen_at DESC
             LIMIT $1",
            &[&limit, &q.site, &q.line, &q.area, &q.cell, &q.edge, &edge_stale_after_secs_default],
        )
        .await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        out.push(EdgeCurrentDto {
            site_code: r.get(0),
            line_code: r.get(1),
            area_code: r.get(2),
            cell_code: r.get(3),
            edge_code: r.get(4),
            status: r.get(5),
            last_seen_at: r.get(6),
            outbox_depth: r.get(7),
            outbox_oldest_secs: r.get(8),
            action_metrics: r.get(9),
        });
    }
    Ok(Json(out))
}

async fn list_tags_current(
    State(state): State<ApiState>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<TagCurrentDto>>, axum::http::StatusCode> {
    let edge_stale_after_secs_default = default_edge_stale_after_secs();
    let limit = q.limit.unwrap_or(500).clamp(1, 5000);
    let rows = state
            .client
            .query(
            "SELECT t.tag_code, d.device_code, s.code, l.code, a.code, c.code, e.edge_code, tcs.ts, tcs.value_json, tcs.quality_json, tcs.source, t.metadata_json,
                    CASE
                        WHEN (t.metadata_json->>'expected_interval_ms') ~ '^[0-9]+$'
                        THEN (t.metadata_json->>'expected_interval_ms')::bigint
                        ELSE NULL
                    END AS expected_interval_ms,
                    ecs.status,
                    ecs.last_seen_at,
                    ccs.state
             FROM tag_current_state tcs
             JOIN tags t ON t.id = tcs.tag_id
             JOIN devices d ON d.id = t.device_id
             JOIN edges e ON e.id = d.edge_id
             LEFT JOIN edge_current_state ecs ON ecs.edge_id = e.id
             LEFT JOIN connection_current_state ccs ON ccs.connection_id = d.connection_id
             JOIN sites s ON s.id = e.site_id
             LEFT JOIN cells c ON c.id = e.cell_id
             LEFT JOIN areas a ON a.id = c.area_id
             LEFT JOIN lines l ON l.id = a.line_id
             WHERE t.tag_code NOT LIKE '%_raw'
               AND ($2::text IS NULL OR s.code = $2)
               AND ($3::text IS NULL OR l.code = $3)
               AND ($4::text IS NULL OR a.code = $4)
               AND ($5::text IS NULL OR c.code = $5)
               AND ($6::text IS NULL OR e.edge_code = $6)
             ORDER BY tcs.ts DESC
             LIMIT $1",
            &[&limit, &q.site, &q.line, &q.area, &q.cell, &q.edge],
        )
        .await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut out = Vec::with_capacity(rows.len());
    let now = chrono::Utc::now();
    for r in rows {
        let ts: chrono::DateTime<chrono::Utc> = r.get(7);
        let expected_interval_ms: Option<i64> = r.get(12);
        let edge_state: Option<String> = r.get(13);
        let edge_last_seen_at: Option<chrono::DateTime<chrono::Utc>> = r.get(14);
        let connection_state: Option<String> = r.get(15);

        let sample_age_secs = now.signed_duration_since(ts).num_seconds().max(0);
        let edge_age_secs = edge_last_seen_at
            .map(|v| now.signed_duration_since(v).num_seconds().max(0));
        let tag_status = evaluate_tag_connection_status(TagStatusInput {
            edge_state: edge_state.as_deref(),
            edge_age_secs,
            edge_stale_after_secs: edge_stale_after_secs_default,
            connection_state: connection_state.as_deref(),
            sample_age_secs,
            expected_interval_ms,
        })
        .as_str()
        .to_string();

        out.push(TagCurrentDto {
            tag_code: r.get(0),
            device_code: r.get(1),
            site_code: r.get(2),
            line_code: r.get(3),
            area_code: r.get(4),
            cell_code: r.get(5),
            edge_code: r.get(6),
            ts,
            value: r.get(8),
            quality: r.get(9),
            source: r.get(10),
            metadata_json: r.get(11),
            expected_interval_ms,
            tag_status,
        });
    }
    Ok(Json(out))
}

async fn list_devices_current(
    State(state): State<ApiState>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<DeviceCurrentDto>>, axum::http::StatusCode> {
    let edge_stale_after_secs_default = default_edge_stale_after_secs();
    let limit = q.limit.unwrap_or(500).clamp(1, 5000);
    let rows = state
        .client
        .query(
            "SELECT
                s.code,
                l.code,
                a.code,
                c.code,
                e.edge_code,
                d.device_code,
                cn.connection_code,
                CASE
                    WHEN ecs.last_seen_at IS NULL THEN 'disconnected'
                    WHEN GREATEST(0, EXTRACT(EPOCH FROM (NOW() - ecs.last_seen_at))::bigint) >
                         $7
                    THEN 'disconnected'
                    ELSE dcs.state
                END AS effective_state,
                CASE
                    WHEN ecs.last_seen_at IS NULL THEN 'error'
                    WHEN GREATEST(0, EXTRACT(EPOCH FROM (NOW() - ecs.last_seen_at))::bigint) >
                         $7
                    THEN 'error'
                    ELSE dcs.severity
                END AS effective_severity,
                CASE
                    WHEN ecs.last_seen_at IS NULL THEN 'edge_offline_or_stale'
                    WHEN GREATEST(0, EXTRACT(EPOCH FROM (NOW() - ecs.last_seen_at))::bigint) >
                         $7
                    THEN 'edge_offline_or_stale'
                    ELSE dcs.reason
                END AS effective_reason,
                CASE
                    WHEN ecs.last_seen_at IS NULL THEN 0
                    WHEN GREATEST(0, EXTRACT(EPOCH FROM (NOW() - ecs.last_seen_at))::bigint) >
                         $7
                    THEN 0
                    ELSE dcs.tags_connected
                END AS effective_tags_connected,
                CASE
                    WHEN ecs.last_seen_at IS NULL THEN 0
                    WHEN GREATEST(0, EXTRACT(EPOCH FROM (NOW() - ecs.last_seen_at))::bigint) >
                         $7
                    THEN 0
                    ELSE dcs.tags_stale
                END AS effective_tags_stale,
                CASE
                    WHEN ecs.last_seen_at IS NULL THEN (dcs.tags_connected + dcs.tags_stale + dcs.tags_disconnected)
                    WHEN GREATEST(0, EXTRACT(EPOCH FROM (NOW() - ecs.last_seen_at))::bigint) >
                         $7
                    THEN (dcs.tags_connected + dcs.tags_stale + dcs.tags_disconnected)
                    ELSE dcs.tags_disconnected
                END AS effective_tags_disconnected,
                dcs.last_change_at,
                dcs.last_seen_at
             FROM device_current_state dcs
             JOIN devices d ON d.id = dcs.device_id
             JOIN edges e ON e.id = d.edge_id
             JOIN sites s ON s.id = e.site_id
             LEFT JOIN edge_current_state ecs ON ecs.edge_id = e.id
             LEFT JOIN cells c ON c.id = e.cell_id
             LEFT JOIN areas a ON a.id = c.area_id
             LEFT JOIN lines l ON l.id = a.line_id
             LEFT JOIN connections cn ON cn.id = dcs.connection_id
             WHERE ($2::text IS NULL OR s.code = $2)
               AND ($3::text IS NULL OR l.code = $3)
               AND ($4::text IS NULL OR a.code = $4)
               AND ($5::text IS NULL OR c.code = $5)
               AND ($6::text IS NULL OR e.edge_code = $6)
             ORDER BY dcs.last_change_at DESC
             LIMIT $1",
            &[&limit, &q.site, &q.line, &q.area, &q.cell, &q.edge, &edge_stale_after_secs_default],
        )
        .await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        out.push(DeviceCurrentDto {
            site_code: r.get(0),
            line_code: r.get(1),
            area_code: r.get(2),
            cell_code: r.get(3),
            edge_code: r.get(4),
            device_code: r.get(5),
            connection_id: r.get(6),
            state: r.get(7),
            severity: r.get(8),
            reason: r.get(9),
            tags_connected: r.get::<_, i32>(10) as i64,
            tags_stale: r.get::<_, i32>(11) as i64,
            tags_disconnected: r.get::<_, i32>(12) as i64,
            last_change_at: r.get(13),
            last_seen_at: r.get(14),
        });
    }
    Ok(Json(out))
}

async fn list_connections_current(
    State(state): State<ApiState>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<ConnectionCurrentDto>>, axum::http::StatusCode> {
    let limit = q.limit.unwrap_or(500).clamp(1, 5000);
    let rows = state
        .client
        .query(
            "SELECT
                    s.code,
                    l.code,
                    a.code,
                    c.code,
                    e.edge_code,
                    cn.connection_code,
                    ccs.state,
                    ccs.severity,
                    ccs.last_change_at,
                    COALESCE(ccs.reason, '')
             FROM connection_current_state ccs
             JOIN connections cn ON cn.id = ccs.connection_id
             JOIN edges e ON e.id = cn.edge_id
             JOIN sites s ON s.id = e.site_id
             LEFT JOIN cells c ON c.id = e.cell_id
             LEFT JOIN areas a ON a.id = c.area_id
             LEFT JOIN lines l ON l.id = a.line_id
             WHERE ($2::text IS NULL OR s.code = $2)
               AND ($3::text IS NULL OR l.code = $3)
               AND ($4::text IS NULL OR a.code = $4)
               AND ($5::text IS NULL OR c.code = $5)
               AND ($6::text IS NULL OR e.edge_code = $6)
             ORDER BY ccs.last_change_at DESC
             LIMIT $1",
            &[&limit, &q.site, &q.line, &q.area, &q.cell, &q.edge],
        )
        .await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        out.push(ConnectionCurrentDto {
            site_code: r.get(0),
            line_code: r.get(1),
            area_code: r.get(2),
            cell_code: r.get(3),
            edge_code: r.get(4),
            connection_id: r.get(5),
            state: r.get(6),
            severity: r.get(7),
            last_change_at: r.get(8),
            message: r.get(9),
        });
    }
    Ok(Json(out))
}

async fn list_lines(
    State(state): State<ApiState>,
    Query(q): Query<ContextQuery>,
) -> Result<Json<Vec<ContextOptionDto>>, axum::http::StatusCode> {
    let rows = state
        .client
        .query(
            "SELECT DISTINCT l.code, l.name
             FROM lines l
             JOIN sites s ON s.id = l.site_id
             WHERE ($1::text IS NULL OR s.code = $1)
             ORDER BY l.code",
            &[&q.site],
        )
        .await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(
        rows.into_iter()
            .map(|r| ContextOptionDto {
                code: r.get(0),
                name: r.get(1),
            })
            .collect(),
    ))
}

async fn list_areas(
    State(state): State<ApiState>,
    Query(q): Query<ContextQuery>,
) -> Result<Json<Vec<ContextOptionDto>>, axum::http::StatusCode> {
    let rows = state
        .client
        .query(
            "SELECT DISTINCT a.code, a.name
             FROM areas a
             JOIN lines l ON l.id = a.line_id
             JOIN sites s ON s.id = l.site_id
             WHERE ($1::text IS NULL OR s.code = $1)
               AND ($2::text IS NULL OR l.code = $2)
             ORDER BY a.code",
            &[&q.site, &q.line],
        )
        .await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(
        rows.into_iter()
            .map(|r| ContextOptionDto {
                code: r.get(0),
                name: r.get(1),
            })
            .collect(),
    ))
}

async fn list_cells(
    State(state): State<ApiState>,
    Query(q): Query<ContextQuery>,
) -> Result<Json<Vec<ContextOptionDto>>, axum::http::StatusCode> {
    let rows = state
        .client
        .query(
            "SELECT DISTINCT c.code, c.name
             FROM cells c
             JOIN areas a ON a.id = c.area_id
             JOIN lines l ON l.id = a.line_id
             JOIN sites s ON s.id = l.site_id
             WHERE ($1::text IS NULL OR s.code = $1)
               AND ($2::text IS NULL OR l.code = $2)
               AND ($3::text IS NULL OR a.code = $3)
             ORDER BY c.code",
            &[&q.site, &q.line, &q.area],
        )
        .await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(
        rows.into_iter()
            .map(|r| ContextOptionDto {
                code: r.get(0),
                name: r.get(1),
            })
            .collect(),
    ))
}

async fn list_operational_events(
    State(state): State<ApiState>,
    Query(q): Query<OpsQuery>,
) -> Result<Json<Vec<OperationalEventDto>>, axum::http::StatusCode> {
    let limit = q.limit.unwrap_or(200).clamp(1, 2000);
    let edge_like = q.edge.as_ref().map(|v| format!("%{}%", v));
    let device_like = q.device.as_ref().map(|v| format!("%{}%", v));
    let tag_like = q.tag.as_ref().map(|v| format!("%{}%", v));
    let severity_like = q.severity.as_ref().map(|v| format!("%{}%", v));
    let event_type_like = q.event_type.as_ref().map(|v| format!("%{}%", v));
    let q_like = q.q.as_ref().map(|v| format!("%{}%", v));
    let rows = state
        .client
        .query(
            "SELECT id, ts, severity, event_type, site_code, edge_code, connection_id, device_code, tag_code, config_hash, op_id, message, payload_json
             FROM operational_events
             WHERE ($2::text IS NULL OR site_code = $2)
               AND ($3::text IS NULL OR edge_code ILIKE $3)
               AND ($4::text IS NULL OR device_code ILIKE $4)
               AND ($5::text IS NULL OR tag_code ILIKE $5)
               AND ($6::text IS NULL OR severity ILIKE $6)
               AND ($7::text IS NULL OR event_type ILIKE $7)
               AND (
                   $8::text IS NULL
                   OR event_type ILIKE $8
                   OR message ILIKE $8
                   OR COALESCE(edge_code, '') ILIKE $8
                   OR COALESCE(tag_code, '') ILIKE $8
                   OR COALESCE(connection_id, '') ILIKE $8
                   OR COALESCE(device_code, '') ILIKE $8
                   OR payload_json::text ILIKE $8
               )
             ORDER BY ts DESC
             LIMIT $1",
            &[ 
                &limit,
                &q.site,
                &edge_like,
                &device_like,
                &tag_like,
                &severity_like,
                &event_type_like,
                &q_like,
            ],
        )
        .await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(
        rows.into_iter()
            .map(|r| OperationalEventDto {
                id: r.get(0),
                ts: r.get(1),
                severity: r.get(2),
                event_type: r.get(3),
                site_code: r.get(4),
                edge_code: r.get(5),
                connection_id: r.get(6),
                device_code: r.get(7),
                tag_code: r.get(8),
                config_hash: r.get(9),
                op_id: r.get(10),
                message: r.get(11),
                payload_json: r.get(12),
            })
            .collect(),
    ))
}

async fn stream_operational_events(
    State(state): State<ApiState>,
    Query(q): Query<OpsStreamQuery>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let client = state.client.clone();
    let replay = q.replay.unwrap_or(false);
    let site = q.site.clone();
    let edge_like = q.edge.as_ref().map(|v| format!("%{}%", v));
    let device_like = q.device.as_ref().map(|v| format!("%{}%", v));
    let tag_like = q.tag.as_ref().map(|v| format!("%{}%", v));
    let severity_like = q.severity.as_ref().map(|v| format!("%{}%", v));
    let event_type_like = q.event_type.as_ref().map(|v| format!("%{}%", v));
    let q_like = q.q.as_ref().map(|v| format!("%{}%", v));

    let out = async_stream::stream! {
        let mut last_id: i64 = if replay {
            0
        } else {
            match client.query_opt(
                "SELECT COALESCE(MAX(id), 0)
                 FROM operational_events
                 WHERE ($1::text IS NULL OR site_code = $1)
                   AND ($2::text IS NULL OR edge_code ILIKE $2)
                   AND ($3::text IS NULL OR device_code ILIKE $3)
                   AND ($4::text IS NULL OR tag_code ILIKE $4)
                   AND ($5::text IS NULL OR severity ILIKE $5)
                   AND ($6::text IS NULL OR event_type ILIKE $6)
                   AND (
                       $7::text IS NULL
                       OR event_type ILIKE $7
                       OR message ILIKE $7
                       OR COALESCE(edge_code, '') ILIKE $7
                       OR COALESCE(tag_code, '') ILIKE $7
                       OR COALESCE(connection_id, '') ILIKE $7
                       OR COALESCE(device_code, '') ILIKE $7
                       OR payload_json::text ILIKE $7
                   )",
                &[&site, &edge_like, &device_like, &tag_like, &severity_like, &event_type_like, &q_like],
            ).await {
                Ok(Some(r)) => r.get(0),
                _ => 0,
            }
        };
        loop {
            let rows = client.query(
                "SELECT id, ts, severity, event_type, site_code, edge_code, connection_id, device_code, tag_code, config_hash, op_id, message, payload_json
                 FROM operational_events
                 WHERE id > $1
                   AND ($2::text IS NULL OR site_code = $2)
                   AND ($3::text IS NULL OR edge_code ILIKE $3)
                   AND ($4::text IS NULL OR device_code ILIKE $4)
                   AND ($5::text IS NULL OR tag_code ILIKE $5)
                   AND ($6::text IS NULL OR severity ILIKE $6)
                   AND ($7::text IS NULL OR event_type ILIKE $7)
                   AND (
                       $8::text IS NULL
                       OR event_type ILIKE $8
                       OR message ILIKE $8
                       OR COALESCE(edge_code, '') ILIKE $8
                       OR COALESCE(tag_code, '') ILIKE $8
                       OR COALESCE(connection_id, '') ILIKE $8
                       OR COALESCE(device_code, '') ILIKE $8
                       OR payload_json::text ILIKE $8
                   )
                 ORDER BY id ASC
                 LIMIT 200",
                &[&last_id, &site, &edge_like, &device_like, &tag_like, &severity_like, &event_type_like, &q_like],
            ).await;
            match rows {
                Ok(rows) => {
                    let rows_len = rows.len();
                    for r in rows {
                        last_id = r.get(0);
                        let evt = OperationalEventDto {
                            id: r.get(0),
                            ts: r.get(1),
                            severity: r.get(2),
                            event_type: r.get(3),
                            site_code: r.get(4),
                            edge_code: r.get(5),
                            connection_id: r.get(6),
                            device_code: r.get(7),
                            tag_code: r.get(8),
                            config_hash: r.get(9),
                            op_id: r.get(10),
                            message: r.get(11),
                            payload_json: r.get(12),
                        };
                        if let Ok(event) = Event::default().event("ops").json_data(evt) {
                            yield Ok(event);
                        }
                    }
                    if rows_len == 0 {
                        tokio::time::sleep(Duration::from_millis(80)).await;
                    } else {
                        tokio::time::sleep(Duration::from_millis(10)).await;
                    }
                }
                Err(e) => {
                    error!("stream_operational_events query error: {}", e);
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
            }
        }
    };

    Sse::new(out).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(10))
            .text("keep-alive"),
    )
}

async fn edge_enroll(
    State(state): State<ApiState>,
    Json(req): Json<EdgeEnrollRequest>,
) -> Result<Json<EdgeEnrollResponse>, axum::http::StatusCode> {
    if req.edge_id.trim().is_empty() {
        return Err(axum::http::StatusCode::BAD_REQUEST);
    }
    if req.enrollment_token != state.edge_cfg.enroll_token {
        return Err(axum::http::StatusCode::UNAUTHORIZED);
    }
    let (_, config_hash) = read_runtime_payload_and_hash(&state, &req.edge_id).await?;
    Ok(Json(EdgeEnrollResponse {
        accepted: true,
        edge_id: req.edge_id,
        issued_at: chrono::Utc::now(),
        config_hash,
    }))
}

async fn edge_reset(
    State(state): State<ApiState>,
    Json(req): Json<EdgeResetRequest>,
) -> Result<Json<EdgeResetResponse>, axum::http::StatusCode> {
    if req.site_code.trim().is_empty() || req.edge_code.trim().is_empty() {
        return Err(axum::http::StatusCode::BAD_REQUEST);
    }
    let Some(client) = state.mqtt_cmd.clone() else {
        return Err(axum::http::StatusCode::SERVICE_UNAVAILABLE);
    };
    let topic = format!(
        "scada/{}/edge/{}/control/reset",
        req.site_code.trim(),
        req.edge_code.trim()
    );
    let msg = EdgeResetCommandMessage {
        schema_version: 1,
        source: "central-api".to_string(),
        request_id: req.request_id.clone(),
        reason: req.reason.clone(),
        operator: req.operator.clone(),
        timestamp: chrono::Utc::now(),
    };
    let payload =
        serde_json::to_vec(&msg).map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
    client
        .publish(topic.clone(), QoS::AtLeastOnce, false, payload)
        .await
        .map_err(|_| axum::http::StatusCode::BAD_GATEWAY)?;

    Ok(Json(EdgeResetResponse {
        accepted: true,
        topic,
        request_id: req.request_id,
    }))
}

async fn edge_action(
    State(state): State<ApiState>,
    Json(req): Json<EdgeActionRequest>,
) -> Result<Json<EdgeActionResponse>, axum::http::StatusCode> {
    if req.site_code.trim().is_empty()
        || req.edge_code.trim().is_empty()
        || req.action_type.trim().is_empty()
    {
        return Err(axum::http::StatusCode::BAD_REQUEST);
    }
    let Some(client) = state.mqtt_cmd.clone() else {
        return Err(axum::http::StatusCode::SERVICE_UNAVAILABLE);
    };
    let topic = format!(
        "scada/{}/edge/{}/cmd/action",
        req.site_code.trim(),
        req.edge_code.trim()
    );
    let msg = EdgeActionCommandMessage {
        schema_version: 1,
        source: req.source.unwrap_or_else(|| "central-api".to_string()),
        request_id: req.request_id.clone(),
        action_type: req.action_type,
        target: req.target.unwrap_or_else(|| "edge".to_string()),
        payload: req.payload,
        timestamp: chrono::Utc::now(),
    };
    let payload =
        serde_json::to_vec(&msg).map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
    client
        .publish(topic.clone(), QoS::AtLeastOnce, false, payload)
        .await
        .map_err(|_| axum::http::StatusCode::BAD_GATEWAY)?;

    Ok(Json(EdgeActionResponse {
        accepted: true,
        topic,
        request_id: req.request_id,
    }))
}

async fn edge_config_check(
    State(state): State<ApiState>,
    Json(req): Json<EdgeConfigCheckRequest>,
) -> Result<Json<EdgeConfigCheckResponse>, axum::http::StatusCode> {
    if req.edge_id.trim().is_empty() {
        return Err(axum::http::StatusCode::BAD_REQUEST);
    }
    if req.enrollment_token != state.edge_cfg.enroll_token {
        return Err(axum::http::StatusCode::UNAUTHORIZED);
    }
    let (_, current_hash) = read_runtime_payload_and_hash(&state, &req.edge_id).await?;
    let same = req
        .current_config_hash
        .as_ref()
        .map(|h| h.eq_ignore_ascii_case(&current_hash))
        .unwrap_or(false);
    Ok(Json(EdgeConfigCheckResponse {
        accepted: true,
        edge_id: req.edge_id,
        config_changed: !same,
        target_config_hash: current_hash,
        poll_after_secs: 120,
    }))
}

async fn get_edge_runtime_config(
    State(state): State<ApiState>,
    Query(q): Query<EdgeRuntimeConfigQuery>,
) -> Result<Json<SignedRuntimeConfigEnvelope>, axum::http::StatusCode> {
    if q.edge_id.trim().is_empty() {
        return Err(axum::http::StatusCode::BAD_REQUEST);
    }
    let (payload_json, config_hash) = read_runtime_payload_and_hash(&state, &q.edge_id).await?;
    if let Some(want) = q.want_hash.as_ref() {
        if want.eq_ignore_ascii_case(&config_hash) {
            return Err(axum::http::StatusCode::NOT_MODIFIED);
        }
    }
    let payload_hash = from_hex(&config_hash).map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut mac = Hmac::<Sha256>::new_from_slice(state.edge_cfg.signing_secret.as_bytes())
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
    mac.update(&payload_hash);
    let sig = mac.finalize().into_bytes();

    let env = SignedRuntimeConfigEnvelope {
        edge_id: q.edge_id,
        issued_at: chrono::Utc::now(),
        algorithm: "hmac-sha256".to_string(),
        key_id: state.edge_cfg.signing_key_id.clone(),
        payload_json,
        config_hash,
        signature_hex: to_hex(sig.as_slice()),
    };
    Ok(Json(env))
}

async fn read_runtime_payload_and_hash(
    state: &ApiState,
    edge_id: &str,
) -> Result<(String, String), axum::http::StatusCode> {
    let payload_json = if let Some(db_payload) = build_runtime_payload_from_db(state, edge_id).await? {
        db_payload
    } else {
        fs::read_to_string(&state.edge_cfg.runtime_config_path)
            .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?
    };
    let config_hash = to_hex(Sha256::digest(payload_json.as_bytes()).as_slice());
    Ok((payload_json, config_hash))
}

async fn build_runtime_payload_from_db(
    state: &ApiState,
    edge_id: &str,
) -> Result<Option<String>, axum::http::StatusCode> {
    let connection_rows = state
        .client
        .query(
            "SELECT c.id, c.connection_code, c.name, c.driver_type, c.metadata_json
             FROM connections c
             JOIN edges e ON e.id = c.edge_id
             WHERE e.edge_code = $1
             ORDER BY c.id",
            &[&edge_id],
        )
        .await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
    if connection_rows.is_empty() {
        return Ok(None);
    }

    let mut connections_out = Vec::with_capacity(connection_rows.len());
    let mut automations_out: Vec<Value> = Vec::new();
    for row in connection_rows {
        let connection_pk: i64 = row.get(0);
        let connection_code: String = row.get(1);
        let connection_name: String = row.get(2);
        let driver_type: String = row.get(3);
        let metadata_json: Value = row.get(4);
        if !is_supported_edge_driver(&driver_type) {
            info!(
                "skipping runtime config connection '{}' with unsupported driver_type '{}'",
                connection_code, driver_type
            );
            continue;
        }
        if let Some(entries) = metadata_json.get("automations").and_then(|v| v.as_array()) {
            for entry in entries {
                if !entry.is_object() {
                    continue;
                }
                automations_out.push(entry.clone());
            }
        }

        let tags_rows = state
            .client
            .query(
                "SELECT t.tag_code, t.name, d.device_code, t.source, t.value_type, t.metadata_json
                 FROM tags t
                 JOIN devices d ON d.id = t.device_id
                 WHERE d.connection_id = $1
                 ORDER BY t.id",
                &[&connection_pk],
            )
            .await
            .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

        let mut tags_out = Vec::with_capacity(tags_rows.len());
        let mut serial_tag_map = Map::new();
        for tr in tags_rows {
            let tag_code: String = tr.get(0);
            let tag_code_for_transport = tag_code.clone();
            let tag_name: String = tr.get(1);
            let device_code: String = tr.get(2);
            let source: String = tr.get(3);
            let source_for_transport = source.clone();
            let value_type: String = tr.get(4);
            let tag_meta: Value = tr.get(5);

            let enabled = tag_meta
                .get("enabled")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            let (update_mode, interval_ms) = parse_tag_update_runtime_fields(&tag_meta);

            tags_out.push(json!({
                "id": tag_code,
                "name": tag_name,
                "device_id": device_code,
                "source": source,
                "enabled": enabled,
                "value_type": value_type.to_ascii_lowercase(),
                "update_mode": update_mode,
                "interval_ms": interval_ms,
                "metadata_json": tag_meta.clone()
            }));
            serial_tag_map.insert(
                tag_code_for_transport.clone(),
                Value::String(source_for_transport),
            );

            // Tag-scoped automations: infer trigger.tag_id from the tag context when omitted.
            if let Some(entries) = tag_meta.get("automations").and_then(|v| v.as_array()) {
                for entry in entries {
                    if let Some(enriched) = enrich_automation_with_tag_context(entry, &tag_code_for_transport) {
                        automations_out.push(enriched);
                    }
                }
            }
        }

        let mut transport =
            build_transport_for_runtime(state, connection_pk, &driver_type, &metadata_json).await?;
        if driver_type.eq_ignore_ascii_case("SerialAscii") {
            if !transport.is_object() {
                transport = Value::Object(Map::new());
            }
            if let Some(obj) = transport.as_object_mut() {
                if !obj.contains_key("tag_map") {
                    obj.insert("tag_map".to_string(), Value::Object(serial_tag_map));
                }
            }
        }
        let timeout_ms = extract_connection_timeout_ms(&metadata_json).unwrap_or(1500);
        let reconnect_delay_ms = extract_reconnect_delay_ms(&metadata_json).unwrap_or(1000);
        let max_retries = extract_max_retries(&metadata_json);

        connections_out.push(json!({
            "id": connection_code,
            "name": connection_name,
            "driver_type": driver_type,
            "timeout_ms": timeout_ms,
            "reconnect_delay_ms": reconnect_delay_ms,
            "max_retries": max_retries,
            "transport": transport,
            "tags": tags_out
        }));
    }

    let payload = json!({
        "connections": connections_out,
        "automations": automations_out
    });
    let payload_json =
        serde_json::to_string(&payload).map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Some(payload_json))
}

fn is_supported_edge_driver(driver_type: &str) -> bool {
    matches!(
        driver_type.trim().to_ascii_lowercase().as_str(),
        "modbusrtu" | "modbustcp" | "serialascii" | "simulator"
    )
}

fn enrich_automation_with_tag_context(entry: &Value, tag_code: &str) -> Option<Value> {
    let mut auto = entry.as_object()?.clone();
    let mut trigger = auto
        .get("trigger")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();
    if !trigger.contains_key("tag_id") {
        trigger.insert("tag_id".to_string(), Value::String(tag_code.to_string()));
    }
    auto.insert("trigger".to_string(), Value::Object(trigger));
    Some(Value::Object(auto))
}

async fn build_transport_for_runtime(
    state: &ApiState,
    connection_pk: i64,
    driver_type: &str,
    metadata_json: &Value,
) -> Result<Value, axum::http::StatusCode> {
    if driver_type.eq_ignore_ascii_case("ModbusRTU") {
        let mut out = Map::new();
        let serial = metadata_json
            .pointer("/transport/serial")
            .cloned()
            .or_else(|| metadata_json.get("serial").cloned())
            .unwrap_or_else(|| json!({"port":"COM10","baud_rate":9600,"data_bits":8,"stop_bits":1,"parity":"N"}));
        out.insert("serial".to_string(), serial);

        if let Some(protocol) = metadata_json.get("protocol").and_then(|v| v.as_object()) {
            for (k, v) in protocol {
                out.insert(k.clone(), v.clone());
            }
        }

        let device_rows = state
            .client
            .query(
                "SELECT device_code, metadata_json
                 FROM devices
                 WHERE connection_id = $1",
                &[&connection_pk],
            )
            .await
            .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
        let mut device_unit_map = Map::new();
        for dr in device_rows {
            let device_code: String = dr.get(0);
            let meta: Value = dr.get(1);
            if let Some(unit) = meta
                .pointer("/modbus/slave_id")
                .and_then(|v| v.as_u64())
                .and_then(|v| u8::try_from(v).ok())
            {
                device_unit_map.insert(
                    device_code,
                    Value::Number(serde_json::Number::from(unit)),
                );
            }
        }
        out.insert("device_unit_map".to_string(), Value::Object(device_unit_map));
        return Ok(Value::Object(out));
    }

    if driver_type.eq_ignore_ascii_case("SerialAscii") {
        let mut out = Map::new();
        if let Some(serial) = metadata_json.pointer("/transport/serial").cloned() {
            out.insert("serial".to_string(), serial);
        }
        if let Some(frame) = metadata_json.get("frame").cloned() {
            out.insert("frame".to_string(), frame);
        }
        if let Some(parser) = metadata_json.get("parser").cloned() {
            out.insert("parser".to_string(), parser);
        }
        if let Some(read_timeout_ms) = metadata_json
            .pointer("/frame/read_timeout_ms")
            .and_then(|v| v.as_u64())
            .or_else(|| metadata_json.get("read_timeout_ms").and_then(|v| v.as_u64()))
        {
            out.insert(
                "read_timeout_ms".to_string(),
                Value::Number(serde_json::Number::from(read_timeout_ms)),
            );
        }
        return Ok(Value::Object(out));
    }

    if let Some(transport) = metadata_json.get("transport").cloned() {
        return Ok(transport);
    }
    Ok(metadata_json.clone())
}

fn extract_connection_timeout_ms(metadata_json: &Value) -> Option<u64> {
    metadata_json
        .pointer("/timeouts/request_timeout_ms")
        .and_then(|v| v.as_u64())
        .or_else(|| metadata_json.get("timeout_ms").and_then(|v| v.as_u64()))
}

fn extract_reconnect_delay_ms(metadata_json: &Value) -> Option<u64> {
    metadata_json
        .pointer("/timeouts/reconnect_delay_ms")
        .and_then(|v| v.as_u64())
        .or_else(|| metadata_json.get("reconnect_delay_ms").and_then(|v| v.as_u64()))
}

fn extract_max_retries(metadata_json: &Value) -> Option<u32> {
    metadata_json
        .pointer("/timeouts/max_retries")
        .and_then(|v| v.as_u64())
        .and_then(|v| u32::try_from(v).ok())
        .or_else(|| {
            metadata_json
                .get("max_retries")
                .and_then(|v| v.as_u64())
                .and_then(|v| u32::try_from(v).ok())
        })
}

fn parse_tag_update_runtime_fields(tag_meta: &Value) -> (String, u64) {
    let update_mode = tag_meta
        .get("update_mode")
        .and_then(|v| v.as_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_else(|| "polling".to_string());
    let interval_ms = tag_meta
        .get("interval_ms")
        .and_then(|v| v.as_u64())
        .unwrap_or(1000);
    (update_mode, interval_ms)
}

fn from_hex(input: &str) -> anyhow::Result<Vec<u8>> {
    let bytes = input.trim().as_bytes();
    if bytes.len() % 2 != 0 {
        return Err(anyhow::anyhow!("invalid hex length"));
    }
    let mut out = Vec::with_capacity(bytes.len() / 2);
    for i in (0..bytes.len()).step_by(2) {
        let hi = hex_nibble(bytes[i])?;
        let lo = hex_nibble(bytes[i + 1])?;
        out.push((hi << 4) | lo);
    }
    Ok(out)
}

fn hex_nibble(b: u8) -> anyhow::Result<u8> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(10 + b - b'a'),
        b'A'..=b'F' => Ok(10 + b - b'A'),
        _ => Err(anyhow::anyhow!("invalid hex character '{}'", b as char)),
    }
}

fn to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

async fn get_tag_history(
    State(state): State<ApiState>,
    Path(tag_code): Path<String>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<TagHistoryDto>>, axum::http::StatusCode> {
    let limit = q.limit.unwrap_or(500).clamp(1, 5000);
    let offset = q.offset.unwrap_or(0).max(0);
    let rows = state
            .client
            .query(
            "SELECT ts, site_code, edge_code, tag_code, value_json, quality_status
             FROM telemetry_ingest_events
             WHERE tag_code = $1
             ORDER BY ts DESC
             LIMIT $2 OFFSET $3",
            &[&tag_code, &limit, &offset],
        )
        .await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        out.push(TagHistoryDto {
            ts: r.get(0),
            site_code: r.get(1),
            edge_code: r.get(2),
            tag_code: r.get(3),
            value: r.get(4),
            quality_status: r.get(5),
        });
    }
    Ok(Json(out))
}

async fn stream_events(
    State(state): State<ApiState>,
    Query(q): Query<StreamQuery>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let client = state.client.clone();
    let exclude_raw = q.exclude_raw.unwrap_or(true);
    let replay = q.replay.unwrap_or(false);
    let site = q.site.clone();
    let line = q.line.clone();
    let area = q.area.clone();
    let cell = q.cell.clone();
    let edge = q.edge.clone();
    let tag = q.tag.clone();
    let out = async_stream::stream! {
        let mut last_id: i64 = if replay {
            0
        } else {
            match client.query_opt(
                "SELECT COALESCE(MAX(tie.id), 0)
                 FROM telemetry_ingest_events tie
                 JOIN tags t ON t.tag_code = tie.tag_code
                 JOIN devices d ON d.id = t.device_id
                 JOIN edges e ON e.id = d.edge_id AND e.edge_code = tie.edge_code
                 JOIN sites s ON s.id = e.site_id AND s.code = tie.site_code
                 LEFT JOIN cells c ON c.id = e.cell_id
                 LEFT JOIN areas a ON a.id = c.area_id
                 LEFT JOIN lines l ON l.id = a.line_id
                 WHERE ($1::text IS NULL OR tie.site_code = $1)
                   AND ($2::text IS NULL OR tie.edge_code = $2)
                   AND ($3::text IS NULL OR tie.tag_code = $3)
                   AND (NOT $4::bool OR tie.tag_code NOT LIKE '%_raw')
                   AND ($5::text IS NULL OR l.code = $5)
                   AND ($6::text IS NULL OR a.code = $6)
                   AND ($7::text IS NULL OR c.code = $7)",
                &[&site, &edge, &tag, &exclude_raw, &line, &area, &cell],
            ).await {
                Ok(Some(r)) => r.get(0),
                _ => 0,
            }
        };
        loop {
            let rows = client.query(
                "SELECT tie.id, tie.site_code, tie.edge_code, tie.payload_json, tie.ts
                 FROM telemetry_ingest_events tie
                 JOIN tags t ON t.tag_code = tie.tag_code
                 JOIN devices d ON d.id = t.device_id
                 JOIN edges e ON e.id = d.edge_id AND e.edge_code = tie.edge_code
                 JOIN sites s ON s.id = e.site_id AND s.code = tie.site_code
                 LEFT JOIN cells c ON c.id = e.cell_id
                 LEFT JOIN areas a ON a.id = c.area_id
                 LEFT JOIN lines l ON l.id = a.line_id
                 WHERE tie.id > $1
                   AND ($2::text IS NULL OR tie.site_code = $2)
                   AND ($3::text IS NULL OR tie.edge_code = $3)
                   AND ($4::text IS NULL OR tie.tag_code = $4)
                   AND (NOT $5::bool OR tie.tag_code NOT LIKE '%_raw')
                   AND ($6::text IS NULL OR l.code = $6)
                   AND ($7::text IS NULL OR a.code = $7)
                   AND ($8::text IS NULL OR c.code = $8)
                 ORDER BY tie.id ASC
                 LIMIT 200",
                &[&last_id, &site, &edge, &tag, &exclude_raw, &line, &area, &cell],
            ).await;

            match rows {
                Ok(rows) => {
                    let rows_len = rows.len();
                    for r in rows {
                        last_id = r.get(0);
                        let evt = RtEventDto {
                            event_type: "telemetry.tag",
                            site: r.get(1),
                            agent: r.get(2),
                            payload: r.get(3),
                            published_at: r.get(4),
                        };
                        if let Ok(event) = Event::default().event("runtime").json_data(evt) {
                            yield Ok(event);
                        }
                    }
                    if rows_len == 0 {
                        tokio::time::sleep(Duration::from_millis(30)).await;
                    } else {
                        tokio::time::sleep(Duration::from_millis(10)).await;
                    }
                }
                Err(e) => {
                    error!("stream_events query error: {}", e);
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
            }
        }
    };

    Sse::new(out).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(10))
            .text("keep-alive"),
    )
}

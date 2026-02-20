use axum::{
    Router,
    extract::{Path, State},
    response::{
        IntoResponse, Json,
        sse::{Event, Sse},
    },
    routing::{get, post},
};
use futures::Stream;
use serde_json::json;
use std::{sync::Arc, time::Duration};
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

use crate::state::AppState;

use tower_http::cors::{Any, CorsLayer};

pub fn create_router(state: Arc<AppState>) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .route("/api/agents", get(get_agents))
        .route("/api/tags", get(get_all_tags))
        .route("/api/tags/batch-print", post(batch_print_events))
        .route("/api/tags/{id}", get(get_tag))
        .route("/api/events", get(sse_handler))
        .route("/api/agents/{id}/command", post(send_command))
        .route("/api/reports", get(get_reports))
        .route("/api/reports/{id}", get(get_report_details))
        .route("/api/reports/{id}/reprint", post(reprint_report))
        .route("/api/tags/{id}/history", get(get_tag_history))
        .layer(cors)
        .fallback_service(
            tower_http::services::ServeDir::new("static")
                .not_found_service(tower_http::services::ServeFile::new("static/index.html")),
        )
        .with_state(state)
}

async fn get_agents(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let agents = state.agents.read().unwrap();
    // Note: is_registered will be true only for agents present in the edge_agents table.
    // Agents created dynamically via heartbeats (ghosts) will have is_registered: false.
    let list: Vec<_> = agents.values().cloned().collect();
    Json(list)
}

async fn get_all_tags(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    // tags.device_id -> devices.edge_agent_id gives us the agent
    let tags = sqlx::query!(
        r#"
        SELECT t.id, d.edge_agent_id, t.last_value, t.quality, t.status, t.last_update
        FROM tags t
        JOIN devices d ON t.device_id = d.id
        ORDER BY t.id ASC
        "#
    )
    .fetch_all(&state.pool)
    .await;

    match tags {
        Ok(rows) => {
            let list: Vec<_> = rows
                .into_iter()
                .map(|r| {
                    json!({
                        "id": r.id,
                        "agent_id": r.edge_agent_id,
                        "value": r.last_value,
                        "quality": r.quality,
                        "status": r.status,
                        "timestamp": r.last_update
                    })
                })
                .collect();
            Json(list)
        }
        Err(e) => Json(vec![json!({ "error": e.to_string() })]),
    }
}

async fn get_tag(Path(id): Path<String>, State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let tags = state.tags.read().unwrap();
    if let Some(tag) = tags.get(&id) {
        Json(json!(tag))
    } else {
        Json(json!({ "error": "Tag not found" }))
    }
}

async fn send_command(
    Path(agent_id): Path<String>,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<serde_json::Value>,
) -> impl IntoResponse {
    let topic = format!("scada/cmd/{}", agent_id);
    let payload_str = payload.to_string();

    match state.mqtt_client.publish(&topic, &payload_str, false).await {
        Ok(_) => Json(json!({ "status": "Command sent" })),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

async fn sse_handler(
    State(state): State<Arc<AppState>>,
) -> Sse<impl Stream<Item = Result<Event, axum::Error>>> {
    let rx = state.tx.subscribe();
    let stream = BroadcastStream::new(rx).map(|msg| match msg {
        Ok(event) => Event::default()
            .json_data(event)
            .map_err(|_| axum::Error::new("Serialization error")),
        Err(_) => Ok(Event::default().comment("keep-alive")),
    });

    Sse::new(stream)
        .keep_alive(axum::response::sse::KeepAlive::new().interval(Duration::from_secs(15)))
}

#[derive(serde::Deserialize)]
struct Pagination {
    limit: Option<i64>,
    offset: Option<i64>,
}

async fn get_reports(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(pagination): axum::extract::Query<Pagination>,
) -> impl IntoResponse {
    let limit = pagination.limit.unwrap_or(20);
    let offset = pagination.offset.unwrap_or(0);

    let reports = sqlx::query!(
        r#"
        SELECT id, report_id, agent_id, start_time, end_time, total_value, created_at
        FROM reports
        ORDER BY created_at DESC
        LIMIT $1 OFFSET $2
        "#,
        limit,
        offset
    )
    .fetch_all(&state.pool)
    .await;

    match reports {
        Ok(list) => {
            let reports_json: Vec<_> = list
                .iter()
                .map(|r| {
                    json!({
                        "id": r.id,
                        "report_id": r.report_id,
                        "agent_id": r.agent_id,
                        "start_time": r.start_time,
                        "end_time": r.end_time,
                        "total_value": r.total_value,
                        "created_at": r.created_at
                    })
                })
                .collect();
            Json(json!(reports_json))
        }
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

async fn get_report_details(
    Path(id): Path<sqlx::types::Uuid>,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let report = sqlx::query!(
        r#"
        SELECT id, report_id, agent_id, start_time, end_time, total_value FROM reports WHERE id = $1
        "#,
        id
    )
    .fetch_optional(&state.pool)
    .await;

    match report {
        Ok(Some(r)) => {
            let items = sqlx::query!(
                r#"
                SELECT value, timestamp FROM report_items
                WHERE report_id = $1
                ORDER BY timestamp ASC
                "#,
                id
            )
            .fetch_all(&state.pool)
            .await;

            let items_json = match items {
                Ok(ilist) => json!(
                    ilist
                        .iter()
                        .map(|i| {
                            json!({
                                "value": i.value,
                                "timestamp": i.timestamp
                            })
                        })
                        .collect::<Vec<_>>()
                ),
                Err(_) => json!([]),
            };

            Json(json!({
                "id": r.id,
                "report_id": r.report_id,
                "agent_id": r.agent_id,
                "start_time": r.start_time,
                "end_time": r.end_time,
                "total_value": r.total_value,
                "items": items_json
            }))
        }
        Ok(None) => Json(json!({ "error": "Report not found" })),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

async fn reprint_report(
    Path(id): Path<sqlx::types::Uuid>,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    // Get report_id and agent via join with devices
    let report = sqlx::query!("SELECT report_id, agent_id FROM reports WHERE id = $1", id)
        .fetch_optional(&state.pool)
        .await;

    match report {
        Ok(Some(r)) => {
            let topic = format!("scada/cmd/{}", r.agent_id);
            let payload = json!({
                "type": "ReprintReport",
                "report_id": r.report_id
            });

            match state
                .mqtt_client
                .publish(&topic, &payload.to_string(), false)
                .await
            {
                Ok(_) => Json(json!({ "status": "Reprint command sent" })),
                Err(e) => Json(json!({ "error": e.to_string() })),
            }
        }
        _ => Json(json!({ "error": "Report not found" })),
    }
}

#[derive(serde::Deserialize)]
struct HistoryQuery {
    limit: Option<i64>,
    offset: Option<i64>,
    start: Option<String>,
    end: Option<String>,
}

async fn get_tag_history(
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
    axum::extract::Query(query): axum::extract::Query<HistoryQuery>,
) -> impl IntoResponse {
    let limit = query.limit.unwrap_or(100);
    let offset = query.offset.unwrap_or(0);

    // Common struct to unify return types from different sqlx macros
    struct HistoryRow {
        id: i64,
        value: serde_json::Value,
        quality: String,
        timestamp: time::OffsetDateTime,
        created_at: Option<time::OffsetDateTime>,
    }

    let history_result: Result<Vec<HistoryRow>, _> =
        if let (Some(start), Some(end)) = (&query.start, &query.end) {
            sqlx::query!(
                r#"
            SELECT id, value, quality, timestamp, created_at
            FROM tag_events
            WHERE tag_id = $1 AND timestamp >= $4::timestamptz AND timestamp <= $5::timestamptz
            ORDER BY timestamp ASC
            LIMIT $2 OFFSET $3
            "#,
                id,
                limit,
                offset,
                start as &String,
                end as &String
            )
            .fetch_all(&state.pool)
            .await
            .map(|rows| {
                rows.into_iter()
                    .map(|r| HistoryRow {
                        id: r.id,
                        value: r.value,
                        quality: r.quality,
                        timestamp: r.timestamp,
                        created_at: r.created_at,
                    })
                    .collect()
            })
        } else if let Some(start) = &query.start {
            sqlx::query!(
                r#"
            SELECT id, value, quality, timestamp, created_at
            FROM tag_events
            WHERE tag_id = $1 AND timestamp >= $4::timestamptz
            ORDER BY timestamp ASC
            LIMIT $2 OFFSET $3
            "#,
                id,
                limit,
                offset,
                start as &String
            )
            .fetch_all(&state.pool)
            .await
            .map(|rows| {
                rows.into_iter()
                    .map(|r| HistoryRow {
                        id: r.id,
                        value: r.value,
                        quality: r.quality,
                        timestamp: r.timestamp,
                        created_at: r.created_at,
                    })
                    .collect()
            })
        } else {
            sqlx::query!(
                r#"
            SELECT id, value, quality, timestamp, created_at
            FROM tag_events
            WHERE tag_id = $1
            ORDER BY timestamp DESC
            LIMIT $2 OFFSET $3
            "#,
                id,
                limit,
                offset
            )
            .fetch_all(&state.pool)
            .await
            .map(|rows| {
                rows.into_iter()
                    .map(|r| HistoryRow {
                        id: r.id,
                        value: r.value,
                        quality: r.quality,
                        timestamp: r.timestamp,
                        created_at: r.created_at,
                    })
                    .collect()
            })
        };

    match history_result {
        Ok(list) => {
            let history_json: Vec<_> = list
                .iter()
                .map(|r| {
                    json!({
                        "id": r.id,
                        "value": r.value,
                        "quality": r.quality,
                        "timestamp": r.timestamp,
                        "created_at": r.created_at
                    })
                })
                .collect();
            Json(json!(history_json))
        }
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

#[derive(serde::Deserialize)]
struct BatchPrintRequest {
    event_ids: Vec<i64>,
}

async fn batch_print_events(
    State(state): State<Arc<AppState>>,
    Json(req): Json<BatchPrintRequest>,
) -> impl IntoResponse {
    if req.event_ids.is_empty() {
        return Json(json!({ "error": "No event IDs provided" }));
    }

    // Fetch events and join tags->devices to get edge_agent_id
    // Use ! to force non-null if sqlx is over-cautious
    let events = sqlx::query!(
        r#"
        SELECT e.value as "value!", e.timestamp, d.edge_agent_id, t.id as tag_id
        FROM tag_events e
        JOIN tags t ON e.tag_id = t.id
        JOIN devices d ON t.device_id = d.id
        WHERE e.id = ANY($1)
        ORDER BY e.timestamp ASC
        "#,
        &req.event_ids
    )
    .fetch_all(&state.pool)
    .await;

    match events {
        Ok(rows) if !rows.is_empty() => {
            let agent_id = &rows[0].edge_agent_id;
            let topic = format!("scada/cmd/{}", agent_id);

            let items: Vec<_> = rows
                .iter()
                .map(|r| {
                    let val = match &r.value {
                        v if v.is_number() => v.as_f64().unwrap_or(0.0),
                        v if v.is_object() => {
                            v.get("value").and_then(|v| v.as_f64()).unwrap_or(0.0)
                        }
                        _ => 0.0,
                    };
                    let unit = if let Some(obj) = r.value.as_object() {
                        obj.get("unit")
                            .and_then(|v| v.as_str())
                            .unwrap_or("kg")
                            .to_string()
                    } else {
                        "kg".to_string()
                    };

                    let ts_str = r
                        .timestamp
                        .format(&time::format_description::well_known::Rfc3339)
                        .unwrap_or_else(|_| r.timestamp.to_string());
                    json!({
                        "value": val,
                        "unit": unit,
                        "timestamp": ts_str
                    })
                })
                .collect::<Vec<_>>();

            let payload = json!({
                "type": "PrintBatchManual",
                "tag_id": rows[0].tag_id,
                "items": items
            });

            match state
                .mqtt_client
                .publish(&topic, &payload.to_string(), false)
                .await
            {
                Ok(_) => {
                    Json(json!({ "status": "Batch print command sent", "count": items.len() }))
                }
                Err(e) => Json(json!({ "error": e.to_string() })),
            }
        }
        Ok(_) => Json(json!({ "error": "No events found for given IDs" })),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

//! Contract tests for the out-of-band control channel.
//!
//! Follows the house pattern: skipped when `CENTRAL_PG_DSN` is unset, because these need a
//! real Postgres to exercise the queue.

use central_server::api::{run_api_server, ApiState, EdgeConfigSettings};
use central_server::edge_control::EdgeWaiters;
use std::net::TcpListener;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio_postgres::{Client, NoTls};

const TOKEN: &str = "test-token";

async fn connect_pg(dsn: &str) -> Client {
    let (client, connection) = tokio_postgres::connect(dsn, NoTls).await.expect("pg connect");
    tokio::spawn(async move {
        let _ = connection.await;
    });
    client
}

async fn run_migrations(client: &Client) {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    client
        .batch_execute("SELECT pg_advisory_lock(86234106);")
        .await
        .expect("migration lock");
    for file in [
        "migrations/0001_core_postgres.sql",
        "migrations/0020_edge_control_command.sql",
    ] {
        let sql = std::fs::read_to_string(base.join(file)).expect("read migration file");
        client.batch_execute(&sql).await.expect("apply migration");
    }
    client
        .batch_execute("SELECT pg_advisory_unlock(86234106);")
        .await
        .expect("migration unlock");
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("bind")
        .local_addr()
        .expect("addr")
        .port()
}

async fn start_server(client: Client, control_wait: Duration) -> String {
    let port = free_port();
    let bind = format!("127.0.0.1:{}", port);
    let state = ApiState {
        client: Arc::new(client),
        edge_cfg: EdgeConfigSettings {
            enroll_token: TOKEN.to_string(),
            signing_secret: "test-secret".to_string(),
            signing_key_id: "v1".to_string(),
            runtime_config_path: "crates/edge-agent/config/bootstrap.example.json".to_string(),
        },
        mqtt_cmd: None,
        waiters: EdgeWaiters::new(),
        control_wait,
    };
    let serve_bind = bind.clone();
    tokio::spawn(async move {
        let _ = run_api_server(state, &serve_bind).await;
    });

    let base = format!("http://{}", bind);
    let http = reqwest::Client::new();
    for _ in 0..100 {
        if http
            .get(format!("{}/health/live", base))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
        {
            return base;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("api server never became healthy");
}

/// Every test uses its own edge code so they can run against a shared database without
/// seeing each other's orders.
fn unique_edge(prefix: &str) -> String {
    format!(
        "{}-{}",
        prefix,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    )
}

#[tokio::test]
async fn a_queued_restart_is_handed_to_the_supervisor_and_then_acknowledged() {
    let Ok(dsn) = std::env::var("CENTRAL_PG_DSN") else {
        return;
    };
    let db = connect_pg(&dsn).await;
    run_migrations(&db).await;
    let base = start_server(connect_pg(&dsn).await, Duration::from_millis(300)).await;
    let http = reqwest::Client::new();
    let edge = unique_edge("ctl-happy");

    // The operator presses the button.
    let reset: serde_json::Value = http
        .post(format!("{}/api/edges/reset", base))
        .json(&serde_json::json!({
            "site_code": "plant-a",
            "edge_code": edge,
            "reason": "colgado",
            "operator": "mathias"
        }))
        .send()
        .await
        .expect("reset failed")
        .json()
        .await
        .expect("reset body");
    assert_eq!(reset["accepted"], true);
    assert!(
        reset.get("topic").is_none(),
        "topic named an MQTT topic that is no longer published"
    );
    let request_id = reset["request_id"].as_str().expect("request_id").to_string();

    // The supervisor asks and gets it straight away, without waiting out the window.
    let started = Instant::now();
    let pending: serde_json::Value = http
        .post(format!("{}/api/edge/control/pending", base))
        .json(&serde_json::json!({ "edge_id": edge, "enrollment_token": TOKEN }))
        .send()
        .await
        .expect("pending failed")
        .json()
        .await
        .expect("pending body");
    assert_eq!(pending["kind"], "restart");
    assert_eq!(pending["request_id"], serde_json::json!(request_id));
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "an order already queued must not be made to wait"
    );

    let delivered: Option<std::time::SystemTime> = db
        .query_one(
            "SELECT delivered_at FROM edge_control_command WHERE edge_code=$1 AND request_id=$2",
            &[&edge, &request_id],
        )
        .await
        .expect("read delivered_at")
        .get(0);
    assert!(
        delivered.is_some(),
        "delivered_at distinguishes 'never asked' from 'asked and did not confirm'"
    );

    // The supervisor confirms, and the order stops being handed back.
    let ack = http
        .post(format!("{}/api/edge/control/ack", base))
        .json(&serde_json::json!({
            "edge_id": edge, "enrollment_token": TOKEN, "request_id": request_id
        }))
        .send()
        .await
        .expect("ack failed");
    assert!(ack.status().is_success());

    let after: serde_json::Value = http
        .post(format!("{}/api/edge/control/pending", base))
        .json(&serde_json::json!({ "edge_id": edge, "enrollment_token": TOKEN }))
        .send()
        .await
        .expect("second pending failed")
        .json()
        .await
        .expect("second pending body");
    assert!(
        after.get("request_id").is_none(),
        "a confirmed order must not be handed out again, got {}",
        after
    );
}

/// A supervisor that dies mid-restart leaves the order delivered but unconfirmed. It has
/// to come back: one restart too many is a nuisance, one too few is a fault.
#[tokio::test]
async fn an_unconfirmed_order_is_handed_out_again() {
    let Ok(dsn) = std::env::var("CENTRAL_PG_DSN") else {
        return;
    };
    let db = connect_pg(&dsn).await;
    run_migrations(&db).await;
    let base = start_server(connect_pg(&dsn).await, Duration::from_millis(300)).await;
    let http = reqwest::Client::new();
    let edge = unique_edge("ctl-redeliver");

    http.post(format!("{}/api/edges/reset", base))
        .json(&serde_json::json!({ "site_code": "plant-a", "edge_code": edge }))
        .send()
        .await
        .expect("reset failed");

    let first: serde_json::Value = http
        .post(format!("{}/api/edge/control/pending", base))
        .json(&serde_json::json!({ "edge_id": edge, "enrollment_token": TOKEN }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let second: serde_json::Value = http
        .post(format!("{}/api/edge/control/pending", base))
        .json(&serde_json::json!({ "edge_id": edge, "enrollment_token": TOKEN }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    assert_eq!(
        first["request_id"], second["request_id"],
        "an order nobody confirmed must still be pending"
    );
}

#[tokio::test]
async fn orders_are_served_oldest_first() {
    let Ok(dsn) = std::env::var("CENTRAL_PG_DSN") else {
        return;
    };
    let db = connect_pg(&dsn).await;
    run_migrations(&db).await;
    let base = start_server(connect_pg(&dsn).await, Duration::from_millis(300)).await;
    let http = reqwest::Client::new();
    let edge = unique_edge("ctl-order");

    for id in ["first", "second"] {
        http.post(format!("{}/api/edges/reset", base))
            .json(&serde_json::json!({
                "site_code": "plant-a", "edge_code": edge, "request_id": id
            }))
            .send()
            .await
            .expect("reset failed");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let pending: serde_json::Value = http
        .post(format!("{}/api/edge/control/pending", base))
        .json(&serde_json::json!({ "edge_id": edge, "enrollment_token": TOKEN }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(pending["request_id"], "first");
    let _ = db;
}

/// Nothing to do must not keep a supervisor's request forever, and must not look like a
/// failure either.
#[tokio::test]
async fn a_quiet_window_answers_empty_within_the_wait() {
    let Ok(dsn) = std::env::var("CENTRAL_PG_DSN") else {
        return;
    };
    let db = connect_pg(&dsn).await;
    run_migrations(&db).await;
    let base = start_server(connect_pg(&dsn).await, Duration::from_millis(300)).await;
    let http = reqwest::Client::new();
    let edge = unique_edge("ctl-quiet");

    let started = Instant::now();
    let resp = http
        .post(format!("{}/api/edge/control/pending", base))
        .json(&serde_json::json!({ "edge_id": edge, "enrollment_token": TOKEN }))
        .send()
        .await
        .expect("pending failed");

    assert!(resp.status().is_success(), "silence is not an error");
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body.get("request_id").is_none());
    assert!(
        started.elapsed() >= Duration::from_millis(250),
        "the request must actually be held, not answered instantly"
    );
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "the wait must expire on its own"
    );
    let _ = db;
}

#[tokio::test]
async fn the_control_endpoints_reject_a_wrong_token() {
    let Ok(dsn) = std::env::var("CENTRAL_PG_DSN") else {
        return;
    };
    let db = connect_pg(&dsn).await;
    run_migrations(&db).await;
    let base = start_server(connect_pg(&dsn).await, Duration::from_millis(300)).await;
    let http = reqwest::Client::new();
    let edge = unique_edge("ctl-auth");

    let pending = http
        .post(format!("{}/api/edge/control/pending", base))
        .json(&serde_json::json!({ "edge_id": edge, "enrollment_token": "wrong" }))
        .send()
        .await
        .expect("pending failed");
    assert_eq!(pending.status().as_u16(), 401);

    let ack = http
        .post(format!("{}/api/edge/control/ack", base))
        .json(&serde_json::json!({
            "edge_id": edge, "enrollment_token": "wrong", "request_id": "x"
        }))
        .send()
        .await
        .expect("ack failed");
    assert_eq!(ack.status().as_u16(), 401);
    let _ = db;
}

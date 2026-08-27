use central_server::api::{ApiState, EdgeConfigSettings, run_api_server};
use futures_util::StreamExt;
use std::net::TcpListener;
use std::sync::Arc;
use tokio_postgres::{Client, NoTls};

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
        .batch_execute("SELECT pg_advisory_lock(86234107);")
        .await
        .expect("migration lock");
    for file in [
        "migrations/0001_core_postgres.sql",
        "migrations/0003_tag_naming_governance.sql",
        "migrations/0005_fix_tag_naming_constraint_regex.sql",
    ] {
        let sql = std::fs::read_to_string(base.join(file)).expect("read migration file");
        client.batch_execute(&sql).await.expect("apply migration");
    }
    client
        .batch_execute("SELECT pg_advisory_unlock(86234107);")
        .await
        .expect("migration unlock");
}

async fn seed_min_catalog(
    client: &Client,
    site: &str,
    edge: &str,
    device: &str,
    tag_compound: &str,
    tag_raw: &str,
) {
    let site_seg: String = site.to_ascii_uppercase().chars().take(12).collect();

    client
        .execute(
            "INSERT INTO sites(code,name,timezone) VALUES ($1,$2,'UTC')
             ON CONFLICT (code) DO NOTHING",
            &[&site, &format!("Site {}", site)],
        )
        .await
        .expect("site");
    client
        .execute(
            "INSERT INTO edges(site_id,edge_code,name,status)
             SELECT id,$2,$3,'online' FROM sites WHERE code=$1
             ON CONFLICT (site_id, edge_code) DO NOTHING",
            &[&site, &edge, &format!("Edge {}", edge)],
        )
        .await
        .expect("edge");
    client
        .execute(
            "INSERT INTO devices(edge_id,device_code,name,driver_type)
             SELECT e.id,$3,$4,'SerialAscii'
             FROM edges e
             JOIN sites s ON s.id=e.site_id
             WHERE s.code=$1 AND e.edge_code=$2
             ON CONFLICT (edge_id, device_code) DO NOTHING",
            &[&site, &edge, &device, &format!("Device {}", device)],
        )
        .await
        .expect("device");

    client
        .execute(
            "INSERT INTO tags(device_id,tag_code,name,value_type,source,metadata_json,tag_code_canonical,display_name,aliases_json)
             SELECT d.id,$4,$5,'string','scale:compound','{}'::jsonb,$6,$5,'[]'::jsonb
             FROM devices d
             JOIN edges e ON e.id=d.edge_id
             JOIN sites s ON s.id=e.site_id
             WHERE s.code=$1 AND e.edge_code=$2 AND d.device_code=$3
             ON CONFLICT (device_id, tag_code) DO NOTHING",
            &[
                &site,
                &edge,
                &device,
                &tag_compound,
                &format!("Tag {}", tag_compound),
                &format!(
                    "{}.AREA1.UN01.DEV001.SIG01.PV",
                    site_seg
                ),
            ],
        )
        .await
        .expect("tag compound");
    client
        .execute(
            "INSERT INTO tags(device_id,tag_code,name,value_type,source,metadata_json,tag_code_canonical,display_name,aliases_json)
             SELECT d.id,$4,$5,'string','scale:raw','{}'::jsonb,$6,$5,'[]'::jsonb
             FROM devices d
             JOIN edges e ON e.id=d.edge_id
             JOIN sites s ON s.id=e.site_id
             WHERE s.code=$1 AND e.edge_code=$2 AND d.device_code=$3
             ON CONFLICT (device_id, tag_code) DO NOTHING",
            &[
                &site,
                &edge,
                &device,
                &tag_raw,
                &format!("Tag {}", tag_raw),
                &format!(
                    "{}.AREA1.UN01.DEV001.SIG01.RAW",
                    site_seg
                ),
            ],
        )
        .await
        .expect("tag raw");
}

async fn insert_telem(client: &Client, site: &str, edge: &str, tag: &str, value: &str) {
    insert_telem_at(client, site, edge, tag, value, chrono::Utc::now()).await;
}

async fn insert_telem_at(
    client: &Client,
    site: &str,
    edge: &str,
    tag: &str,
    value: &str,
    ts: chrono::DateTime<chrono::Utc>,
) {
    let payload = serde_json::json!({
        "schema_version": 1,
        "source": "edge/edge-01",
        "tag_id": tag,
        "value": value,
        "quality": { "status":"Good", "reason": null },
        "timestamp": ts,
    });
    client
        .execute(
            "INSERT INTO telemetry_ingest_events(site_code, edge_code, tag_code, quality_status, value_json, payload_json, ts)
             VALUES ($1,$2,$3,'Good',$4::jsonb,$5::jsonb, $6)",
            &[&site, &edge, &tag, &serde_json::json!(value), &payload, &ts],
        )
        .await
        .expect("insert telemetry");
}

async fn spawn_test_server(read_client: Client) -> (String, tokio::task::JoinHandle<()>) {
    let port = free_port();
    let bind = format!("127.0.0.1:{}", port);
    let state = ApiState {
        client: Arc::new(read_client),
        edge_cfg: EdgeConfigSettings {
            enroll_token: "test-token".to_string(),
            signing_secret: "test-secret".to_string(),
            signing_key_id: "v1".to_string(),
            runtime_config_path: "crates/edge-agent/config/bootstrap.example.json".to_string(),
        },
        mqtt_cmd: None,
    };
    let server = tokio::spawn(async move {
        let _ = run_api_server(state, &bind).await;
    });
    let base = format!("http://127.0.0.1:{}", port);
    wait_health(&base).await;
    (base, server)
}

fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    listener.local_addr().expect("addr").port()
}

async fn wait_health(base: &str) {
    let client = reqwest::Client::new();
    for _ in 0..50 {
        if let Ok(resp) = client.get(format!("{}/health/live", base)).send().await {
            if resp.status().is_success() {
                return;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    }
    panic!("api health timeout");
}

async fn read_one_sse_data(url: &str, timeout_ms: u64) -> Option<String> {
    let resp = reqwest::Client::new()
        .get(url)
        .send()
        .await
        .expect("sse open");
    let mut stream = resp.bytes_stream();
    let mut buf = String::new();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
    loop {
        if tokio::time::Instant::now() > deadline {
            return None;
        }
        let next = match tokio::time::timeout(std::time::Duration::from_millis(300), stream.next()).await {
            Ok(v) => v,
            Err(_) => continue,
        };
        let chunk = match next {
            Some(Ok(c)) => c,
            Some(Err(_)) => continue,
            None => return None,
        };
        if chunk.is_empty() {
            continue;
        };
        buf.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(split_idx) = buf.find("\n\n") {
            let block = buf[..split_idx].to_string();
            buf = buf[(split_idx + 2)..].to_string();
            if let Some(line) = block.lines().find(|l| l.starts_with("data: ")) {
                return Some(line.trim_start_matches("data: ").to_string());
            }
        }
    }
}

#[tokio::test]
async fn sse_default_excludes_raw_and_no_replay() {
    let dsn = match std::env::var("CENTRAL_PG_DSN") {
        Ok(v) => v,
        Err(_) => return,
    };
    let write_client = connect_pg(&dsn).await;
    run_migrations(&write_client).await;

    let site = format!("t{}", chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0));
    let edge = "edge-test";
    let device = "dev-test";
    let tag_compound = "tag_contract_compound";
    let tag_raw = "tag_contract_raw";
    seed_min_catalog(&write_client, &site, edge, device, tag_compound, tag_raw).await;

    // Historical events before SSE connect.
    insert_telem(&write_client, &site, edge, tag_compound, "{\"value\":12.3,\"unit\":\"g\"}").await;
    insert_telem(&write_client, &site, edge, tag_raw, "+ 12.3000 g").await;

    let read_client = connect_pg(&dsn).await;
    let port = free_port();
    let bind = format!("127.0.0.1:{}", port);
    let state = ApiState {
        client: Arc::new(read_client),
        edge_cfg: EdgeConfigSettings {
            enroll_token: "test-token".to_string(),
            signing_secret: "test-secret".to_string(),
            signing_key_id: "v1".to_string(),
            runtime_config_path: "crates/edge-agent/config/bootstrap.example.json".to_string(),
        },
        mqtt_cmd: None,
    };
    let server = tokio::spawn(async move {
        let _ = run_api_server(state, &bind).await;
    });
    let base = format!("http://127.0.0.1:{}", port);
    wait_health(&base).await;

    // Default is replay=false, so no historical event should be emitted immediately.
    let sse_url = format!("{}/api/stream/events?site={}&edge={}&exclude_raw=true", base, site, edge);
    let first = read_one_sse_data(&sse_url, 1200).await;
    assert!(first.is_none(), "expected no historical replay by default");

    // New events after subscribe: raw should be excluded.
    let sse_url_live = sse_url.clone();
    let live_fut = tokio::spawn(async move { read_one_sse_data(&sse_url_live, 3000).await });
    tokio::time::sleep(std::time::Duration::from_millis(120)).await;
    insert_telem(&write_client, &site, edge, tag_raw, "+ 13.0000 g").await;
    insert_telem(
        &write_client,
        &site,
        edge,
        tag_compound,
        "{\"value\":13.0,\"unit\":\"g\"}",
    )
    .await;
    let live = live_fut
        .await
        .expect("sse join")
        .expect("expected live event");
    assert!(live.contains("\"tag_id\":\"tag_contract_compound\""));
    assert!(!live.contains("\"tag_id\":\"tag_contract_raw\""));

    server.abort();
}

#[tokio::test]
async fn sse_replay_true_returns_history() {
    let dsn = match std::env::var("CENTRAL_PG_DSN") {
        Ok(v) => v,
        Err(_) => return,
    };
    let client = connect_pg(&dsn).await;
    run_migrations(&client).await;

    let site = format!("r{}", chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0));
    let edge = "edge-test";
    let device = "dev-test";
    let tag_compound = "tag_replay_compound";
    let tag_raw = "tag_replay_raw";
    seed_min_catalog(&client, &site, edge, device, tag_compound, tag_raw).await;
    insert_telem(&client, &site, edge, tag_compound, "{\"value\":22.0,\"unit\":\"g\"}").await;

    let read_client = connect_pg(&dsn).await;
    let port = free_port();
    let bind = format!("127.0.0.1:{}", port);
    let state = ApiState {
        client: Arc::new(read_client),
        edge_cfg: EdgeConfigSettings {
            enroll_token: "test-token".to_string(),
            signing_secret: "test-secret".to_string(),
            signing_key_id: "v1".to_string(),
            runtime_config_path: "crates/edge-agent/config/bootstrap.example.json".to_string(),
        },
        mqtt_cmd: None,
    };
    let server = tokio::spawn(async move {
        let _ = run_api_server(state, &bind).await;
    });
    let base = format!("http://127.0.0.1:{}", port);
    wait_health(&base).await;

    let sse_url = format!(
        "{}/api/stream/events?site={}&edge={}&replay=true&exclude_raw=true",
        base, site, edge
    );
    let replayed = read_one_sse_data(&sse_url, 3000).await.expect("expected replay event");
    assert!(replayed.contains("\"tag_id\":\"tag_replay_compound\""));

    server.abort();
}

#[tokio::test]
async fn history_filters_by_date_range() {
    let dsn = match std::env::var("CENTRAL_PG_DSN") {
        Ok(v) => v,
        Err(_) => return,
    };
    let write_client = connect_pg(&dsn).await;
    run_migrations(&write_client).await;

    let site = format!("h{}", chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0));
    let edge = "edge-test";
    let device = "dev-test";
    let tag_compound = format!("tag_history_range_compound_{}", site);
    let tag_raw = format!("tag_history_range_raw_{}", site);
    seed_min_catalog(&write_client, &site, edge, device, &tag_compound, &tag_raw).await;

    let in_range: chrono::DateTime<chrono::Utc> = "2026-08-05T10:00:00Z".parse().unwrap();
    let out_of_range: chrono::DateTime<chrono::Utc> = "2026-08-01T10:00:00Z".parse().unwrap();
    insert_telem_at(&write_client, &site, edge, &tag_compound, "{\"value\":41.0,\"unit\":\"g\"}", in_range).await;
    insert_telem_at(&write_client, &site, edge, &tag_compound, "{\"value\":40.0,\"unit\":\"g\"}", out_of_range).await;

    let read_client = connect_pg(&dsn).await;
    let (base, server) = spawn_test_server(read_client).await;

    let url = format!(
        "{}/api/tags/{}/history?from=2026-08-05T00:00:00Z&to=2026-08-06T00:00:00Z",
        base, tag_compound
    );
    let body: serde_json::Value = reqwest::Client::new()
        .get(&url)
        .send()
        .await
        .expect("history request")
        .json()
        .await
        .expect("history json");
    let rows = body.as_array().expect("array response");
    assert_eq!(rows.len(), 1, "expected exactly one row inside the range, got {:?}", rows);
    assert_eq!(rows[0]["ts"], serde_json::json!(in_range));

    server.abort();
}

#[tokio::test]
async fn history_date_range_with_no_matches_returns_empty() {
    let dsn = match std::env::var("CENTRAL_PG_DSN") {
        Ok(v) => v,
        Err(_) => return,
    };
    let write_client = connect_pg(&dsn).await;
    run_migrations(&write_client).await;

    let site = format!("h{}", chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0));
    let edge = "edge-test";
    let device = "dev-test";
    let tag_compound = format!("tag_history_empty_compound_{}", site);
    let tag_raw = format!("tag_history_empty_raw_{}", site);
    seed_min_catalog(&write_client, &site, edge, device, &tag_compound, &tag_raw).await;

    let sample_ts: chrono::DateTime<chrono::Utc> = "2026-08-05T10:00:00Z".parse().unwrap();
    insert_telem_at(&write_client, &site, edge, &tag_compound, "{\"value\":41.0,\"unit\":\"g\"}", sample_ts).await;

    let read_client = connect_pg(&dsn).await;
    let (base, server) = spawn_test_server(read_client).await;

    let url = format!(
        "{}/api/tags/{}/history?from=2025-01-01T00:00:00Z&to=2025-01-02T00:00:00Z",
        base, tag_compound
    );
    let body: serde_json::Value = reqwest::Client::new()
        .get(&url)
        .send()
        .await
        .expect("history request")
        .json()
        .await
        .expect("history json");
    let rows = body.as_array().expect("array response");
    assert_eq!(rows.len(), 0, "expected no rows outside the range, got {:?}", rows);

    server.abort();
}

#[tokio::test]
async fn history_without_date_range_returns_all_recent() {
    let dsn = match std::env::var("CENTRAL_PG_DSN") {
        Ok(v) => v,
        Err(_) => return,
    };
    let write_client = connect_pg(&dsn).await;
    run_migrations(&write_client).await;

    let site = format!("h{}", chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0));
    let edge = "edge-test";
    let device = "dev-test";
    let tag_compound = format!("tag_history_regression_compound_{}", site);
    let tag_raw = format!("tag_history_regression_raw_{}", site);
    seed_min_catalog(&write_client, &site, edge, device, &tag_compound, &tag_raw).await;

    let old_ts: chrono::DateTime<chrono::Utc> = "2026-08-01T10:00:00Z".parse().unwrap();
    let recent_ts: chrono::DateTime<chrono::Utc> = "2026-08-05T10:00:00Z".parse().unwrap();
    insert_telem_at(&write_client, &site, edge, &tag_compound, "{\"value\":40.0,\"unit\":\"g\"}", old_ts).await;
    insert_telem_at(&write_client, &site, edge, &tag_compound, "{\"value\":41.0,\"unit\":\"g\"}", recent_ts).await;

    let read_client = connect_pg(&dsn).await;
    let (base, server) = spawn_test_server(read_client).await;

    let url = format!("{}/api/tags/{}/history", base, tag_compound);
    let body: serde_json::Value = reqwest::Client::new()
        .get(&url)
        .send()
        .await
        .expect("history request")
        .json()
        .await
        .expect("history json");
    let rows = body.as_array().expect("array response");
    assert_eq!(rows.len(), 2, "expected both rows without a date filter, got {:?}", rows);
    // ORDER BY ts DESC: most recent first.
    assert_eq!(rows[0]["ts"], serde_json::json!(recent_ts));
    assert_eq!(rows[1]["ts"], serde_json::json!(old_ts));

    server.abort();
}

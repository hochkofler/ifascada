use central_server::api::{ApiState, EdgeConfigSettings, run_api_server};
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
        .batch_execute("SELECT pg_advisory_lock(86234101);")
        .await
        .expect("migration lock");
    for file in [
        "migrations/0001_core_postgres.sql",
        "migrations/0003_tag_naming_governance.sql",
        "migrations/0005_fix_tag_naming_constraint_regex.sql",
        "migrations/0006_context_hierarchy.sql",
        "migrations/0009_operational_events.sql",
        "migrations/0010_connection_domain_state.sql",
        "migrations/0011_device_domain_state.sql",
        "migrations/0012_edges_metadata_json.sql",
        "migrations/0016_telemetry_received_at.sql",
    ] {
        let sql = std::fs::read_to_string(base.join(file)).expect("read migration file");
        client.batch_execute(&sql).await.expect("apply migration");
    }
    client
        .batch_execute("SELECT pg_advisory_unlock(86234101);")
        .await
        .expect("migration unlock");
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

async fn start_api_server(dsn: &str) -> (tokio::task::JoinHandle<()>, String) {
    let read_client = connect_pg(dsn).await;
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
    (server, base)
}

async fn seed_site_edge(client: &Client, site: &str, edge: &str) {
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
}

#[tokio::test]
async fn edges_current_marks_disconnected_when_heartbeat_expired() {
    let dsn = match std::env::var("CENTRAL_PG_DSN") {
        Ok(v) => v,
        Err(_) => return,
    };
    let client = connect_pg(&dsn).await;
    run_migrations(&client).await;

    let nonce = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);
    let site = format!("hb-site-{}", nonce);
    let edge = format!("hb-edge-{}", nonce);
    seed_site_edge(&client, &site, &edge).await;

    client
        .execute(
            "INSERT INTO edge_current_state(edge_id,status,last_seen_at,outbox_depth,outbox_oldest_secs,updated_at)
             SELECT e.id,'online', NOW() - INTERVAL '120 seconds', 0, NULL, NOW()
             FROM edges e
             JOIN sites s ON s.id = e.site_id
             WHERE s.code = $1 AND e.edge_code = $2
             ON CONFLICT (edge_id) DO UPDATE
             SET status = EXCLUDED.status,
                 last_seen_at = EXCLUDED.last_seen_at,
                 updated_at = NOW()",
            &[&site, &edge],
        )
        .await
        .expect("edge_current_state");

    let (server, base) = start_api_server(&dsn).await;
    let rows: serde_json::Value = reqwest::Client::new()
        .get(format!(
            "{}/api/edges/current?site={}&edge={}&limit=10",
            base, site, edge
        ))
        .send()
        .await
        .expect("edges current response")
        .json()
        .await
        .expect("edges current json");
    let items = rows.as_array().expect("array response");
    assert_eq!(items.len(), 1);
    assert_eq!(
        items[0].get("status").and_then(|v| v.as_str()),
        Some("disconnected")
    );
    server.abort();
}

#[tokio::test]
async fn devices_current_marks_disconnected_when_edge_heartbeat_expired() {
    let dsn = match std::env::var("CENTRAL_PG_DSN") {
        Ok(v) => v,
        Err(_) => return,
    };
    let client = connect_pg(&dsn).await;
    run_migrations(&client).await;

    let nonce = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);
    let site = format!("hbd-site-{}", nonce);
    let edge = format!("hbd-edge-{}", nonce);
    seed_site_edge(&client, &site, &edge).await;

    client
        .execute(
            "INSERT INTO devices(edge_id,device_code,name,driver_type,metadata_json)
             SELECT e.id,'dev_hb_1','Device HB 1','Simulator','{}'::jsonb
             FROM edges e
             JOIN sites s ON s.id = e.site_id
             WHERE s.code = $1 AND e.edge_code = $2
             ON CONFLICT (edge_id, device_code) DO NOTHING",
            &[&site, &edge],
        )
        .await
        .expect("device");

    client
        .execute(
            "INSERT INTO edge_current_state(edge_id,status,last_seen_at,outbox_depth,outbox_oldest_secs,updated_at)
             SELECT e.id,'online', NOW() - INTERVAL '120 seconds', 0, NULL, NOW()
             FROM edges e
             JOIN sites s ON s.id = e.site_id
             WHERE s.code = $1 AND e.edge_code = $2
             ON CONFLICT (edge_id) DO UPDATE
             SET status = EXCLUDED.status,
                 last_seen_at = EXCLUDED.last_seen_at,
                 updated_at = NOW()",
            &[&site, &edge],
        )
        .await
        .expect("edge_current_state");

    client
        .execute(
            "INSERT INTO device_current_state
             (device_id, state, severity, reason, connection_id, tags_connected, tags_stale, tags_disconnected, last_change_at, last_seen_at, payload_json, updated_at)
             SELECT d.id, 'connected', 'info', 'tag_connected', NULL, 1, 0, 0, NOW(), NOW(), '{}'::jsonb, NOW()
             FROM devices d
             JOIN edges e ON e.id = d.edge_id
             JOIN sites s ON s.id = e.site_id
             WHERE s.code = $1 AND e.edge_code = $2 AND d.device_code = 'dev_hb_1'
             ON CONFLICT (device_id) DO UPDATE
             SET state = EXCLUDED.state,
                 severity = EXCLUDED.severity,
                 reason = EXCLUDED.reason,
                 tags_connected = EXCLUDED.tags_connected,
                 tags_stale = EXCLUDED.tags_stale,
                 tags_disconnected = EXCLUDED.tags_disconnected,
                 last_change_at = EXCLUDED.last_change_at,
                 last_seen_at = EXCLUDED.last_seen_at,
                 payload_json = EXCLUDED.payload_json,
                 updated_at = NOW()",
            &[&site, &edge],
        )
        .await
        .expect("device_current_state");

    let (server, base) = start_api_server(&dsn).await;
    let rows: serde_json::Value = reqwest::Client::new()
        .get(format!(
            "{}/api/devices/current?site={}&edge={}&limit=10",
            base, site, edge
        ))
        .send()
        .await
        .expect("devices current response")
        .json()
        .await
        .expect("devices current json");
    let items = rows.as_array().expect("array response");
    assert_eq!(items.len(), 1);
    assert_eq!(
        items[0].get("state").and_then(|v| v.as_str()),
        Some("disconnected")
    );
    assert_eq!(
        items[0].get("reason").and_then(|v| v.as_str()),
        Some("edge_offline_or_stale")
    );
    assert_eq!(
        items[0].get("tags_connected").and_then(|v| v.as_i64()),
        Some(0)
    );
    assert_eq!(
        items[0].get("tags_disconnected").and_then(|v| v.as_i64()),
        Some(1)
    );
    server.abort();
}

#[tokio::test]
async fn tags_current_marks_disconnected_when_edge_heartbeat_expired() {
    let dsn = match std::env::var("CENTRAL_PG_DSN") {
        Ok(v) => v,
        Err(_) => return,
    };
    let client = connect_pg(&dsn).await;
    run_migrations(&client).await;

    let nonce = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);
    let site = format!("hbt-site-{}", nonce);
    let edge = format!("hbt-edge-{}", nonce);
    seed_site_edge(&client, &site, &edge).await;

    client
        .execute(
            "INSERT INTO devices(edge_id,device_code,name,driver_type,metadata_json)
             SELECT e.id,'dev_hb_tag_1','Device HB Tag 1','Simulator','{}'::jsonb
             FROM edges e
             JOIN sites s ON s.id = e.site_id
             WHERE s.code = $1 AND e.edge_code = $2
             ON CONFLICT (edge_id, device_code) DO NOTHING",
            &[&site, &edge],
        )
        .await
        .expect("device");

    client
        .execute(
            "INSERT INTO tags(device_id,tag_code,name,value_type,source,unit,metadata_json,tag_code_canonical,display_name,aliases_json)
             SELECT d.id,'tag_hb_001','Tag HB 001','float','sim:1','u','{}'::jsonb,
                    CONCAT(UPPER(SUBSTRING(REPLACE($1, '-', '_') FROM 1 FOR 12)), '.SC.ED.DEV1.T01.PV'),
                    'Tag HB 001','[]'::jsonb
             FROM devices d
             JOIN edges e ON e.id = d.edge_id
             JOIN sites s ON s.id = e.site_id
             WHERE s.code = $1 AND e.edge_code = $2 AND d.device_code = 'dev_hb_tag_1'
             ON CONFLICT (device_id, tag_code) DO NOTHING",
            &[&site, &edge],
        )
        .await
        .expect("tag");

    client
        .execute(
            "INSERT INTO edge_current_state(edge_id,status,last_seen_at,outbox_depth,outbox_oldest_secs,updated_at)
             SELECT e.id,'online', NOW() - INTERVAL '120 seconds', 0, NULL, NOW()
             FROM edges e
             JOIN sites s ON s.id = e.site_id
             WHERE s.code = $1 AND e.edge_code = $2
             ON CONFLICT (edge_id) DO UPDATE
             SET status = EXCLUDED.status,
                 last_seen_at = EXCLUDED.last_seen_at,
                 updated_at = NOW()",
            &[&site, &edge],
        )
        .await
        .expect("edge_current_state");

    client
        .execute(
            "INSERT INTO tag_current_state(tag_id, ts, value_json, quality_json, source, updated_at)
             SELECT t.id, NOW(), '12.34'::jsonb, '{\"status\":\"Good\",\"reason\":null}'::jsonb, 'sim', NOW()
             FROM tags t
             JOIN devices d ON d.id = t.device_id
             JOIN edges e ON e.id = d.edge_id
             JOIN sites s ON s.id = e.site_id
             WHERE s.code = $1 AND e.edge_code = $2 AND t.tag_code = 'tag_hb_001'
             ON CONFLICT (tag_id) DO UPDATE
             SET ts = EXCLUDED.ts,
                 value_json = EXCLUDED.value_json,
                 quality_json = EXCLUDED.quality_json,
                 source = EXCLUDED.source,
                 updated_at = NOW()",
            &[&site, &edge],
        )
        .await
        .expect("tag_current_state");

    let (server, base) = start_api_server(&dsn).await;
    let rows: serde_json::Value = reqwest::Client::new()
        .get(format!(
            "{}/api/tags/current?site={}&edge={}&limit=10",
            base, site, edge
        ))
        .send()
        .await
        .expect("tags current response")
        .json()
        .await
        .expect("tags current json");
    let items = rows.as_array().expect("array response");
    assert_eq!(items.len(), 1);
    assert_eq!(
        items[0].get("tag_status").and_then(|v| v.as_str()),
        Some("disconnected")
    );
    server.abort();
}

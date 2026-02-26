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

#[tokio::test]
async fn connections_current_returns_connection_domain_state() {
    let dsn = match std::env::var("CENTRAL_PG_DSN") {
        Ok(v) => v,
        Err(_) => return,
    };

    let write_client = connect_pg(&dsn).await;
    run_migrations(&write_client).await;

    let nonce = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);
    let site = format!("conn-site-{}", nonce);
    let edge = format!("conn-edge-{}", nonce);
    let connection_code = "conn_serial_1";

    write_client
        .execute(
            "INSERT INTO sites(code,name,timezone) VALUES ($1,$2,'UTC')
             ON CONFLICT (code) DO NOTHING",
            &[&site, &format!("Site {}", site)],
        )
        .await
        .expect("site");
    write_client
        .execute(
            "INSERT INTO edges(site_id,edge_code,name,status)
             SELECT id,$2,$3,'online' FROM sites WHERE code=$1
             ON CONFLICT (site_id, edge_code) DO NOTHING",
            &[&site, &edge, &format!("Edge {}", edge)],
        )
        .await
        .expect("edge");
    write_client
        .execute(
            "INSERT INTO connections(edge_id, connection_code, name, driver_type, metadata_json)
             SELECT e.id, $3, $4, 'SerialAscii', '{}'::jsonb
             FROM edges e
             JOIN sites s ON s.id = e.site_id
             WHERE s.code = $1 AND e.edge_code = $2
             ON CONFLICT (edge_id, connection_code) DO NOTHING",
            &[&site, &edge, &connection_code, &connection_code],
        )
        .await
        .expect("connection");
    write_client
        .execute(
            "INSERT INTO connection_current_state(connection_id, state, severity, reason, payload_json, last_change_at, last_seen_at, updated_at)
             SELECT c.id, 'connected', 'info', NULL, '{\"state\":\"connected\"}'::jsonb, NOW(), NOW(), NOW()
             FROM connections c
             JOIN edges e ON e.id = c.edge_id
             JOIN sites s ON s.id = e.site_id
             WHERE s.code = $1 AND e.edge_code = $2 AND c.connection_code = $3
             ON CONFLICT (connection_id) DO UPDATE
             SET state = EXCLUDED.state,
                 severity = EXCLUDED.severity,
                 reason = EXCLUDED.reason,
                 payload_json = EXCLUDED.payload_json,
                 last_change_at = EXCLUDED.last_change_at,
                 last_seen_at = EXCLUDED.last_seen_at,
                 updated_at = NOW()",
            &[&site, &edge, &connection_code],
        )
        .await
        .expect("connection current state");

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

    let rows: serde_json::Value = reqwest::Client::new()
        .get(format!(
            "{}/api/connections/current?site={}&edge={}&limit=10",
            base, site, edge
        ))
        .send()
        .await
        .expect("connections current response")
        .json()
        .await
        .expect("connections current json");

    let items = rows.as_array().expect("array response");
    assert_eq!(items.len(), 1);
    assert_eq!(
        items[0].get("connection_id").and_then(|v| v.as_str()),
        Some(connection_code)
    );
    assert_eq!(
        items[0].get("state").and_then(|v| v.as_str()),
        Some("connected")
    );
    assert_eq!(
        items[0].get("edge_code").and_then(|v| v.as_str()),
        Some(edge.as_str())
    );

    server.abort();
}

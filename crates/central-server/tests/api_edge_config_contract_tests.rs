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
        .batch_execute("SELECT pg_advisory_lock(86234105);")
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
        .batch_execute("SELECT pg_advisory_unlock(86234105);")
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
async fn edge_config_endpoints_enforce_token_and_hash_contract() {
    let dsn = match std::env::var("CENTRAL_PG_DSN") {
        Ok(v) => v,
        Err(_) => return,
    };

    let write_client = connect_pg(&dsn).await;
    run_migrations(&write_client).await;

    let nonce = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);
    let site = format!("cfg-site-{}", nonce);
    let edge = format!("cfg-edge-{}", nonce);
    let conn_code = format!("conn-cfg-{}", nonce);
    let dev_code = format!("dev-cfg-{}", nonce);
    let tag_code = format!("tag-cfg-{}", nonce);
    let short = ((nonce.unsigned_abs() % 9000) + 1000) as u64;
    let tag_code_canonical = format!("PA01.LN01.AR01.DV{}.TG{}.PV", short, short);

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
            "INSERT INTO edges(site_id,edge_code,name,status,metadata_json)
             SELECT id,$2,$3,'online','{}'::jsonb FROM sites WHERE code=$1
             ON CONFLICT (site_id, edge_code) DO NOTHING",
            &[&site, &edge, &format!("Edge {}", edge)],
        )
        .await
        .expect("edge");
    write_client
        .execute(
            "INSERT INTO connections(edge_id, connection_code, name, driver_type, metadata_json)
             SELECT e.id, $3, $4, 'SerialAscii',
                    '{\"transport\":{\"serial\":{\"port\":\"COM7\",\"baud_rate\":9600,\"data_bits\":8,\"stop_bits\":1,\"parity\":\"N\"}},\"frame\":{\"mode\":\"line\",\"terminator\":\"\\r\",\"max_len\":128,\"read_timeout_ms\":30},\"parser\":{\"regex\":\"^\\\\s*([+-])?\\\\s*(\\\\d+(?:\\\\.\\\\d+)?)\\\\s*([A-Za-z]+)\\\\s*$\",\"sign_group\":1,\"value_group\":2,\"unit_group\":3}}'::jsonb
             FROM edges e
             JOIN sites s ON s.id = e.site_id
             WHERE s.code = $1 AND e.edge_code = $2
             ON CONFLICT (edge_id, connection_code) DO NOTHING",
            &[&site, &edge, &conn_code, &conn_code],
        )
        .await
        .expect("connection");
    write_client
        .execute(
            "INSERT INTO devices(edge_id, connection_id, device_code, name, driver_type, metadata_json)
             SELECT e.id, c.id, $4, $5, 'SerialAscii', '{}'::jsonb
             FROM connections c
             JOIN edges e ON e.id = c.edge_id
             JOIN sites s ON s.id = e.site_id
             WHERE s.code = $1 AND e.edge_code = $2 AND c.connection_code = $3
             ON CONFLICT (edge_id, device_code) DO NOTHING",
            &[&site, &edge, &conn_code, &dev_code, &dev_code],
        )
        .await
        .expect("device");
    write_client
        .execute(
            "INSERT INTO tags(device_id, tag_code, name, value_type, source, unit, metadata_json, tag_code_canonical, display_name, aliases_json)
             SELECT d.id, $5, $6, 'string', 'scale:compound', 'g',
                    '{\"update_mode\":\"on_message\",\"enabled\":true}'::jsonb, $7, $8, '[]'::jsonb
             FROM devices d
             JOIN connections c ON c.id = d.connection_id
             JOIN edges e ON e.id = c.edge_id
             JOIN sites s ON s.id = e.site_id
             WHERE s.code = $1 AND e.edge_code = $2 AND c.connection_code = $3 AND d.device_code = $4
             ON CONFLICT (device_id, tag_code) DO NOTHING",
            &[
                &site,
                &edge,
                &conn_code,
                &dev_code,
                &tag_code,
                &tag_code,
                &tag_code_canonical,
                &tag_code,
            ],
        )
        .await
        .expect("tag");

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
        waiters: central_server::edge_control::EdgeWaiters::new(),
        control_wait: std::time::Duration::from_millis(50),
    };
    let server = tokio::spawn(async move {
        let _ = run_api_server(state, &bind).await;
    });
    let base = format!("http://127.0.0.1:{}", port);
    wait_health(&base).await;
    let http = reqwest::Client::new();

    let unauthorized = http
        .post(format!("{}/api/edge/config/enroll", base))
        .json(&serde_json::json!({
            "edge_id": edge,
            "enrollment_token": "wrong-token"
        }))
        .send()
        .await
        .expect("unauthorized enroll");
    assert_eq!(unauthorized.status(), reqwest::StatusCode::UNAUTHORIZED);

    let enroll: serde_json::Value = http
        .post(format!("{}/api/edge/config/enroll", base))
        .json(&serde_json::json!({
            "edge_id": edge,
            "enrollment_token": "test-token"
        }))
        .send()
        .await
        .expect("enroll response")
        .json()
        .await
        .expect("enroll json");
    let cfg_hash = enroll
        .get("config_hash")
        .and_then(|v| v.as_str())
        .expect("config_hash")
        .to_string();

    let check_same: serde_json::Value = http
        .post(format!("{}/api/edge/config/check", base))
        .json(&serde_json::json!({
            "edge_id": edge,
            "enrollment_token": "test-token",
            "current_config_hash": cfg_hash
        }))
        .send()
        .await
        .expect("check response")
        .json()
        .await
        .expect("check json");
    assert_eq!(
        check_same
            .get("config_changed")
            .and_then(|v| v.as_bool())
            .unwrap_or(true),
        false
    );
    assert_eq!(
        check_same
            .get("target_config_hash")
            .and_then(|v| v.as_str())
            .unwrap_or_default(),
        cfg_hash
    );

    let runtime: serde_json::Value = http
        .get(format!("{}/api/edge/config/runtime?edge_id={}", base, edge))
        .send()
        .await
        .expect("runtime response")
        .json()
        .await
        .expect("runtime json");
    assert_eq!(runtime.get("edge_id").and_then(|v| v.as_str()), Some(edge.as_str()));
    assert_eq!(runtime.get("key_id").and_then(|v| v.as_str()), Some("v1"));
    assert_eq!(
        runtime.get("algorithm").and_then(|v| v.as_str()),
        Some("hmac-sha256")
    );
    assert_eq!(
        runtime.get("config_hash").and_then(|v| v.as_str()),
        Some(cfg_hash.as_str())
    );
    let payload_json = runtime
        .get("payload_json")
        .and_then(|v| v.as_str())
        .expect("payload_json");
    let payload: serde_json::Value =
        serde_json::from_str(payload_json).expect("payload parse");
    let payload_str = payload.to_string();
    assert!(
        payload_str.contains(&conn_code),
        "runtime payload does not include connection code"
    );
    assert!(
        payload_str.contains(&tag_code),
        "runtime payload does not include tag code"
    );

    let not_modified = http
        .get(format!(
            "{}/api/edge/config/runtime?edge_id={}&want_hash={}",
            base, edge, cfg_hash
        ))
        .send()
        .await
        .expect("runtime not modified response");
    assert_eq!(not_modified.status(), reqwest::StatusCode::NOT_MODIFIED);

    server.abort();
}

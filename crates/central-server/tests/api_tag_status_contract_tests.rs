use central_server::api::{ApiState, EdgeConfigSettings, run_api_server};
use std::collections::HashMap;
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
async fn tags_current_returns_domain_tag_status() {
    let dsn = match std::env::var("CENTRAL_PG_DSN") {
        Ok(v) => v,
        Err(_) => return,
    };
    let client = connect_pg(&dsn).await;
    run_migrations(&client).await;

    let nonce = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);
    let site = format!("ts-site-{}", nonce);
    let edge = format!("ts-edge-{}", nonce);

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
            "INSERT INTO edge_current_state(edge_id,status,last_seen_at,outbox_depth,outbox_oldest_secs,updated_at)
             SELECT e.id,'online',NOW(),0,NULL,NOW()
             FROM edges e
             JOIN sites s ON s.id = e.site_id
             WHERE s.code = $1 AND e.edge_code = $2
             ON CONFLICT (edge_id) DO UPDATE
             SET status = EXCLUDED.status,
                 last_seen_at = EXCLUDED.last_seen_at,
                 outbox_depth = EXCLUDED.outbox_depth,
                 outbox_oldest_secs = EXCLUDED.outbox_oldest_secs,
                 updated_at = NOW()",
            &[&site, &edge],
        )
        .await
        .expect("edge current state");

    // Device/Tag 1: connected (on-change, no expected interval).
    client
        .execute(
            "INSERT INTO devices(edge_id,device_code,name,driver_type,metadata_json)
             SELECT e.id,'dev_conn','Device Connected','SerialAscii','{}'::jsonb
             FROM edges e
             JOIN sites s ON s.id = e.site_id
             WHERE s.code = $1 AND e.edge_code = $2
             ON CONFLICT (edge_id, device_code) DO NOTHING",
            &[&site, &edge],
        )
        .await
        .expect("device connected");
    client
        .execute(
            "INSERT INTO tags(device_id,tag_code,name,value_type,source,unit,metadata_json,tag_code_canonical,display_name,aliases_json)
             SELECT d.id,'tag_status_connected','Tag Connected','float','sim','u','{}'::jsonb,
                    CONCAT(UPPER(SUBSTRING(REPLACE($1, '-', '_') FROM 1 FOR 12)), '.STAT.EDGE.DEVCN.TAG01.PV'),
                    'Tag Connected','[]'::jsonb
             FROM devices d
             JOIN edges e ON e.id = d.edge_id
             JOIN sites s ON s.id = e.site_id
             WHERE s.code = $1 AND e.edge_code = $2 AND d.device_code = 'dev_conn'
             ON CONFLICT (device_id, tag_code) DO NOTHING",
            &[&site, &edge],
        )
        .await
        .expect("tag connected");

    // Device/Tag 2: stale (expected interval exceeded).
    client
        .execute(
            "INSERT INTO devices(edge_id,device_code,name,driver_type,metadata_json)
             SELECT e.id,'dev_stale','Device Stale','SerialAscii','{}'::jsonb
             FROM edges e
             JOIN sites s ON s.id = e.site_id
             WHERE s.code = $1 AND e.edge_code = $2
             ON CONFLICT (edge_id, device_code) DO NOTHING",
            &[&site, &edge],
        )
        .await
        .expect("device stale");
    client
        .execute(
            "INSERT INTO tags(device_id,tag_code,name,value_type,source,unit,metadata_json,tag_code_canonical,display_name,aliases_json)
             SELECT d.id,'tag_status_stale','Tag Stale','float','sim','u','{\"expected_interval_ms\":1000}'::jsonb,
                    CONCAT(UPPER(SUBSTRING(REPLACE($1, '-', '_') FROM 1 FOR 12)), '.STAT.EDGE.DEVST.TAG02.PV'),
                    'Tag Stale','[]'::jsonb
             FROM devices d
             JOIN edges e ON e.id = d.edge_id
             JOIN sites s ON s.id = e.site_id
             WHERE s.code = $1 AND e.edge_code = $2 AND d.device_code = 'dev_stale'
             ON CONFLICT (device_id, tag_code) DO NOTHING",
            &[&site, &edge],
        )
        .await
        .expect("tag stale");

    // Device/Tag 3: disconnected (connection state failed).
    client
        .execute(
            "INSERT INTO devices(edge_id,device_code,name,driver_type,metadata_json)
             SELECT e.id,'dev_disc','Device Disconnected','SerialAscii','{}'::jsonb
             FROM edges e
             JOIN sites s ON s.id = e.site_id
             WHERE s.code = $1 AND e.edge_code = $2
             ON CONFLICT (edge_id, device_code) DO NOTHING",
            &[&site, &edge],
        )
        .await
        .expect("device disconnected");
    client
        .execute(
            "INSERT INTO tags(device_id,tag_code,name,value_type,source,unit,metadata_json,tag_code_canonical,display_name,aliases_json)
             SELECT d.id,'tag_status_disconnected','Tag Disconnected','float','sim','u','{}'::jsonb,
                    CONCAT(UPPER(SUBSTRING(REPLACE($1, '-', '_') FROM 1 FOR 12)), '.STAT.EDGE.DEVDC.TAG03.PV'),
                    'Tag Disconnected','[]'::jsonb
             FROM devices d
             JOIN edges e ON e.id = d.edge_id
             JOIN sites s ON s.id = e.site_id
             WHERE s.code = $1 AND e.edge_code = $2 AND d.device_code = 'dev_disc'
             ON CONFLICT (device_id, tag_code) DO NOTHING",
            &[&site, &edge],
        )
        .await
        .expect("tag disconnected");

    client
        .execute(
            "INSERT INTO connections(edge_id,connection_code,name,driver_type,metadata_json)
             SELECT e.id,'conn_fail_1','conn_fail_1','SerialAscii','{}'::jsonb
             FROM edges e
             JOIN sites s ON s.id = e.site_id
             WHERE s.code = $1 AND e.edge_code = $2
             ON CONFLICT (edge_id, connection_code) DO NOTHING",
            &[&site, &edge],
        )
        .await
        .expect("connection row");
    client
        .execute(
            "UPDATE devices
             SET connection_id = (
                 SELECT c.id
                 FROM connections c
                 JOIN edges e ON e.id = c.edge_id
                 JOIN sites s ON s.id = e.site_id
                 WHERE s.code = $1 AND e.edge_code = $2 AND c.connection_code = 'conn_fail_1'
             )
             WHERE device_code = 'dev_disc'
               AND edge_id = (
                 SELECT e.id
                 FROM edges e
                 JOIN sites s ON s.id = e.site_id
                 WHERE s.code = $1 AND e.edge_code = $2
               )",
            &[&site, &edge],
        )
        .await
        .expect("device connection_id");
    client
        .execute(
            "INSERT INTO connection_current_state(connection_id,state,severity,reason,payload_json,last_change_at,last_seen_at,updated_at)
             SELECT c.id,'failed','error','fail','{\"state\":\"failed\"}'::jsonb,NOW(),NOW(),NOW()
             FROM connections c
             JOIN edges e ON e.id = c.edge_id
             JOIN sites s ON s.id = e.site_id
             WHERE s.code = $1 AND e.edge_code = $2 AND c.connection_code = 'conn_fail_1'
             ON CONFLICT (connection_id) DO UPDATE
             SET state = EXCLUDED.state,
                 severity = EXCLUDED.severity,
                 reason = EXCLUDED.reason,
                 payload_json = EXCLUDED.payload_json,
                 last_change_at = EXCLUDED.last_change_at,
                 last_seen_at = EXCLUDED.last_seen_at,
                 updated_at = NOW()",
            &[&site, &edge],
        )
        .await
        .expect("connection current state");

    client
        .execute(
            "INSERT INTO tag_current_state(tag_id, ts, value_json, quality_json, source, updated_at)
             SELECT t.id, NOW(), '1.0'::jsonb, '{\"status\":\"Good\",\"reason\":null}'::jsonb, 'sim', NOW()
             FROM tags t
             JOIN devices d ON d.id = t.device_id
             JOIN edges e ON e.id = d.edge_id
             JOIN sites s ON s.id = e.site_id
             WHERE s.code = $1 AND e.edge_code = $2 AND t.tag_code = 'tag_status_connected'
             ON CONFLICT (tag_id) DO UPDATE
             SET ts = EXCLUDED.ts,
                 value_json = EXCLUDED.value_json,
                 quality_json = EXCLUDED.quality_json,
                 source = EXCLUDED.source,
                 updated_at = NOW()",
            &[&site, &edge],
        )
        .await
        .expect("tag state connected");
    client
        .execute(
            "INSERT INTO tag_current_state(tag_id, ts, value_json, quality_json, source, updated_at)
             SELECT t.id, NOW() - INTERVAL '15 seconds', '1.0'::jsonb, '{\"status\":\"Good\",\"reason\":null}'::jsonb, 'sim', NOW()
             FROM tags t
             JOIN devices d ON d.id = t.device_id
             JOIN edges e ON e.id = d.edge_id
             JOIN sites s ON s.id = e.site_id
             WHERE s.code = $1 AND e.edge_code = $2 AND t.tag_code = 'tag_status_stale'
             ON CONFLICT (tag_id) DO UPDATE
             SET ts = EXCLUDED.ts,
                 value_json = EXCLUDED.value_json,
                 quality_json = EXCLUDED.quality_json,
                 source = EXCLUDED.source,
                 updated_at = NOW()",
            &[&site, &edge],
        )
        .await
        .expect("tag state stale");
    client
        .execute(
            "INSERT INTO tag_current_state(tag_id, ts, value_json, quality_json, source, updated_at)
             SELECT t.id, NOW(), '1.0'::jsonb, '{\"status\":\"Good\",\"reason\":null}'::jsonb, 'sim', NOW()
             FROM tags t
             JOIN devices d ON d.id = t.device_id
             JOIN edges e ON e.id = d.edge_id
             JOIN sites s ON s.id = e.site_id
             WHERE s.code = $1 AND e.edge_code = $2 AND t.tag_code = 'tag_status_disconnected'
             ON CONFLICT (tag_id) DO UPDATE
             SET ts = EXCLUDED.ts,
                 value_json = EXCLUDED.value_json,
                 quality_json = EXCLUDED.quality_json,
                 source = EXCLUDED.source,
                 updated_at = NOW()",
            &[&site, &edge],
        )
        .await
        .expect("tag state disconnected");

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
            "{}/api/tags/current?site={}&edge={}&limit=50",
            base, site, edge
        ))
        .send()
        .await
        .expect("tags current response")
        .json()
        .await
        .expect("tags current json");

    let items = rows.as_array().expect("array response");
    assert!(items.len() >= 3);
    let mut statuses = HashMap::<String, String>::new();
    for row in items {
        let Some(tag_code) = row.get("tag_code").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(tag_status) = row.get("tag_status").and_then(|v| v.as_str()) else {
            continue;
        };
        statuses.insert(tag_code.to_string(), tag_status.to_string());
    }

    assert_eq!(
        statuses.get("tag_status_connected").map(|s| s.as_str()),
        Some("connected")
    );
    assert_eq!(
        statuses.get("tag_status_stale").map(|s| s.as_str()),
        Some("stale")
    );
    assert_eq!(
        statuses.get("tag_status_disconnected").map(|s| s.as_str()),
        Some("disconnected")
    );

    server.abort();
}

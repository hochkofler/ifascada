use central_server::api::{ApiState, EdgeConfigSettings, run_api_server};
use central_server::messages::{ConnectionStateMessage, DeviceConnectionStateMessage};
use central_server::persistence::CentralPersistence;
use central_server::persistence::postgres::PostgresCentralPersistence;
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

async fn seed_device_with_connection(
    client: &Client,
    site: &str,
    edge: &str,
    device: &str,
    connection_code: &str,
) {
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
            "INSERT INTO edges(site_id,edge_code,name,status,metadata_json)
             SELECT id,$2,$3,'online','{}'::jsonb FROM sites WHERE code=$1
             ON CONFLICT (site_id, edge_code) DO NOTHING",
            &[&site, &edge, &format!("Edge {}", edge)],
        )
        .await
        .expect("edge");
    client
        .execute(
            "INSERT INTO devices(edge_id,device_code,name,driver_type,metadata_json)
             SELECT e.id,$3,$4,'SerialAscii','{}'::jsonb
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
            "INSERT INTO connections(edge_id,connection_code,name,driver_type,metadata_json)
             SELECT e.id,$3,$3,'SerialAscii','{}'::jsonb
             FROM edges e
             JOIN sites s ON s.id=e.site_id
             WHERE s.code=$1 AND e.edge_code=$2
             ON CONFLICT (edge_id, connection_code) DO NOTHING",
            &[&site, &edge, &connection_code],
        )
        .await
        .expect("connection");
    client
        .execute(
            "UPDATE devices
             SET connection_id = c.id
             FROM connections c
             JOIN edges e ON e.id = c.edge_id
             JOIN sites s ON s.id = e.site_id
             WHERE s.code = $1 AND e.edge_code = $2
               AND c.connection_code = $3
               AND devices.device_code = $4
               AND devices.edge_id = e.id",
            &[&site, &edge, &connection_code, &device],
        )
        .await
        .expect("device connection");
}

#[tokio::test]
async fn devices_current_endpoint_contract() {
    let dsn = match std::env::var("CENTRAL_PG_DSN") {
        Ok(v) => v,
        Err(_) => return,
    };
    let client = connect_pg(&dsn).await;
    run_migrations(&client).await;

    let nonce = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);
    let site = format!("dc-site-{}", nonce);
    let edge = format!("dc-edge-{}", nonce);
    let device = "dev-current-1";
    let conn = "conn-current-1";
    seed_device_with_connection(&client, &site, &edge, device, conn).await;
    client
        .execute(
            "INSERT INTO edge_current_state(edge_id, status, last_seen_at, outbox_depth, outbox_oldest_secs, updated_at)
             SELECT e.id, 'online', NOW(), 0, NULL, NOW()
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
        .expect("edge current state");

    client
        .execute(
            "INSERT INTO device_current_state
             (device_id, state, severity, reason, connection_id, tags_connected, tags_stale, tags_disconnected, last_change_at, last_seen_at, payload_json, updated_at)
             SELECT d.id, 'connected', 'info', 'tag_connected', d.connection_id, 1, 0, 0, NOW(), NOW(), '{}'::jsonb, NOW()
             FROM devices d
             JOIN edges e ON e.id = d.edge_id
             JOIN sites s ON s.id = e.site_id
             WHERE s.code = $1 AND e.edge_code = $2 AND d.device_code = $3
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
            &[&site, &edge, &device],
        )
        .await
        .expect("device current state");

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
        items[0].get("device_code").and_then(|v| v.as_str()),
        Some(device)
    );
    assert_eq!(
        items[0].get("state").and_then(|v| v.as_str()),
        Some("connected")
    );
    assert_eq!(
        items[0].get("connection_id").and_then(|v| v.as_str()),
        Some(conn)
    );
    server.abort();
}

#[tokio::test]
async fn device_status_transitions_are_not_duplicated() {
    let dsn = match std::env::var("CENTRAL_PG_DSN") {
        Ok(v) => v,
        Err(_) => return,
    };
    let client = connect_pg(&dsn).await;
    run_migrations(&client).await;

    let nonce = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);
    let site = format!("dt-site-{}", nonce);
    let edge = format!("dt-edge-{}", nonce);
    let device = "dev-transition-1";
    let conn = "conn-transition-1";
    seed_device_with_connection(&client, &site, &edge, device, conn).await;

    client
        .execute(
            "INSERT INTO edge_current_state(edge_id, status, last_seen_at, outbox_depth, outbox_oldest_secs, updated_at)
             SELECT e.id, 'online', NOW(), 0, NULL, NOW()
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
        .expect("edge state");

    let persistence = PostgresCentralPersistence::connect(&dsn)
        .await
        .expect("postgres persistence");

    let ts1 = chrono::Utc::now();
    persistence
        .insert_connection_state(
            &site,
            &edge,
            &ConnectionStateMessage {
                schema_version: 1,
                source: "edge/test".to_string(),
                connection_id: conn.to_string(),
                state: "connected".to_string(),
                timestamp: ts1,
            },
        )
        .await
        .expect("conn connected");
    persistence
        .insert_connection_state(
            &site,
            &edge,
            &ConnectionStateMessage {
                schema_version: 1,
                source: "edge/test".to_string(),
                connection_id: conn.to_string(),
                state: "connected".to_string(),
                timestamp: ts1 + chrono::Duration::seconds(1),
            },
        )
        .await
        .expect("conn connected duplicate");
    persistence
        .insert_connection_state(
            &site,
            &edge,
            &ConnectionStateMessage {
                schema_version: 1,
                source: "edge/test".to_string(),
                connection_id: conn.to_string(),
                state: "failed".to_string(),
                timestamp: ts1 + chrono::Duration::seconds(2),
            },
        )
        .await
        .expect("conn failed");
    persistence
        .insert_connection_state(
            &site,
            &edge,
            &ConnectionStateMessage {
                schema_version: 1,
                source: "edge/test".to_string(),
                connection_id: conn.to_string(),
                state: "failed".to_string(),
                timestamp: ts1 + chrono::Duration::seconds(7),
            },
        )
        .await
        .expect("conn failed after debounce");

    let row = client
        .query_one(
            "SELECT COUNT(*)::bigint
             FROM operational_events
             WHERE site_code = $1
               AND edge_code = $2
               AND device_code = $3
               AND event_type LIKE 'device.status.%'",
            &[&site, &edge, &device],
        )
        .await
        .expect("event count");
    let count: i64 = row.get(0);
    assert_eq!(count, 2);

    let row = client
        .query_one(
            "SELECT state
             FROM device_current_state dcs
             JOIN devices d ON d.id = dcs.device_id
             JOIN edges e ON e.id = d.edge_id
             JOIN sites s ON s.id = e.site_id
             WHERE s.code = $1 AND e.edge_code = $2 AND d.device_code = $3",
            &[&site, &edge, &device],
        )
        .await
        .expect("device state");
    let state: String = row.get(0);
    assert_eq!(state, "disconnected");
}

#[tokio::test]
async fn device_status_changes_with_device_protocol_events() {
    let dsn = match std::env::var("CENTRAL_PG_DSN") {
        Ok(v) => v,
        Err(_) => return,
    };
    let client = connect_pg(&dsn).await;
    run_migrations(&client).await;

    let nonce = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);
    let site = format!("dp-site-{}", nonce);
    let edge = format!("dp-edge-{}", nonce);
    let device = "dev-protocol-1";
    let conn = "conn-protocol-1";
    seed_device_with_connection(&client, &site, &edge, device, conn).await;

    client
        .execute(
            "INSERT INTO edge_current_state(edge_id, status, last_seen_at, outbox_depth, outbox_oldest_secs, updated_at)
             SELECT e.id, 'online', NOW(), 0, NULL, NOW()
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
        .expect("edge state");

    let persistence = PostgresCentralPersistence::connect(&dsn)
        .await
        .expect("postgres persistence");
    let ts1 = chrono::Utc::now();
    persistence
        .insert_connection_state(
            &site,
            &edge,
            &ConnectionStateMessage {
                schema_version: 1,
                source: "edge/test".to_string(),
                connection_id: conn.to_string(),
                state: "connected".to_string(),
                timestamp: ts1,
            },
        )
        .await
        .expect("conn connected");
    persistence
        .insert_device_connection_state(
            &site,
            &edge,
            &DeviceConnectionStateMessage {
                schema_version: 1,
                source: "edge/test".to_string(),
                connection_id: conn.to_string(),
                device_id: device.to_string(),
                tag_id: None,
                state: "Error".to_string(),
                reason: Some("modbus read timeout".to_string()),
                timestamp: ts1 + chrono::Duration::seconds(1),
            },
        )
        .await
        .expect("protocol error");
    persistence
        .insert_device_connection_state(
            &site,
            &edge,
            &DeviceConnectionStateMessage {
                schema_version: 1,
                source: "edge/test".to_string(),
                connection_id: conn.to_string(),
                device_id: device.to_string(),
                tag_id: None,
                state: "Connected".to_string(),
                reason: None,
                timestamp: ts1 + chrono::Duration::seconds(2),
            },
        )
        .await
        .expect("protocol recovered");

    let row = client
        .query_one(
            "SELECT state, reason
             FROM device_current_state dcs
             JOIN devices d ON d.id = dcs.device_id
             JOIN edges e ON e.id = d.edge_id
             JOIN sites s ON s.id = e.site_id
             WHERE s.code = $1 AND e.edge_code = $2 AND d.device_code = $3",
            &[&site, &edge, &device],
        )
        .await
        .expect("device state");
    let state: String = row.get(0);
    let reason: Option<String> = row.get(1);
    assert_eq!(state, "connected");
    assert!(
        matches!(
            reason.as_deref(),
            Some("device_protocol_connected") | Some("connection_connected")
        ),
        "unexpected reason: {:?}",
        reason
    );

    let row = client
        .query_one(
            "SELECT COUNT(*)::bigint
             FROM operational_events
             WHERE site_code = $1
               AND edge_code = $2
               AND device_code = $3
               AND event_type IN ('device.connection.error', 'device.connection.connected')",
            &[&site, &edge, &device],
        )
        .await
        .expect("ops count");
    let count: i64 = row.get(0);
    assert_eq!(count, 2);
}

/// `last_seen_at` de un dispositivo se deriva de la telemetria de sus tags, NO de la fila de
/// `device_current_state`.
///
/// Reproduce el caso real de produccion: un dispositivo sano que lleva dias `connected` conserva
/// en `device_current_state` la fecha de su ultimo CAMBIO de estado, porque
/// `should_apply_device_transition` corta antes de escribir cuando el estado no cambio, y
/// `last_seen_at` viajaba en ese mismo INSERT. Medido en planta: un dispositivo figuraba visto
/// hacia 176 h cuando su tag habia reportado hacia 6 minutos.
#[tokio::test]
async fn devices_current_last_seen_at_derives_from_tag_telemetry() {
    let dsn = match std::env::var("CENTRAL_PG_DSN") {
        Ok(v) => v,
        Err(_) => return,
    };
    let client = connect_pg(&dsn).await;
    run_migrations(&client).await;

    let nonce = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);
    let site = format!("ls-site-{}", nonce);
    let edge = format!("ls-edge-{}", nonce);
    let device = "dev-lastseen-1";
    let conn = "conn-lastseen-1";
    // tag_code_canonical tiene UNIQUE global y un CHECK que limita cada segmento a 2-16
    // caracteres [A-Z0-9_], asi que el nonce no entra entero: se usan sus ultimos 7 digitos
    // para que el test sea idempotente entre corridas.
    let canonical = format!("SITEA.LINEA.AREAA.CELDA.DEV01.T{:07}", nonce.rem_euclid(10_000_000));
    seed_device_with_connection(&client, &site, &edge, device, conn).await;

    client
        .execute(
            "INSERT INTO edge_current_state(edge_id, status, last_seen_at, outbox_depth, outbox_oldest_secs, updated_at)
             SELECT e.id, 'online', NOW(), 0, NULL, NOW()
             FROM edges e JOIN sites s ON s.id = e.site_id
             WHERE s.code=$1 AND e.edge_code=$2
             ON CONFLICT (edge_id) DO UPDATE SET status='online', last_seen_at=EXCLUDED.last_seen_at, updated_at=NOW()",
            &[&site, &edge],
        )
        .await
        .expect("edge current state");

    // El estado del dispositivo quedo escrito hace 7 dias, cuando cambio por ultima vez.
    let stale = chrono::Utc::now() - chrono::Duration::days(7);
    client
        .execute(
            "INSERT INTO device_current_state
             (device_id, state, severity, reason, connection_id, tags_connected, tags_stale, tags_disconnected, last_change_at, last_seen_at, payload_json, updated_at)
             SELECT d.id,'connected','info','ok',d.connection_id,1,0,0,$4,$4,'{}'::jsonb,NOW()
             FROM devices d
             JOIN edges e ON e.id=d.edge_id
             JOIN sites s ON s.id=e.site_id
             WHERE s.code=$1 AND e.edge_code=$2 AND d.device_code=$3
             ON CONFLICT (device_id) DO UPDATE
             SET last_change_at=EXCLUDED.last_change_at, last_seen_at=EXCLUDED.last_seen_at, updated_at=NOW()",
            &[&site, &edge, &device, &stale],
        )
        .await
        .expect("device current state");

    // ...pero su tag reporto hace un minuto.
    let fresh = chrono::Utc::now() - chrono::Duration::minutes(1);
    client
        .execute(
            "INSERT INTO tags(device_id,tag_code,tag_code_canonical,display_name,name,value_type,source,metadata_json)
             SELECT d.id,'tag_lastseen',$4,'Tag','Tag','number','modbus','{}'::jsonb
             FROM devices d
             JOIN edges e ON e.id=d.edge_id
             JOIN sites s ON s.id=e.site_id
             WHERE s.code=$1 AND e.edge_code=$2 AND d.device_code=$3
             ON CONFLICT (device_id, tag_code) DO NOTHING",
            &[&site, &edge, &device, &canonical],
        )
        .await
        .expect("tag");
    client
        .execute(
            "INSERT INTO tag_current_state(tag_id, ts, value_json, quality_json, source, updated_at)
             SELECT t.id,$4,'1'::jsonb,'{\"status\":\"Good\"}'::jsonb,'modbus',NOW()
             FROM tags t
             JOIN devices d ON d.id=t.device_id
             JOIN edges e ON e.id=d.edge_id
             JOIN sites s ON s.id=e.site_id
             WHERE s.code=$1 AND e.edge_code=$2 AND d.device_code=$3 AND t.tag_code='tag_lastseen'
             ON CONFLICT (tag_id) DO UPDATE SET ts=EXCLUDED.ts, updated_at=NOW()",
            &[&site, &edge, &device, &fresh],
        )
        .await
        .expect("tag current state");

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

    let last_seen: chrono::DateTime<chrono::Utc> = items[0]
        .get("last_seen_at")
        .and_then(|v| v.as_str())
        .expect("last_seen_at presente")
        .parse()
        .expect("last_seen_at parseable");
    let last_change: chrono::DateTime<chrono::Utc> = items[0]
        .get("last_change_at")
        .and_then(|v| v.as_str())
        .expect("last_change_at presente")
        .parse()
        .expect("last_change_at parseable");

    // last_seen_at sigue a la telemetria del tag, no a la fila del dispositivo.
    assert!(
        (last_seen - fresh).num_seconds().abs() <= 1,
        "last_seen_at deberia seguir al ts del tag ({}), pero fue {}",
        fresh,
        last_seen
    );
    // ...y last_change_at conserva su significado: cuando cambio de estado.
    assert!(
        (last_change - stale).num_seconds().abs() <= 1,
        "last_change_at deberia conservar la fecha del cambio de estado ({}), pero fue {}",
        stale,
        last_change
    );
    // Antes del arreglo ambos eran el mismo valor; ahora tienen que diferir.
    assert!(
        last_seen > last_change,
        "last_seen_at ({}) tiene que ser posterior a last_change_at ({})",
        last_seen,
        last_change
    );

    server.abort();
}

/// Un dispositivo sin tags no tiene de donde derivar `last_seen_at`: devuelve null en vez de la
/// fecha del cambio de estado, que seria la misma mentira que este arreglo elimina.
#[tokio::test]
async fn devices_current_last_seen_at_is_null_without_tags() {
    let dsn = match std::env::var("CENTRAL_PG_DSN") {
        Ok(v) => v,
        Err(_) => return,
    };
    let client = connect_pg(&dsn).await;
    run_migrations(&client).await;

    let nonce = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);
    let site = format!("nt-site-{}", nonce);
    let edge = format!("nt-edge-{}", nonce);
    let device = "dev-notags-1";
    let conn = "conn-notags-1";
    seed_device_with_connection(&client, &site, &edge, device, conn).await;

    client
        .execute(
            "INSERT INTO edge_current_state(edge_id, status, last_seen_at, outbox_depth, outbox_oldest_secs, updated_at)
             SELECT e.id, 'online', NOW(), 0, NULL, NOW()
             FROM edges e JOIN sites s ON s.id = e.site_id
             WHERE s.code=$1 AND e.edge_code=$2
             ON CONFLICT (edge_id) DO UPDATE SET status='online', last_seen_at=EXCLUDED.last_seen_at, updated_at=NOW()",
            &[&site, &edge],
        )
        .await
        .expect("edge current state");
    client
        .execute(
            "INSERT INTO device_current_state
             (device_id, state, severity, reason, connection_id, tags_connected, tags_stale, tags_disconnected, last_change_at, last_seen_at, payload_json, updated_at)
             SELECT d.id,'connected','info','ok',d.connection_id,0,0,0,NOW(),NOW(),'{}'::jsonb,NOW()
             FROM devices d
             JOIN edges e ON e.id=d.edge_id
             JOIN sites s ON s.id=e.site_id
             WHERE s.code=$1 AND e.edge_code=$2 AND d.device_code=$3
             ON CONFLICT (device_id) DO NOTHING",
            &[&site, &edge, &device],
        )
        .await
        .expect("device current state");

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
    assert!(
        items[0].get("last_seen_at").map(|v| v.is_null()).unwrap_or(false),
        "un dispositivo sin tags deberia devolver last_seen_at null, devolvio {:?}",
        items[0].get("last_seen_at")
    );

    server.abort();
}

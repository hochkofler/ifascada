# Edge Signed Runtime Config (Enroll + Pull)

This flow allows `edge-agent` to receive runtime config from `central-server` with integrity validation before loading into memory.

## Flow

1. Edge calls `POST /api/edge/config/check` with:
   - `edge_id`
   - `enrollment_token`
   - `current_config_hash` (optional)
2. Central responds with:
   - `config_changed`
   - `target_config_hash`
3. Only when `config_changed=true`, edge calls:
   - `GET /api/edge/config/runtime?edge_id=...&want_hash=...`
4. Central returns `SignedRuntimeConfigEnvelope`:
   - `payload_json`
   - `config_hash` (SHA-256)
   - `signature_hex` (HMAC-SHA256 over hash)
5. Edge verifies:
   - `edge_id`, `key_id`, `algorithm`, `config_hash`, `issued_at`
   - payload hash
   - signature
6. Only then edge deserializes payload and starts runtime connections.
7. Edge stores verified envelope at local cache for offline startup fallback.

## Central env vars

- `CENTRAL_EDGE_ENROLL_TOKEN` (default: `dev-edge-enroll-token`)
- `CENTRAL_EDGE_CONFIG_SIGNING_SECRET` (default: `dev-edge-config-signing-secret`)
- `CENTRAL_EDGE_CONFIG_SIGNING_KEY_ID` (default: `v1`)
- `CENTRAL_EDGE_RUNTIME_CONFIG_PATH` (default: `crates/edge-agent/config/bootstrap.example.json`) fallback only when edge has no runtime config in DB catalog.

Runtime payload source in central:
1. Primary: DB catalog (`connections` + `devices` + `tags`) filtered by `edge_id`.
2. Automation source: `connections.metadata_json.automations[]` merged into top-level `automations` in payload.
3. Fallback: local file path `CENTRAL_EDGE_RUNTIME_CONFIG_PATH`.

## Edge env vars

- `EDGE_CONFIG_URL` (example: `http://127.0.0.1:8088`)
- `EDGE_ENROLL_TOKEN` (must match central token)
- `EDGE_CONFIG_HMAC_SECRET` (must match central signing secret)
- `EDGE_CONFIG_KEY_ID` (optional strict key id validation)
- `EDGE_RUNTIME_CACHE_PATH` (default: `./data/runtime_config.signed.json`)
- `EDGE_CONFIG_CHECK_INTERVAL_SECS` (default: `120`)
- `EDGE_CONFIG_CHECK_JITTER_SECS` (default: `20`)
- `EDGE_CONFIG_HMAC_SECRET` (used by periodic check to verify staged config)
- `EDGE_CONFIG_KEY_ID` (optional strict key id validation)
- `EDGE_CONFIG_APPLY_RECEIPT_PATH` (default: `./data/config_apply_receipt.json`)

## Startup precedence in edge-agent

1. Remote signed config (`EDGE_CONFIG_URL`)
2. Local signed cache (`EDGE_RUNTIME_CACHE_PATH`)
3. Legacy local bootstrap file (`EDGE_BOOTSTRAP_PATH` / `./config/bootstrap.json`) only when `EDGE_CONFIG_URL` is not configured.

Operational note:
1. When `EDGE_CONFIG_URL` is set, bootstrap is not used as source of truth.
2. In that mode, edge starts only from remote signed config or verified local signed cache.

## Runtime periodic check

When MQTT bridge is running, edge performs periodic `config/check` independently from health heartbeat:

- heartbeat remains operational (fast cadence)
- config check runs with `interval + jitter`
- health payload includes:
  - `config_hash`
  - `config_sync_state` (`unknown`, `in_sync`, `changed_staged`, `apply_requested`, `error`)
  - `config_target_hash`
  - `config_last_check_at`

## Manual apply via MQTT

When config is `changed_staged`, you can request apply/restart from MQTT:

- command topic: `scada/{site}/edge/{agent}/config/apply`
- payload example:

```json
{
  "schema_version": 1,
  "source": "hmi",
  "request_id": "cfg-apply-001"
}
```

- result topic: `scada/{site}/edge/{agent}/config/apply/result`
- result fields include:
  - `accepted`
  - `reason`
  - `current_config_hash`
  - `target_config_hash`

If accepted, edge stops MQTT bridge so supervisor/service can restart process and load staged config.

On next startup, edge publishes an `applied_after_restart` result to:

- `scada/{site}/edge/{agent}/config/apply/result`

using the persisted receipt (`request_id`, `target_config_hash`).

## Tracing and debugging

### Runtime log levels

Both services use `tracing` and can be filtered with `RUST_LOG`.

- edge default: `edge_agent=info,info`
- central default: `central_server=info,central-server=info,info`

Examples:

```powershell
$env:RUST_LOG="edge_agent=debug,application=debug,infrastructure=info,info"
cargo run -p edge-agent
```

```powershell
$env:RUST_LOG="central_server=debug,info"
cargo run -p central-server
```

### What to watch during config sync

- edge logs:
  - periodic check failures
  - `changed_staged` detection
  - `apply_requested` and graceful stop message
- heartbeat topic:
  - `config_hash`
  - `config_sync_state`
  - `config_target_hash`
  - `config_last_check_at`

Subscribe example:

```powershell
mosquitto_sub -h 127.0.0.1 -p 51883 -t "scada/+/edge/+/health/runtime" -v
```

# UC-CENTRAL-001: Central Ingestion with PostgreSQL + Optional Redis

## Goal
Run `central-server` as MQTT consumer, persist incoming events into PostgreSQL/Timescale, and optionally fan-out current-state updates via Redis.

## Runtime switches
1. Required:
- `CENTRAL_MQTT_ENABLED=true`
- `MQTT_HOST=127.0.0.1`
- `MQTT_PORT=1883`
- `CENTRAL_PG_DSN=host=127.0.0.1 user=postgres password=postgres dbname=ifascada`

2. Optional Redis:
- `CENTRAL_REDIS_ENABLED=true`
- `CENTRAL_REDIS_URL=redis://127.0.0.1:6379/`
- `CENTRAL_REDIS_EVENT_CHANNEL=scada:rt:events`
- `CENTRAL_REDIS_KEY_TTL_SECS=300`

## Flow
1. Consumer subscribes `scada/+/edge/+/#`.
2. Ingestion parses topic and payload.
3. PostgreSQL adapter persists:
- raw telemetry ingest event
- historian sample (`telemetry_samples`) when tag mapping exists
- current state upserts (`tag_current_state`, `edge_current_state`)
- health/audit/ack events
4. If Redis enabled:
- current keys updated for edge/tag
- realtime event published to `scada:rt:events`

## Quick run
```powershell
$env:CENTRAL_MQTT_ENABLED = "true"
$env:MQTT_HOST = "127.0.0.1"
$env:MQTT_PORT = "1883"
$env:CENTRAL_PG_DSN = "host=127.0.0.1 user=postgres password=postgres dbname=ifascada"
$env:CENTRAL_REDIS_ENABLED = "true"
$env:CENTRAL_REDIS_URL = "redis://127.0.0.1:6379/"
cargo run -p central-server
```

## Automated E2E script
1. Script:
- `scripts/e2e-central-ingestion.ps1`

2. Run:
```powershell
powershell -ExecutionPolicy Bypass -File scripts/e2e-central-ingestion.ps1
```

3. Output:
- central logs in `data/e2e/central-server.log` and `data/e2e/central-server.err.log`
- if `psql` is available, table counters are printed after publish burst.

## API quick checks (Phase C initial)
With `CENTRAL_API_ENABLED=true` and `CENTRAL_API_BIND=127.0.0.1:8088`:

1. Live:
```powershell
Invoke-RestMethod "http://127.0.0.1:8088/health/live"
```

2. Current edges:
```powershell
Invoke-RestMethod "http://127.0.0.1:8088/api/edges/current?limit=50"
```

3. Current tags:
```powershell
Invoke-RestMethod "http://127.0.0.1:8088/api/tags/current?limit=200"
```

4. Tag history (raw ingest):
```powershell
Invoke-RestMethod "http://127.0.0.1:8088/api/tags/tag_hr_0/history?limit=100"
```

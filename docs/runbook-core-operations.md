# Runbook: Core Operations (Failure, Replay, Recovery)

## 1. Objective
Standardize minimum operator/developer actions for incident triage and recovery in core SCADA flows.

## 2. Quick Health Checklist
1. Central API:
```powershell
Invoke-RestMethod "http://127.0.0.1:8088/health/live"
```
2. MQTT reachability:
```powershell
Test-NetConnection 127.0.0.1 -Port 51883
```
3. Edge heartbeat visible in central:
```sql
SELECT e.edge_code, ecs.status, ecs.last_seen_at
FROM edge_current_state ecs
JOIN edges e ON e.id = ecs.edge_id
ORDER BY ecs.last_seen_at DESC;
```
4. Device lamps source:
```sql
SELECT d.device_code, dcs.state, dcs.reason, dcs.last_seen_at
FROM device_current_state dcs
JOIN devices d ON d.id = dcs.device_id
ORDER BY dcs.last_seen_at DESC;
```

## 3. Failure Scenarios
### 3.1 Broker down
1. Expect edge and central to log MQTT reconnect/errors.
2. Edge keeps polling runtime; outbound messages go to SQLite outbox.
3. Recovery:
   1. Restore broker.
   2. Confirm outbox flush via edge logs and central ingest.

### 3.2 Central DB unavailable
1. Central ingest logs DB errors and must not acknowledge failed ingest path.
2. MQTT broker retains unacked QoS1 flow for re-delivery when consumer recovers.
3. Recovery:
   1. Restore DB.
   2. Restart central consumer.
   3. Validate ingest resume from `telemetry_ingest_events` and `telemetry_samples`.

### 3.3 Edge remote config unavailable
1. Edge attempts `/api/edge/config/check`.
2. On failure, edge starts from verified local signed cache.
3. Recovery:
   1. Restore central API.
   2. Verify new hash check/apply cycle.

## 4. Replay and Recovery Checks
1. Config apply lifecycle e2e:
```powershell
powershell -ExecutionPolicy Bypass -File scripts/e2e-config-apply-restart.ps1
```
2. Central ingestion e2e:
```powershell
powershell -ExecutionPolicy Bypass -File scripts/e2e-central-ingestion.ps1 -PgDsn "$env:CENTRAL_PG_DSN"
```
3. Baseline contracts:
```powershell
powershell -ExecutionPolicy Bypass -File scripts/baseline-contracts.ps1 -ForceStopRunning
```

## 5. Evidence to Collect During Incident
1. `data/dev-run-all` logs (central, edge, web).
2. Last 200 lines from central and edge process logs.
3. SQL snapshots:
   - `edge_current_state`
   - `connection_current_state`
   - `device_current_state`
   - `tag_current_state`
   - `operational_events` (last 500 rows)
4. Active `.env` files used to start each process.

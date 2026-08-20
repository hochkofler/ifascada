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

### 3.4 Edge shows "connected" but central ingests no new data for it
Symptom: the edge's own log is healthy (`mqtt publish ok topic='.../conn/state'`, `.../health/runtime`, no reconnect errors), and other edges are being ingested fine, but `docker logs ifascada-central-server | Select-String <edge_code>` returns nothing for this one specific edge.

Root cause: `central-server`'s own MQTT consumer client (`central-server-01`) uses a **persistent session** (`clean_session=false`). If the broker (mosquitto) got restarted one or more times while `central-server` itself kept running, that persistent session can desync — it still reports the correct wildcard subscription (`scada/+/edge/+/#`) at boot, but stops actually receiving messages from edges that (re)connected after the broker instability. This is not something the edge or the broker restart fixes on their own; only central's own session needs refreshing.

Recovery:
1. `docker restart ifascada-central-server` (only this container — not mosquitto, not the full stack).
2. Confirm: `docker logs ifascada-central-server --tail 100 | Select-String '<edge_code>'` should show fresh `ingest parsed topic=...` lines within seconds.

**Rule of thumb:** if mosquitto was restarted for any reason (intentionally or not) while `central-server` kept running, restart `central-server` once afterward too, even if nothing looks obviously broken yet.

**Related, edge side:** the same class of problem — a session the broker considers dead that the client still believes is alive — was independently confirmed on an edge during the 2026-08-18 incident, and went undetected for 1h13m with a fully "healthy-looking" edge log the whole time. See `docs/finding-mqtt-client-stale-session-detection.md` for the full evidence and proposed engineering fix (this is a code-level gap, not something a runbook step can fully close — a manual process restart is today's only recovery on either side).

### 3.5 Edge connects and opens the serial port, but reads zero/garbled bytes
Symptom: `serial-ascii connected on COMx (baud=..., ...)` appears in the edge log, but there are no further lines for that connection — no `failed to parse scale line` warnings either (there is nothing to fail to parse), and no `telemetry/tag/<tag_id>` publish ever appears even after a real weight change on the scale. A raw listen on the port (see snippet below) shows 0 bytes, or an occasional short burst of unreadable garbage.

Common cause: generic USB-to-serial adapters (e.g. CH340) getting into a bad state — typically after physical cable/USB handling while wiring a new device, host reboots, or many rapid open/close cycles during diagnostics on the same port.

Raw port check (stop the edge first, it holds the port open):
```powershell
Stop-ScheduledTask -TaskName 'ifascada-edge'
Get-Process edge-agent -ErrorAction SilentlyContinue | Stop-Process -Force
$p = New-Object System.IO.Ports.SerialPort COM5,9600,None,8,One
$p.ReadTimeout = 20000; $p.Open()
$p.ReadExisting()   # run repeatedly for ~20s while triggering a weight change
$p.Close()
```

Fix (no physical access to the machine required — a PnP-level "unplug/replug"):
```powershell
Get-PnpDevice -Class Ports | Where-Object FriendlyName -like '*CH340*' | ForEach-Object {
    Disable-PnpDevice -InstanceId $_.InstanceId -Confirm:$false
    Start-Sleep -Seconds 2
    Enable-PnpDevice -InstanceId $_.InstanceId -Confirm:$false
}
Stop-ScheduledTask -TaskName 'ifascada-edge'
Get-Process edge-agent -ErrorAction SilentlyContinue | Stop-Process -Force
Start-ScheduledTask -TaskName 'ifascada-edge'
```
Verify by triggering a real weight change and confirming `mqtt publish ok topic='.../telemetry/tag/<tag_id>'` appears in the edge log — `conn/state`/`health/runtime` alone are not proof the device itself is producing data.

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

## 6. Adding a New Edge or Device — Do's and Don'ts

**Context:** on 2026-08-18, adding a single new `SerialAscii` scale (`lcc01` / COM6) turned into a multi-hour, plant-wide "edges won't connect" incident. None of it was a code bug — see the timeline below. This section exists so it does not happen again.

**DO:**
1. Add the new connection/device/tags purely via SQL (see `crates/central-server/migrations/`) plus a signed runtime config update. `SerialAscii` needs no new Rust code and no container restart — it is fully config-driven.
2. Before assuming anything is broken, confirm the edge actually pulled the new config: `POST /api/edge/config/check` for that `edge_id` should return `"accepted":true`, and `"config_changed":true` if it hasn't applied it yet.
3. If mosquitto had to be restarted for any reason, restart `central-server` once afterward too (see 3.4) — don't wait for a symptom to show up first.
4. When "connected" but "no data": rule out (in order) — (a) device only reports on-change, not continuously; (b) USB-serial adapter flakiness (3.5); (c) central's stale MQTT session (3.4) — before touching parser/regex config.

**DON'T:**
1. Don't restart the whole Docker stack (`docker compose restart`/`up`) to "make sure" a new device shows up. Every restart drops *every* edge's live MQTT session plant-wide, and if repeated faster than edges can stabilize, it looks exactly like "nothing connects" incident-wide.
2. Don't leave a test/demo edge-agent instance running with the same `edge_id`/`MQTT_CLIENT_ID` as a real plant edge — even on the central host itself. (Known cleanup item: the central host currently runs its own `ifascada-edge` scheduled task locally, cycling through test identities such as `edge-com-01`/`edge-scale-com3-test`. It did not cause the 2026-08-18 incident, but is a standing risk and should be disabled when not actively needed for testing.)
3. Don't conclude "software is broken" from a UI status light alone — connection state (green/red) and actual tag telemetry are ingested and can fail independently (3.4).

**2026-08-18 incident timeline, for reference** (cross-checked to the second between the edge host and the central host — see `docs/finding-mqtt-client-stale-session-detection.md` for the full evidence table):
1. Mosquitto was restarted manually 4 times between 11:47 and 12:32 UTC while wiring the new scale. Each restart drops every edge's live MQTT session plant-wide.
2. After the first restart, the edge (`lcc01`) reconnected at 11:47:15 UTC but was kicked by the broker 16s later (11:47:31, missed keepalive) — and then **silently believed it stayed connected for the next 1h13m** (kept logging `mqtt publish ok` with no reconnect error), while mosquitto shows zero trace of it until 13:00:40 UTC. This is a client-side bug, not just restart churn (3.4/finding doc) — the 3 subsequent mosquitto restarts (12:07, 12:14, 12:32) happened entirely inside this undetected gap.
3. What actually ended the gap was an unrelated manual restart of the edge-agent process at 13:00:36 UTC (done for other troubleshooting reasons) — not any self-healing.
4. Separately, `central-server`'s own persistent MQTT session had desynced from the same round of broker restarts and stopped ingesting `lcc01`'s messages despite a healthy wildcard subscription — fixed by `docker restart ifascada-central-server` (3.4).
5. Separately again, the 4 USB-serial (CH340) adapters on the edge host had gotten into a bad state from the same physical/USB disruption while wiring the new scale — ports opened but produced no readable data (3.5) until each was cycled at the PnP level.
6. Total distinct root causes: 3 (broker restart churn during setup, an undetected dead MQTT session on both edge and central requiring manual restarts, and flaky USB-serial adapters) — none of them a config or parser bug. The one item that *is* a code-level gap (item 2) is tracked separately in `docs/finding-mqtt-client-stale-session-detection.md`.

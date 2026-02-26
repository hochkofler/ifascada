# UC-CENTRAL-DEVICE-001: Device Status with Hybrid Persistence (Postgres + Redis)

## Goal
Provide reliable and scalable `device_status` for SCADA HMI lamps and audit without coupling status to tag quality payload.

## Scope
1. Compute `device_status` in central from:
   - `edge_current_state`
   - `connection_current_state`
   - `tag_status` aggregate for the device
2. Persist `device_current_state` in PostgreSQL as source of truth.
3. Persist `device.status.*` events only on real transitions.
4. Publish realtime device status snapshots to Redis (cache/fan-out only).

## State Rules
1. Connection-first policy:
   - edge offline/stale -> `disconnected`
   - connection `failed/disconnected` -> `disconnected`
   - connection `connecting/unknown` -> `stale`
2. If connection is healthy:
   - any tag connected -> `connected`
   - else any tag stale -> `stale`
   - else -> `disconnected`

## Anti-flapping Rules
1. Hysteresis:
   - `connected -> stale`: 1 consecutive evaluation
   - `stale -> disconnected`: 2 consecutive evaluations
   - `disconnected -> connected`: 2 consecutive evaluations
2. Debounce:
   - suppress repeated transitions inside 5 seconds, except recovery (`disconnected -> connected`).

## Acceptance Criteria
1. `GET /api/devices/current` returns deterministic state and counters.
2. No duplicated `device.status.*` events on unchanged input.
3. Device lamps in Live view use backend `device_status` (not quality payload).
4. Redis publish failures do not break PostgreSQL persistence.
5. Heartbeat timeout forces effective `disconnected` status in current APIs even if no new telemetry arrives.

## Test Mapping
Implemented in:
1. `crates/central-server/tests/api_device_status_contract_tests.rs`
   - `devices_current_endpoint_contract`
   - `device_status_transitions_are_not_duplicated`
2. `crates/central-server/tests/api_runtime_status_heartbeat_contract_tests.rs`
   - `edges_current_marks_disconnected_when_heartbeat_expired`
   - `devices_current_marks_disconnected_when_edge_heartbeat_expired`
   - `tags_current_marks_disconnected_when_edge_heartbeat_expired`
3. `crates/domain/src/device/status.rs`
   - domain unit tests for connectivity precedence.

## Implementation Mapping
1. Domain:
   - `crates/domain/src/device/status.rs`
2. Central persistence:
   - `crates/central-server/src/persistence/postgres.rs`
3. Central API:
   - `crates/central-server/src/api.rs`
4. Live UI:
   - `web-ui/app/live/page.tsx`

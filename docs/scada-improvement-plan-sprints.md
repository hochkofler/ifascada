# SCADA Improvement Plan (Sprints)

> Note: this sprint file is maintained as an execution log.
> The consolidated modernization roadmap is now in:
> `docs/scada-modernization-master-plan.md`

## Sprint 1 - Stabilization and Operability (in progress)

### Goal
Stabilize local/dev operation and remove non-deterministic behavior before adding more features.

### Work items
1. Startup reliability for dev stack
- Add Postgres readiness wait before migrations.
- Ensure migration commands fail fast on non-zero `psql` exit code.
- Status: done in `scripts/dev-run-all-local.ps1`.

2. Process shutdown reliability
- Ensure complete process tree termination (`npm -> node` children) at script stop.
- Add explicit port-release verification (`8088`, `ui`) after stop.
- Status: done in `scripts/dev-run-all-local.ps1`, `scripts/verify-stop-ports.ps1`.

3. API/HMI real-time contract hardening
- SSE default no-replay.
- SSE filtering by context (`site`, `edge`, `tag`, `exclude_raw`).
- Ignore out-of-context tag updates in live view.
- Automated contract tests in `crates/central-server/tests/api_sse_contract_tests.rs`.
- Status: done (`crates/central-server/src/api.rs`, `web-ui/lib/sse.ts`, `web-ui/app/live/page.tsx`).

4. Live UI stability
- Remove chart animations.
- Fix table layout jitter and overflow.
- Surface quality reason as tooltip instead of inline layout expansion.
- Status: done (`web-ui/components/trend-chart.tsx`, `web-ui/app/live/page.tsx`, `web-ui/app/globals.css`).

5. Historian write policy baseline
- Persist historian samples only when one of these happens:
  - value change above deadband,
  - quality transition,
  - max interval elapsed.
- Keep `tag_current_state` updates on every telemetry.
- Status: done in `crates/central-server/src/persistence/postgres.rs`.

### Acceptance criteria
1. `dev-run-all-local.ps1` does not continue to app start if migrations fail.
2. After stop, UI port (e.g. 3015) is released.
3. Refreshing `/live` does not inject historical events by default.
4. Live table columns keep fixed proportions with long values.

## Sprint 2 - Contextual HMI and Navigation

### Goal
Move from flat telemetry view to operational context (`site/line/area/cell/edge/device/tag`).

### Work items
1. Extend central schema with context hierarchy (`line`, `area`, `cell`).
2. Expose context filters in API and SSE.
3. Add HMI context selector and scoped live views.
4. Keep per-view stream subscriptions (avoid global stream).

### Acceptance criteria
1. Operator can drill down context and only see relevant tags.
2. SSE traffic per screen is bounded by selected context.

## Sprint 2.1 - Device Connectivity State (implemented)

### Goal
Make `device` lamps deterministic and auditable using explicit connectivity state independent from tag quality payload.

### Work items
1. Add `device_current_state` snapshot table in central PostgreSQL.
2. Derive device state from `edge_current_state + connection_current_state + tag_status`.
3. Persist `device.status.*` only on transitions (no spam on unchanged state).
4. Publish realtime `device_status` snapshots to Redis channel/cache (non-source-of-truth).
5. Expose `GET /api/devices/current` and consume it in Live HMI for device lamps.

### Acceptance criteria
1. Device lamp does not depend on `quality.reason`.
2. Repeated unchanged events do not duplicate `device.status.*` in `operational_events`.
3. Live view reflects device status by context (`site/line/area/cell/edge/device`).

## Sprint 2.2 - Unified Connection State Policy (next)

### Goal
Align edge/connection/device/tag status semantics under one canonical policy and separate connectivity from data quality.

### Work items
1. Adopt policy in `docs/scada-connection-state-policy.md` as canonical contract.
2. Add explicit tag soft/hard timeout windows (`stale` then `disconnected`).
3. Standardize `reason_code` in `/api/*/current`.
4. Integrate `device.connection.*` signals into device status recomputation.
5. Add integration tests for realtime transitions (disconnect/reconnect scenarios).

### Acceptance criteria
1. Tag without fresh sample cannot stay `connected` indefinitely.
2. Lamp color is driven only by `state`; quality is rendered separately.
3. Audit shows deterministic transition sequence without duplicate spam.

## Sprint 3 - Catalog/Configuration UI (CMDB)

### Goal
Manage SCADA inventory and naming from UI instead of manual SQL.

### Work items
1. CRUD for sites/lines/edges/devices/tags.
2. Naming validations in API.
3. Catalog-driven tag activation flags for HMI visibility.

### Acceptance criteria
1. New tags/devices can be created and activated from UI.
2. Invalid naming is rejected with actionable error.

## Sprint 4 - Quality Policy and Semantics

### Goal
Define and apply explicit quality semantics (not just Good/Bad without cause).

### Work items
1. Standard reason codes (`Timeout`, `CommFailure`, `ParseError`, `Stale`, `Uncertain`).
2. Add transition metadata (`since`, `last_transition_at`).
3. HMI badges for `comms_status`, `freshness`, and `quality`.
4. Operator runbook page for interpretation.

### Acceptance criteria
1. Any non-Good quality includes explicit reason code.
2. HMI distinguishes connectivity vs data freshness vs value quality.

## Sprint 5 - Centralized Edge Configuration Lifecycle

### Goal
Central desired/applied config management with safe rollout.

### Work items
1. Store versioned edge config in DB.
2. Publish desired config via broker.
3. Edge reports applied version and apply result.
4. Rollback support.

### Acceptance criteria
1. UI shows desired vs applied config version per edge.
2. Failed apply is auditable with rollback path.

# SCADA Target Architecture Blueprint

## 1. Purpose
Define the target end-to-end architecture for IFASCADA with robust edge autonomy, centralized governance, and real-time visualization.

## 2. Architectural Principles
1. Clean Architecture: domain and use cases are independent from infrastructure.
2. SOLID: each service has explicit responsibilities and stable interfaces.
3. Event-driven first: MQTT is the operational backbone between edge and central.
4. Edge autonomy: edge keeps operating even with temporary central/broker issues.
5. Command governance: manual command is an exception path, audited and controlled from central/frontend.

## 3. High-Level Components
1. Edge Agent
- Connects to field protocols (Modbus RTU/TCP, Serial, etc.).
- Parses and normalizes tag data.
- Publishes telemetry/events through MQTT.
- Handles write commands with priority, idempotency, rate-limit, and circuit breaker.
- Persists outbox locally for store-and-forward.

2. MQTT Broker
- Transport backbone for telemetry, commands, health, alerts, and acknowledgements.
- No business logic.

3. Central Ingestion Service
- Subscribes to edge topics.
- Validates schema/version/source.
- Persists telemetry, write audit, health, alerts, and edge heartbeat/state.

4. Central Configuration Service
- Manages edge bootstrap/config versions.
- Publishes config updates and tracks applied versions.

5. Central API Service
- Query API for frontend (tags, states, history, trends, command audit).
- Real-time stream API (SSE or WebSocket) for current values and status changes.
- Command API endpoint that publishes command messages to MQTT.

6. Web Frontend (Angular)
- Real-time dashboard for edge state, tag values, quality, alarms.
- Historical trends and reports from central API.
- Manual command UI for authorized operators.

## 4. Runtime Data Flows
1. Telemetry flow (main)
- Protocol read in edge -> runtime normalization -> MQTT telemetry topic -> central ingestion -> time-series persistence -> API/SSE -> frontend.

2. Command flow (minority, controlled)
- Frontend command action -> central API validation/authorization -> MQTT `cmd/write` -> edge execution -> MQTT `cmd/write/ack` + `audit/write` -> central persistence -> frontend feedback.

3. Health and alert flow
- Edge publishes runtime health and alerts -> central persists and evaluates -> frontend status/notification.

4. Config flow
- Central emits versioned config -> edge pulls/applies -> edge reports effective version + health.

## 5. MQTT Contract Baseline
Namespace:
- `scada/{site}/edge/{agent}/...`

Core topics:
1. Telemetry by tag
- `telemetry/tag/{tag_id}`

2. Command write
- `cmd/write`

3. Command ack
- `cmd/write/ack`

4. Write audit
- `audit/write`

5. Runtime health
- `health/runtime`

6. Runtime alerts
- `alerts/runtime`
- `alerts/runtime/ack`
- `alerts/runtime/ack/result`

Message baseline:
- `schema_version`
- `source`
- domain payload
- `timestamp`

## 6. Persistence Model (Central)
1. Operational state (current)
- edge online/offline status
- latest tag value and quality
- active alerts

2. Historical time series
- tag value history (for trend and reports)
- retention policy by tag class

3. Audit and command trail
- write command records: requested, applied/rejected/deduplicated, reason
- full trace by `command_id`, `tag_id`, `edge`, `operator`

4. Config and inventory
- edge definitions, connection profiles, protocol mappings, version history

Suggested storage split:
- relational DB for configuration, audit, metadata
- time-series optimized storage for telemetry history

## 7. API and Real-Time Interface (Central)
1. Query endpoints (examples)
- `GET /api/edges`
- `GET /api/edges/{edge_id}/status`
- `GET /api/tags/current?edge_id=...`
- `GET /api/tags/{tag_id}/history?from=...&to=...&interval=...`
- `GET /api/commands/{command_id}`

2. Command endpoint
- `POST /api/commands/write`
- Validates role, target, bounds, and optional priority.
- Emits MQTT command and stores intent.

3. Real-time stream
- `GET /api/stream/events` (SSE)
- Emits edge status, tag updates, alert transitions, command outcomes.

## 8. Edge Reliability Rules
1. Outbox persistence for MQTT publish failures.
2. Bounded outbox size with safe discard policy.
3. Priority queue for writes (`high` before `normal` in pending queue).
4. Write idempotency by (`tag_id`, `command_id`) window.
5. Write rate limit and circuit breaker per connection.
6. Tag quality transitions for timeout/communication failure.

## 9. Security and Governance
1. Broker auth (per edge identity).
2. Topic ACLs per edge/site.
3. Command authorization in central API (role-based).
4. End-to-end audit of manual commands.
5. Optional payload signing/encryption for sensitive channels.

## 10. Difference vs Previous Simpler Architecture
1. Before: central was the main control point and edge acted mostly as adapter.
2. Now: edge has stronger local runtime capabilities and resilience.
3. Before: limited protection for transient broker/network issues.
4. Now: store-forward + health + alerts + write governance increase operational robustness.
5. Before: manual commands could be a direct dominant path.
6. Now: manual command is intentionally minority, centralized, audited, and policy-driven.

## 11. Recommended Implementation Phases
1. Phase A: stabilize edge protocol/runtime core
- complete Modbus RTU/TCP shared core, batching, conversions, retries.

2. Phase B: central ingestion + persistence
- telemetry ingestion, command audit ingestion, health/alerts ingestion.

3. Phase C: central API + SSE for frontend
- current state + historical queries + real-time feed.

4. Phase D: command governance from frontend
- operator auth, validation rules, full traceability.

5. Phase E: production hardening
- broker HA, retention strategy, backups, observability and SLOs.

## 12. Target Operating Model
1. 99% autonomous operation via edge runtime and protocol loops.
2. Central provides visibility, governance, analytics, and controlled command issuance.
3. Frontend is the operator interface, not the real-time control engine.

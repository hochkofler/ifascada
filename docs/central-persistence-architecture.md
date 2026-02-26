# Central Persistence Architecture (PostgreSQL + TimescaleDB + Redis)

## 1. Objective
Define a production-grade persistence architecture for the SCADA central platform.

Design goals:
1. Reliable operational state and command governance.
2. Efficient long-term telemetry history for trends and analytics.
3. Low-latency real-time fan-out for frontend.
4. Clear separation between domain/application and infrastructure adapters.

## 2. Recommended Stack
1. PostgreSQL 16+
- System of record for configuration, metadata, command/audit, alarm lifecycle, and current state.

2. TimescaleDB extension on PostgreSQL
- Historian layer for telemetry time-series as hypertables with compression and retention policies.

3. Redis 7+
- Real-time cache and pub/sub fan-out support for SSE/WebSocket gateways.

## 3. Why Not SQLite in Central
SQLite remains valid for edge local outbox, but central needs:
1. Concurrent writes from many edges.
2. Better horizontal read scaling.
3. Rich retention/compression/window analytics.
4. Operational HA and backup tooling.

## 4. Logical Data Domains
1. Config Domain (PostgreSQL)
- Sites, edges, devices, connections, tags, protocol mappings, config versions.

2. Runtime State Domain (PostgreSQL + Redis cache)
- Current edge health, current tag value/quality, active alerts.

3. Historian Domain (TimescaleDB hypertables)
- Tag telemetry history for trend/reporting.

4. Governance Domain (PostgreSQL)
- Command intent, command ack, command audit trail, operator actions.

## 5. Topic-to-Persistence Mapping
MQTT namespace baseline:
- `scada/{site}/edge/{agent}/...`

Persistence routing:
1. `telemetry/tag/{tag_id}` -> `telemetry_samples` hypertable + update `tag_current_state`.
2. `health/runtime` -> `edge_health_events` + update `edge_current_state`.
3. `alerts/runtime` -> `runtime_alert_events` + update `runtime_alert_active`.
4. `cmd/write/ack` -> `command_ack_events`.
5. `audit/write` -> `command_audit_events`.
6. `conn/state` + `telemetry/tag` + `health/runtime` -> recompute `device_current_state` (write only on transition).

## 6. Proposed Core Schema

### 6.1 Configuration Tables (PostgreSQL)
1. `sites(id, code, name, timezone, created_at)`
2. `edges(id, site_id, edge_code, name, status, created_at, updated_at)`
3. `devices(id, edge_id, device_code, name, driver_type, metadata_json, created_at)`
4. `tags(id, edge_id, device_id, tag_code, name, value_type, source, unit, metadata_json, created_at)`
5. `edge_config_versions(id, edge_id, version, payload_json, created_at, applied_at, status)`

Key indexes:
- `edges(site_id, edge_code)` unique
- `tags(edge_id, tag_code)` unique
- `devices(edge_id, device_code)` unique

### 6.2 Runtime Current State (PostgreSQL)
1. `edge_current_state(edge_id PK, status, last_seen_at, outbox_depth, outbox_oldest_secs, updated_at)`
2. `tag_current_state(tag_id PK, ts, value_json, quality_json, source, updated_at)`
3. `connection_current_state(connection_id PK, state, severity, reason, payload_json, last_change_at, last_seen_at, updated_at)`
4. `device_current_state(device_id PK, state, severity, reason, connection_id, tags_connected, tags_stale, tags_disconnected, last_change_at, last_seen_at, payload_json, updated_at)`
5. `runtime_alert_active(alert_key PK, edge_id, alert_type, severity, first_raised_at, last_seen_at, payload_json)`
6. `edges.metadata_json` for edge-level runtime tuning and policy overrides.

Key indexes:
- `edge_current_state(last_seen_at)`
- `tag_current_state(updated_at)`
- `connection_current_state(state, last_change_at)`
- `device_current_state(state, last_change_at)`
- `runtime_alert_active(edge_id, alert_type)`

### 6.3 Historian (TimescaleDB)
1. `telemetry_samples`
- columns: `ts timestamptz`, `site_id`, `edge_id`, `tag_id`, `quality_status`, `value_num`, `value_bool`, `value_text`, `value_json`, `source`
- hypertable on `ts`, recommended chunk interval: 1 day (adjust by ingest rate).

Indexes:
- `(tag_id, ts DESC)`
- `(edge_id, ts DESC)`
- `(site_id, ts DESC)`

Policies:
- compression after 7 days
- retention after 180/365 days (by class)
- continuous aggregates for 1m/5m/1h windows

### 6.4 Command Governance (PostgreSQL)
1. `command_requests(id uuid PK, command_id unique, site_id, edge_id, tag_id, requested_value_json, priority, requested_by, requested_at, status)`
2. `command_ack_events(id bigserial PK, command_id, edge_id, tag_id, success, reason, payload_json, ts)`
3. `command_audit_events(id bigserial PK, command_id, edge_id, tag_id, outcome, reason, value_json, ts)`

Indexes:
- `command_requests(command_id)` unique
- `command_ack_events(command_id, ts DESC)`
- `command_audit_events(command_id, ts DESC)`
- `command_audit_events(tag_id, ts DESC)`

### 6.5 Health/Alert History (PostgreSQL)
1. `edge_health_events(id bigserial PK, edge_id, status, payload_json, ts)`
2. `runtime_alert_events(id bigserial PK, edge_id, alert_type, state, severity, payload_json, ts)`

Indexes:
- `edge_health_events(edge_id, ts DESC)`
- `runtime_alert_events(edge_id, alert_type, ts DESC)`

## 7. Redis Responsibilities
Use Redis as acceleration, not source of truth.

1. Real-time cache keys
- `scada:edge:{edge_id}:status` (hash)
- `scada:tag:{tag_id}:current` (hash/json)
- `scada:alert:{edge_id}:active` (set/hash)

2. Stream / pub-sub fan-out
- channel `scada:rt:events` for SSE gateway push.

3. Optional command response correlation
- short-lived key by `command_id` with TTL for fast UI acknowledgement.
4. Device status cache
- `scada:device:{site}:{edge}:{device}:status` with rolling TTL (30-120s).

5. TTL strategy
- current state keys with rolling refresh and TTL safety (e.g., 2-5 minutes)
- avoid permanent business data in Redis.

## 8. Ingestion Service Design (Central)
1. MQTT consumer group responsibilities
- parse topic and payload
- validate `schema_version` and `source`
- idempotency key generation (topic + edge + command_id + ts/message hash)
- write to PostgreSQL/Timescale in transaction when required
- publish cache updates to Redis

2. Idempotency strategy
- unique constraints where possible (`command_id`, dedup hashes)
- ingestion dedup table for at-least-once MQTT deliveries.

3. Backpressure strategy
- bounded internal queue
- retry policy with DLQ table/topic for poison messages.

## 9. API/SSE Read Strategy
1. Current views
- Prefer Redis for hot current reads; fallback to PostgreSQL.

2. Historical views
- Query TimescaleDB directly.

3. Command trace
- Query PostgreSQL command tables by `command_id`, `tag_id`, and time windows.

4. SSE
- API process subscribes to Redis channel or DB NOTIFY pipeline.

## 10. HA, Backup, and Operations
1. PostgreSQL/Timescale
- primary + replica (streaming replication)
- WAL archiving and PITR backup policy
- migration tool (`sqlx migrate` or `refinery`) with controlled rollout

2. Redis
- persistence mode AOF (if needed) + replica/sentinel for HA
- acceptable data-loss posture: cache-only semantics

3. Observability
- ingestion lag, write latency, error rates, dropped/invalid messages
- historian cardinality and storage growth alerts

## 11. Retention and Downsampling Policy (Example)
1. Raw telemetry
- keep 30 days uncompressed.

2. Compressed telemetry
- keep 365 days compressed.

3. Aggregates
- 1-minute aggregates keep 2 years.
- 1-hour aggregates keep 5 years.

4. Audit/commands
- keep 2-5 years (compliance dependent).

## 12. Implementation Plan (Pragmatic)
1. Phase B1
- create central ingestion crate modules and DB schema migrations for PostgreSQL/Timescale.
- ingest `telemetry/tag`, `health/runtime`, `audit/write`, `cmd/write/ack`.

2. Phase B2
- add Redis cache updater and SSE event bus integration.

3. Phase B3
- add idempotency table + DLQ + replay tooling.

4. Phase C
- API query endpoints + SSE gateway with role-based command endpoint.

## 13. Clean Architecture Boundaries
1. Domain/Application layers define repository traits and use cases.
2. Infrastructure layer implements:
- Postgres/Timescale repositories
- Redis cache adapters
- MQTT consumer adapter
3. No domain type depends on broker, Redis, or SQL driver specifics.

## 14. Decision Summary
For central-server production:
1. Use PostgreSQL + TimescaleDB as primary persisted platform.
2. Use Redis for acceleration and real-time distribution.
3. Keep SQLite only at edge for local outbox/offline resilience.

## 15. Heartbeat-Derived Runtime Status
To avoid stale "active" lamps when an edge stops publishing:
1. `edge_current_state.last_seen_at` is treated as heartbeat source of truth.
2. API current views derive effective status by age:
- if `now - last_seen_at > edge_stale_after_secs` then edge/device effective status is `disconnected`.
3. Global fallback threshold is configurable by env:
- `CENTRAL_EDGE_STALE_AFTER_SECS_DEFAULT` (default `45`).
4. This derivation is independent of new telemetry arrival, so status changes still appear when stream is silent.
5. Frontend live view must refresh current endpoints periodically (not only SSE) to reflect heartbeat timeout transitions.

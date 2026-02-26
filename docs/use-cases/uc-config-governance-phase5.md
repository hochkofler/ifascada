# UC: Config Governance from Central DB (Phase 5)

## Goal
Keep Central DB as source of truth for runtime config, with deterministic signed delivery to edge and offline fallback via verified local cache.

## Contract
1. Edge enroll:
   - `POST /api/edge/config/enroll`
   - Validates `enrollment_token`
   - Returns current `config_hash`
2. Edge check:
   - `POST /api/edge/config/check`
   - Compares `current_config_hash` vs central hash
   - Returns `config_changed` + `target_config_hash`
3. Edge runtime fetch:
   - `GET /api/edge/config/runtime?edge_id=...&want_hash=...`
   - Returns signed envelope (`hmac-sha256`) when changed
   - Returns `304 Not Modified` when hash matches

## Integrity Rules
1. Envelope verification on edge:
   - `edge_id` must match local agent id
   - `algorithm = hmac-sha256`
   - optional strict `key_id` check
   - `config_hash = sha256(payload_json)`
   - `signature_hex = HMAC(signing_secret, config_hash_bytes)`
2. On successful verification, edge writes cache file (`EDGE_RUNTIME_CACHE_PATH`).
3. If central is unavailable, edge starts from verified local signed cache.

## Apply Workflow
1. Edge receives `config/apply` command by MQTT.
2. If a staged hash differs from current hash, edge writes apply receipt (`EDGE_CONFIG_APPLY_RECEIPT_PATH`) and requests restart.
3. On restart, edge publishes apply result with `current_config_hash` and `target_config_hash`.

## Tests Added
1. `crates/central-server/tests/api_edge_config_contract_tests.rs`
   - token enforcement
   - deterministic hash check
   - runtime envelope shape
   - `want_hash -> 304`
2. `crates/edge-agent/src/mqtt_bridge.rs`
   - config apply parse backward compatibility
   - apply receipt write/read/remove roundtrip

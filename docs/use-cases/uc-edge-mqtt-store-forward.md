# UC-EDGE-004: MQTT Store-and-Forward for ACK and Audit

## Goal
Guarantee delivery resilience for MQTT ACK and audit messages when broker connectivity is intermittent.

## Scope
1. Edge-agent persists failed MQTT publishes in a local SQLite outbox.
2. Edge-agent retries pending outbox messages before new publishes.
3. Outbox survives process restart.

## Configuration
1. `MQTT_OUTBOX_PATH` default: `./data/mqtt_outbox.db`
2. `MQTT_OUTBOX_FLUSH_BATCH` default: `50`
3. `MQTT_OUTBOX_MAX_MESSAGES` default: `10000`
4. `MQTT_OUTBOX_ENCRYPTION_SECRET` optional (enables AES-GCM payload encryption at rest)
5. `MQTT_OUTBOX_HMAC_SECRET` optional (enables payload signature verification)
6. `MQTT_OUTBOX_ACTIVE_KEY_ID` default: `v1`
7. `MQTT_OUTBOX_PREV_KEY_ID` optional
8. `MQTT_OUTBOX_PREV_ENCRYPTION_SECRET` optional
9. `MQTT_OUTBOX_PREV_HMAC_SECRET` optional

## Capacity Policy
1. Outbox applies bounded size (`MQTT_OUTBOX_MAX_MESSAGES`).
2. ACK messages have priority over audit messages.
3. When full:
   - incoming ACK evicts oldest audit first (or oldest any if no audit exists).
   - incoming audit is dropped if only ACK messages remain.

## Security Policy
1. If `MQTT_OUTBOX_ENCRYPTION_SECRET` is set, outbox stores encrypted payloads (AES-GCM).
2. If `MQTT_OUTBOX_HMAC_SECRET` is set, outbox stores/validates HMAC-SHA256 signatures.
3. Corrupted/unverifiable rows are dropped during flush to avoid deadlock in queue processing.
4. Each row stores `crypto_version` and `key_id` to support key rotation.
5. During rotation, edge-agent can verify/decrypt rows signed/encrypted with previous key material.

## Test Mapping
Implemented in `crates/edge-agent/src/mqtt_outbox.rs`:

1. `test_outbox_enqueue_persists_and_reloads`
2. `test_outbox_flush_success_drains_queue`
3. `test_outbox_flush_failure_keeps_queue`
4. `test_outbox_capacity_preserves_ack_priority`
5. `test_outbox_security_encrypts_and_signs_roundtrip`
6. `test_outbox_key_rotation_reads_old_messages_with_previous_key`

## Implementation Mapping
Implemented in:

1. `crates/edge-agent/src/mqtt_outbox.rs`
   - `PersistentMqttOutbox`
   - persisted pending queue in SQLite
   - capacity limit + safe discard strategy
   - optional payload encryption/signature
   - batch flush with retry behavior
2. `crates/edge-agent/src/mqtt_bridge.rs`
   - `publish_with_outbox(...)`
   - ack/audit publishing uses outbox fallback
   - periodic outbox flush in runtime loop
3. `crates/edge-agent/src/main.rs`
   - outbox configuration wiring from env vars

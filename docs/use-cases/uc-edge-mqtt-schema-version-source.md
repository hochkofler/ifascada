# UC-EDGE-003: MQTT Schema Versioning and Source Metadata

## Goal
Add schema version and source metadata to MQTT command/ack/audit payloads for traceability and safe evolution.

## Scope
1. Incoming command supports optional `schema_version` and `source` (backward compatible).
2. Outgoing ACK includes `schema_version` and `source`.
3. Outgoing audit includes `schema_version` and `source`.

## Contract
1. `schema_version`: `1` for current contract.
2. `source`: identifier of producer (`edge/{agent}` for edge publishes).

## Test Mapping
Implemented in `crates/edge-agent/src/mqtt_bridge.rs`:

1. `test_parse_write_command_message`
2. `test_parse_write_command_message_backward_compatible`
3. `test_build_write_command_ack_success`
4. `test_build_write_command_ack_error`
5. `test_build_invalid_payload_ack`
6. `test_to_write_audit_message_maps_runtime_event`

## Implementation Mapping
Implemented in:

1. `crates/edge-agent/src/mqtt_bridge.rs`
   - version/source fields in command/ack/audit models
   - constant schema version `1`
   - edge source propagation in publish path

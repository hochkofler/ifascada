# UC-EDGE-001: MQTT Write Command + Audit Bridge

## Goal
Enable edge-agent communication through MQTT for write commands and write audit events.

## Scope
1. Edge-agent subscribes to a write command topic.
2. Incoming command payload executes `RuntimeEngine` write API, preserving `command_id` idempotency path.
3. Edge-agent publishes `TagWriteCommandHandled` runtime events to MQTT audit topic.

## MQTT Contract
Topic conventions:

1. Command: `scada/{site}/edge/{agent}/cmd/write`
2. Audit: `scada/{site}/edge/{agent}/audit/write`

Command payload:

```json
{
  "tag_id": "tag1",
  "value": 42.0,
  "command_id": "cmd-123"
}
```

## Test Mapping
Implemented in `crates/edge-agent/src/mqtt_bridge.rs`:

1. `test_execute_write_command_uses_command_id_path`
2. `test_execute_write_command_without_command_id`
3. `test_parse_write_command_message`
4. `test_to_write_audit_message_maps_runtime_event`
5. `test_topics_follow_convention`

## Implementation Mapping
Implemented in:

1. `crates/edge-agent/src/mqtt_bridge.rs`
   - MQTT bridge loop (`run_mqtt_bridge`)
   - command parser + executor abstraction (`WriteCommandExecutor`)
   - runtime audit event -> MQTT message mapping
2. `crates/edge-agent/src/main.rs`
   - optional MQTT mode by env flag `EDGE_MQTT_ENABLED`
3. `crates/edge-agent/Cargo.toml`
   - MQTT/serialization dependencies (`rumqttc`, `serde`, `serde_json`, `chrono`)

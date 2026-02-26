# UC-EDGE-002: MQTT Write Command Acknowledgement

## Goal
Return explicit acknowledgement for each MQTT write command handled by edge-agent.

## Scope
1. Publish ACK for valid command execution outcome (success/failure).
2. Publish ACK for invalid payloads.
3. Include `tag_id`, optional `command_id`, success flag, reason and timestamp.

## MQTT Contract
1. Command topic: `scada/{site}/edge/{agent}/cmd/write`
2. Ack topic: `scada/{site}/edge/{agent}/cmd/write/ack`

ACK payload:

```json
{
  "tag_id": "tag1",
  "command_id": "cmd-123",
  "success": true,
  "reason": null,
  "timestamp": "2026-01-01T00:00:00Z"
}
```

## Test Mapping
Implemented in `crates/edge-agent/src/mqtt_bridge.rs`:

1. `test_build_write_command_ack_success`
2. `test_build_write_command_ack_error`
3. `test_build_invalid_payload_ack`
4. `test_topics_follow_convention`

## Implementation Mapping
Implemented in:

1. `crates/edge-agent/src/mqtt_bridge.rs`
   - `MqttBridgeConfig::ack_topic()`
   - `WriteCommandAckMessage`
   - `build_write_command_ack(...)`
   - `build_invalid_payload_ack(...)`
   - publish ACK on command handling and invalid payload

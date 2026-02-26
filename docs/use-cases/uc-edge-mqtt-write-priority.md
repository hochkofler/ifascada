# UC-EDGE-006: MQTT Write Priority (high|normal)

## Goal
Allow central systems to mark write commands with execution priority so critical commands are processed before normal writes.

## Command Contract
1. Topic:
   - `scada/{site}/edge/{agent}/cmd/write`
2. Payload fields:
   - existing fields unchanged (`tag_id`, `value`, optional `command_id`)
   - new optional field: `priority`
3. Accepted values:
   - `high`
   - `normal`
4. Backward compatibility:
   - if `priority` is absent, runtime uses `normal`
   - invalid values are rejected (`unsupported write priority`)

## TDD Mapping
1. `crates/edge-agent/src/mqtt_bridge.rs`
   - `test_execute_write_command_uses_command_id_path`
   - `test_execute_write_command_without_command_id`
   - `test_parse_write_command_message`
   - `test_parse_write_command_message_backward_compatible`
   - `test_parse_write_priority_validation`
2. `crates/application/tests/runtime_tests.rs`
   - `test_runtime_write_priority_processes_high_before_normal`
   - `test_runtime_write_priority_burst_drains_high_queue_before_normal_queue`

## Implementation Mapping
1. `crates/edge-agent/src/mqtt_bridge.rs`
   - `WriteTagCommandMessage` extended with `priority`
   - `parse_write_priority(...)`
   - execution path maps MQTT priority to runtime priority
2. `crates/application/src/runtime/runtime_engine.rs`
   - `write_tag_with_priority(...)`
   - `write_tag_with_command_id_and_priority(...)`
3. `crates/application/src/runtime/connection_runtime.rs`
   - dual queues (`pending_writes_high`, `pending_writes_normal`)
   - scheduler always pops high queue first

## E2E Manual Validation
1. Script:
   - `scripts/e2e-mqtt-priority-order.ps1`
2. Prerequisites:
   - MQTT broker reachable (default `127.0.0.1:1883`)
   - tools installed: `mosquitto_pub`, `mosquitto_sub`, `cargo`
3. Run:
   - `powershell -ExecutionPolicy Bypass -File scripts/e2e-mqtt-priority-order.ps1`
4. Expected evidence:
   - `data/e2e/priority-audit.log` contains command audit events with `command_id`
   - early audit window should include `cmd-high-1` ahead of or between normal commands under burst load

## Notes
1. Determinism depends on queue contention (burst timing and runtime poll cadence).
2. For strict deterministic validation in CI, prefer `test_runtime_write_priority_processes_high_before_normal`.

# UC-EDGE-005: Operational Health Telemetry for MQTT Bridge

## Goal
Expose runtime health of edge MQTT command/audit flow for SCADA operations.

## Scope
1. Track operational counters for command handling and publish failures.
2. Track outbox runtime stats (depth and oldest pending age).
3. Publish periodic health messages to MQTT.
4. Emit degraded warnings when thresholds are exceeded.

## MQTT Contract
1. Health topic: `scada/{site}/edge/{agent}/health/runtime`
2. Payload fields:
   - `status` (`ok` | `degraded`)
   - outbox stats (`outbox_depth`, `outbox_oldest_age_secs`)
   - command counters (`cmd_received_total`, `cmd_failed_total`)
   - transport counters (`ack_publish_fail_total`, `audit_publish_fail_total`)
   - outbox counters (`outbox_enqueued_total`, `outbox_flushed_total`)

## Configuration
1. `MQTT_HEALTH_PUBLISH_INTERVAL_SECS` default: `30`
2. `MQTT_HEALTH_OUTBOX_DEPTH_WARN` default: `1000`
3. `MQTT_HEALTH_OUTBOX_OLDEST_SECS_WARN` default: `300`

## Test Mapping
Implemented in `crates/edge-agent/src/mqtt_bridge.rs`:

1. `test_topics_follow_convention`
2. `test_compute_health_status_transitions`

## Implementation Mapping
Implemented in:

1. `crates/edge-agent/src/mqtt_outbox.rs`
   - `OutboxStats`
   - `stats()` query API
2. `crates/edge-agent/src/mqtt_bridge.rs`
   - bridge counters (`BridgeMetrics`)
   - health status evaluation
   - periodic health publish task
3. `crates/edge-agent/src/main.rs`
   - health threshold and interval env wiring

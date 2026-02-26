# UC-EDGE-007: Alert Deduplication and Central ACK Flow

## Goal
Reduce alert noise and close operational loop with explicit alert acknowledgements from central.

## Scope
1. Deduplicate raised/cleared alerts by alert type and state within a time window.
2. Subscribe to central ACK commands for runtime alerts.
3. Publish ACK result indicating accepted/rejected with reason.

## MQTT Contract
1. Alert topic: `scada/{site}/edge/{agent}/alerts/runtime`
2. Alert ACK command topic: `scada/{site}/edge/{agent}/alerts/runtime/ack`
3. Alert ACK result topic: `scada/{site}/edge/{agent}/alerts/runtime/ack/result`

## Configuration
1. `MQTT_ALERT_DEDUP_WINDOW_SECS` default: `300`
2. `MQTT_ALERT_DEGRADED_STREAK` default: `3`
3. `MQTT_ALERT_RECOVERED_STREAK` default: `3`

## Acceptance Criteria
1. Duplicate raised/cleared alert transitions within dedup window are not re-emitted.
2. Central ACK for active `runtime_health_degraded` alert is accepted and clears active state.
3. ACK for unsupported/inactive alerts returns rejection with reason.

## Test Mapping
Implemented in `crates/edge-agent/src/mqtt_bridge.rs`:

1. `test_should_emit_alert_for_window_deduplicates`
2. `test_parse_alert_ack_command_message`
3. `test_topics_follow_convention`

## Implementation Mapping
Implemented in:

1. `crates/edge-agent/src/mqtt_bridge.rs`
   - alert dedup window logic
   - ACK command parser + ACK result payload
   - MQTT subscription to alert ACK topic
   - alert ACK result publish path
2. `crates/edge-agent/src/main.rs`
   - `MQTT_ALERT_DEDUP_WINDOW_SECS` env wiring

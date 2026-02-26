# UC-EDGE-006: Active Alerts on Sustained Degraded Health

## Goal
Emit active MQTT alerts when runtime health remains degraded for a configured number of consecutive health samples.

## Scope
1. Publish alert `raised` only after sustained degraded streak.
2. Publish alert `cleared` only after sustained recovered streak.
3. Avoid alert flapping with configurable thresholds.

## MQTT Contract
1. Alert topic: `scada/{site}/edge/{agent}/alerts/runtime`
2. Alert payload fields:
   - `alert_type`: `runtime_health_degraded`
   - `state`: `raised` | `cleared`
   - `severity`: `warning` | `info`
   - `message`
   - outbox context (`outbox_depth`, `outbox_oldest_age_secs`)

## Configuration
1. `MQTT_ALERT_DEGRADED_STREAK` default: `3`
2. `MQTT_ALERT_RECOVERED_STREAK` default: `3`

## Test Mapping
Implemented in `crates/edge-agent/src/mqtt_bridge.rs`:

1. `test_topics_follow_convention`
2. `test_evaluate_alert_transition_raise_and_clear`

## Implementation Mapping
Implemented in:

1. `crates/edge-agent/src/mqtt_bridge.rs`
   - `AlertState` and transition evaluator
   - alert message model and builder
   - periodic alert publication in health task
2. `crates/edge-agent/src/main.rs`
   - alert threshold env wiring

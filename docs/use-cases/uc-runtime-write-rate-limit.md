# UC-RUNTIME-009: Write Rate Limit Per Tag

## Goal
Protect field devices from burst write traffic by enforcing a minimum interval between writes on the same tag.

## Contract
1. Connection transport config:
   - `write_rate_limit_ms` (default `0`, disabled)
2. Scope:
   - per tag (not global connection)
3. Behavior:
   - if a write arrives before window expires, runtime rejects it with rate-limit error

## TDD Mapping
1. `test_runtime_write_rate_limit_rejects_burst_same_tag`
2. `test_runtime_write_rate_limit_is_scoped_per_tag`

## Implementation Mapping
1. `crates/application/src/runtime/connection_runtime.rs`
   - tracks `last_write_applied_at`
   - evaluates `write_rate_limit_ms` from connection transport config
   - rejects burst writes before dispatching to driver

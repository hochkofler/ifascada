# UC-RUNTIME-010: Write Circuit Breaker Per Connection

## Goal
Prevent repeated failing writes from hammering device links by opening a temporary breaker after consecutive failures.

## Contract
1. Connection transport config:
   - `write_circuit_fail_threshold` (default `0`, disabled)
   - `write_circuit_cooldown_ms` (default `0`, disabled)
2. Behavior:
   - after `N` consecutive write failures, breaker opens
   - while open, writes are rejected with `write circuit breaker open`
   - after cooldown, breaker closes automatically and writes are attempted again

## TDD Mapping
1. `test_runtime_write_circuit_breaker_opens_after_consecutive_failures`
2. `test_runtime_write_circuit_breaker_recovers_after_cooldown`

## Implementation Mapping
1. `crates/application/src/runtime/connection_runtime.rs`
   - tracks `write_fail_streak`
   - tracks `write_circuit_open_until`
   - checks breaker before dispatching write to driver
   - resets breaker state on successful write

# UC-RUNTIME-001: Tag Scheduling and Connection Reconnection

## Goal
Guarantee deterministic runtime behavior for two critical SCADA concerns:

1. Per-tag emission scheduling based on `TagUpdateMode`.
2. Connection retry behavior based on `ReconnectionPolicy`.

## Context
The runtime polls a physical/logical connection and receives values for multiple tags.
Each tag may require a different publication mode:

- `Polling { interval_ms }`: publish at configured cadence.
- `OnChange`: publish only when value changes.
- `PollingOnChange { interval_ms }`: publish only when value changes and cadence window elapsed.

Connection startup/recovery must follow domain policy:

- `ReconnectStrategy::Fixed { delay_ms }` with optional `max_retries`.
- `ReconnectStrategy::Exponential { initial_delay_ms, max_delay_ms }` with optional `max_retries`.

## Acceptance Criteria
1. Runtime emits more events for a fast polling tag than for a slow polling tag under the same driver input stream.
2. Runtime stops retrying connect attempts once `max_retries` is reached.
3. Runtime keeps processing normally after successful connection and publishes tag updates through:
   - `LiveState`
   - `EventBus`
4. Runtime applies `ReconnectStrategy::Exponential` delays between retries.
5. Runtime supports hot reload of connection policy and resets retry state after reload.

## Test Mapping
Implemented in `crates/application/tests/runtime_tests.rs`:

1. `test_runtime_respects_tag_update_mode_intervals`
   - Verifies criterion #1.
2. `test_runtime_honors_reconnection_max_retries`
   - Verifies criterion #2.
3. Existing `test_runtime_engine_flow`
   - Covers criterion #3 baseline path.
4. `test_runtime_exponential_reconnect_backoff`
   - Verifies criterion #4.
5. `test_runtime_reload_resets_retry_budget`
   - Verifies criterion #5.

## Implementation Mapping
Implemented in `crates/application/src/runtime/connection_runtime.rs`:

1. Tag-level scheduling gate in `should_publish`.
2. Retry state machine:
   - `attempt_connect_if_due`
   - `record_connection_failure`
   - `compute_retry_delay`
3. Event/state propagation:
   - `LiveState::update_tag`
   - `EventBus::publish(RuntimeEvent::TagChanged | ConnectionStateChanged)`
4. Runtime command orchestration:
   - `RuntimeEngine::reload_connection`
   - `ConnectionCommand::Reload`

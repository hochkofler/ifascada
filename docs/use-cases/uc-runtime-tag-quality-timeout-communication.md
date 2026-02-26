# UC-RUNTIME-002: Tag Quality on Timeout and Communication Failure

## Goal
Ensure runtime publishes reliable data quality transitions for critical SCADA observability.

## Scope
For each running tag:

1. If no fresh value is received within timeout window, quality must transition to:
   - `Bad(Timeout)`
2. If connection polling fails (`driver.poll()` returns error), quality must transition to:
   - `Bad(CommunicationFailure)`
3. When valid value is received again, quality must recover to:
   - `Good`

## Acceptance Criteria
1. Runtime writes timeout quality to `LiveState` when tag becomes stale.
2. Runtime writes communication failure quality to `LiveState` when polling fails.
3. Runtime recovers quality to `Good` on first successful value after failure.
4. Runtime emits corresponding `RuntimeEvent::TagChanged` transitions.

## Test Mapping
Implemented in `crates/application/tests/runtime_tests.rs`:

1. `test_runtime_marks_tag_timeout_quality_when_data_stops`
2. `test_runtime_marks_communication_failure_and_recovers_to_good`

## Implementation Mapping
Implemented in `crates/application/src/runtime/connection_runtime.rs`:

1. Per-tag freshness tracking (`last_received_at`).
2. Timeout evaluator (`evaluate_timeouts`).
3. Connection-failure propagation (`mark_all_tags_bad_communication`).
4. Recovery path on successful value (force publish when quality was bad).

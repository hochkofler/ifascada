# UC-RUNTIME-005: Tag Write Command Audit Trail

## Goal
Emit explicit audit events for every write command handling outcome in runtime.

## Scope
For tag write commands handled by runtime:

1. Successful execution emits an `Applied` audit outcome.
2. Deduplicated command emits a `Deduplicated` audit outcome.
3. Rejected command emits a `Rejected` audit outcome with reason.

## Acceptance Criteria
1. A write with `command_id` executed once and retried emits both `Applied` and `Deduplicated`.
2. A driver write failure emits `Rejected` with failure reason.
3. An unknown tag route emits `Rejected` with route-not-found reason.

## Test Mapping
Implemented in `crates/application/tests/runtime_tests.rs`:

1. `test_runtime_write_audit_emits_applied_and_deduplicated_outcomes`
2. `test_runtime_write_audit_emits_rejected_on_driver_error`
3. `test_runtime_write_audit_emits_rejected_for_unknown_tag_route`

## Implementation Mapping
Implemented in:

1. `crates/application/src/runtime/event_bus.rs`
   - `WriteCommandOutcome`
   - `RuntimeEvent::TagWriteCommandHandled`
2. `crates/application/src/runtime/connection_runtime.rs`
   - publish write audit events for applied/deduplicated/rejected execution
3. `crates/application/src/runtime/runtime_engine.rs`
   - publish rejected audit events for routing/dispatch failures before runtime handling

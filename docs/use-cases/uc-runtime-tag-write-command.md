# UC-RUNTIME-003: Tag Write Command Routing and Quality

## Goal
Provide deterministic runtime behavior for command writes (`write`) to tags:

1. Route each write to the owning connection runtime.
2. Propagate success/failure to caller.
3. Reflect command outcome in `LiveState` and `RuntimeEvent`.

## Acceptance Criteria
1. Writing a known tag updates `LiveState` with written value and `Good` quality.
2. Runtime emits `RuntimeEvent::TagChanged` for successful writes.
3. Writing an unknown tag returns `DomainError::NotFound`.
4. Driver write error returns `DomainError::DriverError` and marks tag as
   `Bad(CommunicationFailure)` in `LiveState`.

## Test Mapping
Implemented in `crates/application/tests/runtime_tests.rs`:

1. `test_runtime_write_tag_success_updates_live_state_and_events`
2. `test_runtime_write_unknown_tag_returns_not_found`
3. `test_runtime_write_error_marks_bad_communication`

## Implementation Mapping
Implemented in:

1. `crates/application/src/runtime/runtime_engine.rs`
   - `tag_routes` map
   - `write_tag(...)`
2. `crates/application/src/runtime/connection_runtime.rs`
   - `ConnectionCommand::WriteTag`
   - `handle_write_command(...)`

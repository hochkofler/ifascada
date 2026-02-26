# UC-RUNTIME-006: Persisted Write Audit History

## Goal
Persist write command audit records in runtime and expose query methods for operational traceability.

## Scope
For all `TagWriteCommandHandled` outcomes:

1. Runtime persists each audit record in an internal store.
2. Query by `tag_id` returns historical records for that tag.
3. Query by `command_id` returns historical records for that command.
4. Routing rejections produced by `RuntimeEngine` are also persisted.

## Acceptance Criteria
1. Applied and deduplicated outcomes can be retrieved by tag and command id.
2. Route-not-found rejection is persisted and retrievable by command id.
3. Global history query returns accumulated records.

## Test Mapping
Implemented in `crates/application/tests/runtime_tests.rs`:

1. `test_runtime_write_audit_store_queries_by_tag_and_command_id`
2. `test_runtime_write_audit_store_persists_rejected_route_errors`

## Implementation Mapping
Implemented in:

1. `crates/application/src/runtime/write_audit.rs`
   - `WriteAuditRecord`
   - `WriteAuditStore` (append + query methods)
2. `crates/application/src/runtime/connection_runtime.rs`
   - persists applied/deduplicated/rejected command outcomes
3. `crates/application/src/runtime/runtime_engine.rs`
   - persists rejected routing/dispatch outcomes
   - exposes query API:
     - `write_audit_all()`
     - `write_audit_by_tag(...)`
     - `write_audit_by_command_id(...)`

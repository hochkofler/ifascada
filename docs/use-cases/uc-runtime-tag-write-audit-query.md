# UC-RUNTIME-008: Write Audit Query (Filters + Pagination + Time Window)

## Goal
Provide operational query capabilities over write audit history for SCADA investigations.

## Scope
1. Query audit history by `connection_id`, `tag_id`, `command_id`, `outcome`.
2. Filter by time window (`from`, `to`).
3. Support pagination (`offset`, `limit`).

## Acceptance Criteria
1. Query can return only deduplicated outcomes for a specific command id.
2. Query supports offset/limit paging over the same command history.
3. Query supports inclusive time filtering and excludes records outside the range.

## Test Mapping
Implemented in `crates/application/tests/runtime_tests.rs`:

1. `test_runtime_write_audit_query_filters_by_outcome_and_paginates`
2. `test_runtime_write_audit_query_filters_by_time_window`

## Implementation Mapping
Implemented in:

1. `crates/application/src/runtime/write_audit.rs`
   - `WriteAuditQuery`
   - `WriteAuditRepository::query(...)`
   - in-memory query engine
2. `crates/application/src/runtime/runtime_engine.rs`
   - `write_audit_query(...)`
3. `crates/infrastructure/src/repositories/write_audit_jsonl.rs`
   - query support in file-backed repository adapter

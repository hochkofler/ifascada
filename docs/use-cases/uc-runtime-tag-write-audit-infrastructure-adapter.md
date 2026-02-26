# UC-RUNTIME-007: Write Audit Repository Port + Infrastructure Adapter

## Goal
Decouple runtime audit persistence from application internals and provide an infrastructure-backed adapter.

## Scope
1. Application defines a repository port for write audit records.
2. Runtime uses the port via dependency injection.
3. Infrastructure provides a JSONL adapter that persists records across process restart.

## Acceptance Criteria
1. Runtime composes with default in-memory repository without breaking existing behavior.
2. Runtime can be built with an injected repository implementation.
3. JSONL repository persists records and reloads them from disk.

## Test Mapping
Implemented in `crates/application/tests/runtime_tests.rs`:

1. `test_runtime_write_audit_jsonl_repository_persists_across_restart`

## Implementation Mapping
Implemented in:

1. `crates/application/src/runtime/write_audit.rs`
   - `WriteAuditRepository` port
   - `WriteAuditStore` in-memory adapter (default)
2. `crates/application/src/runtime/runtime_engine.rs`
   - `new_with_write_audit_repository(...)`
   - queries delegated to repository port
3. `crates/application/src/runtime/connection_runtime.rs`
   - write audit persistence through repository port
4. `crates/infrastructure/src/repositories/write_audit_jsonl.rs`
   - `JsonlWriteAuditRepository` file-backed adapter

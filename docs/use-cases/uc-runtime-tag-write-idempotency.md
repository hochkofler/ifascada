# UC-RUNTIME-004: Idempotent Tag Write Commands (Anti-Replay)

## Goal
Prevent duplicate write side-effects when the same command is retried/replayed.

## Scope
For `write` commands that include `command_id`:

1. Same `(tag_id, command_id)` within dedup window must be executed at most once.
2. Same `command_id` on different tags must be treated independently.
3. After dedup window expires, the same `(tag_id, command_id)` can execute again.

## Acceptance Criteria
1. Duplicate command for same tag returns success but does not call driver write twice.
2. Same command_id for different tags executes once per tag.
3. Expired command_id is no longer deduplicated and executes again.

## Test Mapping
Implemented in `crates/application/tests/runtime_tests.rs`:

1. `test_runtime_write_command_id_deduplicates_same_tag`
2. `test_runtime_write_command_id_is_scoped_per_tag`
3. `test_runtime_write_command_id_expires_after_window`

## Implementation Mapping
Implemented in:

1. `crates/application/src/runtime/runtime_engine.rs`
   - `write_tag_with_command_id(...)`
2. `crates/application/src/runtime/connection_runtime.rs`
   - `ConnectionCommand::WriteTag { command_id }`
   - command dedup store + TTL pruning
   - duplicate short-circuit path

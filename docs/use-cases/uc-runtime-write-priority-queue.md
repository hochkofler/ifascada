# UC-RUNTIME-011: Write Priority Queue Per Connection

## Goal
Guarantee that high-priority write commands are executed before normal-priority writes within the same connection runtime.

## Contract
1. Runtime API supports priority:
   - `WritePriority::Normal`
   - `WritePriority::High`
2. Existing write APIs remain backward compatible and default to `Normal`.
3. Runtime processes pending writes using two queues:
   - high-priority queue first
   - normal queue second

## TDD Mapping
1. `test_runtime_write_priority_processes_high_before_normal`

## Implementation Mapping
1. `crates/application/src/runtime/connection_runtime.rs`
   - `ConnectionCommand::WriteTag { priority }`
   - `WritePriority` enum
   - high/normal pending queues and prioritized dequeue
2. `crates/application/src/runtime/runtime_engine.rs`
   - `write_tag_with_priority(...)`
   - default paths keep `Normal` priority

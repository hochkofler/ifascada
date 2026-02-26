# UC-PROTOCOL-002: Modbus TCP + Shared Performance Core

## Goal
Add Modbus TCP driver and consolidate shared Modbus logic to cover protocol performance and type conversion concerns.

## Scope
1. Implement Modbus TCP driver on shared Modbus core.
2. Reuse shared parser/read/write logic between RTU and TCP.
3. Add batched polling (block reads) for efficiency.
4. Add type conversion support for register encodings:
   - `u16`, `i16`, `u32`, `f32`
5. Add request policy controls:
   - timeout
   - retries
   - retry backoff

## Shared Tag Source Contract
1. `hr:{address}:u16|i16|u32|f32`
2. `ir:{address}:u16|i16|u32|f32`
3. `coil:{address}:bool`
4. `di:{address}:bool`

## RTU Transport Additions
1. `request_timeout_ms`
2. `request_retries`
3. `retry_backoff_ms`
4. `max_batch_registers`
5. `max_batch_bits`

## TCP Transport Contract (minimal)
1. `host`
2. `port` (default `502`)
3. `unit_id` (default `1`)
4. `tag_map`
5. `connect_timeout_ms`
6. request/batch policy fields equal to RTU

## Test Mapping
Implemented in:
1. `crates/infrastructure/src/drivers/modbus_shared.rs`
   - parse/validation tests
   - batching plan tests
   - conversion roundtrip test
2. `crates/infrastructure/src/drivers/modbus_rtu.rs`
   - RTU transport config default test
3. `crates/infrastructure/src/drivers/modbus_tcp.rs`
   - TCP transport config default test

## Implementation Mapping
1. `crates/infrastructure/src/drivers/modbus_shared.rs`
   - `ModbusPoint`, batch planning, batched polling, encode/decode helpers, request policy
2. `crates/infrastructure/src/drivers/modbus_rtu.rs`
   - RTU serial session + shared core usage
3. `crates/infrastructure/src/drivers/modbus_tcp.rs`
   - TCP socket session + shared core usage
4. `crates/infrastructure/src/drivers/mod.rs`
   - exports `modbus_tcp`
5. `crates/edge-agent/src/main.rs`
   - registers `DriverType::new("ModbusTCP")`

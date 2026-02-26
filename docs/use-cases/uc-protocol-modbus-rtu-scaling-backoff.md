# UC-PROTOCOL-003: Modbus RTU Scaling + Retry Backoff Policy

## Goal
Strengthen protocol-core behavior for SCADA reads/writes by adding:
1. Per-point scaling/offset for Modbus register values.
2. RTU-specific retry backoff strategy (`fixed` or `exponential` with cap).

## Scope
1. Extend Modbus source mapping:
   - `area:address:encoding[:scale[:offset]]`
2. Apply forward scaling on read values.
3. Apply inverse scaling on write values.
4. Add retry backoff strategy to shared request policy.
5. Add batch-gap control to read planner for fewer RTU frames.
6. Configure RTU driver to select backoff mode from transport config.

## TDD Mapping
1. `crates/infrastructure/src/drivers/modbus_shared.rs`
   - `test_parse_modbus_point_with_scale_and_offset`
   - `test_parse_modbus_point_rejects_scale_on_bool`
   - `test_decode_applies_scale_and_offset`
   - `test_inverse_scaling_for_write`
   - `test_backoff_delay_fixed`
   - `test_backoff_delay_exponential_capped`
   - `test_plan_batches_allows_small_register_gap_for_rtu_efficiency`
   - `test_plan_batches_does_not_cross_large_register_gap`
2. `crates/infrastructure/src/drivers/modbus_rtu.rs`
   - `test_parse_transport_config_minimal`
   - `test_parse_transport_config_with_exponential_backoff`

## RTU Transport Additions
1. `retry_backoff_mode`: `fixed|exponential` (default `fixed`)
2. `retry_backoff_max_ms`: cap for exponential mode (default `2000`)
3. `max_register_gap`: max register holes allowed to keep tags in one batch (default `0`)
4. `max_bit_gap`: max coil/discrete holes allowed in one batch (default `0`)

## Notes
1. TCP keeps fixed backoff for now.
2. Existing `area:address:encoding` mappings remain valid and unchanged.

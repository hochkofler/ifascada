# UC-EDGE-005: MQTT -> ModbusTCP Write Happy Path (Manual E2E)

## Goal
Guarantee a reproducible manual E2E flow where a valid MQTT write command is accepted and applied through ModbusTCP.

## Scenario
1. Edge agent subscribes to `scada/{site}/edge/{agent}/cmd/write`.
2. A valid command with integer payload targets `tag_hr_10_cmd`.
3. Runtime executes write on ModbusTCP holding register.
4. Edge agent publishes:
   - ACK `success=true` on `cmd/write/ack`
   - Audit `outcome=Applied` on `audit/write`

## TDD Mapping
1. Red test:
   - `crates/infrastructure/src/drivers/modbus_shared.rs`
   - `test_encode_u16_accepts_integral_float`
   - Initially failed because MQTT numeric payload arrived as float (`123.0`) and `u16` encoder only accepted integer variant.
2. Green implementation:
   - `encode_register_value(...)` now accepts integral floats for `u16/i16/u32`.
   - Fractional floats remain rejected for integer encodings.
3. Guardrail test:
   - `test_encode_u16_rejects_fractional_float`

## Configuration Contract for Demo
1. Bootstrap command tag must be writable register:
   - id: `tag_hr_10_cmd`
   - source: `hr:10:u16`
2. Guardrail test:
   - `crates/edge-agent/src/bootstrap.rs`
   - `test_bootstrap_example_has_writable_modbus_tag_for_e2e`

## Manual E2E Script
1. `scripts/e2e-mqtt-modbus-tcp.ps1`
2. Valid command payload:
   - `tag_id=tag_hr_10_cmd`
   - `value=123`
3. Invalid command payload:
   - `tag_id=tag_unknown`
4. Expected outputs:
   - valid command -> ACK success + audit applied
   - invalid command -> ACK rejected + audit rejected

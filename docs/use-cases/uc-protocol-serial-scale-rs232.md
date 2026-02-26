# UC-PROTOCOL-004: RS232 Scale Input (Value + Unit)

## Goal
Read ASCII weight frames from a serial scale and publish:
1. Raw frame tag (`tag_scale_raw`)
2. Compound tag (`tag_scale_compound`) containing `value`, `unit`, `raw`

## Driver
1. Domain driver type: `SerialAscii`
2. Infrastructure implementation:
   - `crates/infrastructure/src/drivers/serial_ascii.rs`

## Input Contract
1. Frame content example: `+ 12.4354 g`
2. Parser rules:
   - optional sign (`+` or `-`)
   - decimal number with dot
   - unit token
   - zero/one/many spaces between tokens
   - optional spaces around full frame
3. Accepted line terminators:
   - `\r\n`
   - `\n`
   - `\r`

## Output Contract
1. `tag_scale_raw` -> plain line string
2. `tag_scale_compound` -> JSON string:
   - `{"value":12.4354,"unit":"g","raw":"+ 12.4354 g"}`
3. MQTT telemetry topic:
   - `scada/{site}/edge/{agent}/telemetry/tag/{tag_id}`

## Runtime Config Notes
1. `SerialAscii` requires `transport.tag_map`.
2. Central runtime builder now injects `tag_map` from catalog tags (`tag_code -> source`).
3. Edge bootstrap has a safety fallback and auto-generates `tag_map` for `SerialAscii` if missing.
4. For event/request-driven scales, use `update_mode = on_message` so every valid frame is published (including repeated equal values).

## TDD Mapping
1. `test_parse_scale_line_plus_value`
2. `test_parse_scale_line_negative_value`
3. `test_parse_scale_line_without_sign_and_many_spaces`
4. `test_parse_scale_line_zero_or_more_spaces_between_parts`
5. `test_find_line_boundary_fallback_newline`
6. `test_find_line_boundary_fallback_carriage_return`
7. `test_map_line_to_outputs_emits_raw_even_if_parse_fails`

## Bootstraps
1. Real serial:
   - `crates/edge-agent/config/bootstrap.serial-scale.example.json`
2. Inline mock (no COM required):
   - `crates/edge-agent/config/bootstrap.serial-scale.mock-inline.example.json`

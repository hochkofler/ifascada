# UC-PROTOCOL-005: Modbus 32-bit Endianness (u32/f32)

## Goal
Support 32-bit Modbus mappings where register word order is little-endian (swapped words), while preserving current big-endian behavior.

## Encoding Contract
1. Existing (big-endian):
   - `u32`
   - `f32`
2. New (little-endian word order):
   - `u32le` (`u32_le`, `u32-le`)
   - `f32le` (`f32_le`, `f32-le`)

## Behavior
1. `u32/f32`: high-word first.
2. `u32le/f32le`: low-word first.
3. Width remains 2 registers for all 32-bit encodings.

## TDD Mapping
1. `test_parse_modbus_point_supports_i16_u32_f32`
2. `test_decode_encode_roundtrip_u32_le`
3. `test_decode_encode_roundtrip_f32_le`

## Implementation Mapping
1. `crates/infrastructure/src/drivers/modbus_shared.rs`
   - `ModbusEncoding` extended with `U32Le`, `F32Le`
   - parser aliases for `u32le/f32le`
   - decode/encode branches for LE word order

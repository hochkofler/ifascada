# UC-PROTOCOL-001: Modbus RTU Baseline Driver (Read/Write Core)

## Goal
Introduce Modbus RTU as first real protocol in infrastructure, with clean parsing and runtime integration.

## Scope
1. Parse Modbus RTU transport config from `Connection.config.transport`.
2. Parse tag source mapping (`tag_map`) into Modbus points.
3. Implement protocol operations over serial RTU:
   - read holding/input registers
   - read coils/discrete inputs
   - write single register/coil
4. Register driver in edge-agent runtime registry.
5. Extract shared Modbus core logic for DRY reuse with future Modbus TCP.

## Transport Contract
Example:

```json
{
  "serial": {
    "port": "COM3",
    "baud_rate": 9600,
    "data_bits": 8,
    "stop_bits": 1,
    "parity": "N"
  },
  "device_unit_map": {
    "dev_50": 50,
    "dev_100": 100
  },
  "tag_map": {
    "tag_temp": { "source": "hr:0:u16", "device_id": "dev_50", "unit_id": 50 },
    "tag_alarm": { "source": "coil:10:bool", "device_id": "dev_100", "unit_id": 100 }
  }
}
```

Rule:
1. For Modbus RTU, `unit_id` is per device (resolved per tag during runtime), not global per connection.

Tag source format:
1. `hr:{address}:u16`
2. `ir:{address}:u16`
3. `coil:{address}:bool`
4. `di:{address}:bool`
5. `hr:{address}:u32` (High Word first, default)
6. `hr:{address}:u32:low_first` (word swap)
7. `hr:{address}:i32` (High Word first, signed 32-bit)
8. `hr:{address}:i32:low_first` (word swap)
9. `hr:{address}:f32` (High Word first, default)
10. `hr:{address}:f32:low_first` (word swap)

Optional scaling/offset:
1. `hr:{address}:u32:high_first:{scale}:{offset}`
2. `hr:{address}:u32:low_first:{scale}:{offset}`
3. `hr:{address}:i32:high_first:{scale}:{offset}`
4. `hr:{address}:i32:low_first:{scale}:{offset}`

Notes:
1. `u32`/`i32`/`f32` consume 2 consecutive registers (`N` and `N+1`).
2. `uint16`/`uint32`/`float32` aliases are accepted.
3. `word_order` only applies to 32-bit encodings.

## Test Mapping
Implemented in `crates/infrastructure/src/drivers/modbus_rtu.rs`:
1. `test_parse_modbus_point_hr_u16`
2. `test_parse_modbus_point_coil_bool`
3. `test_parse_modbus_point_rejects_invalid_combo`
4. `test_parse_transport_config_minimal`

## Implementation Mapping
Implemented in:
1. `crates/infrastructure/src/drivers/modbus_shared.rs`
   - shared point parser and validation
   - shared read/write helpers for Modbus areas
   - shared tag_map -> point_map builder
1. `crates/infrastructure/src/drivers/modbus_rtu.rs`
   - `ModbusRtuDriver`
   - RTU transport-specific config and serial session lifecycle
   - serial RTU client integration (`tokio-modbus`, `tokio-serial`)
2. `crates/infrastructure/src/drivers/mod.rs`
   - exports `modbus_rtu`
   - exports `modbus_shared`
3. `crates/edge-agent/src/main.rs`
   - registers `DriverType::new("ModbusRTU")`

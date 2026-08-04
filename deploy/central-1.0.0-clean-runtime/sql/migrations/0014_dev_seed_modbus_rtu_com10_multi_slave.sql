-- Dev seed: Modbus RTU bus on COM10 with 3 slave devices and 5 tags.
-- Scope:
--   site: plant-a
--   edge: edge-01
-- Model:
--   1 connection (RTU bus) -> 3 devices (slave 50, 100, 70) -> 5 tags

-- 1) Connection catalog (single RTU bus on COM10).
INSERT INTO connections (edge_id, connection_code, name, driver_type, metadata_json)
SELECT
    e.id,
    'conn_modbus_rtu_com10_1',
    'Modbus RTU COM10 Bus',
    'ModbusRTU',
    jsonb_build_object(
        'transport', jsonb_build_object(
            'kind', 'modbus_rtu',
            'serial', jsonb_build_object(
                'port', 'COM10',
                'baud_rate', 9600,
                'data_bits', 8,
                'stop_bits', 1,
                'parity', 'N'
            )
        ),
        'protocol', jsonb_build_object(
            'request_timeout_ms', 1500,
            'request_retries', 1,
            'retry_backoff_ms', 100,
            'retry_backoff_mode', 'fixed',
            'max_batch_registers', 120,
            'max_batch_bits', 2000,
            'max_register_gap', 2,
            'max_bit_gap', 8
        ),
        'bus', jsonb_build_object(
            'slave_ids', jsonb_build_array(50, 100, 70)
        )
    )
FROM edges e
JOIN sites s ON s.id = e.site_id
WHERE s.code = 'plant-a'
  AND e.edge_code = 'edge-01'
ON CONFLICT (edge_id, connection_code)
DO UPDATE SET
    name = EXCLUDED.name,
    driver_type = EXCLUDED.driver_type,
    metadata_json = COALESCE(connections.metadata_json, '{}'::jsonb) || EXCLUDED.metadata_json,
    updated_at = NOW();

-- 2) Devices mapped to the same connection, each with slave metadata.
INSERT INTO devices (edge_id, device_code, name, driver_type, metadata_json)
SELECT
    e.id,
    v.device_code,
    v.name,
    'ModbusRTU',
    jsonb_build_object(
        'modbus', jsonb_build_object(
            'slave_id', v.slave_id
        )
    )
FROM edges e
JOIN sites s ON s.id = e.site_id
JOIN (
    VALUES
      ('dev_modbus_rtu_50',  'RTU Device Slave 50',  50),
      ('dev_modbus_rtu_100', 'Airborne Particle Sensor', 100),
      ('dev_modbus_rtu_70',  'RTU Device Slave 70',  70)
) AS v(device_code, name, slave_id) ON TRUE
WHERE s.code = 'plant-a'
  AND e.edge_code = 'edge-01'
ON CONFLICT (edge_id, device_code)
DO UPDATE SET
    name = EXCLUDED.name,
    driver_type = EXCLUDED.driver_type,
    metadata_json = COALESCE(devices.metadata_json, '{}'::jsonb) || EXCLUDED.metadata_json;

UPDATE devices d
SET connection_id = c.id
FROM connections c
JOIN edges e ON e.id = c.edge_id
JOIN sites s ON s.id = e.site_id
WHERE d.edge_id = e.id
  AND s.code = 'plant-a'
  AND e.edge_code = 'edge-01'
  AND c.connection_code = 'conn_modbus_rtu_com10_1'
  AND d.device_code IN ('dev_modbus_rtu_50', 'dev_modbus_rtu_100', 'dev_modbus_rtu_70');

-- 3) Tags (5 total): 1 for slave50, 3 for slave100, 1 for slave70.
INSERT INTO tags (
    device_id, tag_code, name, value_type, source, unit, metadata_json,
    tag_code_canonical, display_name, aliases_json
)
SELECT
    d.id,
    t.tag_code,
    t.name,
    t.value_type,
    t.source,
    t.unit,
    t.metadata_json::jsonb,
    t.tag_code_canonical,
    t.display_name,
    t.aliases_json::jsonb
FROM devices d
JOIN edges e ON e.id = d.edge_id
JOIN sites s ON s.id = e.site_id
JOIN (
    VALUES
      (
        'dev_modbus_rtu_50',
        'tag_modbus_rtu_50_status',
        'RTU 50 Status',
        'integer',
        'hr:0:u16',
        NULL,
        '{"modbus":{"slave_id":50,"register":0,"encoding":"u16"}}',
        'PLANTA1.RTU.COM10.SL50.STAT.PV',
        'RTU 50 Status',
        '[]'
      ),
      (
        'dev_modbus_rtu_100',
        'tag_airborne_particle_pm1',
        'Airborne PM1',
        'float',
        'hr:10:f32',
        'ug/m3',
        '{"modbus":{"slave_id":100,"register":10,"encoding":"f32"}}',
        'PLANTA1.RTU.COM10.S100.PM1.PV',
        'Airborne PM1',
        '[]'
      ),
      (
        'dev_modbus_rtu_100',
        'tag_airborne_particle_pm25',
        'Airborne PM2_5',
        'float',
        'hr:12:f32',
        'ug/m3',
        '{"modbus":{"slave_id":100,"register":12,"encoding":"f32"}}',
        'PLANTA1.RTU.COM10.S100.PM25.PV',
        'Airborne PM2.5',
        '[]'
      ),
      (
        'dev_modbus_rtu_100',
        'tag_airborne_particle_pm10',
        'Airborne PM10',
        'float',
        'hr:14:f32',
        'ug/m3',
        '{"modbus":{"slave_id":100,"register":14,"encoding":"f32"}}',
        'PLANTA1.RTU.COM10.S100.PM10.PV',
        'Airborne PM10',
        '[]'
      ),
      (
        'dev_modbus_rtu_70',
        'tag_modbus_rtu_70_status',
        'RTU 70 Status',
        'integer',
        'hr:0:u16',
        NULL,
        '{"modbus":{"slave_id":70,"register":0,"encoding":"u16"}}',
        'PLANTA1.RTU.COM10.SL70.STAT.PV',
        'RTU 70 Status',
        '[]'
      )
) AS t(
    device_code, tag_code, name, value_type, source, unit, metadata_json,
    tag_code_canonical, display_name, aliases_json
) ON t.device_code = d.device_code
WHERE s.code = 'plant-a'
  AND e.edge_code = 'edge-01'
ON CONFLICT (device_id, tag_code)
DO UPDATE SET
    name = EXCLUDED.name,
    value_type = EXCLUDED.value_type,
    source = EXCLUDED.source,
    unit = EXCLUDED.unit,
    metadata_json = COALESCE(tags.metadata_json, '{}'::jsonb) || EXCLUDED.metadata_json,
    tag_code_canonical = EXCLUDED.tag_code_canonical,
    display_name = EXCLUDED.display_name,
    aliases_json = EXCLUDED.aliases_json;

-- Minimal clean seed:
-- 1) edge-com-01   -> manual scale tag
-- 2) edge-modbus-01 -> modbus RTU (3 devices / 5 tags)
-- 3) edge-sim-01   -> one simulator tag

-- Site/context
INSERT INTO sites (code, name, timezone)
VALUES ('plant-a', 'Plant A', 'UTC')
ON CONFLICT (code) DO NOTHING;

INSERT INTO lines (site_id, code, name)
SELECT s.id, 'line-main', 'Line Main'
FROM sites s
WHERE s.code = 'plant-a'
ON CONFLICT (site_id, code) DO NOTHING;

INSERT INTO areas (line_id, code, name)
SELECT l.id, 'area-main', 'Area Main'
FROM lines l
JOIN sites s ON s.id = l.site_id
WHERE s.code = 'plant-a' AND l.code = 'line-main'
ON CONFLICT (line_id, code) DO NOTHING;

INSERT INTO cells (area_id, code, name)
SELECT a.id, 'cell-main', 'Cell Main'
FROM areas a
JOIN lines l ON l.id = a.line_id
JOIN sites s ON s.id = l.site_id
WHERE s.code = 'plant-a' AND l.code = 'line-main' AND a.code = 'area-main'
ON CONFLICT (area_id, code) DO NOTHING;

-- Edges
INSERT INTO edges (site_id, edge_code, name, status, metadata_json)
SELECT s.id, v.edge_code, v.name, 'unknown', '{}'::jsonb
FROM sites s
JOIN (
    VALUES
      ('edge-com-01', 'Edge Serial COM'),
      ('edge-modbus-01', 'Edge Modbus RTU'),
      ('edge-sim-01', 'Edge Simulator')
) AS v(edge_code, name) ON TRUE
WHERE s.code = 'plant-a'
ON CONFLICT (site_id, edge_code) DO UPDATE
SET name = EXCLUDED.name,
    metadata_json = EXCLUDED.metadata_json;

UPDATE edges e
SET cell_id = c.id
FROM cells c
JOIN areas a ON a.id = c.area_id
JOIN lines l ON l.id = a.line_id
JOIN sites s ON s.id = l.site_id
WHERE s.code = 'plant-a'
  AND l.code = 'line-main'
  AND a.code = 'area-main'
  AND c.code = 'cell-main'
  AND e.site_id = s.id
  AND e.edge_code IN ('edge-com-01', 'edge-modbus-01', 'edge-sim-01');

-- Connections
INSERT INTO connections (edge_id, connection_code, name, driver_type, metadata_json)
SELECT
    e.id,
    'conn_scale_rs232_manual_1',
    'Scale RS232 Manual',
    'SerialAscii',
    jsonb_build_object(
        'transport', jsonb_build_object(
            'serial', jsonb_build_object(
                'port', 'COM7',
                'baud_rate', 9600,
                'data_bits', 8,
                'stop_bits', 1,
                'parity', 'N'
            )
        ),
        'frame', jsonb_build_object(
            'mode', 'line',
            'terminator', E'\\r',
            'max_len', 128,
            'read_timeout_ms', 30
        ),
        'parser', jsonb_build_object(
            'regex', '^[[:space:]]*([+-])?[[:space:]]*([0-9]+(?:[.][0-9]+)?)[[:space:]]*([A-Za-z]+)[[:space:]]*$',
            'sign_group', 1,
            'value_group', 2,
            'unit_group', 3
        ),
        'timeouts', jsonb_build_object(
            'request_timeout_ms', 200,
            'reconnect_delay_ms', 1000,
            'max_retries', NULL
        )
    )
FROM edges e
JOIN sites s ON s.id = e.site_id
WHERE s.code = 'plant-a'
  AND e.edge_code = 'edge-com-01'
ON CONFLICT (edge_id, connection_code) DO UPDATE
SET name = EXCLUDED.name,
    driver_type = EXCLUDED.driver_type,
    metadata_json = EXCLUDED.metadata_json,
    updated_at = NOW();

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
  AND e.edge_code = 'edge-modbus-01'
ON CONFLICT (edge_id, connection_code) DO UPDATE
SET name = EXCLUDED.name,
    driver_type = EXCLUDED.driver_type,
    metadata_json = EXCLUDED.metadata_json,
    updated_at = NOW();

INSERT INTO connections (edge_id, connection_code, name, driver_type, metadata_json)
SELECT
    e.id,
    'conn_sim_1',
    'Simulator Connection',
    'Simulator',
    jsonb_build_object(
        'transport', jsonb_build_object(
            'tag_ids', jsonb_build_array('tag_sim_1')
        )
    )
FROM edges e
JOIN sites s ON s.id = e.site_id
WHERE s.code = 'plant-a'
  AND e.edge_code = 'edge-sim-01'
ON CONFLICT (edge_id, connection_code) DO UPDATE
SET name = EXCLUDED.name,
    driver_type = EXCLUDED.driver_type,
    metadata_json = EXCLUDED.metadata_json,
    updated_at = NOW();

-- Devices
INSERT INTO devices (edge_id, device_code, name, driver_type, metadata_json)
SELECT e.id, 'dev_scale_manual_1', 'Scale Manual Device', 'SerialAscii', '{}'::jsonb
FROM edges e
JOIN sites s ON s.id = e.site_id
WHERE s.code = 'plant-a' AND e.edge_code = 'edge-com-01'
ON CONFLICT (edge_id, device_code) DO NOTHING;

INSERT INTO devices (edge_id, device_code, name, driver_type, metadata_json)
SELECT e.id, v.device_code, v.name, 'ModbusRTU',
       jsonb_build_object('modbus', jsonb_build_object('slave_id', v.slave_id))
FROM edges e
JOIN sites s ON s.id = e.site_id
JOIN (
    VALUES
      ('dev_modbus_rtu_50',  'RTU Device Slave 50',  50),
      ('dev_modbus_rtu_100', 'Airborne Particle Sensor', 100),
      ('dev_modbus_rtu_70',  'RTU Device Slave 70',  70)
) AS v(device_code, name, slave_id) ON TRUE
WHERE s.code = 'plant-a' AND e.edge_code = 'edge-modbus-01'
ON CONFLICT (edge_id, device_code) DO UPDATE
SET name = EXCLUDED.name,
    driver_type = EXCLUDED.driver_type,
    metadata_json = EXCLUDED.metadata_json;

INSERT INTO devices (edge_id, device_code, name, driver_type, metadata_json)
SELECT e.id, 'dev_sim_1', 'Simulator Device', 'Simulator', '{}'::jsonb
FROM edges e
JOIN sites s ON s.id = e.site_id
WHERE s.code = 'plant-a' AND e.edge_code = 'edge-sim-01'
ON CONFLICT (edge_id, device_code) DO NOTHING;

-- Map device -> connection
UPDATE devices d
SET connection_id = c.id
FROM connections c
JOIN edges e ON e.id = c.edge_id
JOIN sites s ON s.id = e.site_id
WHERE s.code = 'plant-a'
  AND d.edge_id = e.id
  AND (
      (e.edge_code = 'edge-com-01' AND c.connection_code = 'conn_scale_rs232_manual_1' AND d.device_code = 'dev_scale_manual_1')
   OR (e.edge_code = 'edge-modbus-01' AND c.connection_code = 'conn_modbus_rtu_com10_1' AND d.device_code IN ('dev_modbus_rtu_50', 'dev_modbus_rtu_100', 'dev_modbus_rtu_70'))
   OR (e.edge_code = 'edge-sim-01' AND c.connection_code = 'conn_sim_1' AND d.device_code = 'dev_sim_1')
  );

-- Tags
INSERT INTO tags (
    device_id, tag_code, name, value_type, source, unit, metadata_json,
    tag_code_canonical, display_name, aliases_json
)
SELECT d.id,
       'tag_scale_manual_compound',
       'Scale Manual Compound',
       'string',
       'scale:compound',
       NULL,
       jsonb_build_object('update_mode','on_change'),
       'PLANTA1.SERIAL.MANUAL.SCALE.COMPOUND.PV',
       'Scale Manual Compound',
       '[]'::jsonb
FROM devices d
JOIN edges e ON e.id = d.edge_id
JOIN sites s ON s.id = e.site_id
WHERE s.code = 'plant-a'
  AND e.edge_code = 'edge-com-01'
  AND d.device_code = 'dev_scale_manual_1'
ON CONFLICT (device_id, tag_code) DO UPDATE
SET source = EXCLUDED.source,
    value_type = EXCLUDED.value_type,
    metadata_json = EXCLUDED.metadata_json,
    tag_code_canonical = EXCLUDED.tag_code_canonical,
    display_name = EXCLUDED.display_name;

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
    '[]'::jsonb
FROM devices d
JOIN edges e ON e.id = d.edge_id
JOIN sites s ON s.id = e.site_id
JOIN (
    VALUES
      ('dev_modbus_rtu_50',  'tag_modbus_rtu_50_status',   'RTU 50 Status',      'integer', 'hr:0:u16',   NULL,   '{"modbus":{"slave_id":50,"register":0,"encoding":"u16"}, "update_mode":"polling", "interval_ms":500}',  'PLANTA1.MODBUS.RTU50.STATUS.STATE.PV', 'RTU 50 Status'),
      ('dev_modbus_rtu_100', 'tag_airborne_particle_pm1',  'Airborne PM1',       'float',   'hr:10:f32',  'ug/m3','{"modbus":{"slave_id":100,"register":10,"encoding":"f32"}, "update_mode":"polling", "interval_ms":500}', 'PLANTA1.MODBUS.AIRBORNE.PM1.MASS.PV',  'Airborne PM1'),
      ('dev_modbus_rtu_100', 'tag_airborne_particle_pm25', 'Airborne PM2_5',     'float',   'hr:12:f32',  'ug/m3','{"modbus":{"slave_id":100,"register":12,"encoding":"f32"}, "update_mode":"polling", "interval_ms":500}', 'PLANTA1.MODBUS.AIRBORNE.PM25.MASS.PV', 'Airborne PM2.5'),
      ('dev_modbus_rtu_100', 'tag_airborne_particle_pm10', 'Airborne PM10',      'float',   'hr:14:f32',  'ug/m3','{"modbus":{"slave_id":100,"register":14,"encoding":"f32"}, "update_mode":"polling", "interval_ms":500}', 'PLANTA1.MODBUS.AIRBORNE.PM10.MASS.PV', 'Airborne PM10'),
      ('dev_modbus_rtu_70',  'tag_modbus_rtu_70_status',   'RTU 70 Status',      'integer', 'hr:0:u16',   NULL,   '{"modbus":{"slave_id":70,"register":0,"encoding":"u16"}, "update_mode":"polling", "interval_ms":500}',  'PLANTA1.MODBUS.RTU70.STATUS.STATE.PV', 'RTU 70 Status')
) AS t(device_code, tag_code, name, value_type, source, unit, metadata_json, tag_code_canonical, display_name) ON t.device_code = d.device_code
WHERE s.code = 'plant-a'
  AND e.edge_code = 'edge-modbus-01'
ON CONFLICT (device_id, tag_code) DO UPDATE
SET source = EXCLUDED.source,
    value_type = EXCLUDED.value_type,
    unit = EXCLUDED.unit,
    metadata_json = EXCLUDED.metadata_json,
    tag_code_canonical = EXCLUDED.tag_code_canonical,
    display_name = EXCLUDED.display_name;

INSERT INTO tags (
    device_id, tag_code, name, value_type, source, unit, metadata_json,
    tag_code_canonical, display_name, aliases_json
)
SELECT d.id,
       'tag_sim_1',
       'Simulator Tag 1',
       'float',
       'sim:1',
       NULL,
       jsonb_build_object('update_mode','polling','interval_ms',500),
       'PLANTA1.SIMULATOR.RUNTIME.TAG1.VALUE.PV',
       'Simulator Tag 1',
       '[]'::jsonb
FROM devices d
JOIN edges e ON e.id = d.edge_id
JOIN sites s ON s.id = e.site_id
WHERE s.code = 'plant-a'
  AND e.edge_code = 'edge-sim-01'
  AND d.device_code = 'dev_sim_1'
ON CONFLICT (device_id, tag_code) DO UPDATE
SET source = EXCLUDED.source,
    value_type = EXCLUDED.value_type,
    metadata_json = EXCLUDED.metadata_json,
    tag_code_canonical = EXCLUDED.tag_code_canonical,
    display_name = EXCLUDED.display_name;

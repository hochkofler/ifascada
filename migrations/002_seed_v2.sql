-- Seed Data V2 (Device-Centric)
-- Depends on: 001_initial_schema_v2.sql

INSERT INTO edge_agents (id, description, status, last_heartbeat)
VALUES ('test-agent-01', 'Test Agent 01 (V2)', 'online', NOW())
ON CONFLICT (id) DO NOTHING;

-- 1. Insert Devices (Connection Logic)
INSERT INTO devices (
    id, edge_agent_id, name, driver_type, connection_config, enabled
) VALUES
-- Scale: RS232 Connection
(
    'scale_01', 'test-agent-01', 'Balanza de Producción', 'RS232',
    '{"port":"COM7","baud_rate":9600,"data_bits":8,"stop_bits":1,"parity":"None","timeout_ms":500}',
    true
),
-- Thermohygrometer: Modbus Connection
(
    'termohidrometro01', 'test-agent-01', 'Termohidrómetro Ambiental', 'Modbus',
    '{"port":"COM10","baud_rate":9600,"data_bits":8,"stop_bits":1,"parity":"None","slave_id":84,"timeout_ms":3000}',
    true
)
ON CONFLICT (id) DO NOTHING;

-- 2. Insert Tags (Meaning Logic)
INSERT INTO tags (
    id, device_id, source_config,
    update_mode, update_config, value_type, enabled,
    description, pipeline_config
) VALUES
-- Weight: Inherits RS232 from scale_01.
(
    'weigh_scale01', 'scale_01',
    '{}',
    'OnChange', '{"debounce_ms":500,"timeout_ms":5000}',
    'Composite', true,
    'Peso Scale 01',
    NULL
),
-- Temp: Inherits Modbus from termohidrometro01. Specific Register 0.
(
    'Temp', 'termohidrometro01',
    '{"register":0,"count":1,"register_type":"Input"}',
    'Polling', '{"interval_ms":5000}',
    'Simple', true,
    'Temperatura Device 01',
    '{"scaling":{"type":"Linear","slope":0.1,"intercept":0.0}}'
),
-- Humidity: Inherits Modbus from termohidrometro01. Specific Register 1.
(
    'Humedad', 'termohidrometro01',
    '{"register":1,"count":1,"register_type":"Input"}',
    'Polling', '{"interval_ms":5000}',
    'Simple', true,
    'Humedad Device 01',
    '{"scaling":{"type":"Linear","slope":0.1,"intercept":0.0}}'
)
ON CONFLICT (id) DO NOTHING;

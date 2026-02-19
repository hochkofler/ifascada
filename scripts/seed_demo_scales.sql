-- Seed data for multi-scale demo

-- Ensure the edge agent exists
INSERT INTO edge_agents (id, description, status, last_heartbeat, created_at, updated_at)
VALUES ('agent-1', 'Main Production Agent', 'offline', NULL, NOW(), NOW())
ON CONFLICT (id) DO UPDATE 
SET description = EXCLUDED.description, updated_at = NOW();

-- Insert Scale 1 (COM3 - Standard Scale)
INSERT INTO tags (id, driver_type, driver_config, edge_agent_id, update_mode, update_config, value_type, enabled)
VALUES (
    'SCALE_01', 
    'RS232', 
    '{"port": "COM3", "baud_rate": 9600, "data_bits": 8, "stop_bits": 1, "parity": "None", "timeout_ms": 1000}', 
    'agent-1', 
    'OnChange', 
    '{"type": "OnChange", "debounce_ms": 500, "timeout_ms": 5000}', 
    'Simple', 
    true
)
ON CONFLICT (id) DO UPDATE 
SET driver_config = EXCLUDED.driver_config, 
    update_mode = EXCLUDED.update_mode,
    update_config = EXCLUDED.update_config,
    enabled = true,
    updated_at = NOW();

-- Insert Scale 2 (COM4 - Precision Balance)
INSERT INTO tags (id, driver_type, driver_config, edge_agent_id, update_mode, update_config, value_type, enabled)
VALUES (
    'SCALE_02', 
    'RS232', 
    '{"port": "COM4", "baud_rate": 9600, "data_bits": 8, "stop_bits": 1, "parity": "None", "timeout_ms": 1000}', 
    'agent-1', 
    'OnChange', 
    '{"type": "OnChange", "debounce_ms": 500, "timeout_ms": 5000}', 
    'Simple', 
    true
)
ON CONFLICT (id) DO UPDATE 
SET driver_config = EXCLUDED.driver_config, 
    update_mode = EXCLUDED.update_mode,
    update_config = EXCLUDED.update_config,
    enabled = true,
    updated_at = NOW();

-- Clear existing data (optional, remove if you want to keep data)
TRUNCATE tag_events, tags, edge_agents CASCADE;

-- 1. Insert Edge Agent
INSERT INTO edge_agents (id, description, status, last_heartbeat)
VALUES ('agent-1', 'Docker Simulation Agent', 'Online', NOW())
ON CONFLICT (id) DO NOTHING;

-- 2. Insert Simulator Tag
-- This tag uses the 'Simulator' driver we just implemented.
-- It generates a sine wave between 0 and 50 kg every 1 second.
INSERT INTO tags (
    id, 
    edge_agent_id, 
    driver_type, 
    driver_config, 
    update_mode, 
    update_config, 
    value_type, 
    enabled
)
VALUES (
    'SIM_SCALE_01',             -- Tag ID
    'agent-1',                  -- Agent ID
    'Simulator',                -- Driver Type
    '{                          
        "min_value": 0,
        "max_value": 50,
        "interval_ms": 1000,
        "unit": "kg",
        "pattern": "sine"
    }'::jsonb,                  -- Driver Config
    'Polling',                  -- Update Mode (Simulated Polling)
    '{"interval_ms": 1000}'::jsonb,
    'Simple',                   -- Value Type
    true                        -- Enabled
)
ON CONFLICT (id) DO UPDATE 
SET driver_type = EXCLUDED.driver_type,
    driver_config = EXCLUDED.driver_config;

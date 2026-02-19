-- Clear existing data
TRUNCATE tag_events, tags, edge_agents CASCADE;

-- 1. Insert Edge Agent
INSERT INTO edge_agents (id, description, status, last_heartbeat)
VALUES ('agent-1', 'Composite Tag Demo Agent', 'Online', NOW())
ON CONFLICT (id) DO NOTHING;

-- 2. Insert Composite Simulator Tag
-- This tag uses the 'Simulator' driver generating 'ST,GS,  XX.XXkg'
-- The Pipeline parses this into {"value": XX.XX, "unit": "kg"}
-- And validates that value is between 10 and 45.
INSERT INTO tags (
    id, 
    edge_agent_id, 
    driver_type, 
    driver_config, 
    update_mode, 
    update_config, 
    value_type, 
    enabled,
    pipeline_config
)
VALUES (
    'COMPOSITE_SCALE',          -- Tag ID
    'agent-1',                  -- Agent ID
    'Simulator',                -- Driver Type
    '{                          
        "min_value": 0,
        "max_value": 50,
        "interval_ms": 2000,
        "unit": "kg",
        "pattern": "sine"
    }'::jsonb,                  -- Driver Config
    'Polling',                  -- Update Mode
    '{"interval_ms": 1000}'::jsonb,
    'Composite',                -- Value Type (Composite = Value + Unit)
    true,                       -- Enabled
    '{
        "parser": {
            "type": "Custom",
            "name": "ScaleParser"
        },
        "validators": [
            {
                "type": "Range",
                "min": 10,
                "max": 45
            }
        ]
    }'::jsonb                   -- Pipeline Config (JSON)
)
ON CONFLICT (id) DO UPDATE 
SET driver_type = EXCLUDED.driver_type,
    pipeline_config = EXCLUDED.pipeline_config;

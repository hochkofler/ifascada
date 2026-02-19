-- Seed data from config/test_rs232_full_scale.json
-- Target agent: test-agent-rs232

-- 1. Ensure the Edge Agent exists
INSERT INTO edge_agents (id, description, status, created_at, updated_at)
VALUES (
    'test-agent-rs232', 
    'RS232 Test Agent (Full Scale)', 
    'offline', 
    NOW(), 
    NOW()
)
ON CONFLICT (id) DO UPDATE 
SET description = EXCLUDED.description, updated_at = NOW();

-- 2. Insert Tags
-- Scale 1
INSERT INTO tags (
    id, driver_type, driver_config, edge_agent_id, 
    update_mode, update_config, value_type, value_schema, enabled,
    description, pipeline_config
) VALUES (
    'scale_1',
    'RS232',
    '{"port": "COM6", "baud_rate": 9600, "data_bits": 8, "parity": "None", "stop_bits": 1, "timeout_ms": 500}',
    'test-agent-rs232',
    'OnChange',
    '{"type": "OnChange", "debounce_ms": 500, "timeout_ms": 5000}',
    'Composite',
    '{"primary": "value", "labels": {"value": "Peso", "unit": "Ud"}}',
    true,
    'RS232 Scale (Full Feature: ScaleParser + Print)',
    '{
        "parser": {"type": "Custom", "name": "ScaleParser"},
        "validators": [],
        "automations": [
            {
                "name": "AccumulateWeight",
                "trigger": {
                    "type": "ConsecutiveValues",
                    "target_value": 0.0,
                    "count": 1,
                    "operator": "Greater",
                    "within_ms": 1000
                },
                "action": {
                    "type": "AccumulateData",
                    "session_id": "scale_session_1",
                    "template": "ignored_for_now"
                }
            },
            {
                "name": "PrintBatchOutcome",
                "trigger": {
                    "type": "ConsecutiveValues",
                    "target_value": 0.0,
                    "count": 2,
                    "operator": "Equal",
                    "within_ms": 5000
                },
                "action": {
                    "type": "PrintBatch",
                    "session_id": "scale_session_1",
                    "header_template": "LOTE DE PESAJES",
                    "footer_template": "FIN DEL LOTE"
                }
            }
        ]
    }'
)
ON CONFLICT (id) DO UPDATE SET
    driver_config = EXCLUDED.driver_config,
    update_mode = EXCLUDED.update_mode,
    update_config = EXCLUDED.update_config,
    value_type = EXCLUDED.value_type,
    value_schema = EXCLUDED.value_schema,
    pipeline_config = EXCLUDED.pipeline_config,
    updated_at = NOW();

-- Scale 2
INSERT INTO tags (
    id, driver_type, driver_config, edge_agent_id, 
    update_mode, update_config, value_type, value_schema, enabled,
    description, pipeline_config
) VALUES (
    'scale_2',
    'RS232',
    '{"port": "COM2", "baud_rate": 9600, "data_bits": 8, "parity": "None", "stop_bits": 1, "timeout_ms": 500}',
    'test-agent-rs232',
    'OnChange',
    '{"type": "OnChange", "debounce_ms": 500, "timeout_ms": 5000}',
    'Composite',
    '{"primary": "value", "labels": {"value": "Peso", "unit": "Ud"}}',
    true,
    'RS232 Scale (Full Feature: ScaleParser + Print)',
    '{
        "parser": {"type": "Custom", "name": "ScaleParser"},
        "validators": [],
        "automations": [
            {
                "name": "AccumulateWeight",
                "trigger": {
                    "type": "ConsecutiveValues",
                    "target_value": 0.0,
                    "count": 1,
                    "operator": "Greater",
                    "within_ms": 1000
                },
                "action": {
                    "type": "AccumulateData",
                    "session_id": "scale_session_2",
                    "template": "ignored_for_now"
                }
            },
            {
                "name": "PrintBatchOutcome",
                "trigger": {
                    "type": "ConsecutiveValues",
                    "target_value": 0.0,
                    "count": 2,
                    "operator": "Equal",
                    "within_ms": 5000
                },
                "action": {
                    "type": "PrintBatch",
                    "session_id": "scale_session_2",
                    "header_template": "LOTE DE PESAJES",
                    "footer_template": "FIN DEL LOTE"
                }
            }
        ]
    }'
)
ON CONFLICT (id) DO UPDATE SET
    driver_config = EXCLUDED.driver_config,
    update_mode = EXCLUDED.update_mode,
    update_config = EXCLUDED.update_config,
    value_type = EXCLUDED.value_type,
    value_schema = EXCLUDED.value_schema,
    pipeline_config = EXCLUDED.pipeline_config,
    updated_at = NOW();

-- Scale 3
INSERT INTO tags (
    id, driver_type, driver_config, edge_agent_id, 
    update_mode, update_config, value_type, value_schema, enabled,
    description, pipeline_config
) VALUES (
    'scale_3',
    'RS232',
    '{"port": "COM3", "baud_rate": 9600, "data_bits": 8, "parity": "None", "stop_bits": 1, "timeout_ms": 500}',
    'test-agent-rs232',
    'OnChange',
    '{"type": "OnChange", "debounce_ms": 500, "timeout_ms": 5000}',
    'Composite',
    '{"primary": "value", "labels": {"value": "Peso", "unit": "Ud"}}',
    true,
    'RS232 Scale (Full Feature: ScaleParser + Print)',
    '{
        "parser": {"type": "Custom", "name": "ScaleParser"},
        "validators": [],
        "automations": [
            {
                "name": "AccumulateWeight",
                "trigger": {
                    "type": "ConsecutiveValues",
                    "target_value": 0.0,
                    "count": 1,
                    "operator": "Greater",
                    "within_ms": 1000
                },
                "action": {
                    "type": "AccumulateData",
                    "session_id": "scale_session_3",
                    "template": "ignored_for_now"
                }
            },
            {
                "name": "PrintBatchOutcome",
                "trigger": {
                    "type": "ConsecutiveValues",
                    "target_value": 0.0,
                    "count": 2,
                    "operator": "Equal",
                    "within_ms": 5000
                },
                "action": {
                    "type": "PrintBatch",
                    "session_id": "scale_session_3",
                    "header_template": "LOTE DE PESAJES",
                    "footer_template": "FIN DEL LOTE"
                }
            }
        ]
    }'
)
ON CONFLICT (id) DO UPDATE SET
    driver_config = EXCLUDED.driver_config,
    update_mode = EXCLUDED.update_mode,
    update_config = EXCLUDED.update_config,
    value_type = EXCLUDED.value_type,
    pipeline_config = EXCLUDED.pipeline_config,
    updated_at = NOW();

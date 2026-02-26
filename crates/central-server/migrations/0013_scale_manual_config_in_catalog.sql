-- Persist manual scale configuration in existing catalog entities.
-- No new tables: use connections/devices/tags and metadata_json.

-- 1) Connection-level transport/frame/parser/timeouts for edge-01 manual scale.
INSERT INTO connections (edge_id, connection_code, name, driver_type, metadata_json)
SELECT
    e.id,
    'conn_scale_rs232_manual_1',
    'Scale RS232 Manual',
    'SerialAscii',
    jsonb_build_object(
        'transport', jsonb_build_object(
            'kind', 'serial',
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
            'type', 'regex',
            'pattern', '^[[:space:]]*([+-])?[[:space:]]*([0-9]+(?:[.][0-9]+)?)[[:space:]]*([A-Za-z]+)[[:space:]]*$',
            'groups', jsonb_build_object(
                'sign', 1,
                'value', 2,
                'unit', 3
            )
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
  AND e.edge_code = 'edge-01'
ON CONFLICT (edge_id, connection_code)
DO UPDATE SET
    name = EXCLUDED.name,
    driver_type = EXCLUDED.driver_type,
    metadata_json = COALESCE(connections.metadata_json, '{}'::jsonb) || EXCLUDED.metadata_json,
    updated_at = NOW();

-- 2) Device-level association and device policy.
UPDATE devices d
SET
    connection_id = c.id,
    metadata_json = jsonb_strip_nulls(
        COALESCE(d.metadata_json, '{}'::jsonb) ||
        jsonb_build_object(
            'runtime', jsonb_build_object(
                'connection_code', 'conn_scale_rs232_manual_1'
            ),
            'quality_policy', jsonb_build_object(
                'stale_after_secs', 45
            )
        )
    )
FROM connections c
JOIN edges e ON e.id = c.edge_id
JOIN sites s ON s.id = e.site_id
WHERE d.edge_id = e.id
  AND d.device_code = 'dev_scale_manual_1'
  AND s.code = 'plant-a'
  AND e.edge_code = 'edge-01'
  AND c.connection_code = 'conn_scale_rs232_manual_1';

-- 3) Tag-level pipeline and UI display policy.
UPDATE tags t
SET metadata_json = jsonb_strip_nulls(
    COALESCE(t.metadata_json, '{}'::jsonb) ||
    jsonb_build_object(
        'pipeline', jsonb_build_object(
            'extract', 'scale:compound',
            'format', '{value} {unit}',
            'trim', TRUE
        ),
        'ui', jsonb_build_object(
            'display_mode', 'compound_only'
        )
    )
)
FROM devices d
WHERE t.device_id = d.id
  AND d.device_code = 'dev_scale_manual_1'
  AND t.tag_code = 'tag_scale_manual_compound';

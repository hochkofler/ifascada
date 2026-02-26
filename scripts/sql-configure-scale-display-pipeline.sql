-- Configure display pipeline for manual scale compound tag:
-- 1) extract value/unit/raw from compound JSON payload
-- 2) format as "{value} {unit}"
-- 3) trim whitespace
--
-- Target:
--   connection_code = conn_scale_rs232_manual_1
--   tag_code        = tag_scale_manual_compound

WITH target_tag AS (
    SELECT t.id
    FROM tags t
    JOIN devices d ON d.id = t.device_id
    JOIN connections c ON c.id = d.connection_id
    WHERE c.connection_code = 'conn_scale_rs232_manual_1'
      AND t.tag_code = 'tag_scale_manual_compound'
    LIMIT 1
)
UPDATE tags t
SET metadata_json = jsonb_set(
    jsonb_set(
        jsonb_set(
            COALESCE(t.metadata_json, '{}'::jsonb),
            '{pipeline,extract}',
            '"scale:compound"'::jsonb,
            true
        ),
        '{pipeline,format}',
        '"{value} {unit}"'::jsonb,
        true
    ),
    '{pipeline,trim}',
    'true'::jsonb,
    true
)
WHERE t.id IN (SELECT id FROM target_tag);

-- Verify
SELECT
    t.tag_code,
    t.metadata_json->'pipeline' AS pipeline
FROM tags t
JOIN devices d ON d.id = t.device_id
JOIN connections c ON c.id = d.connection_id
WHERE c.connection_code = 'conn_scale_rs232_manual_1'
  AND t.tag_code = 'tag_scale_manual_compound';

-- Hotfix: normalize SerialAscii parser regex to avoid double-escaped sequences.
-- Applies to existing catalogs without reset.

WITH normalized AS (
    SELECT
        c.id,
        '^[[:space:]]*([+-])?[[:space:]]*([0-9]+(?:[.][0-9]+)?)[[:space:]]*([A-Za-z]+)[[:space:]]*$'::text AS rx
    FROM connections c
    WHERE c.driver_type ILIKE 'SerialAscii'
)
UPDATE connections c
SET metadata_json =
    jsonb_set(
        jsonb_set(
            COALESCE(c.metadata_json, '{}'::jsonb),
            '{parser,regex}',
            to_jsonb(n.rx),
            true
        ),
        '{parser,pattern}',
        to_jsonb(n.rx),
        true
    ),
    updated_at = NOW()
FROM normalized n
WHERE c.id = n.id;

-- verify
SELECT
    c.connection_code,
    c.metadata_json #>> '{parser,regex}'   AS parser_regex,
    c.metadata_json #>> '{parser,pattern}' AS parser_pattern
FROM connections c
WHERE c.driver_type ILIKE 'SerialAscii'
ORDER BY c.connection_code;

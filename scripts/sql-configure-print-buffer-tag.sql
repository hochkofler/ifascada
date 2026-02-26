-- Configure tag-scoped automations for:
-- 1) Buffer positive values
-- 2) On double negative -> print buffered values locally on edge
-- 3) Emit print.persist action for central-side persistence/audit pipeline
--
-- Context:
--   edge:         edge-01 (or any; query keys by connection+tag)
--   connection:   conn_scale_rs232_manual_1
--   tag:          tag_scale_manual_compound
--
-- Notes:
-- - trigger.tag_id is intentionally omitted (inferred from tag_code by central runtime builder).
-- - buffer_id can be omitted; explicit value is kept for readability/ops.

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
    COALESCE(t.metadata_json, '{}'::jsonb),
    '{automations}',
    '[
      {
        "id": "auto_buffer_positive",
        "name": "buffer_positive_weights",
        "enabled": true,
        "trigger": {
          "type": "consecutive_numeric",
          "operator": "gt",
          "threshold": 0,
          "count": 1
        },
        "actions": [
          {
            "action_type": "buffer.weights.accumulate",
            "target": "edge",
            "scope": "edge",
            "payload": {
              "buffer_id": "weights_session_1",
              "only_positive": true,
              "max_items": 500
            }
          }
        ]
      },
      {
        "id": "auto_double_negative_print_and_persist",
        "name": "double_negative_print_and_persist",
        "enabled": true,
        "trigger": {
          "type": "consecutive_numeric",
          "operator": "lt",
          "threshold": 0,
          "count": 2,
          "within_ms": 10000
        },
        "actions": [
          {
            "action_type": "print.escpos",
            "target": "edge",
            "scope": "edge",
            "payload": {
              "mode": "from_buffer",
              "buffer_id": "weights_session_1",
              "clear_after_print": true
            }
          },
          {
            "action_type": "print.persist",
            "target": "central",
            "scope": "central",
            "payload": {
              "buffer_id": "weights_session_1",
              "event": "print_done"
            }
          }
        ]
      }
    ]'::jsonb,
    true
)
WHERE t.id IN (SELECT id FROM target_tag);

-- Verify
SELECT
    t.tag_code,
    t.metadata_json->'automations' AS automations
FROM tags t
JOIN devices d ON d.id = t.device_id
JOIN connections c ON c.id = d.connection_id
WHERE c.connection_code = 'conn_scale_rs232_manual_1'
  AND t.tag_code = 'tag_scale_manual_compound';

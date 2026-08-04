-- Phase: device.command + printer on-demand + negative trigger workflow
-- Safe to re-run (idempotent).
--
-- This migration configures:
-- 1) A printer connection/device in catalog (edge-com-01).
-- 2) On-demand status policy for printer device.
-- 3) Scale tag pipeline metadata for display.
-- 4) Tag automation:
--    - accumulate positives
--    - on 2 consecutive negatives: connection check + print from buffer + print.persist

-- 1) Ensure printer connection exists (not runtime-polled; used for operational state/actions).
INSERT INTO connections (edge_id, connection_code, name, driver_type, metadata_json)
SELECT
    e.id,
    'conn_printer_u220_1',
    'Printer U220 Windows Share',
    'Unknown',
    jsonb_build_object(
        'transport', jsonb_build_object(
            'windows', jsonb_build_object(
                'share', '\\\\192.168.103.154\\EPSON TM-U220 Receipt LCC'
            )
        ),
        'usage', 'on_demand_actuator'
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

-- 2) Ensure printer device exists and mapped to connection.
INSERT INTO devices (edge_id, connection_id, device_code, name, driver_type, metadata_json)
SELECT
    e.id,
    c.id,
    'dev_printer_u220',
    'EPSON TM-U220 Receipt',
    'Unknown',
    jsonb_build_object(
        'status_policy', jsonb_build_object(
            'mode', 'on_demand',
            'stale_after_secs', 120
        ),
        'transport', jsonb_build_object(
            'windows', jsonb_build_object(
                'share', '\\\\192.168.103.154\\EPSON TM-U220 Receipt LCC'
            )
        ),
        'kind', 'printer'
    )
FROM edges e
JOIN sites s ON s.id = e.site_id
JOIN connections c ON c.edge_id = e.id
WHERE s.code = 'plant-a'
  AND e.edge_code = 'edge-com-01'
  AND c.connection_code = 'conn_printer_u220_1'
ON CONFLICT (edge_id, device_code) DO UPDATE
SET connection_id = EXCLUDED.connection_id,
    name = EXCLUDED.name,
    driver_type = EXCLUDED.driver_type,
    metadata_json = EXCLUDED.metadata_json;

-- 3) Configure scale tag pipeline + automations.
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
        COALESCE(t.metadata_json, '{}'::jsonb),
        '{pipeline}',
        jsonb_build_object(
            'extract', 'scale:compound',
            'format', '{value} {unit}',
            'trim', true
        ),
        true
    ),
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
        "id": "auto_double_negative_printer_workflow",
        "name": "double_negative_printer_workflow",
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
            "action_type": "device.command",
            "target": "edge",
            "scope": "edge",
            "payload": {
              "device_id": "dev_printer_u220",
              "connection_id": "conn_printer_u220_1",
              "command": "connection.check",
              "device": {
                "id": "dev_printer_u220",
                "transport": {
                  "windows": {
                    "share": "\\\\192.168.103.154\\EPSON TM-U220 Receipt LCC"
                  }
                }
              },
              "args": {
                "timeout_ms": 1200
              }
            }
          },
          {
            "action_type": "device.command",
            "target": "edge",
            "scope": "edge",
            "payload": {
              "device_id": "dev_printer_u220",
              "command": "print",
              "device": {
                "id": "dev_printer_u220",
                "transport": {
                  "windows": {
                    "share": "\\\\192.168.103.154\\EPSON TM-U220 Receipt LCC"
                  }
                }
              },
              "args": {
                "mode": "from_buffer",
                "buffer_id": "weights_session_1",
                "clear_after_print": true
              }
            }
          },
          {
            "action_type": "print.persist",
            "target": "central",
            "scope": "central",
            "payload": {
              "buffer_id": "weights_session_1",
              "event": "print_done",
              "device_id": "dev_printer_u220"
            }
          }
        ]
      }
    ]'::jsonb,
    true
)
WHERE t.id IN (SELECT id FROM target_tag);

-- 4) Ensure event-driven trigger evaluation for manual scale tag.
UPDATE tags t
SET metadata_json = jsonb_set(COALESCE(t.metadata_json, '{}'::jsonb), '{update_mode}', '"on_message"'::jsonb, true)
FROM devices d
JOIN connections c ON c.id = d.connection_id
WHERE t.device_id = d.id
  AND c.connection_code = 'conn_scale_rs232_manual_1'
  AND t.tag_code = 'tag_scale_manual_compound';

-- Verification snapshots
SELECT c.connection_code, c.driver_type, c.metadata_json
FROM connections c
WHERE c.connection_code = 'conn_printer_u220_1';

SELECT d.device_code, d.connection_id, d.metadata_json
FROM devices d
WHERE d.device_code = 'dev_printer_u220';

SELECT t.tag_code, t.metadata_json->'pipeline' AS pipeline, t.metadata_json->'automations' AS automations
FROM tags t
WHERE t.tag_code = 'tag_scale_manual_compound';

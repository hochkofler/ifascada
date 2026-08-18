-- Nueva balanza RS232 con prefijo K/K* en edge lcc01, COM6.
-- Patrón de entrada: "K +  0.0000  g", "K*+  0.8136  g", "K*-  0.8141  g"
-- Regex: ^\s*K\*?\s*([+-])?\s*(\d+(?:\.\d+)?)\s*([A-Za-z]+)\s*$

-- Connection
INSERT INTO connections (edge_id, connection_code, name, driver_type, metadata_json)
SELECT
    e.id,
    'CC-IN-BALA18-25',
    'CC-IN-BALA18-25',
    'SerialAscii',
    jsonb_build_object(
        'transport', jsonb_build_object(
            'serial', jsonb_build_object(
                'port', 'COM6',
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
            'read_timeout_ms', 100
        ),
        'parser', jsonb_build_object(
            'regex', '^[[:space:]]*K\*?[[:space:]]*([+-])?[[:space:]]*([0-9]+(?:[.][0-9]+)?)[[:space:]]*([A-Za-z]+)[[:space:]]*$',
            'sign_group', 1,
            'value_group', 2,
            'unit_group', 3
        ),
        'timeouts', jsonb_build_object(
            'request_timeout_ms', 200,
            'reconnect_delay_ms', 1000,
            'max_retries', NULL
        )
    )
FROM edges e
WHERE e.edge_code = 'lcc01'
ON CONFLICT (edge_id, connection_code) DO UPDATE
SET name = EXCLUDED.name,
    driver_type = EXCLUDED.driver_type,
    metadata_json = EXCLUDED.metadata_json,
    updated_at = NOW();

-- Device
INSERT INTO devices (edge_id, device_code, name, driver_type, metadata_json)
SELECT e.id, 'CC-IN-BALA18-25', 'CC-IN-BALA18-25', 'SerialAscii', '{}'::jsonb
FROM edges e
WHERE e.edge_code = 'lcc01'
ON CONFLICT (edge_id, device_code) DO NOTHING;

-- Map device -> connection
UPDATE devices d
SET connection_id = c.id
FROM connections c
JOIN edges e ON e.id = c.edge_id
WHERE e.edge_code = 'lcc01'
  AND d.edge_id = e.id
  AND d.device_code = 'CC-IN-BALA18-25'
  AND c.connection_code = 'CC-IN-BALA18-25';

-- Tag
INSERT INTO tags (
    device_id, tag_code, name, value_type, source, unit, metadata_json,
    tag_code_canonical, display_name, aliases_json
)
SELECT d.id,
       'tag_cc_in_bala18_25_weight',
       'Peso CC-IN-BALA18-25',
       'string',
       'scale:compound',
       'g',
       jsonb_build_object(
           'update_mode', 'on_message',
           'pipeline', jsonb_build_object(
               'trim', true,
               'format', '{value} {unit}',
               'extract', 'scale:compound'
           ),
           'automations', jsonb_build_array(
               jsonb_build_object(
                   'id', 'auto_buffer_tag_cc_in_bala18_25_weight',
                   'name', 'Acumular pesos positivos',
                   'enabled', true,
                   'trigger', jsonb_build_object(
                       'type', 'consecutive_numeric',
                       'count', 1,
                       'operator', 'gt',
                       'threshold', 0
                   ),
                   'actions', jsonb_build_array(
                       jsonb_build_object(
                           'scope', 'edge',
                           'target', 'edge',
                           'action_type', 'buffer.weights.accumulate',
                           'payload', jsonb_build_object(
                               'buffer_id', 'weights_bala18_25',
                               'max_items', 500,
                               'only_positive', true
                           )
                       )
                   )
               ),
               jsonb_build_object(
                   'id', 'auto_print_tag_cc_in_bala18_25_weight',
                   'name', 'Imprimir al confirmar dos negativos',
                   'enabled', true,
                   'trigger', jsonb_build_object(
                       'type', 'consecutive_numeric',
                       'count', 2,
                       'operator', 'lte',
                       'threshold', 0,
                       'within_ms', 10000
                   ),
                   'actions', jsonb_build_array(
                       jsonb_build_object(
                           'scope', 'edge',
                           'target', 'edge',
                           'action_type', 'device.command',
                           'payload', jsonb_build_object(
                               'command', 'connection.check',
                               'device_id', 'dev_printer_u220',
                               'connection_id', 'conn_printer_u220_1',
                               'device', jsonb_build_object(
                                   'id', 'dev_printer_u220',
                                   'transport', jsonb_build_object(
                                       'windows', jsonb_build_object(
                                           'share', '\\\\192.168.103.154\\IFA-SCADA-TMU220-RAW'
                                       )
                                   )
                               ),
                               'args', jsonb_build_object('timeout_ms', 1200)
                           )
                       ),
                       jsonb_build_object(
                           'scope', 'edge',
                           'target', 'edge',
                           'action_type', 'device.command',
                           'payload', jsonb_build_object(
                               'command', 'print',
                               'device_id', 'dev_printer_u220',
                               'device', jsonb_build_object(
                                   'id', 'dev_printer_u220',
                                   'transport', jsonb_build_object(
                                       'windows', jsonb_build_object(
                                           'share', '\\\\192.168.103.154\\IFA-SCADA-TMU220-RAW'
                                       )
                                   )
                               ),
                               'args', jsonb_build_object(
                                   'buffer_id', 'weights_bala18_25',
                                   'mode', 'from_buffer',
                                   'clear_after_print', true
                               )
                           )
                       ),
                       jsonb_build_object(
                           'scope', 'central',
                           'target', 'central',
                           'action_type', 'print.persist',
                           'payload', jsonb_build_object(
                               'buffer_id', 'weights_bala18_25',
                               'device_id', 'dev_printer_u220',
                               'event', 'print_done'
                           )
                       )
                   )
               )
           )
       ),
       'IFA.LCC.CABINAS.BALA18.WEIGHT.PV',
       'Peso CC-IN-BALA18-25',
       '[]'::jsonb
FROM devices d
JOIN edges e ON e.id = d.edge_id
WHERE e.edge_code = 'lcc01'
  AND d.device_code = 'CC-IN-BALA18-25'
ON CONFLICT (device_id, tag_code) DO UPDATE
SET source = EXCLUDED.source,
    value_type = EXCLUDED.value_type,
    unit = EXCLUDED.unit,
    metadata_json = EXCLUDED.metadata_json,
    tag_code_canonical = EXCLUDED.tag_code_canonical,
    display_name = EXCLUDED.display_name;

INSERT INTO sites (code, name, timezone)
VALUES ('plant-a', 'Plant A', 'UTC')
ON CONFLICT (code) DO NOTHING;

INSERT INTO edges (site_id, edge_code, name, status)
SELECT s.id, 'edge-01', 'Edge 01', 'online'
FROM sites s
WHERE s.code = 'plant-a'
ON CONFLICT (site_id, edge_code) DO NOTHING;

INSERT INTO devices (edge_id, device_code, name, driver_type)
SELECT e.id, 'dev_scale_sim_1', 'Scale Sim Device', 'SerialAscii'
FROM edges e
JOIN sites s ON s.id = e.site_id
WHERE s.code = 'plant-a' AND e.edge_code = 'edge-01'
ON CONFLICT (edge_id, device_code) DO NOTHING;

INSERT INTO devices (edge_id, device_code, name, driver_type)
SELECT e.id, 'dev_scale_manual_1', 'Scale Manual Device', 'SerialAscii'
FROM edges e
JOIN sites s ON s.id = e.site_id
WHERE s.code = 'plant-a' AND e.edge_code = 'edge-01'
ON CONFLICT (edge_id, device_code) DO NOTHING;

INSERT INTO tags (device_id, tag_code, name, value_type, source, unit, metadata_json, tag_code_canonical, display_name, aliases_json)
SELECT d.id, 'tag_scale_sim_compound', 'Scale Sim Compound', 'string', 'scale:compound', NULL, '{}'::jsonb,
       'PLTA.AREA.UN01.SIMSCL.WEIGH.PV', 'Scale Sim Compound', '[]'::jsonb
FROM devices d
JOIN edges e ON e.id = d.edge_id
JOIN sites s ON s.id = e.site_id
WHERE s.code = 'plant-a' AND e.edge_code = 'edge-01' AND d.device_code = 'dev_scale_sim_1'
ON CONFLICT (device_id, tag_code) DO NOTHING;

INSERT INTO tags (device_id, tag_code, name, value_type, source, unit, metadata_json, tag_code_canonical, display_name, aliases_json)
SELECT d.id, 'tag_scale_manual_compound', 'Scale Manual Compound', 'string', 'scale:compound', NULL, '{}'::jsonb,
       'PLTA.AREA.UN01.MANSCL.WEIGH.PV', 'Scale Manual Compound', '[]'::jsonb
FROM devices d
JOIN edges e ON e.id = d.edge_id
JOIN sites s ON s.id = e.site_id
WHERE s.code = 'plant-a' AND e.edge_code = 'edge-01' AND d.device_code = 'dev_scale_manual_1'
ON CONFLICT (device_id, tag_code) DO NOTHING;

DELETE FROM tags
WHERE tag_code IN ('tag_scale_sim_raw', 'tag_scale_manual_raw')
  AND NOT EXISTS (SELECT 1 FROM tag_current_state tcs WHERE tcs.tag_id = tags.id)
  AND NOT EXISTS (SELECT 1 FROM telemetry_samples ts WHERE ts.tag_id = tags.id);

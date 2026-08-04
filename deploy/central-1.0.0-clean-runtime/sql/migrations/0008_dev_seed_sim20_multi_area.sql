-- Multi-area context seed with 20 simulated tags (4 edges x 5 tags)

-- Line
INSERT INTO lines (site_id, code, name)
SELECT s.id, 'line-sim', 'Line Sim'
FROM sites s
WHERE s.code = 'plant-a'
ON CONFLICT (site_id, code) DO NOTHING;

-- Areas
INSERT INTO areas (line_id, code, name)
SELECT l.id, 'area-pack', 'Area Packing'
FROM lines l
JOIN sites s ON s.id = l.site_id
WHERE s.code = 'plant-a' AND l.code = 'line-sim'
ON CONFLICT (line_id, code) DO NOTHING;

INSERT INTO areas (line_id, code, name)
SELECT l.id, 'area-mix', 'Area Mixing'
FROM lines l
JOIN sites s ON s.id = l.site_id
WHERE s.code = 'plant-a' AND l.code = 'line-sim'
ON CONFLICT (line_id, code) DO NOTHING;

-- Cells
INSERT INTO cells (area_id, code, name)
SELECT a.id, 'cell-pack-1', 'Cell Pack 1'
FROM areas a
JOIN lines l ON l.id = a.line_id
JOIN sites s ON s.id = l.site_id
WHERE s.code = 'plant-a' AND l.code = 'line-sim' AND a.code = 'area-pack'
ON CONFLICT (area_id, code) DO NOTHING;

INSERT INTO cells (area_id, code, name)
SELECT a.id, 'cell-pack-2', 'Cell Pack 2'
FROM areas a
JOIN lines l ON l.id = a.line_id
JOIN sites s ON s.id = l.site_id
WHERE s.code = 'plant-a' AND l.code = 'line-sim' AND a.code = 'area-pack'
ON CONFLICT (area_id, code) DO NOTHING;

INSERT INTO cells (area_id, code, name)
SELECT a.id, 'cell-mix-1', 'Cell Mix 1'
FROM areas a
JOIN lines l ON l.id = a.line_id
JOIN sites s ON s.id = l.site_id
WHERE s.code = 'plant-a' AND l.code = 'line-sim' AND a.code = 'area-mix'
ON CONFLICT (area_id, code) DO NOTHING;

INSERT INTO cells (area_id, code, name)
SELECT a.id, 'cell-mix-2', 'Cell Mix 2'
FROM areas a
JOIN lines l ON l.id = a.line_id
JOIN sites s ON s.id = l.site_id
WHERE s.code = 'plant-a' AND l.code = 'line-sim' AND a.code = 'area-mix'
ON CONFLICT (area_id, code) DO NOTHING;

-- Edges (map each edge to one cell)
INSERT INTO edges (site_id, edge_code, name, status, cell_id)
SELECT s.id, 'edge-pack-1', 'Edge Pack 1', 'online', c.id
FROM sites s
JOIN lines l ON l.site_id = s.id AND l.code = 'line-sim'
JOIN areas a ON a.line_id = l.id AND a.code = 'area-pack'
JOIN cells c ON c.area_id = a.id AND c.code = 'cell-pack-1'
WHERE s.code = 'plant-a'
ON CONFLICT (site_id, edge_code) DO NOTHING;

INSERT INTO edges (site_id, edge_code, name, status, cell_id)
SELECT s.id, 'edge-pack-2', 'Edge Pack 2', 'online', c.id
FROM sites s
JOIN lines l ON l.site_id = s.id AND l.code = 'line-sim'
JOIN areas a ON a.line_id = l.id AND a.code = 'area-pack'
JOIN cells c ON c.area_id = a.id AND c.code = 'cell-pack-2'
WHERE s.code = 'plant-a'
ON CONFLICT (site_id, edge_code) DO NOTHING;

INSERT INTO edges (site_id, edge_code, name, status, cell_id)
SELECT s.id, 'edge-mix-1', 'Edge Mix 1', 'online', c.id
FROM sites s
JOIN lines l ON l.site_id = s.id AND l.code = 'line-sim'
JOIN areas a ON a.line_id = l.id AND a.code = 'area-mix'
JOIN cells c ON c.area_id = a.id AND c.code = 'cell-mix-1'
WHERE s.code = 'plant-a'
ON CONFLICT (site_id, edge_code) DO NOTHING;

INSERT INTO edges (site_id, edge_code, name, status, cell_id)
SELECT s.id, 'edge-mix-2', 'Edge Mix 2', 'online', c.id
FROM sites s
JOIN lines l ON l.site_id = s.id AND l.code = 'line-sim'
JOIN areas a ON a.line_id = l.id AND a.code = 'area-mix'
JOIN cells c ON c.area_id = a.id AND c.code = 'cell-mix-2'
WHERE s.code = 'plant-a'
ON CONFLICT (site_id, edge_code) DO NOTHING;

-- Devices (1 device per edge)
INSERT INTO devices (edge_id, device_code, name, driver_type)
SELECT e.id, 'dev-pack-1', 'Pack Device 1', 'Simulator'
FROM edges e
JOIN sites s ON s.id = e.site_id
WHERE s.code = 'plant-a' AND e.edge_code = 'edge-pack-1'
ON CONFLICT (edge_id, device_code) DO NOTHING;

INSERT INTO devices (edge_id, device_code, name, driver_type)
SELECT e.id, 'dev-pack-2', 'Pack Device 2', 'Simulator'
FROM edges e
JOIN sites s ON s.id = e.site_id
WHERE s.code = 'plant-a' AND e.edge_code = 'edge-pack-2'
ON CONFLICT (edge_id, device_code) DO NOTHING;

INSERT INTO devices (edge_id, device_code, name, driver_type)
SELECT e.id, 'dev-mix-1', 'Mix Device 1', 'Simulator'
FROM edges e
JOIN sites s ON s.id = e.site_id
WHERE s.code = 'plant-a' AND e.edge_code = 'edge-mix-1'
ON CONFLICT (edge_id, device_code) DO NOTHING;

INSERT INTO devices (edge_id, device_code, name, driver_type)
SELECT e.id, 'dev-mix-2', 'Mix Device 2', 'Simulator'
FROM edges e
JOIN sites s ON s.id = e.site_id
WHERE s.code = 'plant-a' AND e.edge_code = 'edge-mix-2'
ON CONFLICT (edge_id, device_code) DO NOTHING;

-- Tags helper rows (5 tags per edge/device)
WITH tag_rows AS (
    SELECT 'edge-pack-1'::text AS edge_code, 'dev-pack-1'::text AS device_code, 'tag_p1_t'::text AS prefix UNION ALL
    SELECT 'edge-pack-2','dev-pack-2','tag_p2_t' UNION ALL
    SELECT 'edge-mix-1','dev-mix-1','tag_m1_t' UNION ALL
    SELECT 'edge-mix-2','dev-mix-2','tag_m2_t'
),
nums AS (
    SELECT 1 AS n UNION ALL SELECT 2 UNION ALL SELECT 3 UNION ALL SELECT 4 UNION ALL SELECT 5
)
INSERT INTO tags (
    device_id, tag_code, name, value_type, source, unit, metadata_json,
    tag_code_canonical, display_name, aliases_json
)
SELECT
    d.id,
    tr.prefix || lpad(nums.n::text, 3, '0') AS tag_code,
    upper(tr.prefix) || lpad(nums.n::text, 3, '0') AS name,
    'float',
    'sim:' || nums.n::text,
    'u',
    jsonb_build_object('historian_deadband', 0.2, 'historian_max_interval_secs', 60),
    upper(
      CASE
        WHEN tr.edge_code = 'edge-pack-1' THEN 'PLTA.PACK.C01.DEVP1.S' || lpad(nums.n::text,2,'0') || '.PV'
        WHEN tr.edge_code = 'edge-pack-2' THEN 'PLTA.PACK.C02.DEVP2.S' || lpad(nums.n::text,2,'0') || '.PV'
        WHEN tr.edge_code = 'edge-mix-1' THEN 'PLTA.MIXA.C01.DEVM1.S' || lpad(nums.n::text,2,'0') || '.PV'
        ELSE 'PLTA.MIXA.C02.DEVM2.S' || lpad(nums.n::text,2,'0') || '.PV'
      END
    ),
    upper(tr.prefix) || lpad(nums.n::text, 3, '0'),
    '[]'::jsonb
FROM tag_rows tr
JOIN edges e ON e.edge_code = tr.edge_code
JOIN sites s ON s.id = e.site_id AND s.code = 'plant-a'
JOIN devices d ON d.edge_id = e.id AND d.device_code = tr.device_code
CROSS JOIN nums
ON CONFLICT (device_id, tag_code) DO NOTHING;

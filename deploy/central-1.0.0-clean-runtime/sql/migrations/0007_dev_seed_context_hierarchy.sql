INSERT INTO lines (site_id, code, name)
SELECT s.id, 'line-a', 'Line A'
FROM sites s
WHERE s.code = 'plant-a'
ON CONFLICT (site_id, code) DO NOTHING;

INSERT INTO areas (line_id, code, name)
SELECT l.id, 'area-a', 'Area A'
FROM lines l
JOIN sites s ON s.id = l.site_id
WHERE s.code = 'plant-a' AND l.code = 'line-a'
ON CONFLICT (line_id, code) DO NOTHING;

INSERT INTO cells (area_id, code, name)
SELECT a.id, 'cell-a', 'Cell A'
FROM areas a
JOIN lines l ON l.id = a.line_id
JOIN sites s ON s.id = l.site_id
WHERE s.code = 'plant-a' AND l.code = 'line-a' AND a.code = 'area-a'
ON CONFLICT (area_id, code) DO NOTHING;

UPDATE edges e
SET cell_id = c.id
FROM sites s
JOIN lines l ON l.site_id = s.id
JOIN areas a ON a.line_id = l.id
JOIN cells c ON c.area_id = a.id
WHERE s.code = 'plant-a'
  AND l.code = 'line-a'
  AND a.code = 'area-a'
  AND c.code = 'cell-a'
  AND e.site_id = s.id
  AND e.edge_code = 'edge-01'
  AND (e.cell_id IS DISTINCT FROM c.id);

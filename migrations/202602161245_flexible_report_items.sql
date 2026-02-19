-- Migration: Add flexible JSONB data column to report_items
-- This allows storing both Simple and Composite values without hardcoded structures

ALTER TABLE report_items 
ADD COLUMN data JSONB;

-- Optionally, migrate existing 'value' and 'unit' into 'data'
-- Assuming data structure: {"value": X, "unit": "Y"}
UPDATE report_items 
SET data = jsonb_build_object('value', value, 'unit', unit)
WHERE data IS NULL;

-- Make old columns nullable for transition
ALTER TABLE report_items 
ALTER COLUMN value DROP NOT NULL,
ALTER COLUMN unit DROP NOT NULL;

COMMENT ON COLUMN report_items.data IS 'Flexible storage for simple or composite tag values';

-- Add pipeline_config column to tags table
ALTER TABLE tags
ADD COLUMN pipeline_config JSONB DEFAULT NULL;

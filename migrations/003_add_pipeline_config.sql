-- Migration 003: Add pipeline_config column to tags table
-- This was part of the original V2 schema design but missing from the applied migration.

ALTER TABLE tags ADD COLUMN IF NOT EXISTS pipeline_config JSONB;

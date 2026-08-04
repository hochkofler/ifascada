ALTER TABLE edges
    ADD COLUMN IF NOT EXISTS metadata_json JSONB NOT NULL DEFAULT '{}'::jsonb;

CREATE INDEX IF NOT EXISTS idx_edges_metadata_json_gin
    ON edges USING GIN (metadata_json);

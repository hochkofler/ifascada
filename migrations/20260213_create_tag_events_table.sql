CREATE TABLE IF NOT EXISTS tag_events (
    id SERIAL PRIMARY KEY,
    tag_id TEXT NOT NULL,
    value JSONB NOT NULL,
    quality TEXT NOT NULL,
    timestamp TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_tag_events_tag_id_timestamp ON tag_events(tag_id, timestamp DESC);

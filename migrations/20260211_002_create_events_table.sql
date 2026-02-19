-- Create events table
CREATE TABLE IF NOT EXISTS events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    event_type TEXT NOT NULL,
    payload JSONB NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Create index on event_type for faster lookups
CREATE INDEX idx_events_event_type ON events(event_type);

-- Create index on occurred_at for time-based queries
CREATE INDEX idx_events_occurred_at ON events(occurred_at);

-- Create index on payload for JSONB queries (optional but good for filtering)
CREATE INDEX idx_events_payload ON events USING GIN (payload);

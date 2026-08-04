CREATE TABLE IF NOT EXISTS operational_events (
    id BIGSERIAL PRIMARY KEY,
    ts TIMESTAMPTZ NOT NULL,
    severity TEXT NOT NULL,
    event_type TEXT NOT NULL,
    site_code TEXT NOT NULL,
    edge_code TEXT,
    connection_id TEXT,
    device_code TEXT,
    tag_code TEXT,
    config_hash TEXT,
    op_id TEXT,
    message TEXT NOT NULL,
    payload_json JSONB NOT NULL DEFAULT '{}'::jsonb
);

CREATE INDEX IF NOT EXISTS idx_operational_events_ts
    ON operational_events(ts DESC);

CREATE INDEX IF NOT EXISTS idx_operational_events_site_edge_ts
    ON operational_events(site_code, edge_code, ts DESC);

CREATE INDEX IF NOT EXISTS idx_operational_events_tag_ts
    ON operational_events(tag_code, ts DESC);

CREATE INDEX IF NOT EXISTS idx_operational_events_type_sev_ts
    ON operational_events(event_type, severity, ts DESC);

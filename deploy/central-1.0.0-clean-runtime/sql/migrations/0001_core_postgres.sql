CREATE TABLE IF NOT EXISTS sites (
    id BIGSERIAL PRIMARY KEY,
    code TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    timezone TEXT NOT NULL DEFAULT 'UTC',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS edges (
    id BIGSERIAL PRIMARY KEY,
    site_id BIGINT NOT NULL REFERENCES sites(id),
    edge_code TEXT NOT NULL,
    name TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'unknown',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (site_id, edge_code)
);

CREATE TABLE IF NOT EXISTS devices (
    id BIGSERIAL PRIMARY KEY,
    edge_id BIGINT NOT NULL REFERENCES edges(id),
    device_code TEXT NOT NULL,
    name TEXT NOT NULL,
    driver_type TEXT NOT NULL,
    metadata_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (edge_id, device_code)
);

CREATE TABLE IF NOT EXISTS tags (
    id BIGSERIAL PRIMARY KEY,
    device_id BIGINT NOT NULL REFERENCES devices(id),
    tag_code TEXT NOT NULL,
    name TEXT NOT NULL,
    value_type TEXT NOT NULL,
    source TEXT NOT NULL,
    unit TEXT NULL,
    metadata_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (device_id, tag_code)
);

CREATE TABLE IF NOT EXISTS edge_current_state (
    edge_id BIGINT PRIMARY KEY REFERENCES edges(id),
    status TEXT NOT NULL,
    last_seen_at TIMESTAMPTZ NOT NULL,
    outbox_depth BIGINT NOT NULL DEFAULT 0,
    outbox_oldest_secs BIGINT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS tag_current_state (
    tag_id BIGINT PRIMARY KEY REFERENCES tags(id),
    ts TIMESTAMPTZ NOT NULL,
    value_json JSONB NOT NULL,
    quality_json JSONB NOT NULL,
    source TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS command_requests (
    id UUID PRIMARY KEY,
    command_id TEXT NOT NULL UNIQUE,
    site_id BIGINT NOT NULL REFERENCES sites(id),
    edge_id BIGINT NOT NULL REFERENCES edges(id),
    tag_id BIGINT NOT NULL REFERENCES tags(id),
    requested_value_json JSONB NOT NULL,
    priority TEXT NOT NULL DEFAULT 'normal',
    requested_by TEXT NOT NULL,
    requested_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    status TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS command_ack_events (
    id BIGSERIAL PRIMARY KEY,
    command_id TEXT NULL,
    edge_id BIGINT NULL REFERENCES edges(id),
    tag_id BIGINT NULL REFERENCES tags(id),
    success BOOLEAN NOT NULL,
    reason TEXT NULL,
    payload_json JSONB NOT NULL,
    ts TIMESTAMPTZ NOT NULL
);

CREATE TABLE IF NOT EXISTS command_audit_events (
    id BIGSERIAL PRIMARY KEY,
    command_id TEXT NULL,
    edge_id BIGINT NULL REFERENCES edges(id),
    tag_id BIGINT NULL REFERENCES tags(id),
    outcome TEXT NOT NULL,
    reason TEXT NULL,
    value_json JSONB NOT NULL,
    ts TIMESTAMPTZ NOT NULL
);

CREATE TABLE IF NOT EXISTS edge_health_events (
    id BIGSERIAL PRIMARY KEY,
    edge_id BIGINT NULL REFERENCES edges(id),
    status TEXT NOT NULL,
    payload_json JSONB NOT NULL,
    ts TIMESTAMPTZ NOT NULL
);

CREATE TABLE IF NOT EXISTS runtime_alert_events (
    id BIGSERIAL PRIMARY KEY,
    edge_id BIGINT NULL REFERENCES edges(id),
    alert_type TEXT NOT NULL,
    state TEXT NOT NULL,
    severity TEXT NOT NULL,
    payload_json JSONB NOT NULL,
    ts TIMESTAMPTZ NOT NULL
);

CREATE TABLE IF NOT EXISTS runtime_alert_active (
    alert_key TEXT PRIMARY KEY,
    edge_id BIGINT NULL REFERENCES edges(id),
    alert_type TEXT NOT NULL,
    severity TEXT NOT NULL,
    first_raised_at TIMESTAMPTZ NOT NULL,
    last_seen_at TIMESTAMPTZ NOT NULL,
    payload_json JSONB NOT NULL
);

CREATE TABLE IF NOT EXISTS telemetry_ingest_events (
    id BIGSERIAL PRIMARY KEY,
    site_code TEXT NOT NULL,
    edge_code TEXT NOT NULL,
    tag_code TEXT NOT NULL,
    quality_status TEXT NOT NULL,
    value_json JSONB NOT NULL,
    payload_json JSONB NOT NULL,
    ts TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_tag_current_state_updated_at
    ON tag_current_state(updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_edge_current_state_last_seen_at
    ON edge_current_state(last_seen_at DESC);
CREATE INDEX IF NOT EXISTS idx_command_ack_command_id_ts
    ON command_ack_events(command_id, ts DESC);
CREATE INDEX IF NOT EXISTS idx_command_audit_command_id_ts
    ON command_audit_events(command_id, ts DESC);
CREATE INDEX IF NOT EXISTS idx_command_audit_tag_id_ts
    ON command_audit_events(tag_id, ts DESC);
CREATE INDEX IF NOT EXISTS idx_edge_health_edge_ts
    ON edge_health_events(edge_id, ts DESC);
CREATE INDEX IF NOT EXISTS idx_runtime_alert_edge_type_ts
    ON runtime_alert_events(edge_id, alert_type, ts DESC);
CREATE INDEX IF NOT EXISTS idx_telemetry_ingest_site_edge_tag_ts
    ON telemetry_ingest_events(site_code, edge_code, tag_code, ts DESC);

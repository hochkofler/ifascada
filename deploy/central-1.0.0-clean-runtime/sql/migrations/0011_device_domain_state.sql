CREATE TABLE IF NOT EXISTS device_current_state (
    device_id BIGINT PRIMARY KEY REFERENCES devices(id),
    state TEXT NOT NULL,
    severity TEXT NOT NULL DEFAULT 'info',
    reason TEXT NULL,
    connection_id BIGINT NULL REFERENCES connections(id),
    tags_connected INTEGER NOT NULL DEFAULT 0,
    tags_stale INTEGER NOT NULL DEFAULT 0,
    tags_disconnected INTEGER NOT NULL DEFAULT 0,
    last_change_at TIMESTAMPTZ NOT NULL,
    last_seen_at TIMESTAMPTZ NOT NULL,
    payload_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_device_current_state_state_change
    ON device_current_state(state, last_change_at DESC);
CREATE INDEX IF NOT EXISTS idx_device_current_state_seen
    ON device_current_state(last_seen_at DESC);
CREATE INDEX IF NOT EXISTS idx_device_current_state_connection_id
    ON device_current_state(connection_id);

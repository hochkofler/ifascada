-- Formal connection domain model for observability:
-- edge -> connection -> device -> tag

CREATE TABLE IF NOT EXISTS connections (
    id BIGSERIAL PRIMARY KEY,
    edge_id BIGINT NOT NULL REFERENCES edges(id),
    connection_code TEXT NOT NULL,
    name TEXT NOT NULL,
    driver_type TEXT NOT NULL DEFAULT 'Unknown',
    metadata_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (edge_id, connection_code)
);

ALTER TABLE devices
    ADD COLUMN IF NOT EXISTS connection_id BIGINT NULL REFERENCES connections(id);

-- Opportunistic device->connection mapping when metadata carries connection_id.
UPDATE devices d
SET connection_id = c.id
FROM connections c
WHERE d.connection_id IS NULL
  AND (d.metadata_json ? 'connection_id')
  AND c.edge_id = d.edge_id
  AND c.connection_code = (d.metadata_json->>'connection_id');

-- Backfill connection catalog from historical operational events.
INSERT INTO connections (edge_id, connection_code, name, driver_type, metadata_json)
SELECT DISTINCT
    e.id AS edge_id,
    oe.connection_id AS connection_code,
    oe.connection_id AS name,
    'Unknown'::text AS driver_type,
    '{}'::jsonb AS metadata_json
FROM operational_events oe
JOIN sites s ON s.code = oe.site_code
JOIN edges e ON e.site_id = s.id AND e.edge_code = oe.edge_code
WHERE oe.connection_id IS NOT NULL
  AND oe.event_type LIKE 'connection.%'
ON CONFLICT (edge_id, connection_code) DO NOTHING;

CREATE TABLE IF NOT EXISTS connection_current_state (
    connection_id BIGINT PRIMARY KEY REFERENCES connections(id),
    state TEXT NOT NULL,
    severity TEXT NOT NULL DEFAULT 'info',
    reason TEXT NULL,
    payload_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    last_change_at TIMESTAMPTZ NOT NULL,
    last_seen_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_connections_edge_code
    ON connections(edge_id, connection_code);
CREATE INDEX IF NOT EXISTS idx_devices_connection_id
    ON devices(connection_id);
CREATE INDEX IF NOT EXISTS idx_connection_current_state_seen
    ON connection_current_state(last_seen_at DESC);
CREATE INDEX IF NOT EXISTS idx_connection_current_state_state
    ON connection_current_state(state, last_change_at DESC);

-- Initialize current state from latest connection.* operational events (if any).
WITH latest AS (
    SELECT DISTINCT ON (oe.site_code, oe.edge_code, oe.connection_id)
        oe.site_code,
        oe.edge_code,
        oe.connection_id,
        oe.severity,
        oe.message,
        oe.payload_json,
        oe.ts
    FROM operational_events oe
    WHERE oe.connection_id IS NOT NULL
      AND oe.event_type LIKE 'connection.%'
    ORDER BY oe.site_code, oe.edge_code, oe.connection_id, oe.ts DESC
),
resolved AS (
    SELECT
        c.id AS connection_pk,
        COALESCE(NULLIF(LOWER(lat.payload_json->>'state'), ''), 'unknown') AS state,
        COALESCE(NULLIF(lat.severity, ''), 'info') AS severity,
        NULLIF(lat.message, '') AS reason,
        lat.payload_json,
        lat.ts
    FROM latest lat
    JOIN sites s ON s.code = lat.site_code
    JOIN edges e ON e.site_id = s.id AND e.edge_code = lat.edge_code
    JOIN connections c ON c.edge_id = e.id AND c.connection_code = lat.connection_id
)
INSERT INTO connection_current_state (
    connection_id, state, severity, reason, payload_json, last_change_at, last_seen_at, updated_at
)
SELECT
    r.connection_pk,
    r.state,
    r.severity,
    r.reason,
    r.payload_json,
    r.ts,
    r.ts,
    NOW()
FROM resolved r
ON CONFLICT (connection_id) DO NOTHING;

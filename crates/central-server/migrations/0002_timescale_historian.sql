CREATE EXTENSION IF NOT EXISTS timescaledb;

CREATE TABLE IF NOT EXISTS telemetry_samples (
    ts TIMESTAMPTZ NOT NULL,
    site_id BIGINT NOT NULL REFERENCES sites(id),
    edge_id BIGINT NOT NULL REFERENCES edges(id),
    tag_id BIGINT NOT NULL REFERENCES tags(id),
    quality_status TEXT NOT NULL,
    value_num DOUBLE PRECISION NULL,
    value_bool BOOLEAN NULL,
    value_text TEXT NULL,
    value_json JSONB NULL,
    source TEXT NOT NULL,
    PRIMARY KEY (tag_id, ts)
);

SELECT create_hypertable('telemetry_samples', 'ts', if_not_exists => TRUE);

CREATE INDEX IF NOT EXISTS idx_telemetry_tag_ts
    ON telemetry_samples(tag_id, ts DESC);
CREATE INDEX IF NOT EXISTS idx_telemetry_edge_ts
    ON telemetry_samples(edge_id, ts DESC);
CREATE INDEX IF NOT EXISTS idx_telemetry_site_ts
    ON telemetry_samples(site_id, ts DESC);

ALTER TABLE telemetry_samples SET (
    timescaledb.compress,
    timescaledb.compress_segmentby = 'tag_id'
);

SELECT add_compression_policy('telemetry_samples', INTERVAL '7 days', if_not_exists => TRUE);
SELECT add_retention_policy('telemetry_samples', INTERVAL '365 days', if_not_exists => TRUE);

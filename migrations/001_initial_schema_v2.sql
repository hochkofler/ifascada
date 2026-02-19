-- Initial Schema V2 (Device-Centric)
-- This file is used by sqlx::migrate! in central-server/src/main.rs

-- 1. Common Functions
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = CURRENT_TIMESTAMP;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- 2. Edge Agents Table
CREATE TABLE IF NOT EXISTS edge_agents (
    id VARCHAR(100) PRIMARY KEY,
    description TEXT,
    status VARCHAR(20) DEFAULT 'unknown',
    last_heartbeat TIMESTAMPTZ,
    metadata JSONB,
    created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
);

-- 3. Devices Table (Physical Connection)
CREATE TABLE IF NOT EXISTS devices (
    id VARCHAR(100) PRIMARY KEY,
    edge_agent_id VARCHAR(100) NOT NULL,
    name VARCHAR(255) NOT NULL,
    driver_type VARCHAR(50) NOT NULL,       -- e.g., "Modbus", "RS232"
    connection_config JSONB NOT NULL,       -- COM port, IP, Baud Rate, etc.
    enabled BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT fk_device_agent
        FOREIGN KEY (edge_agent_id)
        REFERENCES edge_agents(id)
        ON DELETE CASCADE
);

-- 4. Tags Table (Logical Meaning)
CREATE TABLE IF NOT EXISTS tags (
    id VARCHAR(100) PRIMARY KEY,
    device_id VARCHAR(100) NOT NULL,
    source_config JSONB NOT NULL,
    update_mode VARCHAR(50) NOT NULL,
    update_config JSONB NOT NULL,
    value_type VARCHAR(20) NOT NULL,
    value_schema JSONB,
    pipeline_config JSONB,
    enabled BOOLEAN NOT NULL DEFAULT true,
    description TEXT,
    metadata JSONB,
    last_value JSONB,
    last_update TIMESTAMPTZ,
    status VARCHAR(20) NOT NULL DEFAULT 'unknown',
    quality VARCHAR(20) NOT NULL DEFAULT 'uncertain',
    error_message TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT fk_tag_device
        FOREIGN KEY (device_id)
        REFERENCES devices(id)
        ON DELETE CASCADE
);

-- 5. Tag Events (renamed from tag_history)
CREATE TABLE IF NOT EXISTS tag_events (
    id BIGSERIAL PRIMARY KEY,
    tag_id VARCHAR(100),                    -- nullable: allows orphan events
    value JSONB NOT NULL,
    quality VARCHAR(20) NOT NULL,
    timestamp TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT fk_tag_event_tag
        FOREIGN KEY (tag_id)
        REFERENCES tags(id)
        ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_tag_events_tag_time ON tag_events(tag_id, timestamp DESC);

-- 6. Reports
CREATE TABLE IF NOT EXISTS reports (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    report_id VARCHAR(100),                 -- external/legacy ID from edge agent
    agent_id VARCHAR(100) NOT NULL,
    start_time TIMESTAMPTZ NOT NULL,
    end_time TIMESTAMPTZ NOT NULL,
    total_value JSONB,
    created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS report_items (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    report_id UUID NOT NULL,
    tag_id VARCHAR(100),
    value JSONB NOT NULL,
    timestamp TIMESTAMPTZ NOT NULL,
    CONSTRAINT fk_report_item_report
        FOREIGN KEY (report_id)
        REFERENCES reports(id)
        ON DELETE CASCADE
);

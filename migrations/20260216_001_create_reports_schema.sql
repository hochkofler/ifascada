-- Create Reports table for summary data
CREATE TABLE IF NOT EXISTS reports (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    report_id TEXT NOT NULL, -- Logical ID from the agent/session
    agent_id TEXT NOT NULL,
    start_time TIMESTAMPTZ NOT NULL,
    end_time TIMESTAMPTZ NOT NULL,
    total_value FLOAT8,
    checksum TEXT, -- SHA-256 or similar for integrity
    created_at TIMESTAMPTZ DEFAULT NOW(),
    
    CONSTRAINT unique_agent_report UNIQUE(agent_id, report_id)
);

-- Create Report Items table for detailed readings
CREATE TABLE IF NOT EXISTS report_items (
    id BIGSERIAL PRIMARY KEY,
    report_id UUID NOT NULL REFERENCES reports(id) ON DELETE CASCADE,
    value FLOAT8 NOT NULL,
    unit TEXT NOT NULL,
    timestamp TIMESTAMPTZ NOT NULL,
    item_order INTEGER NOT NULL -- To preserve the sequence of readings
);

CREATE INDEX idx_reports_agent ON reports(agent_id);
CREATE INDEX idx_report_items_report ON report_items(report_id);

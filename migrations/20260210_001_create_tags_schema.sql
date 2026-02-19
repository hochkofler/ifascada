-- Migration: Create tags schema for PostgreSQL 18
-- Compatible with PostgreSQL 18.x

-- Create edge_agents table
CREATE TABLE IF NOT EXISTS edge_agents (
    id VARCHAR(100) PRIMARY KEY,
    description TEXT,
    status VARCHAR(20) DEFAULT 'unknown',
    last_heartbeat TIMESTAMPTZ,
    metadata JSONB,
    created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
);

-- Create tags table
CREATE TABLE IF NOT EXISTS tags (
    id VARCHAR(100) PRIMARY KEY,
    driver_type VARCHAR(50) NOT NULL,
    driver_config JSONB NOT NULL,
    edge_agent_id VARCHAR(100) NOT NULL,
    
    -- Update configuration
    update_mode VARCHAR(50) NOT NULL,
    update_config JSONB NOT NULL,
    
    -- Value configuration
    value_type VARCHAR(20) NOT NULL,
    value_schema JSONB,
    
    enabled BOOLEAN NOT NULL DEFAULT true,
    
    -- Metadata
    description TEXT,
    metadata JSONB,
    
    -- Runtime state
    last_value JSONB,
    last_update TIMESTAMPTZ,
    status VARCHAR(20) NOT NULL DEFAULT 'unknown',
    quality VARCHAR(20) NOT NULL DEFAULT 'uncertain',
    error_message TEXT,
    
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    
    CONSTRAINT fk_edge_agent 
        FOREIGN KEY (edge_agent_id) 
        REFERENCES edge_agents(id) 
        ON DELETE CASCADE
);

-- Create indexes for tags
CREATE INDEX IF NOT EXISTS idx_tags_edge_agent ON tags(edge_agent_id);
CREATE INDEX IF NOT EXISTS idx_tags_status ON tags(status);
CREATE INDEX IF NOT EXISTS idx_tags_enabled ON tags(enabled) WHERE enabled = true;
CREATE INDEX IF NOT EXISTS idx_tags_updated ON tags(updated_at DESC);

-- Create tag_history table for historical data
CREATE TABLE IF NOT EXISTS tag_history (
    id BIGSERIAL PRIMARY KEY,
    tag_id VARCHAR(100) NOT NULL,
    value JSONB NOT NULL,
    quality VARCHAR(20) NOT NULL,
    timestamp TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    
    CONSTRAINT fk_tag 
        FOREIGN KEY (tag_id) 
        REFERENCES tags(id) 
        ON DELETE CASCADE
);

-- Create indexes for tag_history
CREATE INDEX IF NOT EXISTS idx_tag_history_tag_time ON tag_history(tag_id, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_tag_history_timestamp ON tag_history(timestamp DESC);

-- Create function to auto-update updated_at
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = CURRENT_TIMESTAMP;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Create triggers for auto-updating updated_at
DROP TRIGGER IF EXISTS update_edge_agents_updated_at ON edge_agents;
CREATE TRIGGER update_edge_agents_updated_at 
    BEFORE UPDATE ON edge_agents
    FOR EACH ROW 
    EXECUTE FUNCTION update_updated_at_column();

DROP TRIGGER IF EXISTS update_tags_updated_at ON tags;
CREATE TRIGGER update_tags_updated_at 
    BEFORE UPDATE ON tags
    FOR EACH ROW 
    EXECUTE FUNCTION update_updated_at_column();

-- Add table comments
COMMENT ON TABLE tags IS 'Central registry of all SCADA tags';
COMMENT ON TABLE tag_history IS 'Historical values for all tags';
COMMENT ON TABLE edge_agents IS 'Edge agents that execute tags';

-- Add column comments
COMMENT ON COLUMN tags.update_mode IS 'OnChange, Polling, or PollingOnChange';
COMMENT ON COLUMN tags.value_type IS 'Simple or Composite';
COMMENT ON COLUMN tags.quality IS 'good, bad, uncertain, or timeout';
COMMENT ON COLUMN tags.status IS 'online, offline, error, or unknown';

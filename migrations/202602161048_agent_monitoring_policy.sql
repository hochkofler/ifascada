-- Migration: Add monitoring policy to edge_agents and create status history table

-- 1. Add policy columns to edge_agents
ALTER TABLE edge_agents ADD COLUMN IF NOT EXISTS heartbeat_interval_secs INTEGER DEFAULT 30;
ALTER TABLE edge_agents ADD COLUMN IF NOT EXISTS missed_heartbeat_threshold INTEGER DEFAULT 2;

-- 2. Create agent_status_history table for auditing transitions
CREATE TABLE IF NOT EXISTS agent_status_history (
    id BIGSERIAL PRIMARY KEY,
    agent_id VARCHAR(100) NOT NULL,
    old_status VARCHAR(20),
    new_status VARCHAR(20) NOT NULL,
    reason TEXT,
    timestamp TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT fk_history_agent 
        FOREIGN KEY (agent_id) 
        REFERENCES edge_agents(id) 
        ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_agent_status_history_agent_time ON agent_status_history(agent_id, timestamp DESC);

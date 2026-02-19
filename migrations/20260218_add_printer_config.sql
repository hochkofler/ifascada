-- Migration: Add printer_config to edge_agents
ALTER TABLE edge_agents ADD COLUMN IF NOT EXISTS printer_config JSONB;

COMMENT ON COLUMN edge_agents.printer_config IS 'Configuration for the printer attached to or reachable by this agent';

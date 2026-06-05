-- V4: Add layer column to kg_nodes and cognitions for memory tier management.
-- Creates memory_layers reference table with retrieval priorities.
-- Layer priority: working(40) > episodic(30) > semantic(20) > global(10)
-- Idempotency: memory_db.rs checks pragma_table_info before executing the ALTER TABLE.

ALTER TABLE kg_nodes ADD COLUMN layer TEXT NOT NULL DEFAULT 'semantic';
CREATE INDEX IF NOT EXISTS idx_kg_nodes_layer ON kg_nodes(layer);

ALTER TABLE cognitions ADD COLUMN layer TEXT NOT NULL DEFAULT 'semantic';
CREATE INDEX IF NOT EXISTS idx_cognitions_layer ON cognitions(layer);

CREATE TABLE IF NOT EXISTS memory_layers (
    name                TEXT NOT NULL PRIMARY KEY,
    retrieval_priority  INTEGER NOT NULL DEFAULT 0,
    description         TEXT NOT NULL DEFAULT ''
);

INSERT OR IGNORE INTO memory_layers (name, retrieval_priority, description) VALUES
    ('working', 40, 'Working memory — temporary, current-session entities'),
    ('episodic', 30, 'Episodic memory — event-specific high-confidence entities'),
    ('semantic', 20, 'Semantic memory — general knowledge entities'),
    ('global', 10, 'Global memory — shared cross-session entities');

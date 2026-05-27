-- V10: Memory system optimizations
-- - Session summaries for conversation compression
-- - KG node access tracking for importance scoring

-- Session summaries (for P0 conversation compression)
ALTER TABLE sessions ADD COLUMN summary TEXT DEFAULT '';
ALTER TABLE sessions ADD COLUMN summary_at_count INTEGER DEFAULT 0;

-- KG node access tracking (for importance scoring / temporal decay)
ALTER TABLE kg_nodes ADD COLUMN access_count INTEGER DEFAULT 0;
ALTER TABLE kg_nodes ADD COLUMN last_accessed INTEGER;

-- Index for top_interests access-weighted sorting
CREATE INDEX IF NOT EXISTS idx_kg_nodes_access ON kg_nodes(access_count DESC);
CREATE INDEX IF NOT EXISTS idx_kg_nodes_last_access ON kg_nodes(last_accessed);

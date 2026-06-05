-- Memory quality logging: tracks which KG entities were injected into the system prompt
-- and which ones the assistant actually referenced in its response.
CREATE TABLE IF NOT EXISTS memory_quality_log (
    id                    INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id            TEXT NOT NULL,
    turn_seq              INTEGER NOT NULL,
    injected_entities     TEXT NOT NULL,    -- JSON array of entity names injected into prompt
    referenced_entities   TEXT,             -- JSON array, nullable (filled after assistant response)
    injection_duration_ms INTEGER NOT NULL DEFAULT 0,
    created_at            TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_memory_quality_session ON memory_quality_log(session_id);

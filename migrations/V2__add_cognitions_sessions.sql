CREATE TABLE IF NOT EXISTS cognitions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    subject TEXT NOT NULL,
    trait TEXT NOT NULL,
    value TEXT NOT NULL,
    confidence REAL,
    evidence_count INTEGER,
    first_seen INTEGER,
    last_updated INTEGER,
    version INTEGER DEFAULT 1
);

CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY,
    created_at TEXT NOT NULL,
    message_count INTEGER DEFAULT 0
);

CREATE TABLE IF NOT EXISTS token_usage (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp TEXT NOT NULL,
    session_id TEXT,
    model TEXT NOT NULL,
    prompt_tokens INTEGER,
    completion_tokens INTEGER,
    cached_tokens INTEGER DEFAULT 0,
    latency_ms INTEGER
);

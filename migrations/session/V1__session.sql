CREATE TABLE IF NOT EXISTS sessions (
    id                TEXT PRIMARY KEY,
    created_at        TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at        TEXT NOT NULL DEFAULT (datetime('now')),
    message_count     INTEGER NOT NULL DEFAULT 0,
    title             TEXT,
    pinned_at         TEXT,
    agent_config_name TEXT,
    summary           TEXT NOT NULL DEFAULT '',
    summary_at_count  INTEGER NOT NULL DEFAULT 0,
    workspace_path    TEXT
);

CREATE TABLE IF NOT EXISTS message_history (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL,
    seq        INTEGER NOT NULL,
    role       TEXT NOT NULL,
    content    TEXT NOT NULL,
    timestamp  TEXT NOT NULL,
    metadata   TEXT
);

CREATE TABLE IF NOT EXISTS token_usage (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id         TEXT NOT NULL,
    model              TEXT NOT NULL,
    prompt_tokens      INTEGER NOT NULL DEFAULT 0,
    completion_tokens  INTEGER NOT NULL DEFAULT 0,
    cached_tokens      INTEGER NOT NULL DEFAULT 0,
    cached_read_tokens INTEGER NOT NULL DEFAULT 0,
    cached_write_tokens INTEGER NOT NULL DEFAULT 0,
    latency_ms         INTEGER NOT NULL DEFAULT 0,
    context_window     INTEGER NOT NULL DEFAULT 0,
    created_at         TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS bridge_sessions (
    id               TEXT PRIMARY KEY,
    platform         TEXT NOT NULL,
    external_chat_id TEXT NOT NULL,
    external_user_id TEXT,
    user_name        TEXT,
    user_avatar_url  TEXT,
    access_state     TEXT NOT NULL DEFAULT 'active',
    pairing_code     TEXT,
    created_at       TEXT NOT NULL,
    last_message_at  TEXT,
    message_count    INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS bridge_messages (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id          TEXT NOT NULL REFERENCES bridge_sessions(id) ON DELETE CASCADE,
    direction           TEXT NOT NULL,
    content             TEXT,
    media_type          TEXT NOT NULL DEFAULT 'text',
    media_url           TEXT,
    external_message_id TEXT,
    timestamp           TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS bridge_known_users (
    platform   TEXT NOT NULL,
    user_id    TEXT NOT NULL,
    user_name  TEXT,
    avatar_url TEXT,
    first_seen TEXT NOT NULL,
    last_seen  TEXT,
    PRIMARY KEY (platform, user_id)
);

CREATE INDEX IF NOT EXISTS idx_bridge_sessions_platform ON bridge_sessions(platform);
CREATE INDEX IF NOT EXISTS idx_bridge_messages_session_ts ON bridge_messages(session_id, timestamp);

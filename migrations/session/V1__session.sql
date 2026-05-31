CREATE TABLE IF NOT EXISTS sessions (
    id                TEXT PRIMARY KEY,
    created_at        TEXT NOT NULL DEFAULT (datetime('now')),
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
    id          TEXT PRIMARY KEY,
    bridge_name TEXT NOT NULL,
    channel_id  TEXT NOT NULL,
    peer_id     TEXT NOT NULL,
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE TABLE IF NOT EXISTS bridge_messages (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    bridge_name  TEXT NOT NULL,
    channel_id   TEXT NOT NULL,
    peer_id      TEXT NOT NULL,
    message_id   TEXT NOT NULL,
    content      TEXT NOT NULL,
    role         TEXT NOT NULL,
    timestamp    TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE TABLE IF NOT EXISTS bridge_known_users (
    bridge_name TEXT NOT NULL,
    peer_id     TEXT NOT NULL,
    display_name TEXT,
    PRIMARY KEY (bridge_name, peer_id)
);

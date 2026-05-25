-- Bridge: external messaging platform sessions and messages

CREATE TABLE IF NOT EXISTS bridge_sessions (
    id TEXT PRIMARY KEY,
    platform TEXT NOT NULL,
    external_chat_id TEXT NOT NULL,
    external_user_id TEXT,
    user_name TEXT,
    user_avatar_url TEXT,
    access_state TEXT NOT NULL DEFAULT 'active',
    pairing_code TEXT,
    created_at TEXT NOT NULL,
    last_message_at TEXT,
    message_count INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS bridge_messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES bridge_sessions(id) ON DELETE CASCADE,
    direction TEXT NOT NULL,
    content TEXT,
    media_type TEXT NOT NULL DEFAULT 'text',
    media_url TEXT,
    external_message_id TEXT,
    timestamp TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS bridge_known_users (
    platform TEXT NOT NULL,
    user_id TEXT NOT NULL,
    user_name TEXT,
    avatar_url TEXT,
    first_seen TEXT NOT NULL,
    last_seen TEXT,
    PRIMARY KEY (platform, user_id)
);

CREATE INDEX IF NOT EXISTS idx_bridge_sessions_platform
    ON bridge_sessions(platform);
CREATE INDEX IF NOT EXISTS idx_bridge_messages_session_ts
    ON bridge_messages(session_id, timestamp);

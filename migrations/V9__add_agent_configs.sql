-- V9: Agent configuration support.
-- agent_configs stores named agent profiles (system prompt, model, tools, etc.).
-- sessions.agent_config_name links a session to its configured agent profile.

CREATE TABLE IF NOT EXISTS agent_configs (
    name                      TEXT PRIMARY KEY,
    avatar                    TEXT,
    persona                   TEXT NOT NULL DEFAULT '',
    system_prompt_override    TEXT,
    model                     TEXT,
    thinking_level            TEXT,
    temperature               REAL,
    tool_scope                TEXT,
    allowed_tools             TEXT,
    disallowed_tools          TEXT,
    max_iterations            INTEGER,
    timeout_secs              INTEGER,
    max_concurrent_subagents  INTEGER NOT NULL DEFAULT 5,
    is_primary                INTEGER NOT NULL DEFAULT 0,
    memory_enabled            INTEGER NOT NULL DEFAULT 0,
    created_at                TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at                TEXT NOT NULL DEFAULT (datetime('now'))
);

ALTER TABLE sessions ADD COLUMN agent_config_name TEXT;

-- Seed the default agent config so there is always a fallback.
INSERT OR IGNORE INTO agent_configs (name, max_concurrent_subagents, is_primary, memory_enabled)
VALUES ('default', 5, 1, 1);

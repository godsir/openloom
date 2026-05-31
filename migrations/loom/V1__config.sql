CREATE TABLE IF NOT EXISTS model_configs (
    name              TEXT PRIMARY KEY,
    model             TEXT,
    model_type        TEXT NOT NULL DEFAULT 'Router',
    backend           TEXT NOT NULL DEFAULT 'LmStudio',
    base_url          TEXT,
    api_key_env       TEXT,
    context_size      INTEGER NOT NULL DEFAULT 4096,
    max_output_tokens INTEGER,
    is_active         INTEGER NOT NULL DEFAULT 0,
    backend_label     TEXT,
    capabilities      TEXT NOT NULL DEFAULT '{}',
    api_format        TEXT,
    input_price       REAL NOT NULL DEFAULT 0.0,
    output_price      REAL NOT NULL DEFAULT 0.0,
    cache_read_price  REAL NOT NULL DEFAULT 0.0,
    cache_write_price REAL NOT NULL DEFAULT 0.0,
    created_at        TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at        TEXT NOT NULL DEFAULT (datetime('now'))
);

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

INSERT OR IGNORE INTO agent_configs (name, max_concurrent_subagents, is_primary, memory_enabled)
VALUES ('default', 5, 1, 1);

CREATE TABLE IF NOT EXISTS mcp_servers (
    name                 TEXT PRIMARY KEY,
    transport            TEXT NOT NULL DEFAULT 'stdio',
    command              TEXT NOT NULL DEFAULT '',
    args_json            TEXT NOT NULL DEFAULT '[]',
    url                  TEXT,
    headers_json         TEXT NOT NULL DEFAULT '{}',
    env_json             TEXT NOT NULL DEFAULT '{}',
    cwd                  TEXT,
    startup_timeout_secs INTEGER NOT NULL DEFAULT 30,
    tool_timeout_secs    INTEGER NOT NULL DEFAULT 60,
    enabled_tools_json   TEXT,
    disabled_tools_json  TEXT,
    autostart            INTEGER NOT NULL DEFAULT 1,
    created_at           TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at           TEXT NOT NULL DEFAULT (datetime('now'))
);

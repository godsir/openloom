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
    input_price       REAL,
    output_price      REAL,
    cache_read_price  REAL,
    cache_write_price REAL,
    created_at        TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at        TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS agent_configs (
    name              TEXT PRIMARY KEY,
    persona           TEXT NOT NULL DEFAULT '',
    model             TEXT,
    temperature       REAL,
    max_iterations    INTEGER,
    system_prompt_override TEXT NOT NULL DEFAULT '',
    avatar            TEXT,
    memory_enabled    INTEGER NOT NULL DEFAULT 1,
    created_at        TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at        TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS mcp_servers (
    name                TEXT PRIMARY KEY,
    transport           TEXT NOT NULL DEFAULT 'stdio',
    command             TEXT,
    args_json           TEXT,
    url                 TEXT,
    headers_json        TEXT,
    env_json            TEXT,
    cwd                 TEXT,
    startup_timeout_secs INTEGER NOT NULL DEFAULT 30,
    tool_timeout_secs   INTEGER NOT NULL DEFAULT 60,
    enabled_tools_json  TEXT,
    disabled_tools_json TEXT,
    autostart           INTEGER NOT NULL DEFAULT 1
);

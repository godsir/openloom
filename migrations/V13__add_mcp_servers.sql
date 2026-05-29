-- V13: Persisted MCP server configurations.
-- The orchestrator only kept live connections in memory; restarting the
-- backend or disconnecting a server lost the config and forced the user to
-- re-enter every field. mcp_servers stores the full McpServerConfig so the
-- frontend can list, edit, delete, and auto-reconnect saved entries.

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

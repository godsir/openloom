-- V11: Model configuration support.
-- model_configs stores named model profiles (backend, API key env, context size, etc.).
-- is_active tracks which model is currently selected.

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
    created_at        TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at        TEXT NOT NULL DEFAULT (datetime('now'))
);

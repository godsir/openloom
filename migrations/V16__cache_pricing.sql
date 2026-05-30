ALTER TABLE model_configs ADD COLUMN cache_read_price REAL NOT NULL DEFAULT 0.0;
ALTER TABLE model_configs ADD COLUMN cache_write_price REAL NOT NULL DEFAULT 0.0;

ALTER TABLE token_usage ADD COLUMN cached_read_tokens INTEGER NOT NULL DEFAULT 0;
ALTER TABLE token_usage ADD COLUMN cached_write_tokens INTEGER NOT NULL DEFAULT 0;

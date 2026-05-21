ALTER TABLE cognitions ADD COLUMN scope TEXT NOT NULL DEFAULT 'global';
CREATE INDEX IF NOT EXISTS idx_cognitions_scope ON cognitions(scope);

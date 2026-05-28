-- Add model config extended fields: backend_label, capabilities, api_format
ALTER TABLE model_configs ADD COLUMN backend_label TEXT;
ALTER TABLE model_configs ADD COLUMN capabilities TEXT DEFAULT '{}';
ALTER TABLE model_configs ADD COLUMN api_format TEXT DEFAULT 'openai';

-- Add workspace_path column to sessions table
-- This stores the per-session workspace directory path
ALTER TABLE sessions ADD COLUMN workspace_path TEXT NULL;

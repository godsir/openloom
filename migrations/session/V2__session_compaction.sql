-- Track which model produced the conversation summary and when, for debugging
-- and frontend display. Schema-only for now; save_summary does not yet populate
-- these (a future enhancement can thread the model name through the call site).
ALTER TABLE sessions ADD COLUMN summary_model TEXT;
ALTER TABLE sessions ADD COLUMN summary_updated_at TEXT;

-- V2: Add vector embedding column to kg_nodes for semantic similarity search.
-- Idempotency: memory_db.rs checks pragma_table_info before executing this ALTER TABLE.
ALTER TABLE kg_nodes ADD COLUMN embedding BLOB;

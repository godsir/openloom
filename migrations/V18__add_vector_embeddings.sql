-- V18: Add vector embedding support for knowledge graph nodes.
-- Stores float32 embedding vectors as BLOBs for cosine-similarity search.
-- Column is nullable — existing nodes work fine without embeddings (backward compatible).

ALTER TABLE kg_nodes ADD COLUMN embedding BLOB;

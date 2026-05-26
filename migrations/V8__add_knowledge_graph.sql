-- V8: Add knowledge graph tables alongside existing cognitions.
-- Entity nodes, directed edges, aliases, and evidence linking.
-- Coexists with the flat cognitions table; no data migration.

CREATE TABLE IF NOT EXISTS kg_nodes (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    name            TEXT NOT NULL,
    entity_type     TEXT NOT NULL DEFAULT 'concept',
    description     TEXT NOT NULL DEFAULT '',
    confidence      REAL NOT NULL DEFAULT 0.5,
    evidence_count  INTEGER NOT NULL DEFAULT 0,
    first_seen      INTEGER NOT NULL,
    last_updated    INTEGER NOT NULL,
    source          TEXT NOT NULL DEFAULT 'observed',
    scope           TEXT NOT NULL DEFAULT 'global',
    metadata        TEXT
);

CREATE INDEX IF NOT EXISTS idx_kg_nodes_type ON kg_nodes(entity_type);
CREATE INDEX IF NOT EXISTS idx_kg_nodes_scope ON kg_nodes(scope);
CREATE INDEX IF NOT EXISTS idx_kg_nodes_name ON kg_nodes(name);

CREATE TABLE IF NOT EXISTS kg_edges (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    source_id       INTEGER NOT NULL REFERENCES kg_nodes(id) ON DELETE CASCADE,
    target_id       INTEGER NOT NULL REFERENCES kg_nodes(id) ON DELETE CASCADE,
    relation_type   TEXT NOT NULL,
    fact            TEXT NOT NULL DEFAULT '',
    confidence      REAL NOT NULL DEFAULT 0.5,
    evidence_count  INTEGER NOT NULL DEFAULT 1,
    first_seen      INTEGER NOT NULL,
    last_updated    INTEGER NOT NULL,
    source          TEXT NOT NULL DEFAULT 'observed',
    scope           TEXT NOT NULL DEFAULT 'global',
    metadata        TEXT
);

CREATE INDEX IF NOT EXISTS idx_kg_edges_source ON kg_edges(source_id);
CREATE INDEX IF NOT EXISTS idx_kg_edges_target ON kg_edges(target_id);
CREATE INDEX IF NOT EXISTS idx_kg_edges_relation ON kg_edges(relation_type);

CREATE TABLE IF NOT EXISTS kg_aliases (
    id      INTEGER PRIMARY KEY AUTOINCREMENT,
    node_id INTEGER NOT NULL REFERENCES kg_nodes(id) ON DELETE CASCADE,
    alias   TEXT NOT NULL,
    UNIQUE(node_id, alias)
);

CREATE INDEX IF NOT EXISTS idx_kg_aliases_alias ON kg_aliases(alias);

CREATE TABLE IF NOT EXISTS kg_evidence (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    node_id       INTEGER REFERENCES kg_nodes(id) ON DELETE CASCADE,
    edge_id       INTEGER REFERENCES kg_edges(id) ON DELETE CASCADE,
    cognition_id  INTEGER REFERENCES cognitions(id) ON DELETE SET NULL,
    event_id      INTEGER REFERENCES events(id) ON DELETE SET NULL,
    CHECK (node_id IS NOT NULL OR edge_id IS NOT NULL)
);

CREATE INDEX IF NOT EXISTS idx_kg_evidence_node ON kg_evidence(node_id);
CREATE INDEX IF NOT EXISTS idx_kg_evidence_edge ON kg_evidence(edge_id);

-- FTS5 on node names and descriptions for entity search
CREATE VIRTUAL TABLE IF NOT EXISTS kg_nodes_fts
USING fts5(name, description, content='kg_nodes', content_rowid='id');

CREATE TRIGGER IF NOT EXISTS kg_nodes_ai AFTER INSERT ON kg_nodes BEGIN
    INSERT INTO kg_nodes_fts(rowid, name, description)
    VALUES (new.id, new.name, new.description);
END;

CREATE TRIGGER IF NOT EXISTS kg_nodes_ad AFTER DELETE ON kg_nodes BEGIN
    INSERT INTO kg_nodes_fts(kg_nodes_fts, rowid, name, description)
    VALUES('delete', old.id, old.name, old.description);
END;

CREATE TRIGGER IF NOT EXISTS kg_nodes_au AFTER UPDATE ON kg_nodes BEGIN
    INSERT INTO kg_nodes_fts(kg_nodes_fts, rowid, name, description)
    VALUES('delete', old.id, old.name, old.description);
    INSERT INTO kg_nodes_fts(rowid, name, description)
    VALUES (new.id, new.name, new.description);
END;

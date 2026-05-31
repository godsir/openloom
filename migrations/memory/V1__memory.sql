-- events
CREATE TABLE IF NOT EXISTS events (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp      TEXT NOT NULL,
    event_type     TEXT NOT NULL,
    action         TEXT NOT NULL,
    context        TEXT NOT NULL,
    confidence     REAL NOT NULL DEFAULT 1.0,
    source_session TEXT,
    source_text    TEXT NOT NULL DEFAULT '',
    payload        TEXT
);
CREATE VIRTUAL TABLE IF NOT EXISTS events_fts USING fts5(event_type, action, context);

-- cognitions
CREATE TABLE IF NOT EXISTS cognitions (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    subject         TEXT NOT NULL,
    trait           TEXT NOT NULL,
    value           TEXT NOT NULL,
    confidence      REAL NOT NULL DEFAULT 0.5,
    evidence_count  INTEGER NOT NULL DEFAULT 1,
    first_seen      INTEGER NOT NULL DEFAULT (strftime('%s','now')),
    last_updated    INTEGER NOT NULL DEFAULT (strftime('%s','now')),
    version         INTEGER NOT NULL DEFAULT 1,
    scope           TEXT NOT NULL DEFAULT 'global'
);
CREATE TABLE IF NOT EXISTS cognition_snapshots (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    cognition_id    INTEGER NOT NULL,
    version         INTEGER NOT NULL,
    trait           TEXT NOT NULL,
    value           TEXT NOT NULL,
    confidence      REAL,
    evidence_count  INTEGER,
    snapshot_at     INTEGER NOT NULL,
    FOREIGN KEY (cognition_id) REFERENCES cognitions(id)
);

-- knowledge graph
CREATE TABLE IF NOT EXISTS kg_nodes (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    name           TEXT NOT NULL,
    entity_type    TEXT NOT NULL DEFAULT 'Concept',
    description    TEXT NOT NULL DEFAULT '',
    confidence     REAL NOT NULL DEFAULT 0.5,
    evidence_count INTEGER NOT NULL DEFAULT 1,
    first_seen     INTEGER NOT NULL DEFAULT (strftime('%s','now')),
    last_updated   INTEGER NOT NULL DEFAULT (strftime('%s','now')),
    scope          TEXT NOT NULL DEFAULT 'global',
    access_count   INTEGER NOT NULL DEFAULT 0,
    last_accessed  INTEGER NOT NULL DEFAULT (strftime('%s','now'))
);
CREATE TABLE IF NOT EXISTS kg_edges (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    source_id       INTEGER NOT NULL,
    target_id       INTEGER NOT NULL,
    relation_type   TEXT NOT NULL DEFAULT 'related_to',
    fact            TEXT NOT NULL DEFAULT '',
    confidence      REAL NOT NULL DEFAULT 0.5,
    evidence_count  INTEGER NOT NULL DEFAULT 1,
    first_seen      INTEGER NOT NULL DEFAULT (strftime('%s','now')),
    last_updated    INTEGER NOT NULL DEFAULT (strftime('%s','now')),
    scope           TEXT NOT NULL DEFAULT 'global',
    FOREIGN KEY (source_id) REFERENCES kg_nodes(id),
    FOREIGN KEY (target_id) REFERENCES kg_nodes(id)
);
CREATE TABLE IF NOT EXISTS kg_aliases (
    node_id INTEGER NOT NULL,
    alias   TEXT NOT NULL,
    PRIMARY KEY (node_id, alias),
    FOREIGN KEY (node_id) REFERENCES kg_nodes(id) ON DELETE CASCADE
);
CREATE TABLE IF NOT EXISTS kg_evidence (
    node_id     INTEGER,
    edge_id     INTEGER,
    cognition_id INTEGER,
    event_id    INTEGER,
    FOREIGN KEY (node_id) REFERENCES kg_nodes(id),
    FOREIGN KEY (edge_id) REFERENCES kg_edges(id)
);
CREATE VIRTUAL TABLE IF NOT EXISTS kg_nodes_fts USING fts5(name, description);

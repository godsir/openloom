CREATE TABLE IF NOT EXISTS events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp TEXT NOT NULL,
    type TEXT NOT NULL,
    action TEXT NOT NULL,
    context TEXT NOT NULL DEFAULT '',
    confidence REAL NOT NULL,
    source_session TEXT,
    source_text TEXT NOT NULL DEFAULT '',
    payload TEXT
);

CREATE VIRTUAL TABLE IF NOT EXISTS events_fts
USING fts5(type, action, context, source_text, content='events', content_rowid='id');

CREATE TRIGGER IF NOT EXISTS events_ai AFTER INSERT ON events BEGIN
    INSERT INTO events_fts(rowid, type, action, context, source_text)
    VALUES (new.id, new.type, new.action, new.context, new.source_text);
END;

CREATE TRIGGER IF NOT EXISTS events_ad AFTER DELETE ON events BEGIN
    INSERT INTO events_fts(events_fts, rowid, type, action, context, source_text)
    VALUES('delete', old.id, old.type, old.action, old.context, old.source_text);
END;

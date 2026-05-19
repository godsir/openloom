use crate::event::Event;
use anyhow::Result;
use refinery::embed_migrations;
use rusqlite::{Connection, params};
use std::path::Path;

embed_migrations!("../../migrations");

/// SQLite-backed event store with FTS5 full-text search.
pub struct SqliteEventStore {
    conn: Connection,
}

impl SqliteEventStore {
    /// Open (or create) the database at the given path.
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        let mut store = Self { conn };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&mut self) -> Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS events (
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
            END;",
        )?;
        Ok(())
    }

    /// Run refinery migrations (Phase 1)
    /// Phase 0 compatibility: V1 uses CREATE IF NOT EXISTS, safe no-op on existing DBs
    pub fn run_migrations(conn: &mut Connection) -> Result<()> {
        migrations::runner().run(conn)?;
        Ok(())
    }

    /// Phase 1 recommended: open database with refinery migrations
    pub fn open_with_migrations(path: &std::path::Path) -> Result<Self> {
        let mut conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        Self::run_migrations(&mut conn)?;
        Ok(Self { conn })
    }

    /// Insert an event and return its assigned ID.
    pub fn insert(&mut self, event: &Event) -> Result<i64> {
        let payload = event.payload.as_ref().map(|p| p.to_string());
        self.conn.execute(
            "INSERT INTO events (timestamp, type, action, context, confidence, source_session, source_text, payload)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                event.timestamp.to_rfc3339(),
                event.event_type_as_str(),
                event.action,
                event.context,
                event.confidence,
                event.source_session,
                event.source_text,
                payload,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Query the most recent events.
    pub fn query_all(&self, limit: usize) -> Result<Vec<Event>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, timestamp, type, action, context, confidence, source_session, source_text, payload
             FROM events ORDER BY id DESC LIMIT ?1"
        )?;
        let events = stmt
            .query_map(params![limit as i64], Self::row_to_event)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(events)
    }

    /// Query events by action string.
    pub fn query_by_action(&self, action: &str, limit: usize) -> Result<Vec<Event>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, timestamp, type, action, context, confidence, source_session, source_text, payload
             FROM events WHERE action = ?1 ORDER BY id DESC LIMIT ?2"
        )?;
        let events = stmt
            .query_map(params![action, limit as i64], Self::row_to_event)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(events)
    }

    /// Count events with a given action.
    pub fn count_by_action(&self, action: &str) -> Result<usize> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM events WHERE action = ?1",
            params![action],
            |row| row.get(0),
        )?;
        Ok(count as usize)
    }

    /// Full-text search across events.
    pub fn search(&self, query: &str) -> Result<Vec<Event>> {
        let mut stmt = self.conn.prepare(
            "SELECT e.id, e.timestamp, e.type, e.action, e.context, e.confidence,
                    e.source_session, e.source_text, e.payload
             FROM events e
             INNER JOIN events_fts fts ON e.id = fts.rowid
             WHERE events_fts MATCH ?1
             ORDER BY rank
             LIMIT 20",
        )?;
        let events = stmt
            .query_map(params![query], Self::row_to_event)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(events)
    }

    fn row_to_event(row: &rusqlite::Row) -> rusqlite::Result<Event> {
        let type_str: String = row.get(2)?;
        let event_type = Event::event_type_from_str(&type_str);
        Ok(Event {
            id: Some(row.get(0)?),
            timestamp: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(1)?)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|e| {
                    tracing::warn!("Failed to parse event timestamp, using current time: {}", e);
                    chrono::Utc::now()
                }),
            event_type,
            action: row.get(3)?,
            context: row.get(4)?,
            confidence: row.get(5)?,
            source_session: row.get(6)?,
            source_text: row.get(7)?,
            payload: row.get::<_, Option<String>>(8)?.and_then(|s| {
                serde_json::from_str(&s)
                    .map_err(|e| {
                        tracing::warn!(
                            "Failed to deserialize event payload for row {}: {}",
                            row.get::<_, i64>(0).unwrap_or(-1),
                            e
                        );
                    })
                    .ok()
            }),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{Event, EventType};
    use tempfile::tempdir;

    fn make_event(action: &str, confidence: f64) -> Event {
        Event::new(
            EventType::BehaviorPattern,
            action,
            "test_context",
            confidence,
            "test source text",
        )
    }

    #[test]
    fn test_insert_and_query_events() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let mut store = SqliteEventStore::open(&db_path).unwrap();

        let e1 = make_event("loss_chase", 0.87);
        let e2 = make_event("loss_chase", 0.91);
        let e3 = make_event("prefers_tech", 0.80);

        store.insert(&e1).unwrap();
        store.insert(&e2).unwrap();
        store.insert(&e3).unwrap();

        let all = store.query_all(10).unwrap();
        assert_eq!(all.len(), 3);

        let loss_events = store.query_by_action("loss_chase", 10).unwrap();
        assert_eq!(loss_events.len(), 2);

        let count = store.count_by_action("loss_chase").unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn test_event_fts_search() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let mut store = SqliteEventStore::open(&db_path).unwrap();

        let e = make_event("loss_chase", 0.87);
        store.insert(&e).unwrap();

        let results = store.search("loss").unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].action, "loss_chase");
        assert_eq!(results[0].confidence, 0.87);
    }

    #[test]
    fn test_empty_store() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let store = SqliteEventStore::open(&db_path).unwrap();

        assert!(store.query_all(10).unwrap().is_empty());
        assert_eq!(store.count_by_action("anything").unwrap(), 0);
    }

    #[test]
    fn test_payload_roundtrip() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let mut store = SqliteEventStore::open(&db_path).unwrap();

        let event = make_event("test_action", 0.99)
            .with_payload(serde_json::json!({"key": "value", "num": 42}));

        let id = store.insert(&event).unwrap();
        let all = store.query_all(1).unwrap();
        assert_eq!(all.len(), 1);
        let retrieved = &all[0];
        assert_eq!(retrieved.id, Some(id));
        assert!(retrieved.payload.is_some());
        let payload = retrieved.payload.as_ref().unwrap();
        assert_eq!(payload["key"], "value");
        assert_eq!(payload["num"], 42);
    }
}

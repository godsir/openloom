use anyhow::Result;
use rusqlite::Connection;
use std::path::Path;

pub struct MemoryDb {
    conn: Connection,
}

impl MemoryDb {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode=DELETE;
             PRAGMA foreign_keys=ON;",
        )?;
        conn.execute_batch(include_str!("../../../../migrations/memory/V1__memory.sql"))?;

        // Migrate old schema: rename `type` column to `event_type`
        let has_type_column: bool = conn
            .prepare("SELECT 1 FROM pragma_table_info('events') WHERE name = 'type'")?
            .exists([])?;
        if has_type_column {
            conn.execute_batch(
                "DROP TRIGGER IF EXISTS events_ai;
                 DROP TRIGGER IF EXISTS events_ad;
                 ALTER TABLE events RENAME COLUMN type TO event_type;
                 DROP TABLE IF EXISTS events_fts;
                 CREATE VIRTUAL TABLE events_fts USING fts5(event_type, action, context);
                 INSERT INTO events_fts (event_type, action, context)
                 SELECT event_type, action, context FROM events;",
            )?;
        }

        Ok(Self { conn })
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    pub fn insert_event(&self, event: &crate::store::NewEvent) -> Result<i64> {
        let payload = event.payload.as_ref().map(|p| p.to_string());
        self.conn.execute(
            "INSERT INTO events (timestamp, event_type, action, context, confidence, source_session, source_text, payload)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                event.timestamp.to_rfc3339(),
                event.event_type,
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
}

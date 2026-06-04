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

        // Always drop old V1__initial.sql triggers — they reference the `type`
        // column which no longer exists after migration to `event_type`.
        conn.execute_batch(
            "DROP TRIGGER IF EXISTS events_ai;
             DROP TRIGGER IF EXISTS events_ad;
             DROP TRIGGER IF EXISTS events_au;",
        )?;

        // Migrate old schema: rename `type` column to `event_type` on events table.
        let has_type_column: bool = conn
            .prepare("SELECT 1 FROM pragma_table_info('events') WHERE name = 'type'")?
            .exists([])?;
        if has_type_column {
            conn.execute_batch(
                "ALTER TABLE events RENAME COLUMN type TO event_type;",
            )?;
        }

        // Rebuild events_fts if its schema doesn't match (survives partial
        // migrations where events was migrated but FTS was left stale).
        let fts_has_type: bool = conn
            .prepare("SELECT 1 FROM pragma_table_info('events_fts') WHERE name = 'type'")?
            .exists([])?;
        if fts_has_type || has_type_column {
            conn.execute_batch(
                "DROP TABLE IF EXISTS events_fts;
                 CREATE VIRTUAL TABLE events_fts USING fts5(event_type, action, context);
                 INSERT INTO events_fts (event_type, action, context)
                 SELECT event_type, action, context FROM events;",
            )?;
        }

        // Create FTS sync triggers for the new schema if they don't exist yet.
        conn.execute_batch(
            "CREATE TRIGGER IF NOT EXISTS events_fts_ai AFTER INSERT ON events BEGIN
                 INSERT INTO events_fts(rowid, event_type, action, context)
                 VALUES (new.id, new.event_type, new.action, new.context);
             END;
             CREATE TRIGGER IF NOT EXISTS events_fts_ad AFTER DELETE ON events BEGIN
                 INSERT INTO events_fts(events_fts, rowid, event_type, action, context)
                 VALUES('delete', old.id, old.event_type, old.action, old.context);
             END;
             CREATE TRIGGER IF NOT EXISTS events_fts_au AFTER UPDATE ON events BEGIN
                 INSERT INTO events_fts(events_fts, rowid, event_type, action, context)
                 VALUES('delete', old.id, old.event_type, old.action, old.context);
                 INSERT INTO events_fts(rowid, event_type, action, context)
                 VALUES (new.id, new.event_type, new.action, new.context);
             END;",
        )?;

        // --- kg_nodes_fts sync triggers ---
        // Drop the standalone FTS5 table created in V1__memory.sql and rebuild it
        // with proper sync triggers so insert/update/delete on kg_nodes are
        // reflected in the FTS index used by search_entities().
        conn.execute_batch(
            "DROP TABLE IF EXISTS kg_nodes_fts;
             CREATE VIRTUAL TABLE kg_nodes_fts USING fts5(name, description, content='kg_nodes', content_rowid='id');
             INSERT INTO kg_nodes_fts(rowid, name, description) SELECT id, name, description FROM kg_nodes;
             CREATE TRIGGER IF NOT EXISTS kg_nodes_fts_ai AFTER INSERT ON kg_nodes BEGIN
                 INSERT INTO kg_nodes_fts(rowid, name, description)
                 VALUES (new.id, new.name, new.description);
             END;
             CREATE TRIGGER IF NOT EXISTS kg_nodes_fts_au AFTER UPDATE ON kg_nodes BEGIN
                 INSERT INTO kg_nodes_fts(kg_nodes_fts, rowid, name, description)
                 VALUES('delete', old.id, old.name, old.description);
                 INSERT INTO kg_nodes_fts(rowid, name, description)
                 VALUES (new.id, new.name, new.description);
             END;",
        )?;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kg_nodes_fts_sync_triggers() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("memory.db");
        let db = MemoryDb::open(&db_path).unwrap();
        let conn = db.conn();

        // Insert a kg_node — FTS trigger should auto-populate kg_nodes_fts
        conn.execute(
            "INSERT INTO kg_nodes (name, entity_type, description, confidence, scope)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params!["Rust", "Technology", "Systems programming language", 0.9, "global"],
        )
        .unwrap();

        // Verify it's in kg_nodes
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM kg_nodes", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1, "kg_nodes should have 1 row");

        // Verify it's in kg_nodes_fts (the fix ensures triggers populate it)
        let fts_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM kg_nodes_fts", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            fts_count, 1,
            "kg_nodes_fts should have 1 row — FTS sync trigger is working"
        );

        // Verify FTS search actually finds it
        let fts_match: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM kg_nodes_fts WHERE kg_nodes_fts MATCH ?1",
                rusqlite::params!["Rust"],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            fts_match, 1,
            "FTS search for 'Rust' should return 1 match"
        );

        // Test UPDATE trigger: change description, should still be searchable
        conn.execute(
            "UPDATE kg_nodes SET description = 'Fast systems language' WHERE name = 'Rust'",
            [],
        )
        .unwrap();
        let fts_after_update: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM kg_nodes_fts WHERE kg_nodes_fts MATCH ?1",
                rusqlite::params!["systems"],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            fts_after_update, 1,
            "After UPDATE, FTS should still find the node by new description"
        );

        // Test DELETE trigger
        conn.execute("DELETE FROM kg_nodes WHERE name = 'Rust'", [])
            .unwrap();
        let fts_after_delete: i64 = conn
            .query_row("SELECT COUNT(*) FROM kg_nodes_fts", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            fts_after_delete, 0,
            "After DELETE, FTS should have 0 rows — delete trigger is working"
        );
    }
}

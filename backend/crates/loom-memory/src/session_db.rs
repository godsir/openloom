use anyhow::Result;
use rusqlite::Connection;
use std::path::Path;

pub struct SessionDb {
    conn: Connection,
}

impl SessionDb {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        // WAL improves read/write concurrency (the pipeline writes while the
        // foreground reads/writes); busy_timeout makes a contended writer wait
        // up to 5s instead of surfacing an immediate SQLITE_BUSY error.
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA busy_timeout=5000;
             PRAGMA foreign_keys=ON;",
        )?;
        conn.execute_batch(include_str!(
            "../../../../migrations/session/V1__session.sql"
        ))?;

        // Migration V2: add updated_at column to sessions (last-active timestamp)
        let has_updated_at: bool = conn
            .prepare("SELECT 1 FROM pragma_table_info('sessions') WHERE name = 'updated_at'")?
            .exists([])?;
        if !has_updated_at {
            conn.execute_batch(
                "ALTER TABLE sessions ADD COLUMN updated_at TEXT;
                 UPDATE sessions SET updated_at = created_at;",
            )?;
        }

        // Migration V3: add episodic_summary column for L1 episodic layer summaries
        let has_episodic_summary: bool = conn
            .prepare("SELECT 1 FROM pragma_table_info('sessions') WHERE name = 'episodic_summary'")?
            .exists([])?;
        if !has_episodic_summary {
            conn.execute_batch("ALTER TABLE sessions ADD COLUMN episodic_summary TEXT;")?;
        }

        Ok(Self { conn })
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }
}

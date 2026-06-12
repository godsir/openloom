use anyhow::Result;
use rusqlite::Connection;
use std::path::Path;

pub struct ConfigDb {
    conn: Connection,
}

impl ConfigDb {
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
        // Run loom-specific migrations inline (refinery has naming conflicts with 3 embed_migrations!)
        conn.execute_batch(include_str!("../../../../migrations/loom/V1__config.sql"))?;
        Ok(Self { conn })
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }
}

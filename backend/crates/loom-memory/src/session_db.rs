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
        let mut conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode=DELETE;
             PRAGMA foreign_keys=ON;",
        )?;
        conn.execute_batch(include_str!("../../../../migrations/session/V1__session.sql"))?;
        Ok(Self { conn })
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }
}

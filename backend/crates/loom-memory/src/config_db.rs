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
        conn.execute_batch(include_str!("../../../../migrations/loom/V1__config.sql"))?;
        conn.execute_batch("BEGIN IMMEDIATE")?;
        let has_compact_mode: bool = conn.query_row(
            "SELECT EXISTS(
                SELECT 1 FROM pragma_table_info('model_configs') WHERE name = 'compact_mode'
             )",
            [],
            |row| row.get(0),
        )?;
        if !has_compact_mode {
            conn.execute_batch(include_str!(
                "../../../../migrations/loom/V2__add_model_compact_mode.sql"
            ))?;
        }
        conn.execute_batch("COMMIT")?;
        Ok(Self { conn })
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    use super::ConfigDb;

    #[test]
    fn open_migrates_existing_model_configs_with_compact_mode() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.db");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(include_str!("../../../../migrations/loom/V1__config.sql"))
            .unwrap();
        conn.execute("INSERT INTO model_configs (name) VALUES ('legacy')", [])
            .unwrap();
        drop(conn);

        let db = ConfigDb::open(&path).unwrap();
        let compact_mode_columns: i64 = db
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('model_configs') WHERE name = 'compact_mode'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(compact_mode_columns, 1);
        let compact_mode: i64 = db
            .conn()
            .query_row(
                "SELECT compact_mode FROM model_configs WHERE name = 'legacy'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(compact_mode, 0);
    }
}

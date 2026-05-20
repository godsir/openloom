use super::Engine;
use anyhow::Result;
use openloom_memory::store::{EventRow, SqliteEventStore};

impl Engine {
    pub async fn search_events(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<EventRow>> {
        let conn = rusqlite::Connection::open(&self.db_path)?;
        let store = SqliteEventStore::from_connection(conn);
        store.search_fts(query, limit)
    }

    pub async fn list_events(&self, limit: usize) -> Result<Vec<EventRow>> {
        let conn = rusqlite::Connection::open(&self.db_path)?;
        let store = SqliteEventStore::from_connection(conn);
        store.query_recent(limit)
    }
}

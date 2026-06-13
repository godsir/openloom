//! BridgeStore — SQLite persistence for bridge sessions, messages, and known users.
//! Uses the V7 migration tables: bridge_sessions, bridge_messages, bridge_known_users.

use anyhow::Result;
use rusqlite::{Connection, params};

/// Synchronous get/set of a confirmed long-poll offset/cursor, keyed by a
/// stable string (e.g. the platform name).
///
/// Implementations MUST NOT block on async work — the bridge poll loops call
/// these between `.await` points so that no lock is ever held across an await
/// (which the workspace `await_holding_lock` lint forbids).
pub trait OffsetStore: Send + Sync {
    /// Load the last durably-confirmed offset for `key`, if any.
    fn load_offset(&self, key: &str) -> Option<String>;
    /// Persist the confirmed offset for `key`. Errors are surfaced to the
    /// caller, which logs them; offset persistence failing must not crash the
    /// poll loop.
    fn store_offset(&self, key: &str, value: &str) -> Result<()>;
}

/// No-op [`OffsetStore`] used when no persistence backend is wired in. The
/// offset then lives only in memory for the lifetime of the adapter (previous
/// behaviour), so it does not survive a restart but does not error either.
#[derive(Debug, Default, Clone, Copy)]
pub struct NullOffsetStore;

impl OffsetStore for NullOffsetStore {
    fn load_offset(&self, _key: &str) -> Option<String> {
        None
    }
    fn store_offset(&self, _key: &str, _value: &str) -> Result<()> {
        Ok(())
    }
}

pub struct BridgeStore<'a> {
    conn: &'a Connection,
}

impl<'a> BridgeStore<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Lazily create the bridge key/value table. The shared `V1__session.sql`
    /// migration lives outside this crate, so this table is created defensively
    /// here with `IF NOT EXISTS` rather than via a migration.
    fn ensure_meta_table(&self) -> Result<()> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS bridge_meta (
                 key   TEXT PRIMARY KEY,
                 value TEXT NOT NULL
             )",
            [],
        )?;
        Ok(())
    }

    /// Read a value from the bridge key/value store, if present.
    pub fn get_meta(&self, key: &str) -> Result<Option<String>> {
        self.ensure_meta_table()?;
        let value = self
            .conn
            .query_row(
                "SELECT value FROM bridge_meta WHERE key = ?1",
                params![key],
                |r| r.get::<_, String>(0),
            )
            .ok();
        Ok(value)
    }

    /// Upsert a value into the bridge key/value store.
    pub fn set_meta(&self, key: &str, value: &str) -> Result<()> {
        self.ensure_meta_table()?;
        self.conn.execute(
            "INSERT INTO bridge_meta (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn ensure_session(
        &self,
        platform: &str,
        chat_id: &str,
        sender_name: &str,
    ) -> Result<String> {
        let id = format!("{platform}:{chat_id}");
        self.conn.execute(
            "INSERT OR IGNORE INTO bridge_sessions (id, platform, external_chat_id, user_name, created_at, message_count)
             VALUES (?1, ?2, ?3, ?4, datetime('now'), 0)",
            params![id, platform, chat_id, sender_name],
        )?;
        Ok(id)
    }

    pub fn record_message(
        &self,
        session_id: &str,
        direction: &str,
        content: &str,
        external_msg_id: &str,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO bridge_messages (session_id, direction, content, external_message_id, timestamp)
             VALUES (?1, ?2, ?3, ?4, datetime('now'))",
            params![session_id, direction, content, external_msg_id],
        )?;
        self.conn.execute(
            "UPDATE bridge_sessions SET last_message_at = datetime('now'), message_count = message_count + 1 WHERE id = ?1",
            params![session_id],
        )?;
        Ok(())
    }

    pub fn upsert_known_user(&self, platform: &str, user_id: &str, user_name: &str) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO bridge_known_users (platform, user_id, user_name, first_seen, last_seen)
             VALUES (?1, ?2, ?3, COALESCE((SELECT first_seen FROM bridge_known_users WHERE platform=?1 AND user_id=?2), datetime('now')), datetime('now'))",
            params![platform, user_id, user_name],
        )?;
        Ok(())
    }

    pub fn session_access_state(&self, session_id: &str) -> Result<String> {
        let state: String = self
            .conn
            .query_row(
                "SELECT access_state FROM bridge_sessions WHERE id = ?1",
                params![session_id],
                |r| r.get(0),
            )
            .unwrap_or_else(|_| "active".to_string());
        Ok(state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meta_roundtrip_and_offsetstore_impl() {
        let conn = Connection::open_in_memory().unwrap();
        let store = BridgeStore::new(&conn);

        // Missing key -> None.
        assert_eq!(store.get_meta("telegram:offset").unwrap(), None);

        // Set then get.
        store.set_meta("telegram:offset", "42").unwrap();
        assert_eq!(
            store.get_meta("telegram:offset").unwrap().as_deref(),
            Some("42")
        );

        // Upsert overwrites.
        store.set_meta("telegram:offset", "99").unwrap();
        assert_eq!(
            store.get_meta("telegram:offset").unwrap().as_deref(),
            Some("99")
        );
    }

    #[test]
    fn null_offset_store_is_noop() {
        let s = NullOffsetStore;
        assert_eq!(s.load_offset("k"), None);
        assert!(s.store_offset("k", "v").is_ok());
    }
}

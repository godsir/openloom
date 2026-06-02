//! BridgeStore — SQLite persistence for bridge sessions, messages, and known users.
//! Uses the V7 migration tables: bridge_sessions, bridge_messages, bridge_known_users.

use anyhow::Result;
use rusqlite::{Connection, params};

pub struct BridgeStore<'a> {
    conn: &'a Connection,
}

impl<'a> BridgeStore<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn ensure_session(
        &self,
        platform: &str,
        chat_id: &str,
        sender_name: &str,
    ) -> Result<String> {
        let id = format!("{}:{}", platform, chat_id);
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

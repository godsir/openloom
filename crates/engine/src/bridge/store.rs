use anyhow::Result;
use chrono::Utc;
use rusqlite::{params, Connection};

use super::types::*;

/// Row from bridge_sessions table
#[derive(Debug, Clone)]
pub struct BridgeSession {
    pub id: String,
    pub platform: Platform,
    pub external_chat_id: String,
    pub external_user_id: Option<String>,
    pub user_name: Option<String>,
    pub user_avatar_url: Option<String>,
    pub access_state: AccessState,
    pub pairing_code: Option<String>,
    pub created_at: String,
    pub last_message_at: Option<String>,
    pub message_count: i64,
}

/// Row from bridge_messages table
#[derive(Debug, Clone)]
pub struct BridgeMessageRow {
    pub id: i64,
    pub session_id: String,
    pub direction: MessageDirection,
    pub content: Option<String>,
    pub media_type: String,
    pub media_url: Option<String>,
    pub external_message_id: Option<String>,
    pub timestamp: String,
}

/// Row from bridge_known_users table
#[derive(Debug, Clone)]
pub struct KnownUserRow {
    pub platform: Platform,
    pub user_id: String,
    pub user_name: Option<String>,
    pub avatar_url: Option<String>,
    pub first_seen: String,
    pub last_seen: Option<String>,
}

pub struct BridgeStore<'a> {
    conn: &'a Connection,
}

impl<'a> BridgeStore<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Find or create a bridge session for the given platform + chat_id
    pub fn find_or_create_session(
        &self,
        platform: Platform,
        external_chat_id: &str,
        external_user_id: Option<&str>,
        user_name: Option<&str>,
    ) -> Result<BridgeSession> {
        let session_id = format!("{}:{}", platform.name(), external_chat_id);
        let now = Utc::now().to_rfc3339();

        // Try to find existing
        if let Some(_session) = self.get_session(&session_id)? {
            // Update user info and last_message_at
            self.conn.execute(
                "UPDATE bridge_sessions SET last_message_at = ?1, external_user_id = COALESCE(?2, external_user_id), user_name = COALESCE(?3, user_name) WHERE id = ?4",
                params![now, external_user_id, user_name, session_id],
            )?;
            return self.get_session(&session_id)
                .and_then(|s| s.ok_or_else(|| anyhow::anyhow!("session vanished")));
        }

        // Create new
        self.conn.execute(
            "INSERT INTO bridge_sessions (id, platform, external_chat_id, external_user_id, user_name, access_state, created_at, last_message_at) VALUES (?1, ?2, ?3, ?4, ?5, 'active', ?6, ?6)",
            params![session_id, platform.name(), external_chat_id, external_user_id, user_name, now],
        )?;

        self.get_session(&session_id)
            .and_then(|s| s.ok_or_else(|| anyhow::anyhow!("failed to read back session")))
    }

    pub fn get_session(&self, session_id: &str) -> Result<Option<BridgeSession>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, platform, external_chat_id, external_user_id, user_name, user_avatar_url, access_state, pairing_code, created_at, last_message_at, message_count FROM bridge_sessions WHERE id = ?1",
        )?;

        let result = stmt.query_row(params![session_id], |row| {
            let platform_str: String = row.get(1)?;
            let access_str: String = row.get(6)?;
            Ok(BridgeSession {
                id: row.get(0)?,
                platform: Platform::from_str(&platform_str).unwrap_or(Platform::Telegram),
                external_chat_id: row.get(2)?,
                external_user_id: row.get(3)?,
                user_name: row.get(4)?,
                user_avatar_url: row.get(5)?,
                access_state: AccessState::from_str(&access_str).unwrap_or(AccessState::Active),
                pairing_code: row.get(7)?,
                created_at: row.get(8)?,
                last_message_at: row.get(9)?,
                message_count: row.get(10)?,
            })
        });

        match result {
            Ok(s) => Ok(Some(s)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn list_sessions(&self, platform: Option<Platform>) -> Result<Vec<BridgeSession>> {
        let sql = if platform.is_some() {
            "SELECT id, platform, external_chat_id, external_user_id, user_name, user_avatar_url, access_state, pairing_code, created_at, last_message_at, message_count FROM bridge_sessions WHERE platform = ?1 ORDER BY last_message_at DESC"
        } else {
            "SELECT id, platform, external_chat_id, external_user_id, user_name, user_avatar_url, access_state, pairing_code, created_at, last_message_at, message_count FROM bridge_sessions ORDER BY last_message_at DESC"
        };

        let mut stmt = self.conn.prepare(sql)?;
        let platform_name = platform.map(|p| p.name().to_string());

        let rows = if let Some(ref pname) = platform_name {
            stmt.query_map(params![pname], Self::map_session_row)?
        } else {
            stmt.query_map([], Self::map_session_row)?
        };

        let mut sessions = Vec::new();
        for row in rows {
            sessions.push(row?);
        }
        Ok(sessions)
    }

    fn map_session_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<BridgeSession> {
        let platform_str: String = row.get(1)?;
        let access_str: String = row.get(6)?;
        Ok(BridgeSession {
            id: row.get(0)?,
            platform: Platform::from_str(&platform_str).unwrap_or(Platform::Telegram),
            external_chat_id: row.get(2)?,
            external_user_id: row.get(3)?,
            user_name: row.get(4)?,
            user_avatar_url: row.get(5)?,
            access_state: AccessState::from_str(&access_str).unwrap_or(AccessState::Active),
            pairing_code: row.get(7)?,
            created_at: row.get(8)?,
            last_message_at: row.get(9)?,
            message_count: row.get(10)?,
        })
    }

    pub fn record_message(
        &self,
        session_id: &str,
        direction: MessageDirection,
        content: Option<&str>,
        media_type: &str,
        media_url: Option<&str>,
        external_message_id: Option<&str>,
    ) -> Result<i64> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO bridge_messages (session_id, direction, content, media_type, media_url, external_message_id, timestamp) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![session_id, direction.as_str(), content, media_type, media_url, external_message_id, now],
        )?;

        // Increment session message count
        self.conn.execute(
            "UPDATE bridge_sessions SET message_count = message_count + 1 WHERE id = ?1",
            params![session_id],
        )?;

        Ok(self.conn.last_insert_rowid())
    }

    pub fn list_messages(
        &self,
        session_id: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<BridgeMessageRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, direction, content, media_type, media_url, external_message_id, timestamp FROM bridge_messages WHERE session_id = ?1 ORDER BY timestamp DESC LIMIT ?2 OFFSET ?3",
        )?;

        let rows = stmt.query_map(params![session_id, limit as i64, offset as i64], |row| {
            let dir_str: String = row.get(2)?;
            Ok(BridgeMessageRow {
                id: row.get(0)?,
                session_id: row.get(1)?,
                direction: MessageDirection::from_str(&dir_str).unwrap_or(MessageDirection::Inbound),
                content: row.get(3)?,
                media_type: row.get(4)?,
                media_url: row.get(5)?,
                external_message_id: row.get(6)?,
                timestamp: row.get(7)?,
            })
        })?;

        let mut messages = Vec::new();
        for row in rows {
            messages.push(row?);
        }
        Ok(messages)
    }

    pub fn check_dedup(&self, external_message_id: &str) -> Result<bool> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM bridge_messages WHERE external_message_id = ?1",
            params![external_message_id],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    pub fn upsert_known_user(
        &self,
        platform: Platform,
        user_id: &str,
        user_name: Option<&str>,
        avatar_url: Option<&str>,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO bridge_known_users (platform, user_id, user_name, avatar_url, first_seen, last_seen) VALUES (?1, ?2, ?3, ?4, ?5, ?5) ON CONFLICT(platform, user_id) DO UPDATE SET user_name = COALESCE(?3, user_name), avatar_url = COALESCE(?4, avatar_url), last_seen = ?5",
            params![platform.name(), user_id, user_name, avatar_url, now],
        )?;
        Ok(())
    }

    pub fn list_known_users(&self, platform: Option<Platform>) -> Result<Vec<KnownUserRow>> {
        let sql = if platform.is_some() {
            "SELECT platform, user_id, user_name, avatar_url, first_seen, last_seen FROM bridge_known_users WHERE platform = ?1 ORDER BY last_seen DESC"
        } else {
            "SELECT platform, user_id, user_name, avatar_url, first_seen, last_seen FROM bridge_known_users ORDER BY last_seen DESC"
        };

        let mut stmt = self.conn.prepare(sql)?;
        let platform_name = platform.map(|p| p.name().to_string());

        let rows = if let Some(ref pname) = platform_name {
            stmt.query_map(params![pname], Self::map_known_user_row)?
        } else {
            stmt.query_map([], Self::map_known_user_row)?
        };

        let mut users = Vec::new();
        for row in rows {
            users.push(row?);
        }
        Ok(users)
    }

    fn map_known_user_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<KnownUserRow> {
        let platform_str: String = row.get(0)?;
        Ok(KnownUserRow {
            platform: Platform::from_str(&platform_str).unwrap_or(Platform::Telegram),
            user_id: row.get(1)?,
            user_name: row.get(2)?,
            avatar_url: row.get(3)?,
            first_seen: row.get(4)?,
            last_seen: row.get(5)?,
        })
    }

    pub fn delete_session(&self, session_id: &str) -> Result<bool> {
        let rows = self.conn.execute(
            "DELETE FROM bridge_sessions WHERE id = ?1",
            params![session_id],
        )?;
        Ok(rows > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE bridge_sessions (
                id TEXT PRIMARY KEY, platform TEXT NOT NULL,
                external_chat_id TEXT NOT NULL, external_user_id TEXT,
                user_name TEXT, user_avatar_url TEXT,
                access_state TEXT NOT NULL DEFAULT 'active',
                pairing_code TEXT, created_at TEXT NOT NULL,
                last_message_at TEXT, message_count INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE bridge_messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL REFERENCES bridge_sessions(id) ON DELETE CASCADE,
                direction TEXT NOT NULL, content TEXT,
                media_type TEXT NOT NULL DEFAULT 'text',
                media_url TEXT, external_message_id TEXT,
                timestamp TEXT NOT NULL
            );
            CREATE TABLE bridge_known_users (
                platform TEXT NOT NULL, user_id TEXT NOT NULL,
                user_name TEXT, avatar_url TEXT,
                first_seen TEXT NOT NULL, last_seen TEXT,
                PRIMARY KEY (platform, user_id)
            );",
        )
        .unwrap();
        conn
    }

    #[test]
    fn test_create_and_get_session() {
        let conn = setup_test_db();
        let store = BridgeStore::new(&conn);

        let session = store
            .find_or_create_session(Platform::Telegram, "chat_123", Some("user_1"), Some("Alice"))
            .unwrap();

        assert_eq!(session.id, "telegram:chat_123");
        assert_eq!(session.platform, Platform::Telegram);
        assert_eq!(session.external_user_id, Some("user_1".to_string()));
        assert_eq!(session.user_name, Some("Alice".to_string()));
        assert_eq!(session.access_state, AccessState::Active);
        assert_eq!(session.message_count, 0);
    }

    #[test]
    fn test_find_existing_session() {
        let conn = setup_test_db();
        let store = BridgeStore::new(&conn);

        let s1 = store
            .find_or_create_session(Platform::Telegram, "chat_123", Some("user_1"), Some("Alice"))
            .unwrap();
        let s2 = store
            .find_or_create_session(Platform::Telegram, "chat_123", Some("user_1"), Some("Alice"))
            .unwrap();

        assert_eq!(s1.id, s2.id);
    }

    #[test]
    fn test_record_and_list_messages() {
        let conn = setup_test_db();
        let store = BridgeStore::new(&conn);

        store
            .find_or_create_session(Platform::Telegram, "chat_1", None, None)
            .unwrap();

        let id = store
            .record_message(
                "telegram:chat_1",
                MessageDirection::Inbound,
                Some("hello"),
                "text",
                None,
                Some("ext_1"),
            )
            .unwrap();
        assert!(id > 0);

        let messages = store.list_messages("telegram:chat_1", 10, 0).unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].content, Some("hello".to_string()));
        assert_eq!(messages[0].direction, MessageDirection::Inbound);
    }

    #[test]
    fn test_dedup() {
        let conn = setup_test_db();
        let store = BridgeStore::new(&conn);

        store
            .find_or_create_session(Platform::Telegram, "chat_1", None, None)
            .unwrap();

        assert!(!store.check_dedup("msg_unique").unwrap());

        store
            .record_message(
                "telegram:chat_1",
                MessageDirection::Inbound,
                Some("hi"),
                "text",
                None,
                Some("msg_unique"),
            )
            .unwrap();

        assert!(store.check_dedup("msg_unique").unwrap());
    }

    #[test]
    fn test_known_users_upsert_and_list() {
        let conn = setup_test_db();
        let store = BridgeStore::new(&conn);

        store
            .upsert_known_user(Platform::Telegram, "user_1", Some("Alice"), None)
            .unwrap();
        store
            .upsert_known_user(Platform::Telegram, "user_1", Some("Alice B"), None)
            .unwrap();

        let users = store.list_known_users(Some(Platform::Telegram)).unwrap();
        assert_eq!(users.len(), 1);
        assert_eq!(users[0].user_name, Some("Alice B".to_string()));
    }

    #[test]
    fn test_list_sessions_by_platform() {
        let conn = setup_test_db();
        let store = BridgeStore::new(&conn);

        store.find_or_create_session(Platform::Telegram, "c1", None, None).unwrap();
        store.find_or_create_session(Platform::Feishu, "c2", None, None).unwrap();
        store.find_or_create_session(Platform::Telegram, "c3", None, None).unwrap();

        let tg = store.list_sessions(Some(Platform::Telegram)).unwrap();
        assert_eq!(tg.len(), 2);

        let all = store.list_sessions(None).unwrap();
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn test_delete_session() {
        let conn = setup_test_db();
        let store = BridgeStore::new(&conn);

        store.find_or_create_session(Platform::Telegram, "c1", None, None).unwrap();
        assert!(store.delete_session("telegram:c1").unwrap());
        assert!(!store.delete_session("telegram:c1").unwrap());

        let s = store.get_session("telegram:c1").unwrap();
        assert!(s.is_none());
    }

    #[test]
    fn test_session_message_count_increments() {
        let conn = setup_test_db();
        let store = BridgeStore::new(&conn);

        store.find_or_create_session(Platform::QQ, "q1", None, None).unwrap();
        store.record_message("qq:q1", MessageDirection::Inbound, Some("a"), "text", None, None).unwrap();
        store.record_message("qq:q1", MessageDirection::Outbound, Some("b"), "text", None, None).unwrap();

        let session = store.get_session("qq:q1").unwrap().unwrap();
        assert_eq!(session.message_count, 2);
    }
}

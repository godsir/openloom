use anyhow::Result;
use rusqlite::Connection;
use std::path::PathBuf;

use super::security::{LoopDetector, MessageDedup, RateLimiter};
use super::store::BridgeStore;
use super::types::*;

/// Routes inbound bridge messages through security checks and
/// coordinates the reply flow back to the originating platform.
pub struct MessageRouter {
    db_path: PathBuf,
    rate_limiter: RateLimiter,
    dedup: MessageDedup,
    loop_detector: LoopDetector,
}

impl MessageRouter {
    pub fn new(db_path: PathBuf, rate_limit_per_minute: u32) -> Self {
        Self {
            db_path,
            rate_limiter: RateLimiter::new(rate_limit_per_minute),
            dedup: MessageDedup::new(10_000),
            loop_detector: LoopDetector::new(3),
        }
    }

    /// Process an inbound message: dedup → rate limit → record → return session_id.
    /// Returns `None` if the message should be dropped (duplicate, rate limited, or loop).
    pub fn process_inbound(&mut self, msg: &BridgeMessage) -> Result<Option<String>> {
        // Dedup check
        if self.dedup.is_duplicate(&msg.external_message_id) {
            tracing::debug!(
                external_id = %msg.external_message_id,
                "dropping duplicate bridge message"
            );
            return Ok(None);
        }

        // Rate limit check
        let user_key = format!("{}:{}", msg.platform.name(), msg.sender_id);
        if !self.rate_limiter.check(&user_key) {
            tracing::warn!(user = %user_key, "bridge rate limit exceeded");
            return Ok(None);
        }

        // Loop detection (inbound from user → not bot)
        let chat_key = format!("{}:{}", msg.platform.name(), msg.chat_id);
        self.loop_detector.check(&chat_key, false);

        // Open DB and record
        let conn = Connection::open(&self.db_path)?;
        let store = BridgeStore::new(&conn);

        let session = store.find_or_create_session(
            msg.platform,
            &msg.chat_id,
            Some(&msg.sender_id),
            Some(&msg.sender_name),
        )?;

        store.upsert_known_user(msg.platform, &msg.sender_id, Some(&msg.sender_name), None)?;

        let text_content = msg.content.text_content().map(|s| s.to_string());
        store.record_message(
            &session.id,
            MessageDirection::Inbound,
            text_content.as_deref(),
            msg.content.media_type(),
            None,
            Some(&msg.external_message_id),
        )?;

        Ok(Some(session.id))
    }

    /// Record an outbound reply and update loop detector
    pub fn record_outbound(
        &mut self,
        session_id: &str,
        content: &str,
        external_message_id: Option<&str>,
    ) -> Result<()> {
        let conn = Connection::open(&self.db_path)?;
        let store = BridgeStore::new(&conn);

        store.record_message(
            session_id,
            MessageDirection::Outbound,
            Some(content),
            "text",
            None,
            external_message_id,
        )?;

        // Mark as bot message for loop detection
        let chat_key = session_id.to_string();
        self.loop_detector.check(&chat_key, true);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use tempfile::tempdir;

    fn setup_db(path: &std::path::Path) {
        let conn = Connection::open(path).unwrap();
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
    }

    fn make_test_message(platform: Platform, chat_id: &str, text: &str) -> BridgeMessage {
        BridgeMessage {
            platform,
            chat_id: chat_id.to_string(),
            sender_id: "user_1".to_string(),
            sender_name: "Alice".to_string(),
            content: MessageContent::Text(text.to_string()),
            reply_to: None,
            external_message_id: format!("msg_{}", uuid::Uuid::new_v4()),
            timestamp: Utc::now(),
        }
    }

    #[test]
    fn test_process_inbound_creates_session() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        setup_db(&db_path);

        let mut router = MessageRouter::new(db_path, 30);
        let msg = make_test_message(Platform::Telegram, "chat_1", "hello");

        let session_id = router.process_inbound(&msg).unwrap();
        assert!(session_id.is_some());
        assert_eq!(session_id.unwrap(), "telegram:chat_1");
    }

    #[test]
    fn test_process_inbound_dedup() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        setup_db(&db_path);

        let mut router = MessageRouter::new(db_path, 30);
        let mut msg = make_test_message(Platform::Telegram, "chat_1", "hello");
        msg.external_message_id = "unique_id".to_string();

        let first = router.process_inbound(&msg).unwrap();
        assert!(first.is_some());

        let second = router.process_inbound(&msg).unwrap();
        assert!(second.is_none()); // duplicate
    }

    #[test]
    fn test_process_inbound_rate_limit() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        setup_db(&db_path);

        let mut router = MessageRouter::new(db_path, 2);

        let msg1 = make_test_message(Platform::Telegram, "chat_1", "a");
        let msg2 = make_test_message(Platform::Telegram, "chat_1", "b");
        let msg3 = make_test_message(Platform::Telegram, "chat_1", "c");

        assert!(router.process_inbound(&msg1).unwrap().is_some());
        assert!(router.process_inbound(&msg2).unwrap().is_some());
        assert!(router.process_inbound(&msg3).unwrap().is_none()); // rate limited
    }

    #[test]
    fn test_record_outbound() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        setup_db(&db_path);

        let mut router = MessageRouter::new(db_path.clone(), 30);
        let msg = make_test_message(Platform::Telegram, "chat_1", "hello");
        let session_id = router.process_inbound(&msg).unwrap().unwrap();

        let result = router.record_outbound(&session_id, "hi there", Some("ext_reply_1"));
        assert!(result.is_ok());

        let conn2 = Connection::open(&db_path).unwrap();
        let store = BridgeStore::new(&conn2);
        let messages = store.list_messages(&session_id, 10, 0).unwrap();
        assert_eq!(messages.len(), 2);
    }
}

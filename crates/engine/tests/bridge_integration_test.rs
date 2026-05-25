use chrono::Utc;
use openloom_engine::bridge::{
    BridgeMessage, BridgeStore, MessageContent, MessageDirection,
    MessageRouter, Platform,
};
use rusqlite::Connection;
use tempfile::tempdir;

const BRIDGE_SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS bridge_sessions (
        id TEXT PRIMARY KEY, platform TEXT NOT NULL,
        external_chat_id TEXT NOT NULL, external_user_id TEXT,
        user_name TEXT, user_avatar_url TEXT,
        access_state TEXT NOT NULL DEFAULT 'active',
        pairing_code TEXT, created_at TEXT NOT NULL,
        last_message_at TEXT, message_count INTEGER NOT NULL DEFAULT 0
    );
    CREATE TABLE IF NOT EXISTS bridge_messages (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        session_id TEXT NOT NULL REFERENCES bridge_sessions(id) ON DELETE CASCADE,
        direction TEXT NOT NULL, content TEXT,
        media_type TEXT NOT NULL DEFAULT 'text',
        media_url TEXT, external_message_id TEXT,
        timestamp TEXT NOT NULL
    );
    CREATE TABLE IF NOT EXISTS bridge_known_users (
        platform TEXT NOT NULL, user_id TEXT NOT NULL,
        user_name TEXT, avatar_url TEXT,
        first_seen TEXT NOT NULL, last_seen TEXT,
        PRIMARY KEY (platform, user_id)
    );
    CREATE INDEX IF NOT EXISTS idx_bridge_sessions_platform ON bridge_sessions(platform);
    CREATE INDEX IF NOT EXISTS idx_bridge_messages_session_ts ON bridge_messages(session_id, timestamp);
";

fn make_message(platform: Platform, chat_id: &str, sender: &str, text: &str) -> BridgeMessage {
    BridgeMessage {
        platform,
        chat_id: chat_id.to_string(),
        sender_id: sender.to_string(),
        sender_name: sender.to_string(),
        content: MessageContent::Text(text.to_string()),
        reply_to: None,
        external_message_id: format!("ext_{}", uuid::Uuid::new_v4()),
        timestamp: Utc::now(),
    }
}

#[test]
fn test_full_bridge_flow() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("bridge_test.db");

    // Setup DB
    let conn = Connection::open(&db_path).unwrap();
    conn.execute_batch(BRIDGE_SCHEMA).unwrap();
    drop(conn);

    let mut router = MessageRouter::new(db_path.clone(), 30);

    // 1. Inbound message from Telegram
    let msg1 = make_message(Platform::Telegram, "tg_chat_1", "alice", "Hello bot!");
    let session_id = router.process_inbound(&msg1).unwrap();
    assert!(session_id.is_some());
    let sid = session_id.unwrap();
    assert_eq!(sid, "telegram:tg_chat_1");

    // 2. Verify session was created in DB
    let conn = Connection::open(&db_path).unwrap();
    let store = BridgeStore::new(&conn);
    let session = store.get_session(&sid).unwrap();
    assert!(session.is_some());
    let session = session.unwrap();
    assert_eq!(session.platform, Platform::Telegram);
    assert_eq!(session.user_name, Some("alice".to_string()));

    // 3. Verify message was recorded
    let messages = store.list_messages(&sid, 10, 0).unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].direction, MessageDirection::Inbound);
    assert_eq!(messages[0].content, Some("Hello bot!".to_string()));

    // 4. Verify known user was recorded
    let users = store.list_known_users(Some(Platform::Telegram)).unwrap();
    assert_eq!(users.len(), 1);
    assert_eq!(users[0].user_id, "alice");

    // 5. Record outbound reply
    router.record_outbound(&sid, "Hi Alice!", Some("ext_reply_1")).unwrap();

    // 6. Verify both messages exist
    let messages = store.list_messages(&sid, 10, 0).unwrap();
    assert_eq!(messages.len(), 2);
    drop(conn);

    // 7. Dedup: same external_message_id should be dropped
    let msg_dup = BridgeMessage {
        external_message_id: msg1.external_message_id.clone(),
        ..make_message(Platform::Telegram, "tg_chat_1", "alice", "duplicate")
    };
    let result = router.process_inbound(&msg_dup).unwrap();
    assert!(result.is_none(), "duplicate should be dropped");
}

#[test]
fn test_multi_platform_sessions() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("multi_test.db");
    let conn = Connection::open(&db_path).unwrap();
    conn.execute_batch(BRIDGE_SCHEMA).unwrap();
    drop(conn);

    let mut router = MessageRouter::new(db_path.clone(), 30);

    // Messages from different platforms
    let tg_msg = make_message(Platform::Telegram, "tg_1", "user_a", "from telegram");
    let fs_msg = make_message(Platform::Feishu, "fs_1", "user_b", "from feishu");
    let qq_msg = make_message(Platform::QQ, "qq_1", "user_c", "from qq");

    let tg_sid = router.process_inbound(&tg_msg).unwrap().unwrap();
    let fs_sid = router.process_inbound(&fs_msg).unwrap().unwrap();
    let qq_sid = router.process_inbound(&qq_msg).unwrap().unwrap();

    assert_eq!(tg_sid, "telegram:tg_1");
    assert_eq!(fs_sid, "feishu:fs_1");
    assert_eq!(qq_sid, "qq:qq_1");

    // Verify all sessions exist
    let conn = Connection::open(&db_path).unwrap();
    let store = BridgeStore::new(&conn);

    let all = store.list_sessions(None).unwrap();
    assert_eq!(all.len(), 3);

    let tg_only = store.list_sessions(Some(Platform::Telegram)).unwrap();
    assert_eq!(tg_only.len(), 1);

    let feishu_only = store.list_sessions(Some(Platform::Feishu)).unwrap();
    assert_eq!(feishu_only.len(), 1);
}

#[test]
fn test_rate_limiting_integration() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("rate_test.db");
    let conn = Connection::open(&db_path).unwrap();
    conn.execute_batch(BRIDGE_SCHEMA).unwrap();
    drop(conn);

    // Very low rate limit: 3 per minute
    let mut router = MessageRouter::new(db_path, 3);

    // First 3 should pass
    for i in 0..3 {
        let msg = make_message(Platform::Wechat, "wx_1", "user_x", &format!("msg {i}"));
        let result = router.process_inbound(&msg).unwrap();
        assert!(result.is_some(), "message {i} should pass");
    }

    // 4th should be rate limited
    let msg4 = make_message(Platform::Wechat, "wx_1", "user_x", "msg 4");
    let result = router.process_inbound(&msg4).unwrap();
    assert!(result.is_none(), "4th message should be rate limited");

    // Different user should still pass
    let other_msg = make_message(Platform::Wechat, "wx_2", "user_y", "other user");
    let result = router.process_inbound(&other_msg).unwrap();
    assert!(result.is_some(), "different user should not be rate limited");
}

#[test]
fn test_media_content_routing() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("media_test.db");
    let conn = Connection::open(&db_path).unwrap();
    conn.execute_batch(BRIDGE_SCHEMA).unwrap();
    drop(conn);

    let mut router = MessageRouter::new(db_path.clone(), 30);

    // Image message
    let img_msg = BridgeMessage {
        platform: Platform::Feishu,
        chat_id: "feishu_chat".to_string(),
        sender_id: "user_1".to_string(),
        sender_name: "Bob".to_string(),
        content: MessageContent::Image {
            url: "https://example.com/photo.jpg".to_string(),
            caption: Some("Check this out".to_string()),
        },
        reply_to: None,
        external_message_id: "img_001".to_string(),
        timestamp: Utc::now(),
    };

    let sid = router.process_inbound(&img_msg).unwrap().unwrap();
    assert_eq!(sid, "feishu:feishu_chat");

    let conn = Connection::open(&db_path).unwrap();
    let store = BridgeStore::new(&conn);
    let messages = store.list_messages(&sid, 10, 0).unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].media_type, "image");
    assert_eq!(messages[0].content, Some("Check this out".to_string()));
}

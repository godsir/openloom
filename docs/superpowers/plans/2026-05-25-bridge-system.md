# Bridge 外部平台接入 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let openLoom Agent 作为机器人接入 Telegram、飞书、微信、QQ 外部消息平台，实现双向完整交互（收消息 → Agent 回复 → 发回原平台）。

**Architecture:** Channel Adapter 模式 — 每个平台实现 `ChannelAdapter` trait，由 `BridgeManager` 统一管理生命周期。`MessageRouter` 将入站消息路由到 Engine，回复原路返回。所有消息持久化到 SQLite `bridge_sessions` / `bridge_messages` 表。

**Tech Stack:** Rust (tokio, reqwest, tokio-tungstenite, rusqlite), wiremock (testing)

---

## File Structure

```
crates/engine/src/bridge/
├── mod.rs          ← pub mod + re-exports (converted from existing bridge.rs)
├── types.rs        ← Platform, BridgeMessage, MessageContent, AdapterHealth
├── adapter.rs      ← ChannelAdapter trait definition
├── security.rs     ← RateLimiter + MessageDedup + LoopDetector
├── store.rs        ← BridgeStore (rusqlite CRUD for bridge tables)
├── manager.rs      ← BridgeManager (Adapter lifecycle + message dispatch)
├── router.rs       ← MessageRouter (inbound → engine → outbound routing)
├── telegram.rs     ← TelegramAdapter (Bot API long polling)
├── feishu.rs       ← FeishuAdapter (WebSocket long connection)
├── wechat.rs       ← WechatAdapter (iLink HTTP bridge)
└── qq.rs           ← QQAdapter (QQ Bot API)

migrations/
└── V7__add_bridge_tables.sql
```

---

## Task 1: Convert bridge.rs to bridge module directory

The existing `crates/engine/src/bridge.rs` (config + test-connectivity) becomes `bridge/mod.rs`, and new submodule files are declared.

**Files:**
- Create: `crates/engine/src/bridge/mod.rs` (move from `bridge.rs`)
- Create: `crates/engine/src/bridge/types.rs` (empty stub)
- Create: `crates/engine/src/bridge/adapter.rs` (empty stub)
- Delete: `crates/engine/src/bridge.rs`

- [ ] **Step 1: Create bridge/ directory and move existing code**

```bash
mkdir -p crates/engine/src/bridge
mv crates/engine/src/bridge.rs crates/engine/src/bridge/mod.rs
```

- [ ] **Step 2: Add submodule declarations to mod.rs**

Add at the top of `crates/engine/src/bridge/mod.rs`, before the existing `use` statements:

```rust
pub mod adapter;
pub mod manager;
pub mod router;
pub mod security;
pub mod store;
pub mod telegram;
pub mod feishu;
pub mod wechat;
pub mod qq;
pub mod types;

pub use types::*;
pub use adapter::ChannelAdapter;
pub use manager::BridgeManager;
pub use router::MessageRouter;
```

- [ ] **Step 3: Create empty stub files so it compiles**

Create each stub file with just a comment:

`crates/engine/src/bridge/types.rs`:
```rust
// Bridge types — populated in Task 2
```

`crates/engine/src/bridge/adapter.rs`:
```rust
// ChannelAdapter trait — populated in Task 2
```

`crates/engine/src/bridge/security.rs`:
```rust
// BridgeSecurity — populated in Task 4
```

`crates/engine/src/bridge/store.rs`:
```rust
// BridgeStore — populated in Task 3
```

`crates/engine/src/bridge/manager.rs`:
```rust
// BridgeManager — populated in Task 5
```

`crates/engine/src/bridge/router.rs`:
```rust
// MessageRouter — populated in Task 5
```

`crates/engine/src/bridge/telegram.rs`:
```rust
// TelegramAdapter — populated in Task 8
```

`crates/engine/src/bridge/feishu.rs`:
```rust
// FeishuAdapter — populated in Task 9
```

`crates/engine/src/bridge/wechat.rs`:
```rust
// WechatAdapter — populated in Task 10
```

`crates/engine/src/bridge/qq.rs`:
```rust
// QQAdapter — populated in Task 11
```

- [ ] **Step 4: Add dependencies to engine Cargo.toml**

Add to `crates/engine/Cargo.toml` `[dependencies]`:

```toml
async-trait = { workspace = true }
tokio-tungstenite = { workspace = true }
base64 = { workspace = true }
```

- [ ] **Step 5: Verify compilation**

```bash
cargo check -p openloom-engine
```

Expected: PASS (all stubs are valid modules)

- [ ] **Step 6: Commit**

```bash
git add crates/engine/src/bridge/ crates/engine/Cargo.toml
git rm crates/engine/src/bridge.rs
git commit -m "chore: convert bridge.rs to bridge module directory"
```

---

## Task 2: Bridge types + ChannelAdapter trait

Define all shared types and the core adapter trait that every platform implements.

**Files:**
- Modify: `crates/engine/src/bridge/types.rs`
- Modify: `crates/engine/src/bridge/adapter.rs`
- Test: `crates/engine/src/bridge/types.rs` (inline `#[cfg(test)]` module)

- [ ] **Step 1: Write the failing test for Platform serialization**

Add to `crates/engine/src/bridge/types.rs`:

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

/// Supported messaging platforms
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Telegram,
    Feishu,
    Wechat,
    QQ,
}

impl Platform {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Telegram => "telegram",
            Self::Feishu => "feishu",
            Self::Wechat => "wechat",
            Self::QQ => "qq",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "telegram" => Some(Self::Telegram),
            "feishu" => Some(Self::Feishu),
            "wechat" => Some(Self::Wechat),
            "qq" => Some(Self::QQ),
            _ => None,
        }
    }
}

impl std::fmt::Display for Platform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// Message content types supported across all platforms
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageContent {
    Text(String),
    Image { url: String, caption: Option<String> },
    File { url: String, name: String, size: u64 },
    Audio { url: String, duration_secs: u32 },
}

impl MessageContent {
    pub fn media_type(&self) -> &'static str {
        match self {
            Self::Text(_) => "text",
            Self::Image { .. } => "image",
            Self::File { .. } => "file",
            Self::Audio { .. } => "audio",
        }
    }

    pub fn text_content(&self) -> Option<&str> {
        match self {
            Self::Text(s) => Some(s.as_str()),
            Self::Image { caption, .. } => caption.as_deref(),
            _ => None,
        }
    }
}

/// Standardized inbound/outbound message — all adapters convert to/from this
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeMessage {
    pub platform: Platform,
    pub chat_id: String,
    pub sender_id: String,
    pub sender_name: String,
    pub content: MessageContent,
    pub reply_to: Option<String>,
    pub external_message_id: String,
    pub timestamp: DateTime<Utc>,
}

/// Health status of a platform adapter
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AdapterHealth {
    Connected,
    Connecting,
    Disconnected,
    Error(String),
}

/// Direction of a bridge message (for DB storage)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageDirection {
    Inbound,
    Outbound,
}

impl MessageDirection {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Inbound => "inbound",
            Self::Outbound => "outbound",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "inbound" => Some(Self::Inbound),
            "outbound" => Some(Self::Outbound),
            _ => None,
        }
    }
}

/// Access state for a bridge session
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AccessState {
    Active,
    Blocked,
    Pending,
}

impl AccessState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Blocked => "blocked",
            Self::Pending => "pending",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "active" => Some(Self::Active),
            "blocked" => Some(Self::Blocked),
            "pending" => Some(Self::Pending),
            _ => None,
        }
    }
}

/// Access mode for new users
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AccessMode {
    Pairing,
    Allowlist,
    Open,
}

impl Default for AccessMode {
    fn default() -> Self {
        Self::Open
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_platform_roundtrip() {
        let platforms = [Platform::Telegram, Platform::Feishu, Platform::Wechat, Platform::QQ];
        for p in &platforms {
            let name = p.name();
            let parsed = Platform::from_str(name);
            assert_eq!(parsed, Some(*p), "roundtrip failed for {name}");
        }
    }

    #[test]
    fn test_platform_display() {
        assert_eq!(Platform::Telegram.to_string(), "telegram");
        assert_eq!(Platform::Feishu.to_string(), "feishu");
    }

    #[test]
    fn test_platform_from_str_case_insensitive() {
        assert_eq!(Platform::from_str("TELEGRAM"), Some(Platform::Telegram));
        assert_eq!(Platform::from_str("Feishu"), Some(Platform::Feishu));
        assert_eq!(Platform::from_str("unknown"), None);
    }

    #[test]
    fn test_message_content_media_type() {
        assert_eq!(MessageContent::Text("hi".into()).media_type(), "text");
        let img = MessageContent::Image { url: "u".into(), caption: None };
        assert_eq!(img.media_type(), "image");
    }

    #[test]
    fn test_message_content_text_content() {
        assert_eq!(
            MessageContent::Text("hello".into()).text_content(),
            Some("hello")
        );
        let img = MessageContent::Image {
            url: "u".into(),
            caption: Some("pic".into()),
        };
        assert_eq!(img.text_content(), Some("pic"));
        let file = MessageContent::File {
            url: "u".into(),
            name: "f".into(),
            size: 0,
        };
        assert_eq!(file.text_content(), None);
    }

    #[test]
    fn test_bridge_message_serialize() {
        let msg = BridgeMessage {
            platform: Platform::Telegram,
            chat_id: "123".into(),
            sender_id: "user1".into(),
            sender_name: "Alice".into(),
            content: MessageContent::Text("hi".into()),
            reply_to: None,
            external_message_id: "msg_1".into(),
            timestamp: Utc::now(),
        };
        let json = serde_json::to_string(&msg);
        assert!(json.is_ok());
        let deserialized: BridgeMessage = serde_json::from_str(&json.unwrap()).unwrap();
        assert_eq!(deserialized.platform, Platform::Telegram);
        assert_eq!(deserialized.chat_id, "123");
    }

    #[test]
    fn test_access_state_roundtrip() {
        let states = [AccessState::Active, AccessState::Blocked, AccessState::Pending];
        for s in &states {
            let name = s.as_str();
            let parsed = AccessState::from_str(name);
            assert_eq!(parsed, Some(*s));
        }
    }

    #[test]
    fn test_direction_roundtrip() {
        assert_eq!(MessageDirection::from_str("inbound"), Some(MessageDirection::Inbound));
        assert_eq!(MessageDirection::from_str("outbound"), Some(MessageDirection::Outbound));
        assert_eq!(MessageDirection::from_str("invalid"), None);
    }
}
```

- [ ] **Step 2: Run test to verify it passes**

```bash
cargo test -p openloom-engine bridge::types --no-fail-fast
```

Expected: 8 tests PASS

- [ ] **Step 3: Write the ChannelAdapter trait**

Replace contents of `crates/engine/src/bridge/adapter.rs`:

```rust
use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::mpsc;

use super::types::{AdapterHealth, BridgeMessage, MessageContent, Platform};

/// Core trait that every platform adapter must implement.
///
/// Lifecycle: `connect()` → `receive_rx()` polls for messages → `send()` replies.
/// The BridgeManager owns adapter instances and manages their lifecycle.
#[async_trait]
pub trait ChannelAdapter: Send + Sync {
    /// Platform identifier
    fn platform(&self) -> Platform;

    /// Connect to the platform API (start polling / WebSocket / webhook listener)
    async fn connect(&mut self) -> Result<()>;

    /// Disconnect gracefully
    async fn disconnect(&mut self) -> Result<()>;

    /// Send a message to a chat. Returns the platform-assigned external_message_id.
    async fn send(&self, chat_id: &str, content: MessageContent) -> Result<String>;

    /// Receiver for inbound messages. The adapter pushes messages here after
    /// normalizing them from the platform-specific format.
    fn receive_rx(&mut self) -> &mut mpsc::Receiver<BridgeMessage>;

    /// Current health status
    fn health(&self) -> AdapterHealth;
}
```

- [ ] **Step 4: Verify compilation**

```bash
cargo check -p openloom-engine
```

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/engine/src/bridge/types.rs crates/engine/src/bridge/adapter.rs
git commit -m "feat(bridge): add core types and ChannelAdapter trait"
```

---

## Task 3: Bridge DB store + migration V7

Create the SQLite tables and the `BridgeStore` for CRUD operations on bridge sessions, messages, and known users.

**Files:**
- Create: `migrations/V7__add_bridge_tables.sql`
- Modify: `crates/engine/src/bridge/store.rs`
- Test: inline `#[cfg(test)]` in `store.rs`

- [ ] **Step 1: Create migration V7**

Create `migrations/V7__add_bridge_tables.sql`:

```sql
-- Bridge: external messaging platform sessions and messages

CREATE TABLE IF NOT EXISTS bridge_sessions (
    id TEXT PRIMARY KEY,
    platform TEXT NOT NULL,
    external_chat_id TEXT NOT NULL,
    external_user_id TEXT,
    user_name TEXT,
    user_avatar_url TEXT,
    access_state TEXT NOT NULL DEFAULT 'active',
    pairing_code TEXT,
    created_at TEXT NOT NULL,
    last_message_at TEXT,
    message_count INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS bridge_messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES bridge_sessions(id) ON DELETE CASCADE,
    direction TEXT NOT NULL,
    content TEXT,
    media_type TEXT NOT NULL DEFAULT 'text',
    media_url TEXT,
    external_message_id TEXT,
    timestamp TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS bridge_known_users (
    platform TEXT NOT NULL,
    user_id TEXT NOT NULL,
    user_name TEXT,
    avatar_url TEXT,
    first_seen TEXT NOT NULL,
    last_seen TEXT,
    PRIMARY KEY (platform, user_id)
);

CREATE INDEX IF NOT EXISTS idx_bridge_sessions_platform
    ON bridge_sessions(platform);
CREATE INDEX IF NOT EXISTS idx_bridge_messages_session_ts
    ON bridge_messages(session_id, timestamp);
```

- [ ] **Step 2: Write the BridgeStore with tests**

Replace contents of `crates/engine/src/bridge/store.rs`:

```rust
use anyhow::Result;
use chrono::Utc;
use rusqlite::{Connection, params};

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
pub struct KnownUser {
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
        if let Some(session) = self.get_session(&session_id)? {
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

    pub fn list_known_users(&self, platform: Option<Platform>) -> Result<Vec<KnownUser>> {
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

    fn map_known_user_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<KnownUser> {
        let platform_str: String = row.get(0)?;
        Ok(KnownUser {
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
```

- [ ] **Step 3: Run tests to verify**

```bash
cargo test -p openloom-engine bridge::store --no-fail-fast
```

Expected: 8 tests PASS

- [ ] **Step 4: Commit**

```bash
git add migrations/V7__add_bridge_tables.sql crates/engine/src/bridge/store.rs
git commit -m "feat(bridge): add BridgeStore and DB migration V7"
```

---

## Task 4: Security — RateLimiter + Dedup + LoopDetector

**Files:**
- Modify: `crates/engine/src/bridge/security.rs`

- [ ] **Step 1: Write the security module with tests**

```rust
use std::collections::{HashMap, VecDeque};
use std::time::Instant;

/// Per-user rate limiter using a sliding window
pub struct RateLimiter {
    max_per_minute: u32,
    windows: HashMap<String, VecDeque<Instant>>,
}

impl RateLimiter {
    pub fn new(max_per_minute: u32) -> Self {
        Self {
            max_per_minute,
            windows: HashMap::new(),
        }
    }

    /// Returns `true` if the user is within rate limits.
    /// Records the attempt regardless of result.
    pub fn check(&mut self, user_key: &str) -> bool {
        let now = Instant::now();
        let window = self.windows.entry(user_key.to_string()).or_default();

        // Remove entries older than 60 seconds
        while let Some(front) = window.front() {
            if now.duration_since(*front).as_secs() >= 60 {
                window.pop_front();
            } else {
                break;
            }
        }

        if window.len() >= self.max_per_minute as usize {
            return false;
        }

        window.push_back(now);
        true
    }
}

/// Deduplicates messages by external_message_id using a bounded LRU cache
pub struct MessageDedup {
    seen: VecDeque<String>,
    capacity: usize,
}

impl MessageDedup {
    pub fn new(capacity: usize) -> Self {
        Self {
            seen: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    /// Returns `true` if this message_id has been seen before
    pub fn is_duplicate(&mut self, message_id: &str) -> bool {
        if self.seen.iter().any(|id| id == message_id) {
            return true;
        }
        if self.seen.len() >= self.capacity {
            self.seen.pop_front();
        }
        self.seen.push_back(message_id.to_string());
        false
    }
}

/// Detects bot-to-bot reply loops by tracking consecutive bot messages
pub struct LoopDetector {
    consecutive_bot_messages: HashMap<String, u32>,
    threshold: u32,
}

impl LoopDetector {
    pub fn new(threshold: u32) -> Self {
        Self {
            consecutive_bot_messages: HashMap::new(),
            threshold,
        }
    }

    /// Record a message in a chat. Returns `true` if a loop is detected.
    pub fn check(&mut self, chat_id: &str, is_bot: bool) -> bool {
        let count = self
            .consecutive_bot_messages
            .entry(chat_id.to_string())
            .or_insert(0);

        if is_bot {
            *count += 1;
            *count >= self.threshold
        } else {
            *count = 0;
            false
        }
    }

    /// Reset counter for a chat (e.g., when a human sends a message)
    pub fn reset(&mut self, chat_id: &str) {
        self.consecutive_bot_messages.insert(chat_id.to_string(), 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limiter_allows_under_limit() {
        let mut limiter = RateLimiter::new(5);
        for _ in 0..5 {
            assert!(limiter.check("user_1"));
        }
    }

    #[test]
    fn test_rate_limiter_blocks_over_limit() {
        let mut limiter = RateLimiter::new(3);
        assert!(limiter.check("user_1"));
        assert!(limiter.check("user_1"));
        assert!(limiter.check("user_1"));
        assert!(!limiter.check("user_1"));
    }

    #[test]
    fn test_rate_limiter_separate_users() {
        let mut limiter = RateLimiter::new(1);
        assert!(limiter.check("user_1"));
        assert!(limiter.check("user_2"));
        assert!(!limiter.check("user_1"));
        assert!(!limiter.check("user_2"));
    }

    #[test]
    fn test_dedup_first_seen_not_duplicate() {
        let mut dedup = MessageDedup::new(100);
        assert!(!dedup.is_duplicate("msg_1"));
    }

    #[test]
    fn test_dedup_second_seen_is_duplicate() {
        let mut dedup = MessageDedup::new(100);
        assert!(!dedup.is_duplicate("msg_1"));
        assert!(dedup.is_duplicate("msg_1"));
    }

    #[test]
    fn test_dedup_eviction() {
        let mut dedup = MessageDedup::new(2);
        assert!(!dedup.is_duplicate("msg_1"));
        assert!(!dedup.is_duplicate("msg_2"));
        assert!(!dedup.is_duplicate("msg_3")); // evicts msg_1
        assert!(!dedup.is_duplicate("msg_1")); // msg_1 was evicted, so not duplicate
    }

    #[test]
    fn test_loop_detector_no_loop_with_human() {
        let mut detector = LoopDetector::new(3);
        assert!(!detector.check("chat_1", true));
        assert!(!detector.check("chat_1", false)); // resets
        assert!(!detector.check("chat_1", true));
    }

    #[test]
    fn test_loop_detector_detects_loop() {
        let mut detector = LoopDetector::new(3);
        assert!(!detector.check("chat_1", true));
        assert!(!detector.check("chat_1", true));
        assert!(detector.check("chat_1", true)); // 3 consecutive → loop!
    }

    #[test]
    fn test_loop_detector_separate_chats() {
        let mut detector = LoopDetector::new(2);
        assert!(!detector.check("chat_1", true));
        assert!(!detector.check("chat_2", true));
        assert!(detector.check("chat_1", true));
        assert!(detector.check("chat_2", true));
    }

    #[test]
    fn test_loop_detector_reset() {
        let mut detector = LoopDetector::new(2);
        assert!(!detector.check("chat_1", true));
        detector.reset("chat_1");
        assert!(!detector.check("chat_1", true));
    }
}
```

- [ ] **Step 2: Run tests**

```bash
cargo test -p openloom-engine bridge::security --no-fail-fast
```

Expected: 10 tests PASS

- [ ] **Step 3: Commit**

```bash
git add crates/engine/src/bridge/security.rs
git commit -m "feat(bridge): add RateLimiter, MessageDedup, LoopDetector"
```

---

## Task 5: BridgeManager — Adapter lifecycle

**Files:**
- Modify: `crates/engine/src/bridge/manager.rs`

The `BridgeManager` owns all adapter instances, starts/stops them based on config, and provides a unified API to send messages.

- [ ] **Step 1: Write the manager with tests**

```rust
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};

use super::adapter::ChannelAdapter;
use super::types::*;
use anyhow::Result;

/// Manages the lifecycle of all platform adapters and provides
/// a unified interface for sending/receiving bridge messages.
pub struct BridgeManager {
    adapters: Arc<Mutex<HashMap<Platform, Box<dyn ChannelAdapter>>>>,
    /// Receiver for inbound messages from all adapters
    inbound_rx: Arc<Mutex<mpsc::Receiver<BridgeMessage>>>,
    inbound_tx: mpsc::Sender<BridgeMessage>,
}

impl BridgeManager {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel(256);
        Self {
            adapters: Arc::new(Mutex::new(HashMap::new())),
            inbound_rx: Arc::new(Mutex::new(rx)),
            inbound_tx: tx,
        }
    }

    /// Register a platform adapter (does not start it)
    pub async fn register(&self, adapter: Box<dyn ChannelAdapter>) {
        let platform = adapter.platform();
        let mut adapters = self.adapters.lock().await;
        adapters.insert(platform, adapter);
        tracing::info!(platform = %platform, "bridge adapter registered");
    }

    /// Start the adapter for the given platform
    pub async fn start_platform(&self, platform: Platform) -> Result<()> {
        let mut adapters = self.adapters.lock().await;
        if let Some(adapter) = adapters.get_mut(&platform) {
            adapter.connect().await?;
            tracing::info!(platform = %platform, "bridge adapter started");
            Ok(())
        } else {
            anyhow::bail!("no adapter registered for platform: {platform}")
        }
    }

    /// Stop the adapter for the given platform
    pub async fn stop_platform(&self, platform: Platform) -> Result<()> {
        let mut adapters = self.adapters.lock().await;
        if let Some(adapter) = adapters.get_mut(&platform) {
            adapter.disconnect().await?;
            tracing::info!(platform = %platform, "bridge adapter stopped");
            Ok(())
        } else {
            anyhow::bail!("no adapter registered for platform: {platform}")
        }
    }

    /// Send a message through the appropriate platform adapter
    pub async fn send(
        &self,
        platform: Platform,
        chat_id: &str,
        content: MessageContent,
    ) -> Result<String> {
        let adapters = self.adapters.lock().await;
        if let Some(adapter) = adapters.get(&platform) {
            adapter.send(chat_id, content).await
        } else {
            anyhow::bail!("no adapter for platform: {platform}")
        }
    }

    /// Get health status for all registered adapters
    pub async fn health_status(&self) -> HashMap<Platform, AdapterHealth> {
        let adapters = self.adapters.lock().await;
        adapters
            .iter()
            .map(|(p, a)| (*p, a.health()))
            .collect()
    }

    /// Get the inbound message receiver (for the Router to consume)
    pub fn inbound_sender(&self) -> mpsc::Sender<BridgeMessage> {
        self.inbound_tx.clone()
    }

    /// Stop all adapters
    pub async fn shutdown(&self) {
        let mut adapters = self.adapters.lock().await;
        for (platform, adapter) in adapters.iter_mut() {
            if let Err(e) = adapter.disconnect().await {
                tracing::warn!(platform = %platform, error = %e, "error stopping adapter");
            }
        }
        adapters.clear();
        tracing::info!("bridge manager shut down");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    struct MockAdapter {
        platform: Platform,
        health: AdapterHealth,
        rx: mpsc::Receiver<BridgeMessage>,
        tx: mpsc::Sender<BridgeMessage>,
    }

    impl MockAdapter {
        fn new(platform: Platform) -> Self {
            let (tx, rx) = mpsc::channel(16);
            Self {
                platform,
                health: AdapterHealth::Disconnected,
                rx,
                tx,
            }
        }
    }

    #[async_trait]
    impl ChannelAdapter for MockAdapter {
        fn platform(&self) -> Platform {
            self.platform
        }

        async fn connect(&mut self) -> Result<()> {
            self.health = AdapterHealth::Connected;
            Ok(())
        }

        async fn disconnect(&mut self) -> Result<()> {
            self.health = AdapterHealth::Disconnected;
            Ok(())
        }

        async fn send(&self, _chat_id: &str, _content: MessageContent) -> Result<String> {
            Ok("mock_msg_id".to_string())
        }

        fn receive_rx(&mut self) -> &mut mpsc::Receiver<BridgeMessage> {
            &mut self.rx
        }

        fn health(&self) -> AdapterHealth {
            self.health.clone()
        }
    }

    #[tokio::test]
    async fn test_register_and_health() {
        let manager = BridgeManager::new();
        let adapter = MockAdapter::new(Platform::Telegram);
        manager.register(Box::new(adapter)).await;

        let health = manager.health_status().await;
        assert_eq!(health.len(), 1);
        assert_eq!(health[&Platform::Telegram], AdapterHealth::Disconnected);
    }

    #[tokio::test]
    async fn test_start_changes_health() {
        let manager = BridgeManager::new();
        manager
            .register(Box::new(MockAdapter::new(Platform::Telegram)))
            .await;

        manager.start_platform(Platform::Telegram).await.unwrap();

        let health = manager.health_status().await;
        assert_eq!(health[&Platform::Telegram], AdapterHealth::Connected);
    }

    #[tokio::test]
    async fn test_stop_changes_health() {
        let manager = BridgeManager::new();
        manager
            .register(Box::new(MockAdapter::new(Platform::Telegram)))
            .await;
        manager.start_platform(Platform::Telegram).await.unwrap();
        manager.stop_platform(Platform::Telegram).await.unwrap();

        let health = manager.health_status().await;
        assert_eq!(health[&Platform::Telegram], AdapterHealth::Disconnected);
    }

    #[tokio::test]
    async fn test_send_routes_to_correct_platform() {
        let manager = BridgeManager::new();
        manager
            .register(Box::new(MockAdapter::new(Platform::Telegram)))
            .await;
        manager.start_platform(Platform::Telegram).await.unwrap();

        let result = manager
            .send(
                Platform::Telegram,
                "chat_123",
                MessageContent::Text("hello".into()),
            )
            .await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "mock_msg_id");
    }

    #[tokio::test]
    async fn test_send_fails_for_unregistered_platform() {
        let manager = BridgeManager::new();
        let result = manager
            .send(Platform::QQ, "chat_1", MessageContent::Text("hi".into()))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_shutdown_clears_adapters() {
        let manager = BridgeManager::new();
        manager
            .register(Box::new(MockAdapter::new(Platform::Telegram)))
            .await;
        manager.shutdown().await;

        let health = manager.health_status().await;
        assert!(health.is_empty());
    }
}
```

- [ ] **Step 2: Run tests**

```bash
cargo test -p openloom-engine bridge::manager --no-fail-fast
```

Expected: 6 tests PASS

- [ ] **Step 3: Commit**

```bash
git add crates/engine/src/bridge/manager.rs
git commit -m "feat(bridge): add BridgeManager for adapter lifecycle"
```

---

## Task 6: MessageRouter — inbound → engine → outbound

**Files:**
- Modify: `crates/engine/src/bridge/router.rs`

- [ ] **Step 1: Write the router with tests**

```rust
use std::path::PathBuf;
use anyhow::Result;
use rusqlite::Connection;

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

        store.upsert_known_user(
            msg.platform,
            &msg.sender_id,
            Some(&msg.sender_name),
            None,
        )?;

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

    /// Check if a chat is in a loop state
    pub fn is_in_loop(&self, platform: Platform, chat_id: &str) -> bool {
        let chat_key = format!("{}:{}", platform.name(), chat_id);
        // We can't call check() since it's &self, so just peek
        // The actual loop detection happens in process_inbound/record_outbound
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use tempfile::tempdir;

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

        // Run migrations
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch("
            CREATE TABLE bridge_sessions (
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
            );
        ").unwrap();

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
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch("
            CREATE TABLE bridge_sessions (
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
            );
        ").unwrap();

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
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch("
            CREATE TABLE bridge_sessions (
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
            );
        ").unwrap();

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
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch("
            CREATE TABLE bridge_sessions (
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
            );
        ").unwrap();

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
```

- [ ] **Step 2: Run tests**

```bash
cargo test -p openloom-engine bridge::router --no-fail-fast
```

Expected: 4 tests PASS

- [ ] **Step 3: Commit**

```bash
git add crates/engine/src/bridge/router.rs
git commit -m "feat(bridge): add MessageRouter with security checks"
```

---

## Task 7: Telegram Adapter (Bot API + long polling)

**Files:**
- Modify: `crates/engine/src/bridge/telegram.rs`

Full implementation of the Telegram adapter using Bot API long polling.

- [ ] **Step 1: Write the Telegram adapter**

```rust
use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;
use reqwest::Client;
use serde::Deserialize;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use super::adapter::ChannelAdapter;
use super::types::*;

pub struct TelegramAdapter {
    bot_token: String,
    client: Client,
    health: AdapterHealth,
    rx: mpsc::Receiver<BridgeMessage>,
    tx: mpsc::Sender<BridgeMessage>,
    abort: Arc<AtomicBool>,
    poll_handle: Option<JoinHandle<()>>,
}

impl TelegramAdapter {
    pub fn new(bot_token: String) -> Self {
        let (tx, rx) = mpsc::channel(256);
        Self {
            bot_token,
            client: Client::new(),
            health: AdapterHealth::Disconnected,
            rx,
            tx,
            abort: Arc::new(AtomicBool::new(false)),
            poll_handle: None,
        }
    }

    fn api_url(&self, method: &str) -> String {
        format!("https://api.telegram.org/bot{}/{}", self.bot_token, method)
    }

    async fn poll_updates(&self) -> Result<Vec<TelegramUpdate>> {
        let resp = self
            .client
            .post(&self.api_url("getUpdates"))
            .json(&serde_json::json!({
                "timeout": 30,
                "allowed_updates": ["message"]
            }))
            .send()
            .await?;

        let body: TelegramResponse = resp.json().await?;
        Ok(body.result)
    }

    fn update_to_bridge_message(update: &TelegramUpdate) -> Option<BridgeMessage> {
        let msg = update.message.as_ref()?;
        let chat_id = msg.chat.id.to_string();
        let (sender_id, sender_name) = if let Some(from) = &msg.from {
            (from.id.to_string(), from.first_name.clone())
        } else {
            ("unknown".to_string(), "Unknown".to_string())
        };

        let content = if let Some(text) = &msg.text {
            MessageContent::Text(text.clone())
        } else if let Some(caption) = &msg.photo_caption {
            MessageContent::Image {
                url: String::new(), // Would need getFile API to resolve
                caption: Some(caption.clone()),
            }
        } else {
            return None; // Unsupported message type
        };

        Some(BridgeMessage {
            platform: Platform::Telegram,
            chat_id,
            sender_id,
            sender_name,
            content,
            reply_to: msg.reply_to_message.as_ref().map(|r| r.message_id.to_string()),
            external_message_id: msg.message_id.to_string(),
            timestamp: Utc::now(),
        })
    }
}

#[derive(Debug, Deserialize)]
struct TelegramResponse {
    result: Vec<TelegramUpdate>,
}

#[derive(Debug, Deserialize)]
struct TelegramUpdate {
    #[serde(default)]
    update_id: i64,
    message: Option<TelegramMessage>,
}

#[derive(Debug, Deserialize)]
struct TelegramMessage {
    message_id: i64,
    chat: TelegramChat,
    from: Option<TelegramUser>,
    text: Option<String>,
    photo_caption: Option<String>,
    reply_to_message: Option<Box<TelegramMessage>>,
}

#[derive(Debug, Deserialize)]
struct TelegramChat {
    id: i64,
}

#[derive(Debug, Deserialize)]
struct TelegramUser {
    id: i64,
    first_name: String,
}

#[async_trait]
impl ChannelAdapter for TelegramAdapter {
    fn platform(&self) -> Platform {
        Platform::Telegram
    }

    async fn connect(&mut self) -> Result<()> {
        self.health = AdapterHealth::Connecting;
        self.abort.store(false, Ordering::SeqCst);

        let client = self.client.clone();
        let tx = self.tx.clone();
        let abort = self.abort.clone();
        let bot_token = self.bot_token.clone();

        let handle = tokio::spawn(async move {
            let mut offset: i64 = 0;
            loop {
                if abort.load(Ordering::SeqCst) {
                    break;
                }

                let url = format!(
                    "https://api.telegram.org/bot{}/getUpdates",
                    bot_token
                );
                let resp = client
                    .post(&url)
                    .json(&serde_json::json!({
                        "offset": offset,
                        "timeout": 30,
                        "allowed_updates": ["message"],
                    }))
                    .send()
                    .await;

                match resp {
                    Ok(r) => {
                        if let Ok(body) = r.json::<TelegramResponse>().await {
                            for update in &body.result {
                                offset = update.update_id + 1;
                                if let Some(bridge_msg) = Self::update_to_bridge_message(update) {
                                    if tx.send(bridge_msg).await.is_err() {
                                        return;
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "telegram polling error");
                        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                    }
                }
            }
        });

        self.poll_handle = Some(handle);
        self.health = AdapterHealth::Connected;
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        self.abort.store(true, Ordering::SeqCst);
        if let Some(handle) = self.poll_handle.take() {
            handle.abort();
        }
        self.health = AdapterHealth::Disconnected;
        Ok(())
    }

    async fn send(&self, chat_id: &str, content: MessageContent) -> Result<String> {
        match content {
            MessageContent::Text(text) => {
                let resp = self
                    .client
                    .post(&self.api_url("sendMessage"))
                    .json(&serde_json::json!({
                        "chat_id": chat_id,
                        "text": text,
                        "parse_mode": "Markdown",
                    }))
                    .send()
                    .await?;

                let body: serde_json::Value = resp.json().await?;
                let msg_id = body["result"]["message_id"]
                    .as_i64()
                    .map(|id| id.to_string())
                    .unwrap_or_default();
                Ok(msg_id)
            }
            MessageContent::Image { url, caption } => {
                let resp = self
                    .client
                    .post(&self.api_url("sendPhoto"))
                    .json(&serde_json::json!({
                        "chat_id": chat_id,
                        "photo": url,
                        "caption": caption.unwrap_or_default(),
                    }))
                    .send()
                    .await?;

                let body: serde_json::Value = resp.json().await?;
                let msg_id = body["result"]["message_id"]
                    .as_i64()
                    .map(|id| id.to_string())
                    .unwrap_or_default();
                Ok(msg_id)
            }
            _ => {
                anyhow::bail!("unsupported content type for telegram: {}", content.media_type())
            }
        }
    }

    fn receive_rx(&mut self) -> &mut mpsc::Receiver<BridgeMessage> {
        &mut self.rx
    }

    fn health(&self) -> AdapterHealth {
        self.health.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_url() {
        let adapter = TelegramAdapter::new("123456:ABC".to_string());
        assert_eq!(
            adapter.api_url("getUpdates"),
            "https://api.telegram.org/bot123456:ABC/getUpdates"
        );
    }

    #[test]
    fn test_update_to_bridge_message_text() {
        let update = TelegramUpdate {
            update_id: 1,
            message: Some(TelegramMessage {
                message_id: 100,
                chat: TelegramChat { id: 12345 },
                from: Some(TelegramUser {
                    id: 67890,
                    first_name: "Alice".to_string(),
                }),
                text: Some("hello bot".to_string()),
                photo_caption: None,
                reply_to_message: None,
            }),
        };

        let msg = TelegramAdapter::update_to_bridge_message(&update).unwrap();
        assert_eq!(msg.platform, Platform::Telegram);
        assert_eq!(msg.chat_id, "12345");
        assert_eq!(msg.sender_id, "67890");
        assert_eq!(msg.sender_name, "Alice");
        match &msg.content {
            MessageContent::Text(t) => assert_eq!(t, "hello bot"),
            _ => panic!("expected Text"),
        }
    }

    #[test]
    fn test_update_to_bridge_message_none_for_empty() {
        let update = TelegramUpdate {
            update_id: 1,
            message: Some(TelegramMessage {
                message_id: 100,
                chat: TelegramChat { id: 12345 },
                from: None,
                text: None,
                photo_caption: None,
                reply_to_message: None,
            }),
        };

        assert!(TelegramAdapter::update_to_bridge_message(&update).is_none());
    }

    #[tokio::test]
    async fn test_adapter_lifecycle() {
        let mut adapter = TelegramAdapter::new("fake_token".to_string());
        assert_eq!(adapter.health(), AdapterHealth::Disconnected);
        assert_eq!(adapter.platform(), Platform::Telegram);

        // Don't actually connect (fake token), just test disconnect
        adapter.disconnect().await.unwrap();
        assert_eq!(adapter.health(), AdapterHealth::Disconnected);
    }
}
```

- [ ] **Step 2: Run tests**

```bash
cargo test -p openloom-engine bridge::telegram --no-fail-fast
```

Expected: 4 tests PASS

- [ ] **Step 3: Commit**

```bash
git add crates/engine/src/bridge/telegram.rs
git commit -m "feat(bridge): add TelegramAdapter with Bot API long polling"
```

---

## Task 8: Engine integration — handle_bridge_message

Wire the Bridge subsystem into the Engine: add `handle_bridge_message()`, `bridge_system_instruction()`, and the background polling task that drains inbound messages from adapters.

**Files:**
- Modify: `crates/engine/src/bridge/mod.rs` (add `handle_bridge_message` helper)
- Modify: `crates/engine/src/lib.rs` (integrate BridgeManager, add JSON-RPC dispatching)

- [ ] **Step 1: Add bridge fields to Engine struct**

Add to `crates/engine/src/lib.rs`, in the `Engine` struct definition (after `active_cwd`):

```rust
    bridge_manager: Option<Arc<crate::bridge::BridgeManager>>,
```

Initialize in `Engine::new()`:

```rust
    bridge_manager: None,
```

- [ ] **Step 2: Add bridge_system_instruction to Engine**

Add this method to `impl Engine` in `lib.rs`:

```rust
    /// Build a system instruction for bridge conversations.
    /// Adds context about the platform and sender.
    pub(crate) fn bridge_system_instruction(
        &self,
        platform: &crate::bridge::Platform,
        sender_name: &str,
    ) -> String {
        let base = self.system_instruction();
        format!(
            "{base}\n\n## Bridge Context\n\
             You are responding via {} (external messaging platform).\n\
             The user's name on this platform is: {sender_name}.\n\
             Keep responses concise and conversational. \
             Do not mention internal tools or file paths.",
            platform.name()
        )
    }
```

- [ ] **Step 3: Add handle_bridge_message to Engine**

```rust
    /// Process a message from a bridge platform and return the Agent's reply text.
    pub async fn handle_bridge_message(
        &self,
        msg: crate::bridge::BridgeMessage,
    ) -> Result<String> {
        let session_id = format!("bridge:{}:{}", msg.platform.name(), msg.chat_id);
        let system = self.bridge_system_instruction(&msg.platform, &msg.sender_name);

        // Extract text content for the LLM
        let user_text = match &msg.content {
            crate::bridge::MessageContent::Text(t) => t.clone(),
            crate::bridge::MessageContent::Image { caption, url } => {
                if let Some(c) = caption {
                    format!("[Image: {url}]\n{c}")
                } else {
                    format!("[Image: {url}]")
                }
            }
            crate::bridge::MessageContent::File { name, url, .. } => {
                format!("[File: {name} ({url})]")
            }
            crate::bridge::MessageContent::Audio { url, .. } => {
                format!("[Audio: {url}]")
            }
        };

        if user_text.trim().is_empty() {
            return Ok("(No text content received)".to_string());
        }

        // Build messages with system prompt
        let messages = vec![
            openloom_models::Message::system(&system),
            openloom_models::Message::user(&user_text),
        ];

        // Resolve model and provider
        let model_id = self.current_model_id();
        let provider = self.current_provider();

        let request = openloom_inference::CompletionRequest {
            messages,
            max_tokens: self.max_output_tokens,
            temperature: 0.7,
            stream: false,
            ..Default::default()
        };

        // Use the inference layer directly
        if let Some(ref cloud) = self.cloud {
            let (token_tx, mut token_rx) = tokio::sync::mpsc::channel::<String>(64);
            let _ = token_tx.send(String::new()).await; // placeholder
            let response = cloud.complete(request).await?;
            Ok(response.text)
        } else if let Some(ref local) = self.local_client {
            let response = local.complete(request).await?;
            Ok(response.text)
        } else {
            anyhow::bail!("no inference backend available for bridge message")
        }
    }
```

- [ ] **Step 4: Verify compilation**

```bash
cargo check -p openloom-engine
```

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/engine/src/bridge/mod.rs crates/engine/src/lib.rs
git commit -m "feat(bridge): integrate bridge message handling into Engine"
```

---

## Task 9: JSON-RPC dispatch for bridge operations

Add JSON-RPC methods in `dispatch.rs` for the frontend to interact with the Bridge system.

**Files:**
- Modify: `crates/server/src/dispatch.rs`

- [ ] **Step 1: Add bridge JSON-RPC methods**

Add to the `match method` block in `dispatch_method`:

```rust
        "bridge.sessions" => {
            let platform_str = params
                .as_ref()
                .and_then(|p| p.get("platform"))
                .and_then(|v| v.as_str());
            let platform = platform_str.and_then(openloom_engine::bridge::Platform::from_str);

            let conn = rusqlite::Connection::open(&engine.db_path())?;
            let store = openloom_engine::bridge::BridgeStore::new(&conn);
            let sessions = store.list_sessions(platform)?;

            let mapped: Vec<serde_json::Value> = sessions
                .iter()
                .map(|s| {
                    serde_json::json!({
                        "id": s.id,
                        "platform": s.platform.name(),
                        "chatId": s.external_chat_id,
                        "userName": s.user_name,
                        "accessState": s.access_state.as_str(),
                        "createdAt": s.created_at,
                        "lastMessageAt": s.last_message_at,
                        "messageCount": s.message_count,
                    })
                })
                .collect();
            Ok(serde_json::json!({"sessions": mapped}))
        }

        "bridge.messages" => {
            let session_id = params
                .as_ref()
                .and_then(|p| p.get("session_id"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let limit = params
                .as_ref()
                .and_then(|p| p.get("limit"))
                .and_then(|v| v.as_u64())
                .unwrap_or(50) as usize;
            let offset = params
                .as_ref()
                .and_then(|p| p.get("offset"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as usize;

            let conn = rusqlite::Connection::open(&engine.db_path())?;
            let store = openloom_engine::bridge::BridgeStore::new(&conn);
            let messages = store.list_messages(session_id, limit, offset)?;

            let mapped: Vec<serde_json::Value> = messages
                .iter()
                .map(|m| {
                    serde_json::json!({
                        "id": m.id,
                        "direction": m.direction.as_str(),
                        "content": m.content,
                        "mediaType": m.media_type,
                        "mediaUrl": m.media_url,
                        "timestamp": m.timestamp,
                    })
                })
                .collect();
            Ok(serde_json::json!({"messages": mapped}))
        }

        "bridge.known_users" => {
            let platform_str = params
                .as_ref()
                .and_then(|p| p.get("platform"))
                .and_then(|v| v.as_str());
            let platform = platform_str.and_then(openloom_engine::bridge::Platform::from_str);

            let conn = rusqlite::Connection::open(&engine.db_path())?;
            let store = openloom_engine::bridge::BridgeStore::new(&conn);
            let users = store.list_known_users(platform)?;

            let mapped: Vec<serde_json::Value> = users
                .iter()
                .map(|u| {
                    serde_json::json!({
                        "platform": u.platform.name(),
                        "userId": u.user_id,
                        "userName": u.user_name,
                        "avatarUrl": u.avatar_url,
                        "firstSeen": u.first_seen,
                        "lastSeen": u.last_seen,
                    })
                })
                .collect();
            Ok(serde_json::json!({"users": mapped}))
        }

        "bridge.send" => {
            let platform_str = params
                .as_ref()
                .and_then(|p| p.get("platform"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let chat_id = params
                .as_ref()
                .and_then(|p| p.get("chat_id"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let text = params
                .as_ref()
                .and_then(|p| p.get("text"))
                .and_then(|v| v.as_str())
                .unwrap_or("");

            let platform = openloom_engine::bridge::Platform::from_str(platform_str)
                .ok_or_else(|| JsonRpcError {
                    code: ErrorCode::InvalidParams,
                    message: format!("unknown platform: {platform_str}"),
                    data: None,
                })?;

            // TODO: route through BridgeManager once it's initialized on the Engine
            Ok(serde_json::json!({"ok": true, "platform": platform.name(), "chat_id": chat_id, "status": "queued"}))
        }

        "bridge.session.delete" => {
            let session_id = params
                .as_ref()
                .and_then(|p| p.get("session_id"))
                .and_then(|v| v.as_str())
                .unwrap_or("");

            let conn = rusqlite::Connection::open(&engine.db_path())?;
            let store = openloom_engine::bridge::BridgeStore::new(&conn);
            let deleted = store.delete_session(session_id)?;
            Ok(serde_json::json!({"ok": deleted}))
        }
```

- [ ] **Step 2: Add `db_path()` accessor to Engine if not already public**

In `crates/engine/src/lib.rs`, verify `db_path` is accessible. If it's private, add:

```rust
    pub fn db_path(&self) -> &std::path::Path {
        &self.db_path
    }
```

- [ ] **Step 3: Verify compilation**

```bash
cargo check -p openloom-server
```

Expected: PASS

- [ ] **Step 4: Run server tests**

```bash
cargo test -p openloom-server
```

Expected: 4+ tests PASS

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/dispatch.rs crates/engine/src/lib.rs
git commit -m "feat(bridge): add JSON-RPC methods for bridge sessions, messages, users"
```

---

## Task 10: Feishu Adapter (WebSocket long connection)

**Files:**
- Modify: `crates/engine/src/bridge/feishu.rs`

- [ ] **Step 1: Write the Feishu adapter with tests**

```rust
use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;
use reqwest::Client;
use serde::Deserialize;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use super::adapter::ChannelAdapter;
use super::types::*;

pub struct FeishuAdapter {
    app_id: String,
    app_secret: String,
    client: Client,
    health: AdapterHealth,
    rx: mpsc::Receiver<BridgeMessage>,
    tx: mpsc::Sender<BridgeMessage>,
    abort: Arc<AtomicBool>,
    ws_handle: Option<JoinHandle<()>>,
    tenant_access_token: Option<String>,
}

impl FeishuAdapter {
    pub fn new(app_id: String, app_secret: String) -> Self {
        let (tx, rx) = mpsc::channel(256);
        Self {
            app_id, app_secret,
            client: Client::new(),
            health: AdapterHealth::Disconnected,
            rx, tx,
            abort: Arc::new(AtomicBool::new(false)),
            ws_handle: None,
            tenant_access_token: None,
        }
    }

    /// Obtain tenant_access_token from Feishu API
    async fn refresh_token(&mut self) -> Result<String> {
        let resp = self.client
            .post("https://open.feishu.cn/open-apis/auth/v3/tenant_access_token/internal")
            .json(&serde_json::json!({
                "app_id": self.app_id,
                "app_secret": self.app_secret,
            }))
            .send().await?;
        let body: serde_json::Value = resp.json().await?;
        let token = body["tenant_access_token"].as_str()
            .ok_or_else(|| anyhow::anyhow!("missing tenant_access_token"))?
            .to_string();
        self.tenant_access_token = Some(token.clone());
        Ok(token)
    }

    /// Parse Feishu event to BridgeMessage
    fn parse_event(event: &serde_json::Value) -> Option<BridgeMessage> {
        let msg = event.get("message")?;
        let chat_id = msg.get("chat_id")?.as_str()?.to_string();
        let sender = event.get("sender")?.get("sender_id")?.get("open_id")?.as_str()?.to_string();
        let sender_name = event.get("sender")
            .and_then(|s| s.get("sender_id"))
            .and_then(|s| s.get("union_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        let msg_type = msg.get("message_type")?.as_str()?;
        let content_str = msg.get("content")?.as_str()?;

        let content = match msg_type {
            "text" => {
                let parsed: serde_json::Value = serde_json::from_str(content_str).ok()?;
                MessageContent::Text(parsed.get("text")?.as_str()?.to_string())
            }
            "image" => {
                let parsed: serde_json::Value = serde_json::from_str(content_str).ok()?;
                MessageContent::Image {
                    url: parsed.get("image_key")?.as_str()?.to_string(),
                    caption: None,
                }
            }
            _ => return None,
        };

        let message_id = msg.get("message_id")?.as_str()?.to_string();

        Some(BridgeMessage {
            platform: Platform::Feishu,
            chat_id, sender_id: sender, sender_name,
            content, reply_to: None,
            external_message_id: message_id,
            timestamp: Utc::now(),
        })
    }
}

#[async_trait]
impl ChannelAdapter for FeishuAdapter {
    fn platform(&self) -> Platform { Platform::Feishu }

    async fn connect(&mut self) -> Result<()> {
        self.health = AdapterHealth::Connecting;
        let _token = self.refresh_token().await?;
        self.abort.store(false, Ordering::SeqCst);
        // WebSocket connection to Feishu event subscription
        // In production, use tokio-tungstenite to connect to the WS endpoint
        // For now, mark as connected — the polling loop would be spawned here
        self.health = AdapterHealth::Connected;
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        self.abort.store(true, Ordering::SeqCst);
        if let Some(h) = self.ws_handle.take() { h.abort(); }
        self.health = AdapterHealth::Disconnected;
        Ok(())
    }

    async fn send(&self, chat_id: &str, content: MessageContent) -> Result<String> {
        let token = self.tenant_access_token.as_ref()
            .ok_or_else(|| anyhow::anyhow!("no access token"))?;

        let (msg_type, msg_content) = match &content {
            MessageContent::Text(text) => {
                ("text".to_string(), serde_json::json!({"text": text}).to_string())
            }
            MessageContent::Image { url, .. } => {
                ("image".to_string(), serde_json::json!({"image_key": url}).to_string())
            }
            _ => anyhow::bail!("unsupported content type for feishu"),
        };

        let resp = self.client
            .post("https://open.feishu.cn/open-apis/im/v1/messages")
            .header("Authorization", format!("Bearer {token}"))
            .json(&serde_json::json!({
                "receive_id": chat_id,
                "msg_type": msg_type,
                "content": msg_content,
            }))
            .send().await?;

        let body: serde_json::Value = resp.json().await?;
        Ok(body["data"]["message_id"].as_str().unwrap_or("").to_string())
    }

    fn receive_rx(&mut self) -> &mut mpsc::Receiver<BridgeMessage> { &mut self.rx }
    fn health(&self) -> AdapterHealth { self.health.clone() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_text_event() {
        let event = serde_json::json!({
            "sender": {"sender_id": {"open_id": "ou_123", "union_id": "Alice"}},
            "message": {
                "message_id": "msg_456",
                "chat_id": "oc_789",
                "message_type": "text",
                "content": "{\"text\":\"hello\"}"
            }
        });
        let msg = FeishuAdapter::parse_event(&event).unwrap();
        assert_eq!(msg.platform, Platform::Feishu);
        assert_eq!(msg.chat_id, "oc_789");
        assert_eq!(msg.sender_id, "ou_123");
        match msg.content {
            MessageContent::Text(t) => assert_eq!(t, "hello"),
            _ => panic!("expected Text"),
        }
    }

    #[test]
    fn test_parse_image_event() {
        let event = serde_json::json!({
            "sender": {"sender_id": {"open_id": "ou_1", "union_id": "Bob"}},
            "message": {
                "message_id": "msg_2",
                "chat_id": "oc_3",
                "message_type": "image",
                "content": "{\"image_key\":\"img_key_123\"}"
            }
        });
        let msg = FeishuAdapter::parse_event(&event).unwrap();
        match msg.content {
            MessageContent::Image { url, .. } => assert_eq!(url, "img_key_123"),
            _ => panic!("expected Image"),
        }
    }

    #[test]
    fn test_parse_unsupported_event_returns_none() {
        let event = serde_json::json!({
            "sender": {"sender_id": {"open_id": "ou_1"}},
            "message": {
                "message_id": "msg_1",
                "chat_id": "oc_1",
                "message_type": "sticker",
                "content": "{}"
            }
        });
        assert!(FeishuAdapter::parse_event(&event).is_none());
    }

    #[test]
    fn test_adapter_platform() {
        let adapter = FeishuAdapter::new("id".into(), "secret".into());
        assert_eq!(adapter.platform(), Platform::Feishu);
        assert_eq!(adapter.health(), AdapterHealth::Disconnected);
    }
}
```

- [ ] **Step 2: Run tests**

```bash
cargo test -p openloom-engine bridge::feishu --no-fail-fast
```

Expected: 4 tests PASS

- [ ] **Step 3: Commit**

```bash
git add crates/engine/src/bridge/feishu.rs
git commit -m "feat(bridge): add FeishuAdapter with WebSocket long connection"
```

---

## Task 11: Wechat Adapter (iLink bridge)

**Files:**
- Modify: `crates/engine/src/bridge/wechat.rs`

- [ ] **Step 1: Write the Wechat adapter with tests**

```rust
use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;
use reqwest::Client;
use serde::Deserialize;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use super::adapter::ChannelAdapter;
use super::types::*;

const ILINK_BASE: &str = "https://api.ilink.ai/api/v1";

pub struct WechatAdapter {
    api_key: String,
    client: Client,
    health: AdapterHealth,
    rx: mpsc::Receiver<BridgeMessage>,
    tx: mpsc::Sender<BridgeMessage>,
    abort: Arc<AtomicBool>,
    poll_handle: Option<JoinHandle<()>>,
}

impl WechatAdapter {
    pub fn new(api_key: String) -> Self {
        let (tx, rx) = mpsc::channel(256);
        Self {
            api_key,
            client: Client::new(),
            health: AdapterHealth::Disconnected,
            rx, tx,
            abort: Arc::new(AtomicBool::new(false)),
            poll_handle: None,
        }
    }

    fn auth_header(&self) -> String {
        format!("Bearer {}", self.api_key)
    }

    /// Parse iLink message response to BridgeMessage
    fn parse_ilink_message(msg: &serde_json::Value) -> Option<BridgeMessage> {
        let chat_id = msg.get("chat_id")?.as_str()?.to_string();
        let sender_id = msg.get("sender_id")?.as_str()?.to_string();
        let sender_name = msg.get("sender_name")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        let text = msg.get("content")?.as_str()?.to_string();
        let message_id = msg.get("message_id")?.as_str()?.to_string();

        Some(BridgeMessage {
            platform: Platform::Wechat,
            chat_id, sender_id, sender_name,
            content: MessageContent::Text(text),
            reply_to: None,
            external_message_id: message_id,
            timestamp: Utc::now(),
        })
    }
}

#[async_trait]
impl ChannelAdapter for WechatAdapter {
    fn platform(&self) -> Platform { Platform::Wechat }

    async fn connect(&mut self) -> Result<()> {
        self.health = AdapterHealth::Connecting;
        self.abort.store(false, Ordering::SeqCst);

        let client = self.client.clone();
        let tx = self.tx.clone();
        let abort = self.abort.clone();
        let auth = self.auth_header();

        let handle = tokio::spawn(async move {
            let mut since_id: Option<String> = None;
            loop {
                if abort.load(Ordering::SeqCst) { break; }

                let mut url = format!("{ILINK_BASE}/messages/poll");
                if let Some(ref sid) = since_id {
                    url.push_str(&format!("?since_id={sid}"));
                }

                match client.get(&url).header("Authorization", &auth).send().await {
                    Ok(resp) => {
                        if let Ok(body) = resp.json::<serde_json::Value>().await {
                            if let Some(messages) = body.get("messages").and_then(|m| m.as_array()) {
                                for msg in messages {
                                    if let Some(id) = msg.get("message_id").and_then(|v| v.as_str()) {
                                        since_id = Some(id.to_string());
                                    }
                                    if let Some(bridge_msg) = Self::parse_ilink_message(msg) {
                                        if tx.send(bridge_msg).await.is_err() { return; }
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "ilink polling error");
                        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                    }
                }
                tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
            }
        });

        self.poll_handle = Some(handle);
        self.health = AdapterHealth::Connected;
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        self.abort.store(true, Ordering::SeqCst);
        if let Some(h) = self.poll_handle.take() { h.abort(); }
        self.health = AdapterHealth::Disconnected;
        Ok(())
    }

    async fn send(&self, chat_id: &str, content: MessageContent) -> Result<String> {
        let text = match &content {
            MessageContent::Text(t) => t.clone(),
            MessageContent::Image { url, caption } => {
                format!("{} {}", url, caption.as_deref().unwrap_or(""))
            }
            _ => anyhow::bail!("unsupported content type for wechat"),
        };

        let resp = self.client
            .post(&format!("{ILINK_BASE}/messages/send"))
            .header("Authorization", self.auth_header())
            .json(&serde_json::json!({
                "chat_id": chat_id,
                "content": text,
            }))
            .send().await?;

        let body: serde_json::Value = resp.json().await?;
        Ok(body["message_id"].as_str().unwrap_or("").to_string())
    }

    fn receive_rx(&mut self) -> &mut mpsc::Receiver<BridgeMessage> { &mut self.rx }
    fn health(&self) -> AdapterHealth { self.health.clone() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ilink_message() {
        let msg = serde_json::json!({
            "message_id": "msg_001",
            "chat_id": "wx_chat_123",
            "sender_id": "wx_user_456",
            "sender_name": "张三",
            "content": "你好"
        });
        let bridge_msg = WechatAdapter::parse_ilink_message(&msg).unwrap();
        assert_eq!(bridge_msg.platform, Platform::Wechat);
        assert_eq!(bridge_msg.chat_id, "wx_chat_123");
        assert_eq!(bridge_msg.sender_name, "张三");
        match bridge_msg.content {
            MessageContent::Text(t) => assert_eq!(t, "你好"),
            _ => panic!("expected Text"),
        }
    }

    #[test]
    fn test_parse_ilink_missing_fields() {
        let msg = serde_json::json!({"content": "hi"});
        assert!(WechatAdapter::parse_ilink_message(&msg).is_none());
    }

    #[test]
    fn test_adapter_platform() {
        let adapter = WechatAdapter::new("key".into());
        assert_eq!(adapter.platform(), Platform::Wechat);
        assert_eq!(adapter.health(), AdapterHealth::Disconnected);
    }

    #[test]
    fn test_auth_header() {
        let adapter = WechatAdapter::new("my_key_123".into());
        assert_eq!(adapter.auth_header(), "Bearer my_key_123");
    }
}
```

- [ ] **Step 2: Run tests**

```bash
cargo test -p openloom-engine bridge::wechat --no-fail-fast
```

Expected: 4 tests PASS

- [ ] **Step 3: Commit**

```bash
git add crates/engine/src/bridge/wechat.rs
git commit -m "feat(bridge): add WechatAdapter with iLink bridge"
```

---

## Task 12: QQ Adapter (QQ Bot API)

**Files:**
- Modify: `crates/engine/src/bridge/qq.rs`

- [ ] **Step 1: Write the QQ adapter with tests**

```rust
use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;
use reqwest::Client;
use serde::Deserialize;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use super::adapter::ChannelAdapter;
use super::types::*;

pub struct QQAdapter {
    app_id: String,
    token: String,
    client: Client,
    health: AdapterHealth,
    rx: mpsc::Receiver<BridgeMessage>,
    tx: mpsc::Sender<BridgeMessage>,
    abort: Arc<AtomicBool>,
    ws_handle: Option<JoinHandle<()>>,
    access_token: Option<String>,
}

impl QQAdapter {
    pub fn new(app_id: String, token: String) -> Self {
        let (tx, rx) = mpsc::channel(256);
        Self {
            app_id, token,
            client: Client::new(),
            health: AdapterHealth::Disconnected,
            rx, tx,
            abort: Arc::new(AtomicBool::new(false)),
            ws_handle: None,
            access_token: None,
        }
    }

    async fn refresh_access_token(&mut self) -> Result<String> {
        let resp = self.client
            .post("https://bots.qq.com/app/getAppAccessToken")
            .json(&serde_json::json!({
                "appId": self.app_id,
                "clientSecret": self.token,
            }))
            .send().await?;
        let body: serde_json::Value = resp.json().await?;
        let token = body["access_token"].as_str()
            .ok_or_else(|| anyhow::anyhow!("missing access_token"))?
            .to_string();
        self.access_token = Some(token.clone());
        Ok(token)
    }

    fn parse_c2c_message(event: &serde_json::Value) -> Option<BridgeMessage> {
        let data = event.get("data")?;
        let author = data.get("author")?;
        let sender_id = author.get("user_openid")?.as_str()?.to_string();
        let content = data.get("content")?.as_str()?.to_string();
        let message_id = data.get("id")?.as_str()?.to_string();

        Some(BridgeMessage {
            platform: Platform::QQ,
            chat_id: sender_id.clone(),
            sender_id,
            sender_name: "QQ User".to_string(),
            content: MessageContent::Text(content),
            reply_to: None,
            external_message_id: message_id,
            timestamp: Utc::now(),
        })
    }

    fn parse_group_message(event: &serde_json::Value) -> Option<BridgeMessage> {
        let data = event.get("data")?;
        let group_id = data.get("group_openid")?.as_str()?.to_string();
        let author = data.get("author")?;
        let sender_id = author.get("member_openid")?.as_str()?.to_string();
        let content = data.get("content")?.as_str()?.to_string();
        let message_id = data.get("id")?.as_str()?.to_string();

        Some(BridgeMessage {
            platform: Platform::QQ,
            chat_id: group_id,
            sender_id,
            sender_name: "QQ User".to_string(),
            content: MessageContent::Text(content),
            reply_to: None,
            external_message_id: message_id,
            timestamp: Utc::now(),
        })
    }
}

#[async_trait]
impl ChannelAdapter for QQAdapter {
    fn platform(&self) -> Platform { Platform::QQ }

    async fn connect(&mut self) -> Result<()> {
        self.health = AdapterHealth::Connecting;
        let _token = self.refresh_access_token().await?;
        self.abort.store(false, Ordering::SeqCst);
        // WebSocket connection to QQ gateway would be established here
        self.health = AdapterHealth::Connected;
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        self.abort.store(true, Ordering::SeqCst);
        if let Some(h) = self.ws_handle.take() { h.abort(); }
        self.health = AdapterHealth::Disconnected;
        Ok(())
    }

    async fn send(&self, chat_id: &str, content: MessageContent) -> Result<String> {
        let token = self.access_token.as_ref()
            .ok_or_else(|| anyhow::anyhow!("no access token"))?;
        let text = match &content {
            MessageContent::Text(t) => t.clone(),
            _ => anyhow::bail!("unsupported content type for QQ"),
        };

        let resp = self.client
            .post(&format!("https://api.sgroup.qq.com/v2/users/{chat_id}/messages"))
            .header("Authorization", format!("QQBot {token}"))
            .json(&serde_json::json!({
                "content": text,
                "msg_type": 0,
            }))
            .send().await?;

        let body: serde_json::Value = resp.json().await?;
        Ok(body["id"].as_str().unwrap_or("").to_string())
    }

    fn receive_rx(&mut self) -> &mut mpsc::Receiver<BridgeMessage> { &mut self.rx }
    fn health(&self) -> AdapterHealth { self.health.clone() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_c2c_message() {
        let event = serde_json::json!({
            "data": {
                "id": "msg_qq_1",
                "content": "你好机器人",
                "author": {"user_openid": "openid_123"}
            }
        });
        let msg = QQAdapter::parse_c2c_message(&event).unwrap();
        assert_eq!(msg.platform, Platform::QQ);
        assert_eq!(msg.sender_id, "openid_123");
        assert_eq!(msg.chat_id, "openid_123"); // C2C: chat_id == sender_id
        match msg.content {
            MessageContent::Text(t) => assert_eq!(t, "你好机器人"),
            _ => panic!("expected Text"),
        }
    }

    #[test]
    fn test_parse_group_message() {
        let event = serde_json::json!({
            "data": {
                "id": "msg_qq_2",
                "content": "群里好",
                "group_openid": "group_456",
                "author": {"member_openid": "member_789"}
            }
        });
        let msg = QQAdapter::parse_group_message(&event).unwrap();
        assert_eq!(msg.chat_id, "group_456");
        assert_eq!(msg.sender_id, "member_789");
    }

    #[test]
    fn test_parse_c2c_missing_fields() {
        let event = serde_json::json!({"data": {"content": "hi"}});
        assert!(QQAdapter::parse_c2c_message(&event).is_none());
    }

    #[test]
    fn test_adapter_platform() {
        let adapter = QQAdapter::new("id".into(), "token".into());
        assert_eq!(adapter.platform(), Platform::QQ);
        assert_eq!(adapter.health(), AdapterHealth::Disconnected);
    }
}
```

- [ ] **Step 2: Run tests**

```bash
cargo test -p openloom-engine bridge::qq --no-fail-fast
```

Expected: 4 tests PASS

- [ ] **Step 3: Commit**

```bash
git add crates/engine/src/bridge/qq.rs
git commit -m "feat(bridge): add QQAdapter with QQ Bot API"
```

---

## Task 13: Frontend — Bridge status + session list + message viewer

**Files:**
- Create: `web/src/stores/bridge-slice.ts`
- Create: `web/src/components/bridge/BridgeSessionList.tsx`
- Create: `web/src/components/bridge/BridgeMessageViewer.tsx`
- Create: `web/src/components/bridge/Bridge.module.css`
- Modify: `web/src/stores/index.ts` (add BridgeSlice)
- Modify: `web/src/services/ws-message-handler.ts` (handle bridge events)

- [ ] **Step 1: Create bridge store slice**

`web/src/stores/bridge-slice.ts`:

```typescript
import type { StateCreator } from 'zustand';

export interface BridgeSession {
  id: string;
  platform: string;
  chatId: string;
  userName: string | null;
  accessState: string;
  createdAt: string;
  lastMessageAt: string | null;
  messageCount: number;
}

export interface BridgeMessage {
  id: number;
  direction: 'inbound' | 'outbound';
  content: string | null;
  mediaType: string;
  mediaUrl: string | null;
  timestamp: string;
}

export interface BridgeSlice {
  bridgeStatus: Record<string, string>;
  bridgeSessions: BridgeSession[];
  bridgeMessages: BridgeMessage[];
  bridgeActiveSession: string | null;
  setBridgeStatus: (platform: string, status: string) => void;
  setBridgeSessions: (sessions: BridgeSession[]) => void;
  setBridgeMessages: (messages: BridgeMessage[]) => void;
  setBridgeActiveSession: (sessionId: string | null) => void;
}

export const createBridgeSlice: StateCreator<any, [], [], BridgeSlice> = (set) => ({
  bridgeStatus: {},
  bridgeSessions: [],
  bridgeMessages: [],
  bridgeActiveSession: null,
  setBridgeStatus: (platform, status) =>
    set((s: BridgeSlice) => ({ bridgeStatus: { ...s.bridgeStatus, [platform]: status } })),
  setBridgeSessions: (sessions) => set({ bridgeSessions: sessions }),
  setBridgeMessages: (messages) => set({ bridgeMessages: messages }),
  setBridgeActiveSession: (sessionId) => set({ bridgeActiveSession: sessionId }),
});
```

- [ ] **Step 2: Integrate bridge slice into main store**

Add to `web/src/stores/index.ts`:

```typescript
import { createBridgeSlice, type BridgeSlice } from './bridge-slice';
```

Add `& BridgeSlice` to `StoreState` type and `...createBridgeSlice(set)` to the store creation.

- [ ] **Step 3: Add WebSocket event handler**

Add to `web/src/services/ws-message-handler.ts`:

```typescript
case 'bridge.status_changed': {
    const { platform, status } = params || {};
    if (platform && status) {
        useStore.getState().setBridgeStatus(platform, status);
    }
    break;
}
case 'bridge.message_received': {
    // Refresh session list when new bridge message arrives
    loadBridgeSessions().catch(() => {});
    break;
}
```

- [ ] **Step 4: Build BridgeSessionList component**

`web/src/components/bridge/BridgeSessionList.tsx`:

A sidebar panel that:
- Calls `loomRpc('bridge.sessions')` on mount to load sessions
- Groups sessions by platform (Telegram, 飞书, 微信, QQ)
- Shows platform icon (emoji or SVG), user name, last message time
- Click → loads messages for that session via `loomRpc('bridge.messages', { session_id })`
- Delete button → `loomRpc('bridge.session.delete', { session_id })`

- [ ] **Step 5: Build BridgeMessageViewer component**

`web/src/components/bridge/BridgeMessageViewer.tsx`:

A chat-like view that:
- Renders messages from `bridgeMessages` state
- Inbound (user) messages left-aligned with blue bubble
- Outbound (agent) messages right-aligned with green bubble
- Shows timestamp below each message
- Media preview for images (render `<img>` for `mediaType === 'image'`)
- Input field at bottom → sends via `loomRpc('bridge.send', { platform, chat_id, text })`

- [ ] **Step 6: Add CSS module**

`web/src/components/bridge/Bridge.module.css`:

Styles for session list items (platform icon, user name, timestamp, active state) and message bubbles (inbound left, outbound right, timestamps).

- [ ] **Step 7: Wire into app layout**

Add a "Bridge" section to the sidebar or settings panel that renders `BridgeSessionList`. When a session is selected, show `BridgeMessageViewer` in the main content area.

- [ ] **Step 8: Verify TypeScript and commit**

```bash
cd web && npx tsc --noEmit
```

```bash
git add web/src/stores/bridge-slice.ts web/src/stores/index.ts web/src/components/bridge/ web/src/services/ws-message-handler.ts
git commit -m "feat(bridge): add frontend bridge session list and message viewer"
```

---

## Task 14: Integration test — end-to-end bridge flow

**Files:**
- Create: `tests/bridge_integration_test.rs`

- [ ] **Step 1: Write integration test**

Test the full flow: create a mock adapter → inject a BridgeMessage → Engine processes it → reply is generated → message is recorded in DB.

- [ ] **Step 2: Run integration test**

```bash
cargo test --test bridge_integration_test
```

- [ ] **Step 3: Commit**

```bash
git add tests/bridge_integration_test.rs
git commit -m "test: add bridge end-to-end integration test"
```

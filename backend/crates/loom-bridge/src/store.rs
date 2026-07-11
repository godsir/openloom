//! BridgeStore — SQLite persistence for bridge sessions, messages, and known users.
//! Uses the V7 migration tables: bridge_sessions, bridge_messages, bridge_known_users.

use anyhow::Result;
use rusqlite::{Connection, params};

use crate::channel_config::InstanceConfig;
use crate::types::{AccessMode, Platform};

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

    // ── Channel Config Table ──────────────────────────────────────

    fn ensure_channel_configs_table(&self) -> Result<()> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS bridge_channels (
                 id              TEXT PRIMARY KEY,
                 platform        TEXT NOT NULL,
                 instance_id     TEXT NOT NULL,
                 instance_name   TEXT NOT NULL DEFAULT '',
                 enabled         INTEGER NOT NULL DEFAULT 0,
                 config_json     TEXT NOT NULL DEFAULT '{}',
                 dm_policy       TEXT NOT NULL DEFAULT 'pairing',
                 allow_from      TEXT NOT NULL DEFAULT '[]',
                 group_policy    TEXT NOT NULL DEFAULT 'disabled',
                 group_allow_from TEXT NOT NULL DEFAULT '[]',
                 agent_id        TEXT,
                 created_at      INTEGER NOT NULL DEFAULT (unixepoch()),
                 updated_at      INTEGER NOT NULL DEFAULT (unixepoch()),
                 UNIQUE(platform, instance_id)
             )",
            [],
        )?;
        Ok(())
    }

    pub fn list_channel_configs(&self) -> Result<Vec<InstanceConfig>> {
        self.ensure_channel_configs_table()?;
        let mut stmt = self.conn.prepare(
            "SELECT id, platform, instance_id, instance_name, enabled, config_json,
                    dm_policy, allow_from, group_policy, group_allow_from,
                    agent_id, created_at, updated_at
             FROM bridge_channels ORDER BY platform, instance_id",
        )?;
        let rows = stmt.query_map([], |row| {
            let platform_str: String = row.get(1)?;
            let platform: Platform = match platform_str.as_str() {
                "telegram" => Platform::Telegram,
                "feishu" => Platform::Feishu,
                "wechat" => Platform::Wechat,
                "wecom" => Platform::Wecom,
                "dingtalk" => Platform::Dingtalk,
                "qq" => Platform::QQ,
                "discord" => Platform::Discord,
                "popo" => Platform::Popo,
                _ => Platform::Telegram,
            };
            Ok(InstanceConfig {
                id: row.get(0)?,
                platform,
                instance_id: row.get(2)?,
                instance_name: row.get(3)?,
                enabled: row.get::<_, i32>(4)? != 0,
                config_json: serde_json::from_str(&row.get::<_, String>(5)?).unwrap_or_default(),
                dm_policy: {
                    let s: String = row.get(6)?;
                    match s.as_str() {
                        "open" => AccessMode::Open,
                        "pairing" => AccessMode::Pairing,
                        _ => AccessMode::Allowlist,
                    }
                },
                allow_from: serde_json::from_str(&row.get::<_, String>(7)?).unwrap_or_default(),
                group_policy: {
                    let s: String = row.get(8)?;
                    match s.as_str() {
                        "open" => AccessMode::Open,
                        "allowlist" => AccessMode::Allowlist,
                        _ => AccessMode::Disabled,
                    }
                },
                group_allow_from: serde_json::from_str(&row.get::<_, String>(9)?)
                    .unwrap_or_default(),
                agent_id: row.get(10)?,
                created_at: row.get(11)?,
                updated_at: row.get(12)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn upsert_channel_config(&self, config: &InstanceConfig) -> Result<()> {
        self.ensure_channel_configs_table()?;
        let platform_str = config.platform.name().to_string();
        self.conn.execute(
            "INSERT INTO bridge_channels (id, platform, instance_id, instance_name, enabled,
                 config_json, dm_policy, allow_from, group_policy, group_allow_from,
                 agent_id, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
             ON CONFLICT(platform, instance_id) DO UPDATE SET
                 instance_name = excluded.instance_name,
                 enabled = excluded.enabled,
                 config_json = excluded.config_json,
                 dm_policy = excluded.dm_policy,
                 allow_from = excluded.allow_from,
                 group_policy = excluded.group_policy,
                 group_allow_from = excluded.group_allow_from,
                 agent_id = excluded.agent_id,
                 updated_at = excluded.updated_at",
            rusqlite::params![
                config.id,
                platform_str,
                config.instance_id,
                config.instance_name,
                config.enabled as i32,
                serde_json::to_string(&config.config_json).unwrap_or_default(),
                config.dm_policy.to_string(),
                serde_json::to_string(&config.allow_from).unwrap_or_default(),
                config.group_policy.to_string(),
                serde_json::to_string(&config.group_allow_from).unwrap_or_default(),
                config.agent_id,
                config.created_at,
                config.updated_at,
            ],
        )?;
        Ok(())
    }

    pub fn delete_channel_config(&self, platform: &str, instance_id: &str) -> Result<()> {
        self.ensure_channel_configs_table()?;
        self.conn.execute(
            "DELETE FROM bridge_channels WHERE platform = ?1 AND instance_id = ?2",
            rusqlite::params![platform, instance_id],
        )?;
        Ok(())
    }

    // ── Channel Status Table ──────────────────────────────────────

    fn ensure_channel_status_table(&self) -> Result<()> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS bridge_channel_status (
                 channel_id      TEXT PRIMARY KEY REFERENCES bridge_channels(id),
                 connected       INTEGER NOT NULL DEFAULT 0,
                 started_at      INTEGER,
                 last_error      TEXT,
                 last_inbound_at INTEGER,
                 last_outbound_at INTEGER
             )",
            [],
        )?;
        Ok(())
    }

    pub fn update_channel_status(
        &self,
        channel_id: &str,
        connected: bool,
        error: Option<&str>,
        last_inbound: bool,
        last_outbound: bool,
    ) -> Result<()> {
        self.ensure_channel_status_table()?;
        let now = chrono::Utc::now().timestamp();
        self.conn.execute(
            "INSERT INTO bridge_channel_status (channel_id, connected, started_at, last_error, last_inbound_at, last_outbound_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(channel_id) DO UPDATE SET
                 connected = excluded.connected,
                 last_error = CASE WHEN ?4 IS NOT NULL THEN ?4 ELSE bridge_channel_status.last_error END,
                 last_inbound_at = CASE WHEN ?5 THEN ?3 ELSE bridge_channel_status.last_inbound_at END,
                 last_outbound_at = CASE WHEN ?6 THEN ?3 ELSE bridge_channel_status.last_outbound_at END",
            rusqlite::params![
                channel_id,
                connected as i32,
                now,
                error,
                last_inbound,
                last_outbound,
            ],
        )?;
        Ok(())
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

    #[test]
    fn channel_config_crud() {
        let conn = Connection::open_in_memory().unwrap();
        let store = BridgeStore::new(&conn);

        // Initially empty.
        let configs = store.list_channel_configs().unwrap();
        assert!(configs.is_empty());

        // Upsert a config.
        let cfg = InstanceConfig {
            id: "test-id-1".to_string(),
            platform: Platform::Telegram,
            instance_id: "default".to_string(),
            instance_name: "TestBot".to_string(),
            enabled: true,
            config_json: serde_json::json!({"token": "abc"}),
            dm_policy: AccessMode::Allowlist,
            allow_from: vec!["user1".to_string()],
            group_policy: AccessMode::Open,
            group_allow_from: vec![],
            agent_id: Some("main".to_string()),
            created_at: 1000,
            updated_at: 2000,
        };
        store.upsert_channel_config(&cfg).unwrap();

        // List — should have 1.
        let configs = store.list_channel_configs().unwrap();
        assert_eq!(configs.len(), 1);
        assert_eq!(configs[0].id, "test-id-1");
        assert_eq!(configs[0].platform, Platform::Telegram);
        assert_eq!(configs[0].instance_id, "default");
        assert_eq!(configs[0].dm_policy, AccessMode::Allowlist);
        assert_eq!(configs[0].group_policy, AccessMode::Open);
        assert_eq!(configs[0].agent_id.as_deref(), Some("main"));

        // Upsert overwrites on same (platform, instance_id).
        let cfg2 = InstanceConfig {
            id: "test-id-1".to_string(),
            platform: Platform::Telegram,
            instance_id: "default".to_string(),
            instance_name: "Renamed".to_string(),
            enabled: false,
            config_json: serde_json::json!({}),
            dm_policy: AccessMode::Open,
            allow_from: vec![],
            group_policy: AccessMode::Disabled,
            group_allow_from: vec![],
            agent_id: None,
            created_at: 1000,
            updated_at: 3000,
        };
        store.upsert_channel_config(&cfg2).unwrap();
        let configs = store.list_channel_configs().unwrap();
        assert_eq!(configs.len(), 1, "still one row after upsert");
        assert_eq!(configs[0].instance_name, "Renamed");
        assert_eq!(configs[0].enabled, false);
        assert_eq!(configs[0].updated_at, 3000);

        // Add a second config on another platform.
        let cfg3 = InstanceConfig {
            id: "test-id-2".to_string(),
            platform: Platform::Feishu,
            instance_id: "work".to_string(),
            instance_name: "FeishuBot".to_string(),
            enabled: true,
            config_json: serde_json::json!({}),
            dm_policy: AccessMode::Pairing,
            allow_from: vec![],
            group_policy: AccessMode::Allowlist,
            group_allow_from: vec!["g1".to_string()],
            agent_id: None,
            created_at: 4000,
            updated_at: 5000,
        };
        store.upsert_channel_config(&cfg3).unwrap();
        let configs = store.list_channel_configs().unwrap();
        assert_eq!(configs.len(), 2);

        // Delete the first one.
        store.delete_channel_config("telegram", "default").unwrap();
        let configs = store.list_channel_configs().unwrap();
        assert_eq!(configs.len(), 1);
        assert_eq!(configs[0].platform, Platform::Feishu);
    }

    #[test]
    fn channel_status_update() {
        let conn = Connection::open_in_memory().unwrap();
        let store = BridgeStore::new(&conn);

        // First create a channel config so the FK is satisfied.
        let cfg = InstanceConfig {
            id: "status-test-id".to_string(),
            platform: Platform::Telegram,
            instance_id: "default".to_string(),
            instance_name: "StatusBot".to_string(),
            enabled: true,
            config_json: serde_json::json!({}),
            dm_policy: AccessMode::Open,
            allow_from: vec![],
            group_policy: AccessMode::Disabled,
            group_allow_from: vec![],
            agent_id: None,
            created_at: 1000,
            updated_at: 2000,
        };
        store.upsert_channel_config(&cfg).unwrap();

        // Update status — connected, inbound event.
        store
            .update_channel_status("status-test-id", true, None, true, false)
            .unwrap();

        // Update status again — error, outbound event.
        store
            .update_channel_status("status-test-id", false, Some("timeout"), false, true)
            .unwrap();
    }
}

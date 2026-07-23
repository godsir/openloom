//! SQLite-backed event store with FTS5 full-text search.
//! Ported from crates/memory/src/store.rs with loom-types compatibility.

use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

// ============================================================================
// Row types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventRow {
    pub id: i64,
    pub timestamp: String,
    pub event_type: String,
    pub action: String,
    pub context: String,
    pub confidence: f64,
    pub source_session: Option<String>,
    pub source_text: String,
}

/// Simple event struct for inserting into the store.
#[derive(Debug, Clone)]
pub struct NewEvent {
    pub timestamp: DateTime<Utc>,
    pub event_type: String,
    pub action: String,
    pub context: String,
    pub confidence: f64,
    pub source_session: Option<String>,
    pub source_text: String,
    pub payload: Option<serde_json::Value>,
}

#[derive(Debug, Clone)]
pub struct CognitionRow {
    pub id: i64,
    pub subject: String,
    pub trait_name: String,
    pub value: String,
    pub confidence: f64,
    pub evidence_count: usize,
    pub first_seen: i64,
    pub last_updated: i64,
    pub version: i64,
    pub scope: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CognitionSnapshot {
    pub id: i64,
    pub cognition_id: i64,
    pub version: i64,
    pub trait_name: String,
    pub value: String,
    pub confidence: f64,
    pub evidence_count: usize,
    pub snapshot_at: i64,
}

// ============================================================================
// CognitionStore — versioned trait storage
// ============================================================================

pub struct CognitionStore<'a> {
    conn: &'a Connection,
}

impl<'a> CognitionStore<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        let store = Self { conn };
        let _ = store.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS cognition_snapshots (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                cognition_id INTEGER NOT NULL,
                version INTEGER NOT NULL,
                trait TEXT NOT NULL,
                value TEXT NOT NULL,
                confidence REAL,
                evidence_count INTEGER,
                snapshot_at INTEGER NOT NULL,
                FOREIGN KEY (cognition_id) REFERENCES cognitions(id)
            );",
        );
        store
    }

    pub fn insert(
        &self,
        subject: &str,
        trait_name: &str,
        value: &str,
        confidence: f64,
        evidence_count: usize,
        scope: &str,
    ) -> Result<i64> {
        let now = Utc::now().timestamp();
        let existing: Option<(i64, i64)> = self.conn.query_row(
            "SELECT id, version FROM cognitions WHERE subject = ?1 AND trait = ?2 AND scope = ?3",
            params![subject, trait_name, scope],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ).ok();

        if let Some((id, existing_version)) = existing {
            let old_value: String = self.conn.query_row(
                "SELECT value FROM cognitions WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )?;
            self.conn.execute(
                "INSERT INTO cognition_snapshots (cognition_id, version, trait, value, confidence, evidence_count, snapshot_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![id, existing_version, trait_name, old_value, confidence, evidence_count, now],
            )?;
            self.conn.execute(
                "UPDATE cognitions SET value = ?1, confidence = ?2, evidence_count = evidence_count + 1, last_updated = ?4, version = ?5 WHERE id = ?6",
                params![value, confidence, now, existing_version + 1, id],
            )?;
            Ok(id)
        } else {
            self.conn.execute(
                "INSERT INTO cognitions (subject, trait, value, confidence, evidence_count, first_seen, last_updated, version, scope)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1, ?8)",
                params![subject, trait_name, value, confidence, evidence_count, now, now, scope],
            )?;
            Ok(self.conn.last_insert_rowid())
        }
    }

    pub fn query_by_subject(
        &self,
        subject: &str,
        scope: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<CognitionRow>> {
        let sql = if scope.is_some() {
            "SELECT id, subject, trait, value, confidence, evidence_count, first_seen, last_updated, version, scope
             FROM cognitions WHERE subject = ?1 AND scope = ?4 ORDER BY last_updated DESC LIMIT ?2 OFFSET ?3"
        } else {
            "SELECT id, subject, trait, value, confidence, evidence_count, first_seen, last_updated, version, scope
             FROM cognitions WHERE subject = ?1 ORDER BY last_updated DESC LIMIT ?2 OFFSET ?3"
        };
        let mut stmt = self.conn.prepare(sql)?;
        let map_row = |row: &rusqlite::Row<'_>| {
            Ok(CognitionRow {
                id: row.get(0)?,
                subject: row.get(1)?,
                trait_name: row.get(2)?,
                value: row.get(3)?,
                confidence: row.get(4)?,
                evidence_count: row.get(5)?,
                first_seen: row.get(6)?,
                last_updated: row.get(7)?,
                version: row.get(8)?,
                scope: row.get(9)?,
            })
        };
        let rows = if let Some(s) = scope {
            stmt.query_map(params![subject, limit as i64, offset as i64, s], map_row)?
        } else {
            stmt.query_map(params![subject, limit as i64, offset as i64], map_row)?
        };
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn list_subjects(&self) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT DISTINCT subject FROM cognitions ORDER BY subject")?;
        let rows = stmt.query_map([], |row| row.get(0))?;
        Ok(rows.collect::<std::result::Result<Vec<String>, _>>()?)
    }

    pub fn delete(&self, id: i64) -> Result<bool> {
        // Cascade: delete snapshots first (FK without ON DELETE CASCADE)
        self.conn.execute(
            "DELETE FROM cognition_snapshots WHERE cognition_id = ?1",
            params![id],
        )?;
        let affected = self
            .conn
            .execute("DELETE FROM cognitions WHERE id = ?1", params![id])?;
        Ok(affected > 0)
    }

    /// Promote cognitions with the given scope to "global" where confidence >= threshold.
    /// Returns the number of rows promoted.
    pub fn promote_to_global(&self, scope: &str, min_confidence: f64) -> Result<usize> {
        // Update scope to 'global' for cognitions that don't already have a global duplicate
        let promoted = self.conn.execute(
            "UPDATE cognitions SET scope = 'global'
             WHERE scope = ?1 AND confidence >= ?2
             AND (subject || '|' || trait) NOT IN
                 (SELECT subject || '|' || trait FROM cognitions WHERE scope = 'global')",
            params![scope, min_confidence],
        )?;
        // Delete remaining session-scoped cognitions
        self.conn
            .execute("DELETE FROM cognitions WHERE scope = ?1", params![scope])?;
        Ok(promoted)
    }

    /// Promote specific cognitions by ID to global scope (no deletion of others).
    /// Used for selective promotion from the UI.
    pub fn promote_cognitions_by_id(&self, ids: &[i64]) -> Result<usize> {
        let mut count = 0;
        for id in ids {
            count += self.conn.execute(
                "UPDATE cognitions SET scope = 'global' WHERE id = ?1 AND scope != 'global'",
                params![id],
            )?;
        }
        Ok(count)
    }

    /// Delete all cognitions with a given scope.
    pub fn delete_by_scope(&self, scope: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM cognitions WHERE scope = ?1", params![scope])?;
        Ok(())
    }

    pub fn snapshots_for(&self, cognition_id: i64) -> Result<Vec<CognitionSnapshot>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, cognition_id, version, trait, value, confidence, evidence_count, snapshot_at
             FROM cognition_snapshots WHERE cognition_id = ?1 ORDER BY version DESC",
        )?;
        let rows = stmt.query_map(params![cognition_id], |row| {
            Ok(CognitionSnapshot {
                id: row.get(0)?,
                cognition_id: row.get(1)?,
                version: row.get(2)?,
                trait_name: row.get(3)?,
                value: row.get(4)?,
                confidence: row.get(5)?,
                evidence_count: row.get(6)?,
                snapshot_at: row.get(7)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }
}

// ============================================================================
// AgentConfigStore — named agent profile CRUD
// ============================================================================

pub struct AgentConfigStore<'a> {
    conn: &'a Connection,
}

impl<'a> AgentConfigStore<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn upsert(&self, config: &loom_types::AgentConfig) -> Result<()> {
        let allowed_json = serde_json::to_string(&config.allowed_tools).ok();
        let disallowed_json = serde_json::to_string(&config.disallowed_tools).ok();
        self.conn.execute(
            "INSERT OR REPLACE INTO agent_configs
             (name, avatar, persona, system_prompt_override, model, thinking_level,
              temperature, tool_scope, allowed_tools, disallowed_tools,
              max_iterations, timeout_secs, max_concurrent_subagents,
              is_primary, memory_enabled, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, datetime('now'))",
            params![
                config.name,
                config.avatar,
                config.persona,
                config.system_prompt_override,
                config.model,
                config.thinking_level,
                config.temperature,
                config.tool_scope,
                allowed_json,
                disallowed_json,
                config.max_iterations,
                config.timeout_secs,
                config.max_concurrent_subagents as i64,
                config.is_primary as i64,
                config.memory_enabled as i64,
            ],
        )?;
        Ok(())
    }

    pub fn get(&self, name: &str) -> Result<Option<loom_types::AgentConfig>> {
        let mut stmt = self.conn.prepare(
            "SELECT name, avatar, persona, system_prompt_override, model, thinking_level,
                    temperature, tool_scope, allowed_tools, disallowed_tools,
                    max_iterations, timeout_secs, max_concurrent_subagents,
                    is_primary, memory_enabled
             FROM agent_configs WHERE name = ?1",
        )?;
        let row = stmt
            .query_row(params![name], |row| {
                let allowed_s: Option<String> = row.get(8)?;
                let disallowed_s: Option<String> = row.get(9)?;
                Ok(loom_types::AgentConfig {
                    name: row.get(0)?,
                    avatar: row.get(1)?,
                    persona: row.get::<_, String>(2).unwrap_or_default(),
                    system_prompt_override: row.get(3)?,
                    model: row.get(4)?,
                    thinking_level: row.get(5)?,
                    temperature: row.get(6)?,
                    tool_scope: row.get(7)?,
                    allowed_tools: allowed_s.and_then(|s| serde_json::from_str(&s).ok()),
                    disallowed_tools: disallowed_s.and_then(|s| serde_json::from_str(&s).ok()),
                    max_iterations: row.get(10)?,
                    timeout_secs: row.get(11)?,
                    max_concurrent_subagents: row.get::<_, i64>(12).unwrap_or(5) as usize,
                    is_primary: row.get::<_, i64>(13).unwrap_or(0) != 0,
                    memory_enabled: row.get::<_, i64>(14).unwrap_or(0) != 0,
                    cc_dispatch: false,
                    max_subagent_iterations: None,
                    max_subagent_retries: None,
                    auto_continue: true,
                    auto_continue_max_rounds: 10,
                })
            })
            .ok();
        Ok(row)
    }

    pub fn list(&self) -> Result<Vec<loom_types::AgentConfig>> {
        let mut stmt = self.conn.prepare(
            "SELECT name, avatar, persona, system_prompt_override, model, thinking_level,
                    temperature, tool_scope, allowed_tools, disallowed_tools,
                    max_iterations, timeout_secs, max_concurrent_subagents,
                    is_primary, memory_enabled
             FROM agent_configs ORDER BY name",
        )?;
        let rows = stmt.query_map([], |row| {
            let allowed_s: Option<String> = row.get(8)?;
            let disallowed_s: Option<String> = row.get(9)?;
            Ok(loom_types::AgentConfig {
                name: row.get(0)?,
                avatar: row.get(1)?,
                persona: row.get::<_, String>(2).unwrap_or_default(),
                system_prompt_override: row.get(3)?,
                model: row.get(4)?,
                thinking_level: row.get(5)?,
                temperature: row.get(6)?,
                tool_scope: row.get(7)?,
                allowed_tools: allowed_s.and_then(|s| serde_json::from_str(&s).ok()),
                disallowed_tools: disallowed_s.and_then(|s| serde_json::from_str(&s).ok()),
                max_iterations: row.get(10)?,
                timeout_secs: row.get(11)?,
                max_concurrent_subagents: row.get::<_, i64>(12).unwrap_or(5) as usize,
                is_primary: row.get::<_, i64>(13).unwrap_or(0) != 0,
                memory_enabled: row.get::<_, i64>(14).unwrap_or(0) != 0,
                cc_dispatch: false,
                max_subagent_iterations: None,
                max_subagent_retries: None,
                auto_continue: true,
                auto_continue_max_rounds: 10,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn delete(&self, name: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM agent_configs WHERE name = ?1", params![name])?;
        Ok(())
    }

    pub fn set_session_binding(&self, session_id: &str, config_name: &str) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO sessions (id, created_at, message_count) VALUES (?1, datetime('now'), 0)",
            params![session_id],
        )?;
        self.conn.execute(
            "UPDATE sessions SET agent_config_name = ?1 WHERE id = ?2",
            params![config_name, session_id],
        )?;
        Ok(())
    }

    pub fn get_session_binding(&self, session_id: &str) -> Result<Option<String>> {
        let row: Option<String> = self
            .conn
            .query_row(
                "SELECT agent_config_name FROM sessions WHERE id = ?1",
                params![session_id],
                |row| row.get(0),
            )
            .ok();
        Ok(row)
    }

    pub fn set_session_team_binding(&self, session_id: &str, team_id: &str) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO sessions (id, created_at, message_count) VALUES (?1, datetime('now'), 0)",
            params![session_id],
        )?;
        self.conn.execute(
            "UPDATE sessions SET team_config_id = ?1 WHERE id = ?2",
            params![team_id, session_id],
        )?;
        Ok(())
    }

    pub fn get_session_team_binding(&self, session_id: &str) -> Result<Option<String>> {
        let row: Option<String> = self
            .conn
            .query_row(
                "SELECT team_config_id FROM sessions WHERE id = ?1",
                params![session_id],
                |row| row.get(0),
            )
            .ok();
        Ok(row)
    }

    pub fn set_session_workspace(&self, session_id: &str, workspace_path: &str) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO sessions (id, created_at, message_count) VALUES (?1, datetime('now'), 0)",
            params![session_id],
        )?;
        self.conn.execute(
            "UPDATE sessions SET workspace_path = ?1 WHERE id = ?2",
            params![workspace_path, session_id],
        )?;
        Ok(())
    }

    pub fn get_session_workspace(&self, session_id: &str) -> Result<Option<String>> {
        let row: Option<String> = self
            .conn
            .query_row(
                "SELECT workspace_path FROM sessions WHERE id = ?1",
                params![session_id],
                |row| row.get(0),
            )
            .ok();
        Ok(row)
    }
}

// ============================================================================
// ModelConfigStore — named model profile CRUD
// ============================================================================

pub struct ModelConfigStore<'a> {
    conn: &'a Connection,
}

impl<'a> ModelConfigStore<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn upsert(&self, config: &loom_types::ModelConfig) -> Result<()> {
        let caps_json = serde_json::to_string(&config.capabilities).unwrap_or_default();
        self.conn.execute(
            "INSERT INTO model_configs
             (name, model, model_type, backend, base_url, api_key_env,
              context_size, max_output_tokens, backend_label, capabilities, api_format,
              input_price, output_price, cache_read_price, cache_write_price, compact_mode, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, datetime('now'))
             ON CONFLICT(name) DO UPDATE SET
               model = excluded.model,
               model_type = excluded.model_type,
               backend = excluded.backend,
               base_url = excluded.base_url,
               api_key_env = excluded.api_key_env,
               context_size = excluded.context_size,
               max_output_tokens = excluded.max_output_tokens,
               backend_label = excluded.backend_label,
               capabilities = excluded.capabilities,
               api_format = excluded.api_format,
               input_price = excluded.input_price,
               output_price = excluded.output_price,
               cache_read_price = excluded.cache_read_price,
               cache_write_price = excluded.cache_write_price,
               compact_mode = excluded.compact_mode,
               updated_at = datetime('now')",
            params![
                config.name,
                config.model,
                serde_json::to_string(&config.model_type)
                    .ok()
                    .map(|s| s.trim_matches('"').to_string()),
                serde_json::to_string(&config.backend)
                    .ok()
                    .map(|s| s.trim_matches('"').to_string()),
                config.base_url,
                config.api_key_env,
                config.context_size as i64,
                config.max_output_tokens.map(|v| v as i64),
                config.backend_label,
                caps_json,
                config.api_format,
                config.input_price,
                config.output_price,
                config.cache_read_price,
                config.cache_write_price,
                config.compact_mode,
            ],
        )?;
        Ok(())
    }

    pub fn get(&self, name: &str) -> Result<Option<loom_types::ModelConfig>> {
        let mut stmt = self.conn.prepare(
            "SELECT name, model, model_type, backend, base_url, api_key_env,
                    context_size, max_output_tokens, is_active, backend_label, capabilities, api_format,
                    input_price, output_price, cache_read_price, cache_write_price, compact_mode
             FROM model_configs WHERE name = ?1",
        )?;
        let row = stmt
            .query_row(params![name], |row| {
                let model_type_str: String = row.get::<_, String>(2).unwrap_or_default();
                let backend_str: String = row.get::<_, String>(3).unwrap_or_default();
                let caps_str: String = row.get::<_, String>(10).unwrap_or_default();
                let caps = serde_json::from_str(&caps_str).unwrap_or_default();
                Ok(loom_types::ModelConfig {
                    name: row.get(0)?,
                    model: row.get(1)?,
                    model_type: serde_json::from_str(&format!("\"{}\"", model_type_str))
                        .unwrap_or_default(),
                    backend: serde_json::from_str(&format!("\"{}\"", backend_str))
                        .unwrap_or_default(),
                    base_url: row.get(4)?,
                    api_key_env: row.get(5)?,
                    context_size: row.get::<_, i64>(6).unwrap_or(4096) as usize,
                    max_output_tokens: row
                        .get::<_, Option<i64>>(7)
                        .ok()
                        .flatten()
                        .map(|v| v as usize),
                    path: None,
                    n_gpu_layers: 0,
                    backend_label: row.get(9).ok().flatten(),
                    capabilities: caps,
                    api_format: row.get(11).ok().flatten(),
                    input_price: row.get::<_, f64>(12).unwrap_or(0.0),
                    output_price: row.get::<_, f64>(13).unwrap_or(0.0),
                    cache_read_price: row.get::<_, f64>(14).unwrap_or(0.0),
                    cache_write_price: row.get::<_, f64>(15).unwrap_or(0.0),
                    compact_mode: row.get::<_, i64>(16).unwrap_or(0) != 0,
                })
            })
            .ok();
        Ok(row)
    }

    pub fn list(&self) -> Result<Vec<loom_types::ModelConfig>> {
        let mut stmt = self.conn.prepare(
            "SELECT name, model, model_type, backend, base_url, api_key_env,
                    context_size, max_output_tokens, is_active, backend_label, capabilities, api_format,
                    input_price, output_price, cache_read_price, cache_write_price, compact_mode
             FROM model_configs ORDER BY name",
        )?;
        let rows = stmt.query_map([], |row| {
            let model_type_str: String = row.get::<_, String>(2).unwrap_or_default();
            let backend_str: String = row.get::<_, String>(3).unwrap_or_default();
            let caps_str: String = row.get::<_, String>(10).unwrap_or_default();
            let caps = serde_json::from_str(&caps_str).unwrap_or_default();
            Ok(loom_types::ModelConfig {
                name: row.get(0)?,
                model: row.get(1)?,
                model_type: serde_json::from_str(&format!("\"{}\"", model_type_str))
                    .unwrap_or_default(),
                backend: serde_json::from_str(&format!("\"{}\"", backend_str)).unwrap_or_default(),
                backend_label: row.get(9).ok().flatten(),
                base_url: row.get(4)?,
                api_key_env: row.get(5)?,
                context_size: row.get::<_, i64>(6).unwrap_or(4096) as usize,
                max_output_tokens: row
                    .get::<_, Option<i64>>(7)
                    .ok()
                    .flatten()
                    .map(|v| v as usize),
                path: None,
                n_gpu_layers: 0,
                capabilities: caps,
                api_format: row.get(11).ok().flatten(),
                input_price: row.get::<_, f64>(12).unwrap_or(0.0),
                output_price: row.get::<_, f64>(13).unwrap_or(0.0),
                cache_read_price: row.get::<_, f64>(14).unwrap_or(0.0),
                cache_write_price: row.get::<_, f64>(15).unwrap_or(0.0),
                compact_mode: row.get::<_, i64>(16).unwrap_or(0) != 0,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn delete(&self, name: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM model_configs WHERE name = ?1", params![name])?;
        Ok(())
    }

    pub fn set_active(&self, name: &str) -> Result<()> {
        self.conn
            .execute("UPDATE model_configs SET is_active = 0", [])?;
        self.conn.execute(
            "UPDATE model_configs SET is_active = 1, updated_at = datetime('now') WHERE name = ?1",
            params![name],
        )?;
        Ok(())
    }

    pub fn get_active(&self) -> Result<Option<loom_types::ModelConfig>> {
        let mut stmt = self.conn.prepare(
            "SELECT name, model, model_type, backend, base_url, api_key_env,
                    context_size, max_output_tokens, is_active, backend_label, capabilities, api_format,
                    input_price, output_price, cache_read_price, cache_write_price, compact_mode
             FROM model_configs WHERE is_active = 1 LIMIT 1",
        )?;
        let row = stmt
            .query_row([], |row| {
                let model_type_str: String = row.get::<_, String>(2).unwrap_or_default();
                let backend_str: String = row.get::<_, String>(3).unwrap_or_default();
                let caps_str: String = row.get::<_, String>(10).unwrap_or_default();
                let caps = serde_json::from_str(&caps_str).unwrap_or_default();
                Ok(loom_types::ModelConfig {
                    name: row.get(0)?,
                    model: row.get(1)?,
                    model_type: serde_json::from_str(&format!("\"{}\"", model_type_str))
                        .unwrap_or_default(),
                    backend: serde_json::from_str(&format!("\"{}\"", backend_str))
                        .unwrap_or_default(),
                    base_url: row.get(4)?,
                    api_key_env: row.get(5)?,
                    context_size: row.get::<_, i64>(6).unwrap_or(4096) as usize,
                    max_output_tokens: row
                        .get::<_, Option<i64>>(7)
                        .ok()
                        .flatten()
                        .map(|v| v as usize),
                    path: None,
                    n_gpu_layers: 0,
                    backend_label: row.get(9).ok().flatten(),
                    capabilities: caps,
                    api_format: row.get(11).ok().flatten(),
                    input_price: row.get::<_, f64>(12).unwrap_or(0.0),
                    output_price: row.get::<_, f64>(13).unwrap_or(0.0),
                    cache_read_price: row.get::<_, f64>(14).unwrap_or(0.0),
                    cache_write_price: row.get::<_, f64>(15).unwrap_or(0.0),
                    compact_mode: row.get::<_, i64>(16).unwrap_or(0) != 0,
                })
            })
            .ok();
        Ok(row)
    }
}

// ============================================================================
// McpConfigStore — persisted MCP server profile CRUD
// ============================================================================

/// Wire shape for persisted MCP server rows. The store is intentionally type-
/// agnostic about `McpServerConfig` (which lives in `loom-mcp`) — callers
/// serialise the live config into this shape and back, keeping `loom-memory`
/// free of higher-level crate deps.
#[derive(Debug, Clone)]
pub struct McpServerRow {
    pub name: String,
    pub transport: String,
    pub command: String,
    pub args_json: String,
    pub url: Option<String>,
    pub headers_json: String,
    pub env_json: String,
    pub cwd: Option<String>,
    pub startup_timeout_secs: u64,
    pub tool_timeout_secs: u64,
    pub enabled_tools_json: Option<String>,
    pub disabled_tools_json: Option<String>,
    pub autostart: bool,
}

pub struct McpConfigStore<'a> {
    conn: &'a Connection,
}

impl<'a> McpConfigStore<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn upsert(&self, row: &McpServerRow) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO mcp_servers
             (name, transport, command, args_json, url, headers_json, env_json, cwd,
              startup_timeout_secs, tool_timeout_secs, enabled_tools_json, disabled_tools_json,
              autostart, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, datetime('now'))",
            params![
                row.name,
                row.transport,
                row.command,
                row.args_json,
                row.url,
                row.headers_json,
                row.env_json,
                row.cwd,
                row.startup_timeout_secs as i64,
                row.tool_timeout_secs as i64,
                row.enabled_tools_json,
                row.disabled_tools_json,
                row.autostart as i64,
            ],
        )?;
        Ok(())
    }

    pub fn list(&self) -> Result<Vec<McpServerRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT name, transport, command, args_json, url, headers_json, env_json, cwd,
                    startup_timeout_secs, tool_timeout_secs, enabled_tools_json, disabled_tools_json,
                    autostart
             FROM mcp_servers ORDER BY name",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(McpServerRow {
                name: row.get(0)?,
                transport: row.get(1)?,
                command: row.get(2)?,
                args_json: row.get(3)?,
                url: row.get(4)?,
                headers_json: row.get(5)?,
                env_json: row.get(6)?,
                cwd: row.get(7)?,
                startup_timeout_secs: row.get::<_, i64>(8).unwrap_or(30) as u64,
                tool_timeout_secs: row.get::<_, i64>(9).unwrap_or(60) as u64,
                enabled_tools_json: row.get(10)?,
                disabled_tools_json: row.get(11)?,
                autostart: row.get::<_, i64>(12).unwrap_or(1) != 0,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn get(&self, name: &str) -> Result<Option<McpServerRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT name, transport, command, args_json, url, headers_json, env_json, cwd,
                    startup_timeout_secs, tool_timeout_secs, enabled_tools_json, disabled_tools_json,
                    autostart
             FROM mcp_servers WHERE name = ?1",
        )?;
        let row = stmt
            .query_row(params![name], |row| {
                Ok(McpServerRow {
                    name: row.get(0)?,
                    transport: row.get(1)?,
                    command: row.get(2)?,
                    args_json: row.get(3)?,
                    url: row.get(4)?,
                    headers_json: row.get(5)?,
                    env_json: row.get(6)?,
                    cwd: row.get(7)?,
                    startup_timeout_secs: row.get::<_, i64>(8).unwrap_or(30) as u64,
                    tool_timeout_secs: row.get::<_, i64>(9).unwrap_or(60) as u64,
                    enabled_tools_json: row.get(10)?,
                    disabled_tools_json: row.get(11)?,
                    autostart: row.get::<_, i64>(12).unwrap_or(1) != 0,
                })
            })
            .ok();
        Ok(row)
    }

    pub fn delete(&self, name: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM mcp_servers WHERE name = ?1", params![name])?;
        Ok(())
    }
}

// ============================================================================
// TeamConfigStore — expert-team config CRUD
// ============================================================================

pub struct TeamConfigStore<'a> {
    conn: &'a Connection,
}

impl<'a> TeamConfigStore<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// 保存团队配置（INSERT OR REPLACE 语义）
    pub fn save_team_config(&self, config: &loom_types::TeamConfig) -> Result<()> {
        let config_json = serde_json::to_string(config)?;
        self.conn.execute(
            "INSERT OR REPLACE INTO team_configs (id, name, config_json, updated_at)
             VALUES (?1, ?2, ?3, datetime('now'))",
            params![config.id, config.name, config_json],
        )?;
        Ok(())
    }

    /// 获取单个团队配置
    pub fn get_team_config(&self, id: &str) -> Result<Option<loom_types::TeamConfig>> {
        let mut stmt = self
            .conn
            .prepare("SELECT config_json FROM team_configs WHERE id = ?1")?;
        let result: Option<String> = stmt.query_row(params![id], |row| row.get(0)).optional()?;
        match result {
            Some(json) => Ok(Some(serde_json::from_str(&json)?)),
            None => Ok(None),
        }
    }

    /// 列出所有团队配置
    pub fn list_team_configs(&self) -> Result<Vec<loom_types::TeamConfig>> {
        let mut stmt = self
            .conn
            .prepare("SELECT config_json FROM team_configs ORDER BY name")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut configs = Vec::new();
        for row in rows {
            configs.push(serde_json::from_str(&row?)?);
        }
        Ok(configs)
    }

    /// 删除团队配置
    pub fn delete_team_config(&self, id: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM team_configs WHERE id = ?1", params![id])?;
        Ok(())
    }
}

// ============================================================================
// Default Workspace — stored in ~/.loom/workspace.json
// ============================================================================

/// Get the default workspace path from ~/.loom/workspace.json
pub fn get_default_workspace() -> Option<String> {
    let home = dirs::home_dir()?;
    let config_path = home.join(".loom").join("workspace.json");
    let content = std::fs::read_to_string(&config_path).ok()?;
    let config: serde_json::Value = serde_json::from_str(&content).ok()?;
    config
        .get("default_workspace")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

/// Set the default workspace path in ~/.loom/workspace.json
pub fn set_default_workspace(path: &str) -> Result<()> {
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Cannot find home directory"))?;
    let loom_dir = home.join(".loom");
    std::fs::create_dir_all(&loom_dir)?;
    let config_path = loom_dir.join("workspace.json");
    let config = serde_json::json!({ "default_workspace": path });
    std::fs::write(&config_path, serde_json::to_string_pretty(&config)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use loom_types::ModelConfig;

    use super::ModelConfigStore;
    use crate::config_db::ConfigDb;

    #[test]
    fn model_config_compact_mode_survives_all_reads_and_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.db");
        {
            let db = ConfigDb::open(&path).unwrap();
            let store = ModelConfigStore::new(db.conn());
            store
                .upsert(&ModelConfig {
                    name: "compact".into(),
                    ..Default::default()
                })
                .unwrap();
            store.set_active("compact").unwrap();
            store
                .upsert(&ModelConfig {
                    name: "compact".into(),
                    compact_mode: true,
                    ..Default::default()
                })
                .unwrap();

            assert!(store.get("compact").unwrap().unwrap().compact_mode);
            assert!(store.list().unwrap()[0].compact_mode);
            assert!(store.get_active().unwrap().unwrap().compact_mode);
        }

        let db = ConfigDb::open(&path).unwrap();
        let store = ModelConfigStore::new(db.conn());
        assert!(store.get("compact").unwrap().unwrap().compact_mode);
        assert!(store.get_active().unwrap().unwrap().compact_mode);
    }
}

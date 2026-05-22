use crate::event::Event;
use anyhow::Result;
use refinery::embed_migrations;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::path::Path;

embed_migrations!("../../migrations");

/// SQLite-backed event store with FTS5 full-text search.
pub struct SqliteEventStore {
    conn: Connection,
}

impl SqliteEventStore {
    /// Open (or create) the database at the given path.
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        let mut store = Self { conn };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&mut self) -> Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp TEXT NOT NULL,
                type TEXT NOT NULL,
                action TEXT NOT NULL,
                context TEXT NOT NULL DEFAULT '',
                confidence REAL NOT NULL,
                source_session TEXT,
                source_text TEXT NOT NULL DEFAULT '',
                payload TEXT
            );

            CREATE VIRTUAL TABLE IF NOT EXISTS events_fts
            USING fts5(type, action, context, source_text, content='events', content_rowid='id');

            CREATE TRIGGER IF NOT EXISTS events_ai AFTER INSERT ON events BEGIN
                INSERT INTO events_fts(rowid, type, action, context, source_text)
                VALUES (new.id, new.type, new.action, new.context, new.source_text);
            END;

            CREATE TRIGGER IF NOT EXISTS events_ad AFTER DELETE ON events BEGIN
                INSERT INTO events_fts(events_fts, rowid, type, action, context, source_text)
                VALUES('delete', old.id, old.type, old.action, old.context, old.source_text);
            END;",
        )?;
        Ok(())
    }

    /// Run refinery migrations (Phase 1)
    /// Phase 0 compatibility: V1 uses CREATE IF NOT EXISTS, safe no-op on existing DBs
    pub fn run_migrations(conn: &mut Connection) -> Result<()> {
        migrations::runner().run(conn)?;
        Ok(())
    }

    /// Phase 1 recommended: open database with refinery migrations
    pub fn open_with_migrations(path: &std::path::Path) -> Result<Self> {
        let mut conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        Self::run_migrations(&mut conn)?;
        Ok(Self { conn })
    }

    /// Insert an event and return its assigned ID.
    pub fn insert(&mut self, event: &Event) -> Result<i64> {
        let payload = event.payload.as_ref().map(|p| p.to_string());
        self.conn.execute(
            "INSERT INTO events (timestamp, type, action, context, confidence, source_session, source_text, payload)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                event.timestamp.to_rfc3339(),
                event.event_type_as_str(),
                event.action,
                event.context,
                event.confidence,
                event.source_session,
                event.source_text,
                payload,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Query the most recent events.
    pub fn query_all(&self, limit: usize) -> Result<Vec<Event>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, timestamp, type, action, context, confidence, source_session, source_text, payload
             FROM events ORDER BY id DESC LIMIT ?1"
        )?;
        let events = stmt
            .query_map(params![limit as i64], Self::row_to_event)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(events)
    }

    /// Query events by action string.
    pub fn query_by_action(&self, action: &str, limit: usize) -> Result<Vec<Event>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, timestamp, type, action, context, confidence, source_session, source_text, payload
             FROM events WHERE action = ?1 ORDER BY id DESC LIMIT ?2"
        )?;
        let events = stmt
            .query_map(params![action, limit as i64], Self::row_to_event)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(events)
    }

    /// Count events with a given action.
    pub fn count_by_action(&self, action: &str) -> Result<usize> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM events WHERE action = ?1",
            params![action],
            |row| row.get(0),
        )?;
        Ok(count as usize)
    }

    /// Full-text search across events.
    pub fn search(&self, query: &str) -> Result<Vec<Event>> {
        let mut stmt = self.conn.prepare(
            "SELECT e.id, e.timestamp, e.type, e.action, e.context, e.confidence,
                    e.source_session, e.source_text, e.payload
             FROM events e
             INNER JOIN events_fts fts ON e.id = fts.rowid
             WHERE events_fts MATCH ?1
             ORDER BY rank
             LIMIT 20",
        )?;
        let events = stmt
            .query_map(params![query], Self::row_to_event)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(events)
    }

    fn row_to_event(row: &rusqlite::Row) -> rusqlite::Result<Event> {
        let type_str: String = row.get(2)?;
        let event_type = Event::event_type_from_str(&type_str);
        Ok(Event {
            id: Some(row.get(0)?),
            timestamp: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(1)?)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|e| {
                    tracing::warn!("Failed to parse event timestamp, using current time: {}", e);
                    chrono::Utc::now()
                }),
            event_type,
            action: row.get(3)?,
            context: row.get(4)?,
            confidence: row.get(5)?,
            source_session: row.get(6)?,
            source_text: row.get(7)?,
            payload: row.get::<_, Option<String>>(8)?.and_then(|s| {
                serde_json::from_str(&s)
                    .map_err(|e| {
                        tracing::warn!(
                            "Failed to deserialize event payload for row {}: {}",
                            row.get::<_, i64>(0).unwrap_or(-1),
                            e
                        );
                    })
                    .ok()
            }),
        })
    }
}

use chrono::{DateTime, Utc};

// === Row types ===

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

/// A historical snapshot of a cognition entry, saved before each update.
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

pub struct TokenUsageRow {
    pub id: i64,
    pub timestamp: String,
    pub session_id: String,
    pub model: String,
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub cached_tokens: usize,
    pub latency_ms: u64,
}

// === EventRow (public row type for Engine/CLI queries) ===

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

// === SqliteEventStore additions ===

impl SqliteEventStore {
    /// Creates an EventStore from an externally-owned Connection (shared with other stores)
    pub fn from_connection(conn: Connection) -> Self {
        Self { conn }
    }

    /// Expose the underlying connection for use by other stores in the same thread
    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    /// Return the most recent events (chronological, newest first)
    pub fn query_recent(&self, limit: usize) -> anyhow::Result<Vec<EventRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, timestamp, type, action, context, confidence, source_session, source_text
             FROM events ORDER BY id DESC LIMIT ?1",
        )?;
        let rows = stmt
            .query_map(rusqlite::params![limit as i64], |row| {
                Ok(EventRow {
                    id: row.get(0)?,
                    timestamp: row.get(1)?,
                    event_type: row.get(2)?,
                    action: row.get(3)?,
                    context: row.get(4)?,
                    confidence: row.get(5)?,
                    source_session: row.get(6)?,
                    source_text: row.get(7)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// FTS5 search across events, returning EventRow
    pub fn search_fts(&self, query: &str, limit: usize) -> anyhow::Result<Vec<EventRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT e.id, e.timestamp, e.type, e.action, e.context, e.confidence,
                    e.source_session, e.source_text
             FROM events e
             INNER JOIN events_fts fts ON e.id = fts.rowid
             WHERE events_fts MATCH ?1
             ORDER BY rank
             LIMIT ?2",
        )?;
        let rows = stmt
            .query_map(rusqlite::params![query, limit as i64], |row| {
                Ok(EventRow {
                    id: row.get(0)?,
                    timestamp: row.get(1)?,
                    event_type: row.get(2)?,
                    action: row.get(3)?,
                    context: row.get(4)?,
                    confidence: row.get(5)?,
                    source_session: row.get(6)?,
                    source_text: row.get(7)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}

// === CognitionStore ===

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

    /// Insert or update a cognition trait for a subject within a scope.
    /// If the (subject, trait, scope) triple already exists, snapshots the current
    /// version and increments the version counter. Returns the cognition ID.
    pub fn insert(
        &self,
        subject: &str,
        trait_name: &str,
        value: &str,
        confidence: f64,
        evidence_count: usize,
        scope: &str,
    ) -> anyhow::Result<i64> {
        let now = Utc::now().timestamp();

        // Check if this trait already exists for this subject+scope
        let existing: Option<(i64, i64)> = self
            .conn
            .query_row(
                "SELECT id, version FROM cognitions WHERE subject = ?1 AND trait = ?2 AND scope = ?3",
                rusqlite::params![subject, trait_name, scope],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .ok();

        if let Some((existing_id, existing_version)) = existing {
            let new_version = existing_version + 1;

            // Save snapshot of current version before updating
            let old_value: String = self.conn.query_row(
                "SELECT value FROM cognitions WHERE id = ?1",
                rusqlite::params![existing_id],
                |row| row.get(0),
            )?;

            self.conn.execute(
                "INSERT INTO cognition_snapshots (cognition_id, version, trait, value, confidence, evidence_count, snapshot_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                rusqlite::params![
                    existing_id,
                    existing_version,
                    trait_name,
                    old_value,
                    confidence,
                    evidence_count,
                    now,
                ],
            )?;

            // Update existing
            self.conn.execute(
                "UPDATE cognitions SET value = ?1, confidence = ?2, evidence_count = ?3, last_updated = ?4, version = ?5
                 WHERE id = ?6",
                rusqlite::params![value, confidence, evidence_count, now, new_version, existing_id],
            )?;

            Ok(existing_id)
        } else {
            // Insert new
            self.conn.execute(
                "INSERT INTO cognitions (subject, trait, value, confidence, evidence_count, first_seen, last_updated, version, scope)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1, ?8)",
                rusqlite::params![subject, trait_name, value, confidence, evidence_count, now, now, scope],
            )?;
            Ok(self.conn.last_insert_rowid())
        }
    }

    pub fn query_by_subject(
        &self,
        subject: &str,
        limit: usize,
    ) -> anyhow::Result<Vec<CognitionRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, subject, trait, value, confidence, evidence_count, first_seen, last_updated, version, scope
             FROM cognitions WHERE subject = ?1 ORDER BY last_updated DESC LIMIT ?2",
        )?;
        let rows = stmt
            .query_map(rusqlite::params![subject, limit as i64], |row| {
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
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn latest_version(&self, subject: &str, trait_name: &str) -> Option<i64> {
        self.conn
            .query_row(
                "SELECT version FROM cognitions WHERE subject = ?1 AND trait = ?2 ORDER BY version DESC LIMIT 1",
                rusqlite::params![subject, trait_name],
                |row| row.get(0),
            )
            .ok()
    }

    /// Return all snapshots for a cognition entry, newest version first.
    pub fn snapshots_for(&self, cognition_id: i64) -> anyhow::Result<Vec<CognitionSnapshot>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, cognition_id, version, trait, value, confidence, evidence_count, snapshot_at
             FROM cognition_snapshots WHERE cognition_id = ?1 ORDER BY version DESC",
        )?;
        let rows = stmt
            .query_map(rusqlite::params![cognition_id], |row| {
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
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}

// === SessionStore ===

pub struct SessionStore<'a> {
    conn: &'a Connection,
}

impl<'a> SessionStore<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn insert(&self, id: &str, created_at: DateTime<Utc>) -> anyhow::Result<()> {
        self.conn.execute(
            "INSERT INTO sessions (id, created_at, message_count) VALUES (?1, ?2, 0)",
            rusqlite::params![id, created_at.to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn list_all(&self, limit: usize) -> anyhow::Result<Vec<openloom_models::SessionInfo>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, created_at, message_count FROM sessions ORDER BY created_at DESC LIMIT ?1",
        )?;
        let rows = stmt
            .query_map(rusqlite::params![limit as i64], |row| {
                let created_at_str: String = row.get(1)?;
                Ok(openloom_models::SessionInfo {
                    id: row.get(0)?,
                    created_at: DateTime::parse_from_rfc3339(&created_at_str)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                    message_count: row.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn update_message_count(&self, id: &str, count: usize) -> anyhow::Result<()> {
        self.conn.execute(
            "UPDATE sessions SET message_count = ?1 WHERE id = ?2",
            rusqlite::params![count, id],
        )?;
        Ok(())
    }
}

// === TokenStore ===

pub struct TokenStore<'a> {
    conn: &'a Connection,
}

impl<'a> TokenStore<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn insert(
        &self,
        session_id: &str,
        model: &str,
        prompt_tokens: usize,
        completion_tokens: usize,
        cached_tokens: usize,
        latency_ms: u64,
    ) -> anyhow::Result<()> {
        self.conn.execute(
            "INSERT INTO token_usage (timestamp, session_id, model, prompt_tokens, completion_tokens, cached_tokens, latency_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                Utc::now().to_rfc3339(),
                session_id,
                model,
                prompt_tokens,
                completion_tokens,
                cached_tokens,
                latency_ms,
            ],
        )?;
        Ok(())
    }

    pub fn query_by_session(
        &self,
        session_id: &str,
        limit: usize,
    ) -> anyhow::Result<Vec<TokenUsageRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, timestamp, session_id, model, prompt_tokens, completion_tokens, cached_tokens, latency_ms
             FROM token_usage WHERE session_id = ?1 ORDER BY id DESC LIMIT ?2",
        )?;
        let rows = stmt
            .query_map(rusqlite::params![session_id, limit as i64], |row| {
                Ok(TokenUsageRow {
                    id: row.get(0)?,
                    timestamp: row.get(1)?,
                    session_id: row.get(2)?,
                    model: row.get(3)?,
                    prompt_tokens: row.get(4)?,
                    completion_tokens: row.get(5)?,
                    cached_tokens: row.get(6)?,
                    latency_ms: row.get(7)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn total_usage(&self) -> anyhow::Result<(usize, usize)> {
        let (prompt, completion): (i64, i64) = self.conn.query_row(
            "SELECT COALESCE(SUM(prompt_tokens), 0), COALESCE(SUM(completion_tokens), 0) FROM token_usage",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        Ok((prompt as usize, completion as usize))
    }

    pub fn summary_by_model(&self) -> anyhow::Result<Vec<ModelUsageSummary>> {
        let mut stmt = self.conn.prepare(
            "SELECT model, COALESCE(SUM(prompt_tokens), 0), COALESCE(SUM(completion_tokens), 0),
                    COALESCE(SUM(cached_tokens), 0), COUNT(*)
             FROM token_usage GROUP BY model ORDER BY SUM(prompt_tokens) + SUM(completion_tokens) DESC",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok(ModelUsageSummary {
                    model: row.get(0)?,
                    prompt_tokens: row.get::<_, i64>(1)? as usize,
                    completion_tokens: row.get::<_, i64>(2)? as usize,
                    cached_tokens: row.get::<_, i64>(3)? as usize,
                    request_count: row.get::<_, i64>(4)? as usize,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn usage_since(&self, since: &str) -> anyhow::Result<UsageAggregate> {
        let (prompt, completion, cached, count): (i64, i64, i64, i64) = self.conn.query_row(
            "SELECT COALESCE(SUM(prompt_tokens), 0), COALESCE(SUM(completion_tokens), 0),
                    COALESCE(SUM(cached_tokens), 0), COUNT(*)
             FROM token_usage WHERE timestamp >= ?1",
            rusqlite::params![since],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )?;
        Ok(UsageAggregate {
            prompt_tokens: prompt as usize,
            completion_tokens: completion as usize,
            cached_tokens: cached as usize,
            request_count: count as usize,
        })
    }

    pub fn recent(&self, limit: usize) -> anyhow::Result<Vec<TokenUsageRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, timestamp, session_id, model, prompt_tokens, completion_tokens, cached_tokens, latency_ms
             FROM token_usage ORDER BY id DESC LIMIT ?1",
        )?;
        let rows = stmt
            .query_map(rusqlite::params![limit as i64], |row| {
                Ok(TokenUsageRow {
                    id: row.get(0)?,
                    timestamp: row.get(1)?,
                    session_id: row.get(2)?,
                    model: row.get(3)?,
                    prompt_tokens: row.get(4)?,
                    completion_tokens: row.get(5)?,
                    cached_tokens: row.get(6)?,
                    latency_ms: row.get(7)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}

pub struct ModelUsageSummary {
    pub model: String,
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub cached_tokens: usize,
    pub request_count: usize,
}

pub struct UsageAggregate {
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub cached_tokens: usize,
    pub request_count: usize,
}

// === MessageStore ===

pub struct MessageStore<'a> {
    conn: &'a Connection,
}

impl<'a> MessageStore<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn insert(
        &self,
        session_id: &str,
        seq: usize,
        role: &str,
        content: &str,
    ) -> anyhow::Result<i64> {
        self.conn.execute(
            "INSERT INTO message_history (session_id, seq, role, content, timestamp)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![session_id, seq, role, content, Utc::now().to_rfc3339()],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn recent(
        &self,
        session_id: &str,
        limit: usize,
    ) -> anyhow::Result<Vec<openloom_models::ChatMessage>> {
        let mut stmt = self.conn.prepare(
            "SELECT role, content, timestamp FROM message_history
             WHERE session_id = ?1 ORDER BY seq DESC LIMIT ?2",
        )?;
        let mut rows: Vec<openloom_models::ChatMessage> = stmt
            .query_map(rusqlite::params![session_id, limit as i64], |row| {
                let ts_str: String = row.get(2)?;
                Ok(openloom_models::ChatMessage {
                    role: row.get(0)?,
                    content: row.get(1)?,
                    timestamp: DateTime::parse_from_rfc3339(&ts_str)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows.reverse(); // DESC -> chronological order
        Ok(rows)
    }

    pub fn all(&self, session_id: &str) -> anyhow::Result<Vec<openloom_models::ChatMessage>> {
        let mut stmt = self.conn.prepare(
            "SELECT role, content, timestamp FROM message_history
             WHERE session_id = ?1 ORDER BY seq ASC",
        )?;
        let rows: Vec<openloom_models::ChatMessage> = stmt
            .query_map(rusqlite::params![session_id], |row| {
                let ts_str: String = row.get(2)?;
                Ok(openloom_models::ChatMessage {
                    role: row.get(0)?,
                    content: row.get(1)?,
                    timestamp: DateTime::parse_from_rfc3339(&ts_str)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn max_seq(&self, session_id: &str) -> anyhow::Result<usize> {
        let seq: Option<i64> = self.conn.query_row(
            "SELECT MAX(seq) FROM message_history WHERE session_id = ?1",
            rusqlite::params![session_id],
            |row| row.get(0),
        )?;
        Ok(seq.unwrap_or(0) as usize)
    }

    pub fn insert_batch(
        &self,
        session_id: &str,
        messages: &[openloom_models::ChatMessage],
    ) -> anyhow::Result<()> {
        for (seq, msg) in (self.max_seq(session_id)? + 1..).zip(messages.iter()) {
            self.insert(session_id, seq, &msg.role, &msg.content)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{Event, EventType};
    use tempfile::tempdir;

    fn make_event(action: &str, confidence: f64) -> Event {
        Event::new(
            EventType::BehaviorPattern,
            action,
            "test_context",
            confidence,
            "test source text",
        )
    }

    #[test]
    fn test_insert_and_query_events() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let mut store = SqliteEventStore::open(&db_path).unwrap();

        let e1 = make_event("loss_chase", 0.87);
        let e2 = make_event("loss_chase", 0.91);
        let e3 = make_event("prefers_tech", 0.80);

        store.insert(&e1).unwrap();
        store.insert(&e2).unwrap();
        store.insert(&e3).unwrap();

        let all = store.query_all(10).unwrap();
        assert_eq!(all.len(), 3);

        let loss_events = store.query_by_action("loss_chase", 10).unwrap();
        assert_eq!(loss_events.len(), 2);

        let count = store.count_by_action("loss_chase").unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn test_event_fts_search() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let mut store = SqliteEventStore::open(&db_path).unwrap();

        let e = make_event("loss_chase", 0.87);
        store.insert(&e).unwrap();

        let results = store.search("loss").unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].action, "loss_chase");
        assert_eq!(results[0].confidence, 0.87);
    }

    #[test]
    fn test_empty_store() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let store = SqliteEventStore::open(&db_path).unwrap();

        assert!(store.query_all(10).unwrap().is_empty());
        assert_eq!(store.count_by_action("anything").unwrap(), 0);
    }

    #[test]
    fn test_payload_roundtrip() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let mut store = SqliteEventStore::open(&db_path).unwrap();

        let event = make_event("test_action", 0.99)
            .with_payload(serde_json::json!({"key": "value", "num": 42}));

        let id = store.insert(&event).unwrap();
        let all = store.query_all(1).unwrap();
        assert_eq!(all.len(), 1);
        let retrieved = &all[0];
        assert_eq!(retrieved.id, Some(id));
        assert!(retrieved.payload.is_some());
        let payload = retrieved.payload.as_ref().unwrap();
        assert_eq!(payload["key"], "value");
        assert_eq!(payload["num"], 42);
    }

    #[test]
    fn test_query_recent_events() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let mut store = SqliteEventStore::open(&db_path).unwrap();
        let e1 = make_event("loss_chase", 0.87);
        let e2 = make_event("prefers_tech", 0.80);
        store.insert(&e1).unwrap();
        store.insert(&e2).unwrap();
        let rows = store.query_recent(10).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].action, "prefers_tech"); // newest first by id
    }

    #[test]
    fn test_search_fts_returns_event_row() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let mut store = SqliteEventStore::open(&db_path).unwrap();
        let e = make_event("loss_chase", 0.87);
        store.insert(&e).unwrap();
        let rows = store.search_fts("loss", 10).unwrap();
        assert!(!rows.is_empty());
        assert_eq!(rows[0].action, "loss_chase");
        assert!(!rows[0].timestamp.is_empty());
    }
}

#[cfg(test)]
mod store_v2_tests {
    use super::*;
    use tempfile::tempdir;

    fn setup_v2_tables(conn: &Connection) {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS cognitions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                subject TEXT NOT NULL,
                trait TEXT NOT NULL,
                value TEXT NOT NULL,
                confidence REAL,
                evidence_count INTEGER,
                first_seen INTEGER,
                last_updated INTEGER,
                version INTEGER DEFAULT 1,
                scope TEXT NOT NULL DEFAULT 'global'
            );
            CREATE TABLE IF NOT EXISTS cognition_snapshots (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                cognition_id INTEGER NOT NULL,
                version INTEGER NOT NULL,
                trait TEXT NOT NULL,
                value TEXT NOT NULL,
                confidence REAL,
                evidence_count INTEGER,
                snapshot_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY,
                created_at TEXT NOT NULL,
                message_count INTEGER DEFAULT 0
            );
            CREATE TABLE IF NOT EXISTS token_usage (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp TEXT NOT NULL,
                session_id TEXT,
                model TEXT NOT NULL,
                prompt_tokens INTEGER,
                completion_tokens INTEGER,
                cached_tokens INTEGER DEFAULT 0,
                latency_ms INTEGER
            );",
        )
        .unwrap();
    }

    #[test]
    fn test_cognition_insert_and_query() {
        let dir = tempdir().unwrap();
        let conn = Connection::open(dir.path().join("test.db")).unwrap();
        setup_v2_tables(&conn);
        let store = CognitionStore::new(&conn);
        store
            .insert("USER", "risk_tendency", "gambler_chase", 0.91, 5, "global")
            .unwrap();
        let rows = store.query_by_subject("USER", 10).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].trait_name, "risk_tendency");
        assert_eq!(rows[0].value, "gambler_chase");
        assert!(rows[0].first_seen > 0);
    }

    #[test]
    fn test_cognition_upsert_increments_version() {
        let dir = tempdir().unwrap();
        let conn = Connection::open(dir.path().join("test.db")).unwrap();
        setup_v2_tables(&conn);

        let store = CognitionStore::new(&conn);

        // First insert
        let id1 = store
            .insert("USER", "risk_tendency", "gambler_v1", 0.8, 3, "global")
            .unwrap();
        let rows = store.query_by_subject("USER", 10).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].version, 1);
        assert_eq!(rows[0].value, "gambler_v1");

        // Second insert with same subject+trait should update
        let id2 = store
            .insert("USER", "risk_tendency", "gambler_v2", 0.9, 5, "global")
            .unwrap();
        assert_eq!(id1, id2, "Same row, updated");
        let rows = store.query_by_subject("USER", 10).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].version, 2);
        assert_eq!(rows[0].value, "gambler_v2");
        assert_eq!(rows[0].evidence_count, 5);

        // Check snapshot was created
        let snapshots = store.snapshots_for(id1).unwrap();
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].version, 1);
        assert_eq!(snapshots[0].value, "gambler_v1");
    }

    #[test]
    fn test_cognition_scope_isolation() {
        let dir = tempdir().unwrap();
        let conn = Connection::open(dir.path().join("test.db")).unwrap();
        setup_v2_tables(&conn);
        let store = CognitionStore::new(&conn);

        // Same trait, different scopes — should NOT overwrite each other
        store
            .insert("USER", "tech_stack", "React前端", 0.8, 3, "project:A")
            .unwrap();
        store
            .insert("USER", "tech_stack", "Rust后端", 0.9, 4, "project:B")
            .unwrap();

        let rows = store.query_by_subject("USER", 10).unwrap();
        assert_eq!(rows.len(), 2);

        let a_row = rows.iter().find(|r| r.scope == "project:A").unwrap();
        assert_eq!(a_row.value, "React前端");

        let b_row = rows.iter().find(|r| r.scope == "project:B").unwrap();
        assert_eq!(b_row.value, "Rust后端");
    }

    #[test]
    fn test_cognition_upsert_within_same_scope() {
        let dir = tempdir().unwrap();
        let conn = Connection::open(dir.path().join("test.db")).unwrap();
        setup_v2_tables(&conn);
        let store = CognitionStore::new(&conn);

        store
            .insert("USER", "tech_stack", "React v1", 0.7, 3, "project:A")
            .unwrap();
        store
            .insert("USER", "tech_stack", "React v2 升级", 0.9, 5, "project:A")
            .unwrap();

        let rows = store.query_by_subject("USER", 10).unwrap();
        // Should upsert (1 row), not create 2
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].value, "React v2 升级");
        assert_eq!(rows[0].version, 2);
    }

    #[test]
    fn test_session_insert_and_list() {
        let dir = tempdir().unwrap();
        let conn = Connection::open(dir.path().join("test.db")).unwrap();
        setup_v2_tables(&conn);
        let store = SessionStore::new(&conn);
        store.insert("s1", Utc::now()).unwrap();
        store.insert("s2", Utc::now()).unwrap();
        let sessions = store.list_all(10).unwrap();
        assert_eq!(sessions.len(), 2);
    }

    #[test]
    fn test_token_insert_and_total() {
        let dir = tempdir().unwrap();
        let conn = Connection::open(dir.path().join("test.db")).unwrap();
        setup_v2_tables(&conn);
        let store = TokenStore::new(&conn);
        store.insert("s1", "test-model", 100, 50, 0, 200).unwrap();
        store.insert("s1", "test-model", 200, 100, 0, 300).unwrap();
        let (prompt, completion) = store.total_usage().unwrap();
        assert_eq!(prompt, 300);
        assert_eq!(completion, 150);
    }
}

#[cfg(test)]
mod message_store_tests {
    use super::*;
    use tempfile::tempdir;

    fn setup_message_table(conn: &Connection) {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS message_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL,
                seq INTEGER NOT NULL,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                timestamp TEXT NOT NULL
            );",
        )
        .unwrap();
    }

    #[test]
    fn test_message_insert_and_recent() {
        let dir = tempdir().unwrap();
        let conn = Connection::open(dir.path().join("test.db")).unwrap();
        setup_message_table(&conn);
        let store = MessageStore::new(&conn);

        store.insert("s1", 1, "user", "hello").unwrap();
        store.insert("s1", 2, "assistant", "hi there").unwrap();
        store.insert("s1", 3, "user", "how are you").unwrap();

        let recent = store.recent("s1", 2).unwrap();
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].content, "hi there");
        assert_eq!(recent[1].content, "how are you");
    }

    #[test]
    fn test_message_max_seq() {
        let dir = tempdir().unwrap();
        let conn = Connection::open(dir.path().join("test.db")).unwrap();
        setup_message_table(&conn);
        let store = MessageStore::new(&conn);

        assert_eq!(store.max_seq("s1").unwrap(), 0);
        store.insert("s1", 1, "user", "a").unwrap();
        store.insert("s1", 2, "assistant", "b").unwrap();
        assert_eq!(store.max_seq("s1").unwrap(), 2);
    }

    #[test]
    fn test_message_empty_session() {
        let dir = tempdir().unwrap();
        let conn = Connection::open(dir.path().join("test.db")).unwrap();
        setup_message_table(&conn);
        let store = MessageStore::new(&conn);
        let recent = store.recent("nonexistent", 20).unwrap();
        assert!(recent.is_empty());
    }

    #[test]
    fn test_message_insert_batch() {
        let dir = tempdir().unwrap();
        let conn = Connection::open(dir.path().join("test.db")).unwrap();
        setup_message_table(&conn);
        let store = MessageStore::new(&conn);

        let msgs = vec![
            openloom_models::ChatMessage {
                role: "user".into(),
                content: "a".into(),
                timestamp: Utc::now(),
            },
            openloom_models::ChatMessage {
                role: "assistant".into(),
                content: "b".into(),
                timestamp: Utc::now(),
            },
        ];
        store.insert_batch("s1", &msgs).unwrap();
        assert_eq!(store.max_seq("s1").unwrap(), 2);
        let recent = store.recent("s1", 10).unwrap();
        assert_eq!(recent.len(), 2);
    }
}

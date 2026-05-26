//! SQLite-backed event store with FTS5 full-text search.
//! Ported from crates/memory/src/store.rs with loom-types compatibility.

use anyhow::Result;
use chrono::{DateTime, Utc};
use refinery::embed_migrations;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::Path;

embed_migrations!("../../../migrations");

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
// SqliteEventStore
// ============================================================================

pub struct SqliteEventStore {
    conn: Connection,
}

impl SqliteEventStore {
    /// Open (or create) the database with full migrations.
    pub fn open(path: &Path) -> Result<Self> {
        let mut conn = Connection::open(path)?;
        // Force switch from WAL to DELETE — checkpoint first, then switch
        let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
        conn.execute_batch(
            "PRAGMA journal_mode=DELETE;
             PRAGMA foreign_keys=ON;"
        )?;
        migrations::runner().run(&mut conn)?;
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM sqlite_master WHERE type='table'", [], |r| r.get(0))?;
        tracing::info!(table_count = count, "db opened");
        Ok(Self { conn })
    }

    /// Create from an already-open connection.
    pub fn from_connection(conn: Connection) -> Self {
        Self { conn }
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    /// Insert an event row.
    pub fn insert_event(&self, event: &NewEvent) -> Result<i64> {
        let payload = event.payload.as_ref().map(|p| p.to_string());
        self.conn.execute(
            "INSERT INTO events (timestamp, type, action, context, confidence, source_session, source_text, payload)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                event.timestamp.to_rfc3339(),
                event.event_type,
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

    /// Query recent events.
    pub fn query_recent(&self, limit: usize) -> Result<Vec<EventRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, timestamp, type, action, context, confidence, source_session, source_text
             FROM events ORDER BY id DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |row| {
            Ok(EventRow {
                id: row.get(0)?, timestamp: row.get(1)?, event_type: row.get(2)?,
                action: row.get(3)?, context: row.get(4)?, confidence: row.get(5)?,
                source_session: row.get(6)?, source_text: row.get(7)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// Full-text search (FTS5) across events.
    pub fn search_fts(&self, query: &str, limit: usize) -> Result<Vec<EventRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT e.id, e.timestamp, e.type, e.action, e.context, e.confidence,
                    e.source_session, e.source_text
             FROM events e
             INNER JOIN events_fts fts ON e.id = fts.rowid
             WHERE events_fts MATCH ?1 ORDER BY rank LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![query, limit as i64], |row| {
            Ok(EventRow {
                id: row.get(0)?, timestamp: row.get(1)?, event_type: row.get(2)?,
                action: row.get(3)?, context: row.get(4)?, confidence: row.get(5)?,
                source_session: row.get(6)?, source_text: row.get(7)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// Count events by action type.
    pub fn count_by_action(&self, action: &str) -> Result<usize> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM events WHERE action = ?1", params![action], |row| row.get(0),
        )?;
        Ok(count as usize)
    }
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
        &self, subject: &str, trait_name: &str, value: &str,
        confidence: f64, evidence_count: usize, scope: &str,
    ) -> Result<i64> {
        let now = Utc::now().timestamp();
        let existing: Option<(i64, i64)> = self.conn.query_row(
            "SELECT id, version FROM cognitions WHERE subject = ?1 AND trait = ?2 AND scope = ?3",
            params![subject, trait_name, scope],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ).ok();

        if let Some((id, existing_version)) = existing {
            let old_value: String = self.conn.query_row(
                "SELECT value FROM cognitions WHERE id = ?1", params![id], |row| row.get(0),
            )?;
            self.conn.execute(
                "INSERT INTO cognition_snapshots (cognition_id, version, trait, value, confidence, evidence_count, snapshot_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![id, existing_version, trait_name, old_value, confidence, evidence_count, now],
            )?;
            self.conn.execute(
                "UPDATE cognitions SET value = ?1, confidence = ?2, evidence_count = ?3, last_updated = ?4, version = ?5 WHERE id = ?6",
                params![value, confidence, evidence_count, now, existing_version + 1, id],
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

    pub fn query_by_subject(&self, subject: &str, limit: usize, offset: usize) -> Result<Vec<CognitionRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, subject, trait, value, confidence, evidence_count, first_seen, last_updated, version, scope
             FROM cognitions WHERE subject = ?1 ORDER BY last_updated DESC LIMIT ?2 OFFSET ?3",
        )?;
        let rows = stmt.query_map(params![subject, limit as i64, offset as i64], |row| {
            Ok(CognitionRow {
                id: row.get(0)?, subject: row.get(1)?, trait_name: row.get(2)?,
                value: row.get(3)?, confidence: row.get(4)?, evidence_count: row.get(5)?,
                first_seen: row.get(6)?, last_updated: row.get(7)?, version: row.get(8)?,
                scope: row.get(9)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn delete(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM cognitions WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn snapshots_for(&self, cognition_id: i64) -> Result<Vec<CognitionSnapshot>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, cognition_id, version, trait, value, confidence, evidence_count, snapshot_at
             FROM cognition_snapshots WHERE cognition_id = ?1 ORDER BY version DESC",
        )?;
        let rows = stmt.query_map(params![cognition_id], |row| {
            Ok(CognitionSnapshot {
                id: row.get(0)?, cognition_id: row.get(1)?, version: row.get(2)?,
                trait_name: row.get(3)?, value: row.get(4)?, confidence: row.get(5)?,
                evidence_count: row.get(6)?, snapshot_at: row.get(7)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }
}

// SPDX-License-Identifier: Apache-2.0
//! SQLite storage for cron jobs and execution history.
//!
//! Uses a standalone `cron.db` database (separate from `memory.db`).
//! Connection is wrapped in `std::sync::Mutex` for `Send + Sync` safety.

use anyhow::{Context, Result};
use rusqlite::{Connection, params};
use std::path::Path;
use std::sync::Mutex;

use crate::job::{CronJob, CronRunHistory, RunStatus, SessionMode};

// ── Table DDL ─────────────────────────────────────────────────────────────────

const DDL: &str = "
CREATE TABLE IF NOT EXISTS cron_jobs (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    cron_expression TEXT NOT NULL,
    command         TEXT NOT NULL,
    enabled         INTEGER NOT NULL DEFAULT 1,
    session_mode    TEXT NOT NULL DEFAULT 'isolated',
    timeout_secs    INTEGER NOT NULL DEFAULT 300,
    created_at      INTEGER NOT NULL,
    last_run        INTEGER,
    next_run        INTEGER,
    run_count       INTEGER NOT NULL DEFAULT 0,
    error_count     INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS cron_run_history (
    id          TEXT PRIMARY KEY,
    job_id      TEXT NOT NULL REFERENCES cron_jobs(id) ON DELETE CASCADE,
    started_at  INTEGER NOT NULL,
    finished_at INTEGER,
    status      TEXT NOT NULL DEFAULT 'running',
    stdout      TEXT,
    stderr      TEXT,
    exit_code   INTEGER
);

CREATE INDEX IF NOT EXISTS idx_cron_jobs_enabled ON cron_jobs(enabled);
CREATE INDEX IF NOT EXISTS idx_cron_jobs_next_run ON cron_jobs(next_run);
CREATE INDEX IF NOT EXISTS idx_cron_history_job_id ON cron_run_history(job_id);
CREATE INDEX IF NOT EXISTS idx_cron_history_started_at ON cron_run_history(started_at);
";

// ── Storage ───────────────────────────────────────────────────────────────────

/// Manages the standalone `cron.db` SQLite database.
///
/// Wraps `rusqlite::Connection` in a `Mutex` so the storage is `Send + Sync`
/// and can be shared across tokio tasks (required by `tokio-cron-scheduler`).
pub struct CronStorage {
    conn: Mutex<Connection>,
}

impl CronStorage {
    /// Open (or create) the cron database at the given path.
    pub fn open(db_path: &Path) -> Result<Self> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create cron db parent dir: {}", parent.display()))?;
        }
        let conn = Connection::open(db_path)
            .with_context(|| format!("open cron db: {}", db_path.display()))?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA foreign_keys=ON;",
        )?;
        conn.execute_batch(DDL)?;
        tracing::info!(path = %db_path.display(), "cron storage opened");
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    // ── Job CRUD ──────────────────────────────────────────────────────────

    /// Insert a new cron job.
    pub fn insert_job(&self, job: &CronJob) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO cron_jobs (id, name, cron_expression, command, enabled, session_mode,
             timeout_secs, created_at, last_run, next_run, run_count, error_count)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                job.id,
                job.name,
                job.cron_expression,
                job.command,
                job.enabled as i32,
                job.session_mode.to_string(),
                job.timeout_secs,
                job.created_at,
                job.last_run,
                job.next_run,
                job.run_count,
                job.error_count,
            ],
        )?;
        Ok(())
    }

    /// Load all cron jobs from the database.
    pub fn load_all_jobs(&self) -> Result<Vec<CronJob>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, cron_expression, command, enabled, session_mode,
                    timeout_secs, created_at, last_run, next_run, run_count, error_count
             FROM cron_jobs
             ORDER BY created_at",
        )?;
        let rows = stmt.query_map([], |row| {
            let mode_str: String = row.get(5)?;
            Ok(CronJob {
                id: row.get(0)?,
                name: row.get(1)?,
                cron_expression: row.get(2)?,
                command: row.get(3)?,
                enabled: row.get::<_, i32>(4)? != 0,
                session_mode: parse_session_mode(&mode_str),
                timeout_secs: row.get(6)?,
                created_at: row.get(7)?,
                last_run: row.get(8)?,
                next_run: row.get(9)?,
                run_count: row.get::<_, i64>(10)? as u64,
                error_count: row.get::<_, i64>(11)? as u64,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    /// Load only enabled jobs (used at startup to re-register with the scheduler).
    pub fn load_enabled_jobs(&self) -> Result<Vec<CronJob>> {
        let all = self.load_all_jobs()?;
        Ok(all.into_iter().filter(|j| j.enabled).collect())
    }

    /// Update a job's enabled flag.
    pub fn set_enabled(&self, job_id: &str, enabled: bool) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let rows = conn.execute(
            "UPDATE cron_jobs SET enabled = ?1 WHERE id = ?2",
            params![enabled as i32, job_id],
        )?;
        if rows == 0 {
            anyhow::bail!("cron job not found: {}", job_id);
        }
        Ok(())
    }

    /// Record a completed run: increment counters and update last_run.
    pub fn record_run(&self, job_id: &str, timestamp: i64, success: bool) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        if success {
            conn.execute(
                "UPDATE cron_jobs SET last_run = ?1, run_count = run_count + 1 WHERE id = ?2",
                params![timestamp, job_id],
            )?;
        } else {
            conn.execute(
                "UPDATE cron_jobs SET error_count = error_count + 1 WHERE id = ?1",
                params![job_id],
            )?;
        }
        Ok(())
    }

    /// Delete a cron job by id.
    pub fn delete_job(&self, job_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let rows = conn.execute("DELETE FROM cron_jobs WHERE id = ?1", params![job_id])?;
        if rows == 0 {
            anyhow::bail!("cron job not found: {}", job_id);
        }
        Ok(())
    }

    /// Get a single job by id.
    pub fn get_job(&self, job_id: &str) -> Result<Option<CronJob>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, cron_expression, command, enabled, session_mode,
                    timeout_secs, created_at, last_run, next_run, run_count, error_count
             FROM cron_jobs WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![job_id], |row| {
            let mode_str: String = row.get(5)?;
            Ok(CronJob {
                id: row.get(0)?,
                name: row.get(1)?,
                cron_expression: row.get(2)?,
                command: row.get(3)?,
                enabled: row.get::<_, i32>(4)? != 0,
                session_mode: parse_session_mode(&mode_str),
                timeout_secs: row.get(6)?,
                created_at: row.get(7)?,
                last_run: row.get(8)?,
                next_run: row.get(9)?,
                run_count: row.get::<_, i64>(10)? as u64,
                error_count: row.get::<_, i64>(11)? as u64,
            })
        })?;
        match rows.next() {
            Some(Ok(job)) => Ok(Some(job)),
            Some(Err(e)) => Err(e.into()),
            None => Ok(None),
        }
    }

    // ── History ───────────────────────────────────────────────────────────

    /// Insert a new run history record.
    pub fn insert_history(&self, h: &CronRunHistory) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO cron_run_history (id, job_id, started_at, finished_at, status, stdout, stderr, exit_code)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                h.id,
                h.job_id,
                h.started_at,
                h.finished_at,
                run_status_str(&h.status),
                h.stdout,
                h.stderr,
                h.exit_code,
            ],
        )?;
        Ok(())
    }

    /// Update a run history record on completion.
    pub fn update_history(
        &self,
        run_id: &str,
        finished_at: i64,
        status: &RunStatus,
        stdout: Option<&str>,
        stderr: Option<&str>,
        exit_code: Option<i32>,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE cron_run_history SET finished_at = ?1, status = ?2, stdout = ?3, stderr = ?4, exit_code = ?5
             WHERE id = ?6",
            params![
                finished_at,
                run_status_str(status),
                stdout,
                stderr,
                exit_code,
                run_id,
            ],
        )?;
        Ok(())
    }

    /// Load run history for a job, most recent first.
    pub fn load_history(&self, job_id: &str, limit: usize) -> Result<Vec<CronRunHistory>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, job_id, started_at, finished_at, status, stdout, stderr, exit_code
             FROM cron_run_history
             WHERE job_id = ?1
             ORDER BY started_at DESC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![job_id, limit as i64], |row| {
            let status_str: String = row.get(4)?;
            Ok(CronRunHistory {
                id: row.get(0)?,
                job_id: row.get(1)?,
                started_at: row.get(2)?,
                finished_at: row.get(3)?,
                status: parse_run_status(&status_str),
                stdout: row.get(5)?,
                stderr: row.get(6)?,
                exit_code: row.get(7)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    /// Delete history records older than the given timestamp.
    pub fn prune_history(&self, before_timestamp: i64) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        let count = conn.execute(
            "DELETE FROM cron_run_history WHERE started_at < ?1",
            params![before_timestamp],
        )?;
        Ok(count)
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn parse_session_mode(s: &str) -> SessionMode {
    match s {
        "current" => SessionMode::Current,
        _ => SessionMode::Isolated,
    }
}

fn parse_run_status(s: &str) -> RunStatus {
    match s {
        "completed" => RunStatus::Completed,
        "failed" => RunStatus::Failed,
        "timed_out" => RunStatus::TimedOut,
        _ => RunStatus::Running,
    }
}

fn run_status_str(s: &RunStatus) -> &'static str {
    match s {
        RunStatus::Running => "running",
        RunStatus::Completed => "completed",
        RunStatus::Failed => "failed",
        RunStatus::TimedOut => "timed_out",
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn test_job(id: &str, name: &str, cron: &str) -> CronJob {
        CronJob {
            id: id.into(),
            name: name.into(),
            cron_expression: cron.into(),
            command: "echo hello".into(),
            enabled: true,
            session_mode: SessionMode::Isolated,
            timeout_secs: 300,
            created_at: 1700000000,
            last_run: None,
            next_run: None,
            run_count: 0,
            error_count: 0,
        }
    }

    #[test]
    fn test_insert_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("cron.db");
        let storage = CronStorage::open(&db_path).unwrap();

        let job = test_job("j1", "test job", "0 */6 * * *");
        storage.insert_job(&job).unwrap();

        let loaded = storage.load_all_jobs().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].name, "test job");
        assert_eq!(loaded[0].cron_expression, "0 */6 * * *");
    }

    #[test]
    fn test_enabled_filter() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("cron.db");
        let storage = CronStorage::open(&db_path).unwrap();

        storage
            .insert_job(&test_job("j1", "enabled", "* * * * *"))
            .unwrap();
        let mut disabled = test_job("j2", "disabled", "* * * * *");
        disabled.enabled = false;
        storage.insert_job(&disabled).unwrap();

        let enabled = storage.load_enabled_jobs().unwrap();
        assert_eq!(enabled.len(), 1);
        assert_eq!(enabled[0].id, "j1");
    }

    #[test]
    fn test_set_enabled_toggle() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("cron.db");
        let storage = CronStorage::open(&db_path).unwrap();

        storage
            .insert_job(&test_job("j1", "toggle", "* * * * *"))
            .unwrap();
        storage.set_enabled("j1", false).unwrap();

        let job = storage.get_job("j1").unwrap().unwrap();
        assert!(!job.enabled);

        storage.set_enabled("j1", true).unwrap();
        let job = storage.get_job("j1").unwrap().unwrap();
        assert!(job.enabled);
    }

    #[test]
    fn test_delete_job() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("cron.db");
        let storage = CronStorage::open(&db_path).unwrap();

        storage
            .insert_job(&test_job("j1", "delete me", "* * * * *"))
            .unwrap();
        storage.delete_job("j1").unwrap();
        assert!(storage.get_job("j1").unwrap().is_none());
    }

    #[test]
    fn test_delete_nonexistent() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("cron.db");
        let storage = CronStorage::open(&db_path).unwrap();
        assert!(storage.delete_job("nonexistent").is_err());
    }

    #[test]
    fn test_record_run() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("cron.db");
        let storage = CronStorage::open(&db_path).unwrap();

        storage
            .insert_job(&test_job("j1", "run test", "* * * * *"))
            .unwrap();
        storage.record_run("j1", 1700000100, true).unwrap();

        let job = storage.get_job("j1").unwrap().unwrap();
        assert_eq!(job.last_run, Some(1700000100));
        assert_eq!(job.run_count, 1);
        assert_eq!(job.error_count, 0);

        storage.record_run("j1", 1700000200, false).unwrap();
        let job = storage.get_job("j1").unwrap().unwrap();
        assert_eq!(job.error_count, 1);
    }

    #[test]
    fn test_history_insert_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("cron.db");
        let storage = CronStorage::open(&db_path).unwrap();

        storage
            .insert_job(&test_job("j1", "hist test", "* * * * *"))
            .unwrap();

        let h = CronRunHistory {
            id: "run1".into(),
            job_id: "j1".into(),
            started_at: 1700000000,
            finished_at: Some(1700000005),
            status: RunStatus::Completed,
            stdout: Some("hello\n".into()),
            stderr: None,
            exit_code: Some(0),
        };
        storage.insert_history(&h).unwrap();

        let history = storage.load_history("j1", 10).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].status, RunStatus::Completed);
        assert_eq!(history[0].exit_code, Some(0));
    }

    #[test]
    fn test_history_prune() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("cron.db");
        let storage = CronStorage::open(&db_path).unwrap();

        storage
            .insert_job(&test_job("j1", "prune test", "* * * * *"))
            .unwrap();

        for i in 0..5 {
            storage
                .insert_history(&CronRunHistory {
                    id: format!("run{}", i),
                    job_id: "j1".into(),
                    started_at: 1700000000 + i * 100,
                    finished_at: Some(1700000005 + i * 100),
                    status: RunStatus::Completed,
                    stdout: None,
                    stderr: None,
                    exit_code: Some(0),
                })
                .unwrap();
        }

        // Prune everything before 1700000300 (should delete 3 records)
        let deleted = storage.prune_history(1700000300).unwrap();
        assert_eq!(deleted, 3);
        assert_eq!(storage.load_history("j1", 100).unwrap().len(), 2);
    }
}

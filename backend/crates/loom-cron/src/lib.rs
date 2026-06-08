// SPDX-License-Identifier: Apache-2.0
//! Cron scheduler for openLoom v2.
//!
//! Provides a SQLite-backed periodic task executor using the `cron` crate for
//! schedule parsing and a custom `tokio::time::interval`-based loop.
//! Jobs are persisted to `cron.db` (separate from memory.db) and automatically
//! restored on restart.

pub mod detector;
pub mod job;
pub mod storage;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use anyhow::{Context, Result};
use tokio::sync::RwLock;
use uuid::Uuid;

use job::{CronJob, CronRunHistory, RunStatus, SessionMode};
use storage::CronStorage;

// ── Scheduler ─────────────────────────────────────────────────────────────────

/// Info needed by the scheduler loop for each active job.
struct ActiveJob {
    schedule: cron::Schedule,
    job: CronJob,
}

/// The central cron scheduler.
pub struct CronScheduler {
    /// Registered active jobs (id → schedule + job).
    active: RwLock<HashMap<String, ActiveJob>>,
    /// SQLite storage for jobs and run history.
    storage: Arc<CronStorage>,
    /// Path to the cron database file.
    db_path: PathBuf,
    /// Set of job IDs currently executing (prevents concurrent execution of the same job).
    running: Arc<RwLock<HashSet<String>>>,
}

impl CronScheduler {
    /// Create a new scheduler and load persisted jobs from the database.
    pub async fn new(db_path: PathBuf) -> Result<Self> {
        let storage = Arc::new(CronStorage::open(&db_path)?);
        let mut active = HashMap::new();

        // Restore persisted enabled jobs.
        let jobs = storage.load_enabled_jobs()?;
        for job in jobs {
            let schedule = cron::Schedule::from_str(&job.cron_expression).with_context(|| {
                format!(
                    "invalid cron expression for job '{}': {}",
                    job.name, job.cron_expression
                )
            })?;
            active.insert(job.id.clone(), ActiveJob { schedule, job });
        }

        if !active.is_empty() {
            tracing::info!(count = active.len(), "restored cron jobs from database");
        }

        Ok(Self {
            active: RwLock::new(active),
            storage,
            db_path,
            running: Arc::new(RwLock::new(HashSet::new())),
        })
    }

    /// Start the scheduler loop. Runs until `stop()` is called.
    /// Spawns as a background task; stores the JoinHandle for graceful shutdown.
    pub fn start(self: &Arc<Self>) -> tokio::task::JoinHandle<()> {
        let this = self.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(std::time::Duration::from_secs(1));
            loop {
                tick.tick().await;
                this.check_and_fire().await;
            }
        })
    }

    /// Return the database path (for diagnostics).
    pub fn db_path(&self) -> &PathBuf {
        &self.db_path
    }

    /// Return the storage reference.
    pub fn storage(&self) -> &Arc<CronStorage> {
        &self.storage
    }

    // ── Job management ────────────────────────────────────────────────────

    /// Add a new cron job.
    pub async fn add_job(
        &self,
        name: &str,
        cron_expression: &str,
        command: &str,
        session_mode: SessionMode,
        timeout_secs: u64,
    ) -> Result<String> {
        // Validate timeout bounds.
        if timeout_secs == 0 {
            return Err(anyhow::anyhow!("timeout_secs must be at least 1 second"));
        }
        let timeout_secs = timeout_secs.min(3600); // Cap at 1 hour.

        // Validate the cron expression.
        let schedule = cron::Schedule::from_str(cron_expression)
            .with_context(|| format!("invalid cron expression: {}", cron_expression))?;

        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp();

        let job = CronJob {
            id: id.clone(),
            name: name.to_string(),
            cron_expression: cron_expression.to_string(),
            command: command.to_string(),
            enabled: true,
            session_mode,
            timeout_secs,
            created_at: now,
            last_run: None,
            next_run: None,
            run_count: 0,
            error_count: 0,
        };

        // Persist.
        self.storage.insert_job(&job)?;

        // Register in-memory.
        self.active
            .write()
            .await
            .insert(id.clone(), ActiveJob { schedule, job });

        tracing::info!(id = %id, name = %name, cron = %cron_expression, "cron job added");
        Ok(id)
    }

    /// Remove a cron job.
    pub async fn remove_job(&self, job_id: &str) -> Result<()> {
        self.active.write().await.remove(job_id);
        self.storage.delete_job(job_id)?;
        tracing::info!(id = %job_id, "cron job removed");
        Ok(())
    }

    /// Pause a job.
    pub async fn pause_job(&self, job_id: &str) -> Result<()> {
        self.storage.set_enabled(job_id, false)?;
        self.active.write().await.remove(job_id);
        tracing::info!(id = %job_id, "cron job paused");
        Ok(())
    }

    /// Resume a paused job.
    pub async fn resume_job(&self, job_id: &str) -> Result<()> {
        self.storage.set_enabled(job_id, true)?;
        if let Some(job) = self.storage.get_job(job_id)? {
            let schedule = cron::Schedule::from_str(&job.cron_expression)
                .with_context(|| format!("invalid cron expression: {}", job.cron_expression))?;
            self.active
                .write()
                .await
                .insert(job_id.to_string(), ActiveJob { schedule, job });
        }
        tracing::info!(id = %job_id, "cron job resumed");
        Ok(())
    }

    /// List all jobs with their latest run status.
    pub fn list_jobs(&self) -> Result<Vec<job::CronJobSummary>> {
        let jobs = self.storage.load_all_jobs()?;
        let mut summaries: Vec<job::CronJobSummary> = jobs.iter().map(|j| j.into()).collect();
        for summary in &mut summaries {
            let history = self.storage.load_history(&summary.id, 1)?;
            summary.last_status = history.first().map(|h| h.status.clone());
        }
        Ok(summaries)
    }

    /// Get a single job by ID.
    pub fn get_job(&self, job_id: &str) -> Result<Option<CronJob>> {
        self.storage.get_job(job_id)
    }

    /// Get run history for a job.
    pub fn get_history(&self, job_id: &str, limit: usize) -> Result<Vec<CronRunHistory>> {
        self.storage.load_history(job_id, limit)
    }

    /// Run a job immediately.
    pub async fn run_now(&self, job_id: &str) -> Result<String> {
        // Check if the job is already running.
        {
            let running = self.running.read().await;
            if running.contains(job_id) {
                return Err(anyhow::anyhow!("job already running: {}", job_id));
            }
        }

        let job = self
            .storage
            .get_job(job_id)?
            .ok_or_else(|| anyhow::anyhow!("cron job not found: {}", job_id))?;

        // Mark as running.
        self.running.write().await.insert(job_id.to_string());

        let run_id = Uuid::new_v4().to_string();
        Self::execute_job(&self.storage, &job, &run_id).await;

        // Remove from running set.
        self.running.write().await.remove(job_id);

        Ok(run_id)
    }

    // ── Internal ──────────────────────────────────────────────────────────

    /// Check all active jobs and fire any that are due.
    async fn check_and_fire(&self) {
        let now = chrono::Utc::now();

        // Collect all due jobs first (prevents single-tick starvation).
        let due: Vec<(String, CronJob)> = {
            let active = self.active.read().await;
            active
                .iter()
                .filter(|(_, entry)| entry.schedule.includes(now))
                .map(|(job_id, entry)| (job_id.clone(), entry.job.clone()))
                .collect()
        };

        for (job_id, job) in due {
            // Skip if the job is already running (prevents concurrent execution).
            {
                let running = self.running.read().await;
                if running.contains(&job_id) {
                    continue;
                }
            }

            // Mark as running.
            self.running.write().await.insert(job_id.clone());

            let storage = self.storage.clone();
            let run_id = Uuid::new_v4().to_string();
            let running = self.running.clone();
            let jid = job_id.clone();

            tokio::spawn(async move {
                Self::execute_job(&storage, &job, &run_id).await;
                // Remove from running set after completion.
                running.write().await.remove(&jid);
            });
        }
    }

    /// Execute a single job: record history, run the command, update results.
    async fn execute_job(storage: &Arc<CronStorage>, job: &CronJob, run_id: &str) {
        let now = chrono::Utc::now().timestamp();

        // Record start.
        if let Err(e) = storage.insert_history(&CronRunHistory {
            id: run_id.to_string(),
            job_id: job.id.clone(),
            started_at: now,
            finished_at: None,
            status: RunStatus::Running,
            stdout: None,
            stderr: None,
            exit_code: None,
        }) {
            tracing::warn!(run_id = %run_id, job_id = %job.id, error = %e, "failed to insert run history start");
        }

        tracing::info!(run_id = %run_id, job_id = %job.id, name = %job.name, "cron job triggered");

        let result = execute_command(&job.command, job.timeout_secs).await;
        let finished_at = chrono::Utc::now().timestamp();

        let (status, exit_code) = match &result {
            Ok((_, code)) if *code == 0 => (RunStatus::Completed, Some(0)),
            Ok((_, code)) => (RunStatus::Failed, Some(*code)),
            Err(_) => (RunStatus::TimedOut, None),
        };

        let (stdout, stderr) = match result {
            Ok((out, _)) => (Some(out), None),
            Err(e) => (None, Some(e.to_string())),
        };

        if let Err(e) = storage.update_history(
            run_id,
            finished_at,
            &status,
            stdout.as_deref(),
            stderr.as_deref(),
            exit_code,
        ) {
            tracing::warn!(run_id = %run_id, job_id = %job.id, error = %e, "failed to update run history result");
        }
        if let Err(e) = storage.record_run(&job.id, finished_at, status == RunStatus::Completed) {
            tracing::warn!(run_id = %run_id, job_id = %job.id, error = %e, "failed to record run");
        }

        tracing::info!(
            run_id = %run_id,
            job_id = %job.id,
            name = %job.name,
            status = ?status,
            "cron job execution completed"
        );
    }
}

// ── Command execution ─────────────────────────────────────────────────────────

async fn execute_command(command: &str, timeout_secs: u64) -> Result<(String, i32)> {
    run_shell_command(command, timeout_secs).await
}

/// Execute a shell command with timeout.
///
/// Uses `spawn()` with a channel-based timeout to prevent zombie processes
/// when the timeout fires. On timeout, the child process is killed via
/// platform-specific means (taskkill / kill). The trust boundary here is
/// that this is a local-first personal assistant where the user is the
/// only operator; commands run with the user's own privileges.
async fn run_shell_command(command: &str, timeout_secs: u64) -> Result<(String, i32)> {
    let cmd_str = command.to_string();
    tokio::task::spawn_blocking(move || {
        // Note: /D disables AutoRun, /S strips leading/trailing quotes.
        // This is acceptable for a local-first personal assistant where
        // the user is the only operator.
        #[cfg(windows)]
        let mut cmd = std::process::Command::new("cmd");
        #[cfg(windows)]
        cmd.args(["/D", "/S", "/C", &cmd_str]);
        #[cfg(not(windows))]
        let mut cmd = std::process::Command::new("sh");
        #[cfg(not(windows))]
        cmd.args(["-c", &cmd_str]);

        let child = cmd
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;

        let pid = child.id();
        let timeout = std::time::Duration::from_secs(timeout_secs);

        // Use a channel so we can implement timeout with process kill.
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(child.wait_with_output());
        });

        match rx.recv_timeout(timeout) {
            Ok(output_result) => {
                let output = output_result?;
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let code = output.status.code().unwrap_or(-1);
                Ok((stdout, code))
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                // Timed out — kill the child to prevent zombie processes.
                #[cfg(windows)]
                {
                    let _ = std::process::Command::new("taskkill")
                        .args(["/PID", &pid.to_string(), "/F"])
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .status();
                }
                #[cfg(not(windows))]
                {
                    let _ = std::process::Command::new("kill")
                        .args(["-9", &pid.to_string()])
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .status();
                }
                Err(anyhow::anyhow!("command timed out after {}s", timeout_secs))
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Err(anyhow::anyhow!(
                "command wait thread disconnected unexpectedly"
            )),
        }
    })
    .await
    .map_err(|e| anyhow::anyhow!("spawn_blocking failed: {}", e))?
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execute_command_echo() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let (stdout, code) = rt.block_on(execute_command("echo hello", 5)).unwrap();
        assert!(stdout.contains("hello"));
        assert_eq!(code, 0);
    }

    #[test]
    fn test_cron_expression_parsing() {
        // cron crate requires 7-field format: sec min hour dom month dow year
        assert!(cron::Schedule::from_str("0 0 */6 * * * *").is_ok());
        assert!(cron::Schedule::from_str("0 * * * * * *").is_ok());
        assert!(cron::Schedule::from_str("0 0 0 * * * *").is_ok());
        assert!(cron::Schedule::from_str("0 0 0 1 1 * *").is_ok());
        assert!(cron::Schedule::from_str("invalid").is_err());
    }

    #[tokio::test]
    async fn test_new_scheduler_empty() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("cron.db");
        let scheduler = CronScheduler::new(db_path).await.unwrap();
        let jobs = scheduler.list_jobs().unwrap();
        assert!(jobs.is_empty());
    }

    #[tokio::test]
    async fn test_add_and_list_jobs() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("cron.db");
        let scheduler = CronScheduler::new(db_path).await.unwrap();

        let id = scheduler
            .add_job(
                "test",
                "0 0 */6 * * * *",
                "echo hello",
                SessionMode::Isolated,
                300,
            )
            .await
            .unwrap();

        let jobs = scheduler.list_jobs().unwrap();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].name, "test");
        assert_eq!(jobs[0].cron_expression, "0 0 */6 * * * *");
        assert!(jobs[0].enabled);
        assert_eq!(jobs[0].id, id);
    }

    #[tokio::test]
    async fn test_invalid_cron_expression() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("cron.db");
        let scheduler = CronScheduler::new(db_path).await.unwrap();

        let result = scheduler
            .add_job(
                "bad",
                "not a cron expression",
                "echo hi",
                SessionMode::Isolated,
                300,
            )
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_pause_and_resume_job() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("cron.db");
        let scheduler = CronScheduler::new(db_path).await.unwrap();

        let id = scheduler
            .add_job(
                "pause test",
                "0 * * * * * *",
                "echo hi",
                SessionMode::Isolated,
                300,
            )
            .await
            .unwrap();

        scheduler.pause_job(&id).await.unwrap();
        let job = scheduler.get_job(&id).unwrap().unwrap();
        assert!(!job.enabled);

        scheduler.resume_job(&id).await.unwrap();
        let job = scheduler.get_job(&id).unwrap().unwrap();
        assert!(job.enabled);
    }

    #[tokio::test]
    async fn test_remove_job() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("cron.db");
        let scheduler = CronScheduler::new(db_path).await.unwrap();

        let id = scheduler
            .add_job(
                "delete me",
                "0 * * * * * *",
                "echo bye",
                SessionMode::Isolated,
                300,
            )
            .await
            .unwrap();

        scheduler.remove_job(&id).await.unwrap();
        assert!(scheduler.get_job(&id).unwrap().is_none());
        assert!(scheduler.list_jobs().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_run_now() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("cron.db");
        let scheduler = CronScheduler::new(db_path).await.unwrap();

        let id = scheduler
            .add_job(
                "run now test",
                "0 0 0 1 1 * *",
                "echo instant",
                SessionMode::Isolated,
                5,
            )
            .await
            .unwrap();

        let run_id = scheduler.run_now(&id).await.unwrap();
        assert!(!run_id.is_empty());

        // Give the async task a moment to complete.
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        let history = scheduler.get_history(&id, 1).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].id, run_id);
    }

    #[tokio::test]
    async fn test_restore_jobs_on_restart() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("cron.db");

        // Create and add a job.
        {
            let scheduler = CronScheduler::new(db_path.clone()).await.unwrap();
            scheduler
                .add_job(
                    "persistent",
                    "0 0 0 * * * *",
                    "echo persist",
                    SessionMode::Isolated,
                    300,
                )
                .await
                .unwrap();
        }

        // Reopen — should restore.
        {
            let scheduler = CronScheduler::new(db_path.clone()).await.unwrap();
            let jobs = scheduler.list_jobs().unwrap();
            assert_eq!(jobs.len(), 1);
            assert_eq!(jobs[0].name, "persistent");
            assert!(jobs[0].enabled);
        }
    }

    #[tokio::test]
    async fn test_start_scheduler_loop() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("cron.db");
        let scheduler = Arc::new(CronScheduler::new(db_path).await.unwrap());

        // Start the loop — should not panic.
        let _handle = scheduler.start();
        // Give it a moment.
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }
}

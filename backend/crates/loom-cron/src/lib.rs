// SPDX-License-Identifier: Apache-2.0
//! Cron scheduler for openLoom v2.
//!
//! Provides a SQLite-backed periodic task executor using the `cron` crate for
//! schedule parsing and a custom `tokio::time::interval`-based loop.
//! Jobs are persisted to `cron.db` (separate from memory.db) and automatically
//! restored on restart.
//!
//! ## Architecture
//!
//! Instead of executing shell commands, each cron job stores an **AI prompt**
//! (natural language instruction). When a job fires, the prompt is sent to
//! the AI via a [`PromptExecutor`] implementation provided by the host
//! (typically the Orchestrator). The AI processes the instruction and returns
//! a response — it can call tools, search the web, read files, etc.
//!
//! To use: call [`CronScheduler::set_prompt_executor`] after construction
//! to wire up the AI backend.

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

// ── Prompt Executor ────────────────────────────────────────────────────────────

/// Executes an AI prompt and returns the response text.
///
/// The host (Orchestrator) provides an implementation that sends the prompt
/// to a configured LLM and returns the completion text. The executor may
/// allow the AI to call tools (search, file I/O, etc.) depending on the
/// host's configuration.
pub trait PromptExecutor: Send + Sync {
    /// Execute a prompt and return the AI's response.
    /// Returns an error if no AI backend is available or the request fails.
    fn execute(
        &self,
        prompt: &str,
        timeout_secs: u64,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String>> + Send + '_>>;
}

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
    /// AI prompt executor — set by the host after construction.
    prompt_executor: RwLock<Option<Arc<dyn PromptExecutor>>>,
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
            prompt_executor: RwLock::new(None),
        })
    }

    /// Set the AI prompt executor. Must be called before jobs will execute
    /// successfully — without it, firing jobs will log an error and record
    /// a failure.
    pub async fn set_prompt_executor(&self, executor: Arc<dyn PromptExecutor>) {
        *self.prompt_executor.write().await = Some(executor);
    }

    /// Start the scheduler loop. Runs until the returned JoinHandle is aborted.
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

    /// Add a new cron job with an AI prompt.
    ///
    /// The `prompt` is a natural language instruction that will be sent to
    /// the AI each time this job fires (e.g. "检查服务器状态并发送报告").
    pub async fn add_job(
        &self,
        name: &str,
        cron_expression: &str,
        prompt: &str,
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
            prompt: prompt.to_string(),
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
        let executor = self.prompt_executor.read().await.clone();
        Self::execute_job(&self.storage, &job, &run_id, executor.as_deref()).await;

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
            let executor = self.prompt_executor.read().await.clone();

            tokio::spawn(async move {
                Self::execute_job(&storage, &job, &run_id, executor.as_deref()).await;
                // Remove from running set after completion.
                running.write().await.remove(&jid);
            });
        }
    }

    /// Execute a single job: record history, send prompt to AI, update results.
    async fn execute_job(
        storage: &Arc<CronStorage>,
        job: &CronJob,
        run_id: &str,
        executor: Option<&dyn PromptExecutor>,
    ) {
        let now = chrono::Utc::now().timestamp();

        // Record start.
        if let Err(e) = storage.insert_history(&CronRunHistory {
            id: run_id.to_string(),
            job_id: job.id.clone(),
            started_at: now,
            finished_at: None,
            status: RunStatus::Running,
            response: None,
            error_message: None,
        }) {
            tracing::warn!(run_id = %run_id, job_id = %job.id, error = %e, "failed to insert run history start");
        }

        tracing::info!(run_id = %run_id, job_id = %job.id, name = %job.name, "cron job triggered");

        let result = match executor {
            Some(exec) => exec.execute(&job.prompt, job.timeout_secs).await,
            None => {
                tracing::error!(
                    run_id = %run_id,
                    job_id = %job.id,
                    "no prompt executor configured — cannot execute AI prompt"
                );
                Err(anyhow::anyhow!(
                    "No AI backend configured. Set up a model first — the cron scheduler \
                     requires an AI prompt executor to process job instructions."
                ))
            }
        };

        let finished_at = chrono::Utc::now().timestamp();

        let (status, response, error_message) = match &result {
            Ok(text) => (RunStatus::Completed, Some(text.clone()), None),
            Err(e) => (RunStatus::Failed, None, Some(e.to_string())),
        };

        if let Err(e) = storage.update_history(
            run_id,
            finished_at,
            &status,
            response.as_deref(),
            error_message.as_deref(),
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

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::pin::Pin;

    /// A test executor that echoes the prompt back (no real AI).
    struct EchoExecutor;

    impl PromptExecutor for EchoExecutor {
        fn execute(
            &self,
            prompt: &str,
            _timeout_secs: u64,
        ) -> Pin<Box<dyn std::future::Future<Output = Result<String>> + Send + '_>> {
            let result = Ok(format!("[AI response to: {}]", prompt));
            Box::pin(std::future::ready(result))
        }
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
                "帮我检查系统状态",
                SessionMode::Isolated,
                300,
            )
            .await
            .unwrap();

        let jobs = scheduler.list_jobs().unwrap();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].name, "test");
        assert_eq!(jobs[0].cron_expression, "0 0 */6 * * * *");
        assert_eq!(jobs[0].prompt, "帮我检查系统状态");
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
                "do something",
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
                "check status",
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
                "do cleanup",
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
    async fn test_run_now_with_executor() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("cron.db");
        let scheduler = CronScheduler::new(db_path).await.unwrap();
        scheduler.set_prompt_executor(Arc::new(EchoExecutor)).await;

        let id = scheduler
            .add_job(
                "run now test",
                "0 0 0 1 1 * *",
                "检查天气",
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
        assert_eq!(history[0].status, RunStatus::Completed);
        assert!(history[0]
            .response
            .as_deref()
            .unwrap_or("")
            .contains("检查天气"));
    }

    #[tokio::test]
    async fn test_run_now_without_executor_fails() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("cron.db");
        let scheduler = CronScheduler::new(db_path).await.unwrap();
        // No executor set — should fail

        let id = scheduler
            .add_job(
                "no executor test",
                "0 0 0 1 1 * *",
                "检查天气",
                SessionMode::Isolated,
                5,
            )
            .await
            .unwrap();

        let _run_id = scheduler.run_now(&id).await.unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        let history = scheduler.get_history(&id, 1).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].status, RunStatus::Failed);
        assert!(history[0].error_message.is_some());
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
                    "每日报告",
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
            assert_eq!(jobs[0].prompt, "每日报告");
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

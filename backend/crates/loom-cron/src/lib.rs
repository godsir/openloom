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
use chrono::{DateTime, TimeZone, Utc};
use tokio::sync::RwLock;
use uuid::Uuid;

use job::{CronJob, CronRunHistory, RunStatus, SessionMode};
use storage::CronStorage;

// ── Event Publisher ─────────────────────────────────────────────────────────────

/// Publishes cron lifecycle events to external observers (e.g. WebSocket UI).
/// The host (Orchestrator) provides an implementation wired to the EventBus.
pub trait CronEventPublisher: Send + Sync {
    fn job_triggered(&self, job_id: &str, job_name: &str, run_id: &str);
    fn job_completed(&self, job_id: &str, job_name: &str, run_id: &str, response: &str);
    fn job_failed(&self, job_id: &str, job_name: &str, run_id: &str, error: &str);
    fn job_changed(&self, job_id: &str, action: &str);
}

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
    /// The next instant at which this job should fire (in-memory source of truth).
    ///
    /// `None` means the schedule has no further occurrences (e.g. a one-shot whose
    /// time has fully passed) — such a job is never fired by the tick loop.
    next_fire: Option<DateTime<Utc>>,
}

/// Compute the next fire time strictly after `after` for the given schedule.
///
/// Returns `None` when the schedule has no further occurrences (e.g. a one-shot
/// date that lies entirely in the past). `cron`'s `after().next()` yields the first
/// match *after* the supplied instant.
fn next_fire_after(schedule: &cron::Schedule, after: DateTime<Utc>) -> Option<DateTime<Utc>> {
    schedule.after(&after).next()
}

/// 定时任务的最小触发间隔（秒）。
///
/// 低于此频率的表达式（如秒级 `* * * * * *`、`*/5 * * * * *`）会让 agent 回合被
/// 高频触发——每次触发都跑一个**带全部工具权限的无人值守执行**，既烧 token 又
/// 扩大安全面。故对相邻两次未来触发时间的间隔设下限。
const MIN_CRON_INTERVAL_SECS: i64 = 60;

/// 校验 cron 表达式的触发频率不低于 [`MIN_CRON_INTERVAL_SECS`]。
///
/// 取从 `from` 起相邻的两次未来触发：间隔小于下限则拒绝。只有一次（或零次）
/// 未来触发的表达式（一次性任务）不受此限制。
fn validate_cron_frequency(schedule: &cron::Schedule, from: DateTime<Utc>) -> Result<()> {
    let mut it = schedule.after(&from);
    if let (Some(a), Some(b)) = (it.next(), it.next()) {
        let gap = (b - a).num_seconds();
        if gap < MIN_CRON_INTERVAL_SECS {
            return Err(anyhow::anyhow!(
                "定时任务触发过于频繁（间隔 {gap} 秒）：最小允许间隔为 {MIN_CRON_INTERVAL_SECS} 秒，\
                 以免高频触发无人值守的 agent 执行（费用与安全风险）"
            ));
        }
    }
    Ok(())
}

/// Compute the initial fire time for a job, honoring `last_run` for restart catch-up.
///
/// If the job has a recorded `last_run`, the next fire is computed relative to that
/// timestamp — so a slot missed while the process was down still fires (once) on the
/// next tick. Otherwise it is computed relative to `now`. A returned instant that is
/// already `<= now` is intentional: the tick loop will fire it once and coalesce any
/// further missed slots.
fn initial_next_fire(
    schedule: &cron::Schedule,
    last_run: Option<i64>,
    now: DateTime<Utc>,
) -> Option<DateTime<Utc>> {
    let after = last_run
        .and_then(|ts| Utc.timestamp_opt(ts, 0).single())
        .unwrap_or(now);
    next_fire_after(schedule, after)
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
    /// Optional event publisher for notifying UI of job lifecycle changes.
    event_publisher: RwLock<Option<Arc<dyn CronEventPublisher>>>,
    /// Semaphore to limit concurrent job executions (default 3).
    concurrency_limit: Arc<tokio::sync::Semaphore>,
}

impl CronScheduler {
    /// Create a new scheduler and load persisted jobs from the database.
    pub async fn new(db_path: PathBuf) -> Result<Self> {
        let storage = Arc::new(CronStorage::open(&db_path)?);
        let mut active = HashMap::new();
        let now = Utc::now();

        // Restore persisted enabled jobs.
        let jobs = storage.load_enabled_jobs()?;
        for mut job in jobs {
            let schedule = cron::Schedule::from_str(&job.cron_expression).with_context(|| {
                format!(
                    "invalid cron expression for job '{}': {}",
                    job.name, job.cron_expression
                )
            })?;
            // Compute the next fire time, honoring last_run for restart catch-up.
            let next_fire = initial_next_fire(&schedule, job.last_run, now);
            let next_run = next_fire.map(|t| t.timestamp());
            job.next_run = next_run;
            // Persist the freshly computed next_run so it is no longer perpetually NULL.
            if let Err(e) = storage.update_next_run(&job.id, next_run) {
                tracing::warn!(job_id = %job.id, error = %e, "failed to persist next_run on load");
            }
            active.insert(
                job.id.clone(),
                ActiveJob {
                    schedule,
                    job,
                    next_fire,
                },
            );
        }

        if !active.is_empty() {
            tracing::info!(count = active.len(), "restored cron jobs from database");
        }

        // Recover interrupted jobs — mark any still-"running" history records
        // as failed so they don't appear as permanent zombies after a crash.
        if let Err(e) = storage.recover_interrupted_jobs() {
            tracing::warn!(error = %e, "failed to recover interrupted cron jobs");
        }

        Ok(Self {
            active: RwLock::new(active),
            storage,
            db_path,
            running: Arc::new(RwLock::new(HashSet::new())),
            prompt_executor: RwLock::new(None),
            event_publisher: RwLock::new(None),
            concurrency_limit: Arc::new(tokio::sync::Semaphore::new(3)),
        })
    }

    /// Set the AI prompt executor. Must be called before jobs will execute
    /// successfully — without it, firing jobs will log an error and record
    /// a failure.
    pub async fn set_prompt_executor(&self, executor: Arc<dyn PromptExecutor>) {
        *self.prompt_executor.write().await = Some(executor);
    }

    /// Set the event publisher for notifying UI of job lifecycle changes.
    pub async fn set_event_publisher(&self, publisher: Arc<dyn CronEventPublisher>) {
        *self.event_publisher.write().await = Some(publisher);
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

    /// Helper: publish a CronJobChanged event if a publisher is configured.
    async fn notify_changed(&self, job_id: &str, action: &str) {
        if let Some(pub_) = self.event_publisher.read().await.as_ref() {
            pub_.job_changed(job_id, action);
        }
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
        if timeout_secs > 3600 {
            tracing::warn!(%timeout_secs, "clamping cron job timeout to 3600s");
        }
        let timeout_secs = timeout_secs.min(3600); // Cap at 1 hour.

        // Validate the cron expression.
        let schedule = cron::Schedule::from_str(cron_expression)
            .with_context(|| format!("invalid cron expression: {}", cron_expression))?;
        // 频率下限：拒绝秒级等高频触发（防无人值守高频执行）。
        validate_cron_frequency(&schedule, Utc::now())?;

        let id = Uuid::new_v4().to_string();
        let now = Utc::now();
        let created_at = now.timestamp();

        // Compute the first fire time so next_run is populated from the start.
        let next_fire = next_fire_after(&schedule, now);
        let next_run = next_fire.map(|t| t.timestamp());

        let job = CronJob {
            id: id.clone(),
            name: name.to_string(),
            cron_expression: cron_expression.to_string(),
            prompt: prompt.to_string(),
            enabled: true,
            session_mode,
            timeout_secs,
            created_at,
            last_run: None,
            next_run,
            run_count: 0,
            error_count: 0,
        };

        // Persist.
        self.storage.insert_job(&job)?;

        // Register in-memory.
        self.active.write().await.insert(
            id.clone(),
            ActiveJob {
                schedule,
                job,
                next_fire,
            },
        );

        self.notify_changed(&id, "created").await;
        tracing::info!(id = %id, name = %name, cron = %cron_expression, "cron job added");
        Ok(id)
    }

    /// Update an existing cron job. Re-registers in-memory if the job is enabled.
    pub async fn update_job(
        &self,
        job_id: &str,
        name: &str,
        cron_expression: &str,
        prompt: &str,
        session_mode: SessionMode,
        timeout_secs: u64,
    ) -> Result<()> {
        // Validate timeout bounds.
        if timeout_secs == 0 {
            return Err(anyhow::anyhow!("timeout_secs must be at least 1 second"));
        }
        let timeout_secs = timeout_secs.min(3600);

        // Validate the cron expression.
        let schedule = cron::Schedule::from_str(cron_expression)
            .with_context(|| format!("invalid cron expression: {}", cron_expression))?;
        // 频率下限：拒绝秒级等高频触发（防无人值守高频执行）。
        validate_cron_frequency(&schedule, Utc::now())?;

        // Persist update.
        self.storage.update_job(
            job_id,
            name,
            cron_expression,
            prompt,
            &session_mode,
            timeout_secs,
        )?;

        // Re-register or remove from active set based on the updated job's enabled state.
        let mut active = self.active.write().await;
        if let Some(mut job) = self.storage.get_job(job_id)? {
            if job.enabled {
                // Recompute next_fire from now (the schedule may have changed).
                let next_fire = next_fire_after(&schedule, Utc::now());
                let next_run = next_fire.map(|t| t.timestamp());
                job.next_run = next_run;
                if let Err(e) = self.storage.update_next_run(job_id, next_run) {
                    tracing::warn!(id = %job_id, error = %e, "failed to persist next_run on update");
                }
                active.insert(
                    job_id.to_string(),
                    ActiveJob {
                        schedule,
                        job,
                        next_fire,
                    },
                );
            } else {
                active.remove(job_id);
            }
        }

        self.notify_changed(job_id, "updated").await;
        tracing::info!(id = %job_id, name = %name, cron = %cron_expression, "cron job updated");
        Ok(())
    }

    /// Remove a cron job.
    pub async fn remove_job(&self, job_id: &str) -> Result<()> {
        self.active.write().await.remove(job_id);
        self.storage.delete_job(job_id)?;
        self.notify_changed(job_id, "deleted").await;
        tracing::info!(id = %job_id, "cron job removed");
        Ok(())
    }

    /// Pause a job.
    pub async fn pause_job(&self, job_id: &str) -> Result<()> {
        self.storage.set_enabled(job_id, false)?;
        self.active.write().await.remove(job_id);
        self.notify_changed(job_id, "paused").await;
        tracing::info!(id = %job_id, "cron job paused");
        Ok(())
    }

    /// Resume a paused job.
    pub async fn resume_job(&self, job_id: &str) -> Result<()> {
        self.storage.set_enabled(job_id, true)?;
        if let Some(mut job) = self.storage.get_job(job_id)? {
            let schedule = cron::Schedule::from_str(&job.cron_expression)
                .with_context(|| format!("invalid cron expression: {}", job.cron_expression))?;
            // Recompute next_fire from now so a paused-then-resumed job doesn't
            // immediately fire a backlog from before it was paused.
            let next_fire = next_fire_after(&schedule, Utc::now());
            let next_run = next_fire.map(|t| t.timestamp());
            job.next_run = next_run;
            if let Err(e) = self.storage.update_next_run(job_id, next_run) {
                tracing::warn!(id = %job_id, error = %e, "failed to persist next_run on resume");
            }
            self.active.write().await.insert(
                job_id.to_string(),
                ActiveJob {
                    schedule,
                    job,
                    next_fire,
                },
            );
        }
        self.notify_changed(job_id, "resumed").await;
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
    ///
    /// Acquires a concurrency permit (so manual runs respect the same global limit
    /// as scheduled fires) and atomically marks the job running to prevent a
    /// double-execution race with the tick loop or another `run_now` call.
    pub async fn run_now(&self, job_id: &str) -> Result<String> {
        let job = self
            .storage
            .get_job(job_id)?
            .ok_or_else(|| anyhow::anyhow!("cron job not found: {job_id}"))?;

        // Atomically check-and-mark: `insert` returns false if already present, so a
        // concurrent run_now / tick can never both pass this gate. Hold the write lock
        // across the whole check+insert (a read-then-write would be racy).
        {
            let mut running = self.running.write().await;
            if !running.insert(job_id.to_string()) {
                return Err(anyhow::anyhow!("job already running: {job_id}"));
            }
        }

        // Acquire a permit so manual runs honor the global concurrency limit.
        // On error (semaphore closed), unmark before bailing so the job isn't
        // left permanently stuck in the running set.
        let permit = match self.concurrency_limit.clone().acquire_owned().await {
            Ok(p) => p,
            Err(e) => {
                self.running.write().await.remove(job_id);
                return Err(anyhow::anyhow!("cron concurrency semaphore closed: {e}"));
            }
        };

        let run_id = Uuid::new_v4().to_string();
        let executor = self.prompt_executor.read().await.clone();
        let publisher = self.event_publisher.read().await.clone();
        Self::execute_job(
            &self.storage,
            &job,
            &run_id,
            executor.as_deref(),
            publisher.as_deref(),
        )
        .await;
        drop(permit);

        // Remove from running set.
        self.running.write().await.remove(job_id);

        Ok(run_id)
    }

    // ── Internal ──────────────────────────────────────────────────────────

    /// Check all active jobs and fire any whose next fire time has arrived.
    ///
    /// Firing is driven by a per-job `next_fire` instant (not by `schedule.includes(now)`
    /// on the 1s tick). On each tick we fire every job whose `next_fire <= now`, then
    /// advance `next_fire` to the next occurrence strictly after `now` — coalescing any
    /// slots missed while the process was busy/asleep into a single catch-up fire. This
    /// also means jobs no longer depend on the tick landing exactly on second 0.
    async fn check_and_fire(&self) {
        let now = Utc::now();

        // Phase 1: under the write lock, find due jobs and advance their next_fire.
        // We advance past `now` (coalescing missed slots) and snapshot the job + its
        // freshly computed next_run for firing + persistence outside the lock.
        let mut due: Vec<(String, CronJob, Option<i64>)> = Vec::new();
        {
            let mut active = self.active.write().await;
            for (job_id, entry) in active.iter_mut() {
                if !entry.job.enabled {
                    continue;
                }
                let Some(fire_at) = entry.next_fire else {
                    continue; // No further occurrences.
                };
                if fire_at > now {
                    continue; // Not due yet.
                }

                // Advance next_fire past `now`, coalescing every missed slot into one fire.
                let mut next = next_fire_after(&entry.schedule, fire_at);
                while let Some(t) = next {
                    if t > now {
                        break;
                    }
                    next = next_fire_after(&entry.schedule, t);
                }
                entry.next_fire = next;
                let next_run = next.map(|t| t.timestamp());
                entry.job.next_run = next_run;

                due.push((job_id.clone(), entry.job.clone(), next_run));
            }
        }

        // Phase 2: persist next_run and dispatch each due job.
        for (job_id, job, next_run) in due {
            // Persist the advanced next_run (best-effort scheduling metadata).
            if let Err(e) = self.storage.update_next_run(&job_id, next_run) {
                tracing::warn!(job_id = %job_id, error = %e, "failed to persist next_run after fire");
            }

            // Atomically claim the job: `insert` returns false if it is already running,
            // in which case we skip this fire (the in-flight run covers it). The marker
            // is set BEFORE spawning; the permit is acquired INSIDE the task so a full
            // semaphore never blocks this dispatch loop while holding the marker.
            if !self.running.write().await.insert(job_id.clone()) {
                tracing::debug!(job_id = %job_id, "skipping fire — job already running");
                continue;
            }

            let storage = self.storage.clone();
            let run_id = Uuid::new_v4().to_string();
            let running = self.running.clone();
            let semaphore = self.concurrency_limit.clone();
            let executor = self.prompt_executor.read().await.clone();
            let publisher = self.event_publisher.read().await.clone();

            tokio::spawn(async move {
                // Acquire the permit here (inside the task) so a saturated semaphore
                // does not stall the dispatch loop. Held until the job completes.
                let _permit = match semaphore.acquire_owned().await {
                    Ok(p) => p,
                    Err(e) => {
                        tracing::error!(job_id = %job_id, error = %e, "cron semaphore closed; aborting fire");
                        running.write().await.remove(&job_id);
                        return;
                    }
                };
                Self::execute_job(
                    &storage,
                    &job,
                    &run_id,
                    executor.as_deref(),
                    publisher.as_deref(),
                )
                .await;
                // Remove from running set after completion.
                running.write().await.remove(&job_id);
            });
        }
    }

    /// Execute a single job: record history, send prompt to AI, update results.
    async fn execute_job(
        storage: &Arc<CronStorage>,
        job: &CronJob,
        run_id: &str,
        executor: Option<&dyn PromptExecutor>,
        publisher: Option<&dyn CronEventPublisher>,
    ) {
        let now = chrono::Utc::now().timestamp();

        // Notify observers that the job started.
        if let Some(pub_) = publisher {
            pub_.job_triggered(&job.id, &job.name, run_id);
        }

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

        // Notify observers of completion or failure.
        if let Some(pub_) = publisher {
            match &result {
                Ok(text) => pub_.job_completed(&job.id, &job.name, run_id, text),
                Err(e) => pub_.job_failed(&job.id, &job.name, run_id, &e.to_string()),
            }
        }

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

        // Auto-disable one-shot tasks after successful execution.
        // One-shot cron expressions have a specific year (e.g. "0 30 14 9 6 2026 *")
        // vs recurring ones which have "*" in the year field.
        if status == RunStatus::Completed {
            let is_one_shot = job
                .cron_expression
                .split_whitespace()
                .nth(6) // year field (7-field format: sec min hour dom month dow year)
                .map(|y| y.chars().all(|c| c.is_ascii_digit()))
                .unwrap_or(false);
            if is_one_shot {
                if let Err(e) = storage.set_enabled(&job.id, false) {
                    tracing::warn!(job_id = %job.id, error = %e, "failed to auto-disable one-shot job");
                } else {
                    tracing::info!(job_id = %job.id, name = %job.name, "auto-disabled one-shot cron job after completion");
                }
            }
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

    #[test]
    fn test_validate_cron_frequency_rejects_subminute() {
        // 每秒 / 每 5 秒 / 每 30 秒触发 → 必须拒绝（高频无人值守执行风险）
        for expr in ["* * * * * * *", "*/5 * * * * * *", "*/30 * * * * * *"] {
            let s = cron::Schedule::from_str(expr)
                .unwrap_or_else(|e| panic!("test expr {expr} should parse: {e}"));
            assert!(
                validate_cron_frequency(&s, Utc::now()).is_err(),
                "应拒绝高频表达式 {expr}"
            );
        }
    }

    #[test]
    fn test_validate_cron_frequency_allows_minute_plus() {
        // 每分钟 / 每 6 小时 / 每天 9 点 → 允许
        for expr in ["0 * * * * * *", "0 0 */6 * * * *", "0 0 9 * * * *"] {
            let s = cron::Schedule::from_str(expr)
                .unwrap_or_else(|e| panic!("test expr {expr} should parse: {e}"));
            assert!(
                validate_cron_frequency(&s, Utc::now()).is_ok(),
                "应允许 >=1min 表达式 {expr}"
            );
        }
    }

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
    async fn test_update_job() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("cron.db");
        let scheduler = CronScheduler::new(db_path).await.unwrap();

        let id = scheduler
            .add_job(
                "original",
                "0 * * * * * *",
                "原始指令",
                SessionMode::Isolated,
                300,
            )
            .await
            .unwrap();

        scheduler
            .update_job(
                &id,
                "updated",
                "0 0 12 * * * *",
                "更新后的指令",
                SessionMode::Current,
                600,
            )
            .await
            .unwrap();

        let jobs = scheduler.list_jobs().unwrap();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].name, "updated");
        assert_eq!(jobs[0].cron_expression, "0 0 12 * * * *");
        assert_eq!(jobs[0].prompt, "更新后的指令");
        assert_eq!(jobs[0].session_mode, SessionMode::Current);
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
        assert!(
            history[0]
                .response
                .as_deref()
                .unwrap_or("")
                .contains("检查天气")
        );
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

    // ── Fix #1: next_fire / next_run regression coverage ──────────────────

    #[tokio::test]
    async fn test_next_run_populated_on_add() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("cron.db");
        let scheduler = CronScheduler::new(db_path).await.unwrap();

        let id = scheduler
            .add_job(
                "next run",
                "0 * * * * * *", // top of every minute
                "do thing",
                SessionMode::Isolated,
                300,
            )
            .await
            .unwrap();

        // Previously next_run was always None; it must now be persisted and in the future.
        let job = scheduler.get_job(&id).unwrap().unwrap();
        let next = job.next_run.expect("next_run must be populated on add");
        assert!(
            next >= chrono::Utc::now().timestamp(),
            "next_run should be in the future, got {next}"
        );

        // list_jobs surfaces the same value.
        let summary = &scheduler.list_jobs().unwrap()[0];
        assert_eq!(summary.next_run, Some(next));
    }

    #[tokio::test]
    async fn test_next_run_persisted_across_restart() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("cron.db");

        let id = {
            let scheduler = CronScheduler::new(db_path.clone()).await.unwrap();
            scheduler
                .add_job(
                    "daily",
                    "0 0 0 * * * *",
                    "report",
                    SessionMode::Isolated,
                    300,
                )
                .await
                .unwrap()
        };

        // Reopen — next_run must be (re)computed and persisted, not NULL.
        {
            let scheduler = CronScheduler::new(db_path.clone()).await.unwrap();
            let job = scheduler.get_job(&id).unwrap().unwrap();
            assert!(
                job.next_run.is_some(),
                "next_run must be computed on restore"
            );
        }
    }

    #[tokio::test]
    async fn test_scheduler_fires_via_next_fire() {
        // Proves the loop fires a sub-minute job WITHOUT relying on includes(now)
        // landing exactly on second 0 — the every-second schedule should fire.
        //
        // NOTE: 直接经 storage + active 注册，绕过 add_job 的频率下限策略
        // （那是面向 agent 创建任务的安全策略）；这里测的是调度循环本身的点火
        // 机制——引擎内部仍可对任意已注册的调度（含秒级）正常触发。
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("cron.db");
        let scheduler = Arc::new(CronScheduler::new(db_path).await.unwrap());
        scheduler.set_prompt_executor(Arc::new(EchoExecutor)).await;

        let schedule = cron::Schedule::from_str("* * * * * * *").unwrap(); // every second
        let now = Utc::now();
        let next_fire = next_fire_after(&schedule, now);
        let job = CronJob {
            id: Uuid::new_v4().to_string(),
            name: "every second".to_string(),
            cron_expression: "* * * * * * *".to_string(),
            prompt: "tick".to_string(),
            enabled: true,
            session_mode: SessionMode::Isolated,
            timeout_secs: 5,
            created_at: now.timestamp(),
            last_run: None,
            next_run: next_fire.map(|t| t.timestamp()),
            run_count: 0,
            error_count: 0,
        };
        let id = job.id.clone();
        scheduler.storage.insert_job(&job).unwrap();
        scheduler.active.write().await.insert(
            id.clone(),
            ActiveJob {
                schedule,
                job,
                next_fire,
            },
        );

        let _handle = scheduler.start();
        // Wait long enough for several 1s ticks to elapse.
        tokio::time::sleep(tokio::time::Duration::from_millis(2500)).await;

        let history = scheduler.get_history(&id, 10).unwrap();
        assert!(
            history.iter().any(|h| h.status == RunStatus::Completed),
            "expected at least one completed run from the tick loop, got {} record(s)",
            history.len()
        );
    }

    #[test]
    fn test_next_fire_after_advances_strictly() {
        // Helper sanity: next_fire_after yields a strictly-later instant and the
        // coalescing loop terminates.
        let schedule = cron::Schedule::from_str("* * * * * * *").unwrap();
        let t0 = chrono::Utc::now();
        let t1 = next_fire_after(&schedule, t0).expect("every-second schedule always has a next");
        assert!(t1 > t0);

        // A one-shot fully in the past yields None (no further occurrences).
        let past = cron::Schedule::from_str("0 0 0 1 1 * 2000").unwrap();
        assert!(next_fire_after(&past, chrono::Utc::now()).is_none());
    }

    #[test]
    fn test_initial_next_fire_honors_last_run() {
        // With a last_run far in the past and a per-minute schedule, the initial
        // next_fire is computed relative to last_run (enabling restart catch-up),
        // so it lands at/just-after that historical timestamp rather than now.
        let schedule = cron::Schedule::from_str("0 * * * * * *").unwrap();
        let now = chrono::Utc::now();
        let last_run = now.timestamp() - 3600; // one hour ago
        let nf =
            initial_next_fire(&schedule, Some(last_run), now).expect("schedule has occurrences");
        // The computed slot should be no later than `now` (it's in the past relative
        // to now), proving the tick loop will treat it as a due catch-up fire.
        assert!(nf.timestamp() <= now.timestamp());
        assert!(nf.timestamp() > last_run);
    }
}

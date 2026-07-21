// SPDX-License-Identifier: Apache-2.0
//! Types for the cron scheduler — job definitions, session modes, run statuses.

use serde::{Deserialize, Serialize};

/// Execution mode for a cron job (inspired by OpenClaw's session modes).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SessionMode {
    /// Each trigger runs in an isolated session — no shared context.
    Isolated,
    /// Each trigger reuses the session that created the job, preserving history.
    Current,
}

impl std::fmt::Display for SessionMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionMode::Isolated => write!(f, "isolated"),
            SessionMode::Current => write!(f, "current"),
        }
    }
}

/// A single cron job definition — persisted to SQLite.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronJob {
    /// UUID v4.
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Standard 5-field or 6-field cron expression (e.g. "0 */6 * * *").
    pub cron_expression: String,
    /// AI prompt — natural language instruction sent to the AI when this job fires.
    pub prompt: String,
    /// Whether this job is actively scheduled.
    pub enabled: bool,
    /// Execution isolation mode.
    pub session_mode: SessionMode,
    /// Timeout in seconds for the AI execution (default 300).
    pub timeout_secs: u64,
    /// Optional model name override — when set, this job runs on the named model
    /// instead of the currently active model. None = use the active model.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Unix timestamp (seconds) when this job was created.
    pub created_at: i64,
    /// Unix timestamp (seconds) of the last successful run.
    pub last_run: Option<i64>,
    /// Unix timestamp (seconds) of the next scheduled run.
    pub next_run: Option<i64>,
    /// Total successful executions.
    pub run_count: u64,
    /// Total failed executions.
    pub error_count: u64,
}

/// Status of a single cron job execution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Running,
    Completed,
    Failed,
    TimedOut,
}

/// Execution history record — persisted to SQLite.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronRunHistory {
    /// UUID v4.
    pub id: String,
    /// FK to cron_jobs.id.
    pub job_id: String,
    /// Unix timestamp (seconds) when execution started.
    pub started_at: i64,
    /// Unix timestamp (seconds) when execution finished (None if still running).
    pub finished_at: Option<i64>,
    /// Execution outcome.
    pub status: RunStatus,
    /// AI response text (when execution completed successfully).
    pub response: Option<String>,
    /// Error message (when execution failed or timed out).
    pub error_message: Option<String>,
}

/// Summary returned by list operations — lighter than full CronJob + last result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronJobSummary {
    pub id: String,
    pub name: String,
    pub cron_expression: String,
    pub prompt: String,
    pub enabled: bool,
    pub session_mode: SessionMode,
    /// Model override for this job (None = active model).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub last_run: Option<i64>,
    pub next_run: Option<i64>,
    pub run_count: u64,
    pub error_count: u64,
    /// Status of the most recent run (if any).
    pub last_status: Option<RunStatus>,
}

impl From<&CronJob> for CronJobSummary {
    fn from(job: &CronJob) -> Self {
        Self {
            id: job.id.clone(),
            name: job.name.clone(),
            cron_expression: job.cron_expression.clone(),
            prompt: job.prompt.clone(),
            enabled: job.enabled,
            session_mode: job.session_mode.clone(),
            model: job.model.clone(),
            last_run: job.last_run,
            next_run: job.next_run,
            run_count: job.run_count,
            error_count: job.error_count,
            last_status: None,
        }
    }
}

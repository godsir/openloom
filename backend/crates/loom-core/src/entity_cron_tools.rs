//! Cron job management tool — CRUD wrapper for CronScheduler.
//! Each tool delegates to the CronScheduler which persists to SQLite.
//!
//! See orchestrator.rs for registration.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use loom_types::{ToolDefinition, ToolProgress};
use tokio::sync::RwLock;
use tokio::sync::mpsc::UnboundedSender;

use loom_cron::CronScheduler;
use loom_cron::job::SessionMode;

use crate::tool_context::ToolContext;
use crate::tool_registry::{AgentTool, ToolProvenance, ToolResult};

fn resolve_scheduler<'a>(
    guard: &'a tokio::sync::RwLockReadGuard<'_, Option<Arc<CronScheduler>>>,
) -> Result<&'a CronScheduler> {
    guard
        .as_ref()
        .map(|arc| arc.as_ref())
        .ok_or_else(|| anyhow::anyhow!("CronScheduler not initialized"))
}

// ============================================================================
// manage_cron
// ============================================================================

pub struct ManageCronTool {
    pub cron_scheduler: Arc<RwLock<Option<Arc<CronScheduler>>>>,
}

#[async_trait]
impl AgentTool for ManageCronTool {
    fn tool_name(&self) -> &str {
        "manage_cron"
    }

    fn tool_definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "manage_cron".into(),
            description: "Manage scheduled cron jobs (定时任务). Use when user wants to schedule a recurring task, pause a schedule, resume it, run it now, or delete it.\n\nCommon scenarios:\n- \"run a daily code review every morning at 9am\": action=create, name + prompt + cron_expression=0 9 * * *\n- \"stop the daily report\": action=pause, id=<job_id>\n- \"run the weekly backup now\": action=run_now, id=<job_id>\n- \"show all scheduled tasks\": action=list\n- \"delete the old reminder\": action=delete, id=<job_id>\n\nActions: list | get | create | update | delete | pause | resume | run_now | history.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["list", "get", "create", "update", "delete", "pause", "resume", "run_now", "history"] },
                    "id": { "type": "string", "description": "Job ID (UUID)" },
                    "name": { "type": "string", "description": "Human-readable job name" },
                    "prompt": { "type": "string", "description": "Natural language AI instruction to execute when triggered" },
                    "cron_expression": { "type": "string", "description": "Standard cron expression (5 or 6 fields), e.g. '0 */6 * * *'" },
                    "session_mode": { "type": "string", "enum": ["isolated", "current"], "description": "Execution isolation mode" },
                    "timeout_secs": { "type": "integer", "description": "Timeout in seconds for the AI execution (default 300)" },
                    "limit": { "type": "integer", "description": "Max history entries to return (default 20)" }
                },
                "required": ["action"]
            }),
            tags: vec![],
        }
    }

    fn supports_parallel(&self) -> bool {
        true
    }

    async fn execute(
        &self,
        arguments: serde_json::Value,
        _progress: UnboundedSender<ToolProgress>,
        _ctx: &ToolContext,
    ) -> Result<ToolResult> {
        let action = arguments["action"].as_str().unwrap_or("");
        let guard = self.cron_scheduler.read().await;
        let scheduler = resolve_scheduler(&guard)?;
        let result = exec_cron(action, &arguments, scheduler).await?;
        Ok(ToolResult {
            content: result,
            is_error: false,
            structured_content: None,
        })
    }

    fn provenance(&self) -> ToolProvenance {
        ToolProvenance::Builtin
    }
}

async fn exec_cron(
    action: &str,
    args: &serde_json::Value,
    scheduler: &CronScheduler,
) -> Result<String> {
    match action {
        "list" => {
            let jobs = scheduler.list_jobs()?;
            Ok(serde_json::to_string_pretty(&jobs).unwrap_or_else(|e| e.to_string()))
        }
        "get" => {
            let id = req_str(args, "id")?;
            match scheduler.get_job(id)? {
                Some(job) => Ok(serde_json::to_string_pretty(&job).unwrap_or_else(|e| e.to_string())),
                None => Ok(format!("Cron job \"{id}\" not found.")),
            }
        }
        "create" => {
            let name = req_str(args, "name")?;
            let cron_expr = req_str(args, "cron_expression")?;
            let prompt = args["prompt"].as_str().unwrap_or("");
            if prompt.is_empty() {
                return Err(anyhow::anyhow!("prompt is required"));
            }
            let session_mode = parse_session_mode(args, "session_mode");
            let timeout_secs = args
                .get("timeout_secs")
                .and_then(|v| v.as_u64())
                .unwrap_or(300)
                .min(3600);
            // Optional per-job model override (empty/absent = active model).
            let model = args
                .get("model")
                .and_then(|v| v.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            let id = scheduler
                .add_job(name, cron_expr, prompt, session_mode, timeout_secs, model)
                .await?;
            Ok(format!("Cron job \"{name}\" created (id: {id})."))
        }
        "update" => {
            let id = req_str(args, "id")?;
            let existing = scheduler
                .get_job(id)?
                .ok_or_else(|| anyhow::anyhow!("Cron job \"{id}\" not found"))?;
            let name = args["name"].as_str().unwrap_or(&existing.name);
            let cron_expr = args["cron_expression"].as_str().unwrap_or(&existing.cron_expression);
            let prompt = args["prompt"].as_str().unwrap_or(&existing.prompt);
            let session_mode = if args.get("session_mode").is_some() {
                parse_session_mode(args, "session_mode")
            } else {
                existing.session_mode
            };
            let timeout_secs = args
                .get("timeout_secs")
                .and_then(|v| v.as_u64())
                .unwrap_or(existing.timeout_secs)
                .min(3600);
            let model = if args.get("model").is_some() {
                args.get("model")
                    .and_then(|v| v.as_str())
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
            } else {
                existing.model
            };
            scheduler
                .update_job(id, name, cron_expr, prompt, session_mode, timeout_secs, model)
                .await?;
            Ok(format!("Cron job \"{name}\" updated."))
        }
        "delete" => {
            let id = req_str(args, "id")?;
            scheduler.remove_job(id).await?;
            Ok(format!("Cron job \"{id}\" deleted."))
        }
        "pause" => {
            let id = req_str(args, "id")?;
            scheduler.pause_job(id).await?;
            Ok(format!("Cron job \"{id}\" paused."))
        }
        "resume" => {
            let id = req_str(args, "id")?;
            scheduler.resume_job(id).await?;
            Ok(format!("Cron job \"{id}\" resumed."))
        }
        "run_now" => {
            let id = req_str(args, "id")?;
            let run_id = scheduler.run_now(id).await?;
            Ok(format!("Cron job \"{id}\" triggered (run_id: {run_id})."))
        }
        "history" => {
            let id = req_str(args, "id")?;
            let limit = args
                .get("limit")
                .and_then(|v| v.as_u64())
                .unwrap_or(20)
                .min(1000) as usize;
            let history = scheduler.get_history(id, limit)?;
            Ok(serde_json::to_string_pretty(&history).unwrap_or_else(|e| e.to_string()))
        }
        _ => Err(anyhow::anyhow!(
            "Unknown action: {action}. Use list | get | create | update | delete | pause | resume | run_now | history."
        )),
    }
}

fn parse_session_mode(args: &serde_json::Value, key: &str) -> SessionMode {
    match args[key].as_str() {
        Some("current") => SessionMode::Current,
        _ => SessionMode::Isolated,
    }
}

fn req_str<'a>(args: &'a serde_json::Value, field: &str) -> Result<&'a str> {
    args[field]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("{field} required"))
}

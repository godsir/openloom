//! JSON-RPC dispatch for `cron.*` methods.
//!
//! Methods: cron.list, cron.create, cron.update, cron.delete, cron.pause,
//!          cron.resume, cron.history, cron.run_now, cron.detect
//!
//! v2: Jobs store an AI prompt (natural language instruction) instead of a
//!     shell command. When a job fires, the prompt is sent to the AI for
//!     execution via the PromptExecutor.

use std::sync::Arc;

use anyhow::Result;
use loom_cron::detector;
use loom_cron::job::SessionMode;
use loom_types::{ErrorCode, JsonRpcError};
use serde_json::{Value, json};

use crate::AppState;
use crate::dispatch::err;

pub async fn handle(
    state: &AppState,
    method: &str,
    params: &Value,
) -> Option<Result<Value, JsonRpcError>> {
    // cron.detect can work without cron scheduler
    if method == "cron.detect" {
        return Some(handle_detect(state, params).await);
    }

    let cron = match state.orchestrator.cron_scheduler().await {
        Some(c) => c,
        None => {
            return Some(Err(err(
                ErrorCode::InternalError,
                "Cron scheduler is not initialized. This may happen if the cron database failed to open. Check the server logs for details.",
            )));
        }
    };

    match method {
        "cron.list" => Some(handle_list(&cron).await),
        "cron.create" => Some(handle_create(&cron, params).await),
        "cron.update" => Some(handle_update(&cron, params).await),
        "cron.delete" => Some(handle_delete(&cron, params).await),
        "cron.pause" => Some(handle_pause(&cron, params).await),
        "cron.resume" => Some(handle_resume(&cron, params).await),
        "cron.history" => Some(handle_history(&cron, params).await),
        "cron.run_now" => Some(handle_run_now(&cron, params).await),
        _ => None,
    }
}

// ── detect ──

async fn handle_detect(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let message = p.get("message").and_then(|v| v.as_str()).unwrap_or("");
    if message.is_empty() || !detector::pre_scan(message) {
        return Ok(json!({ "should_create": false }));
    }

    let now = chrono::Utc::now();
    let prompt = detector::build_extraction_prompt(message, &now);

    // Use get_cloud_client for proper async — no block_on.
    let client = state.orchestrator.get_cloud_client().await;
    let client = match client {
        Some(c) => c,
        None => {
            tracing::warn!("cron.detect: no cloud client configured");
            return Ok(json!({ "should_create": false }));
        }
    };

    let req = loom_types::CompletionRequest {
        messages: vec![loom_types::Message::user(&prompt)],
        ..Default::default()
    };

    match client.complete(req).await {
        Ok(resp) => {
            let text = resp.text.trim().to_string();
            match detector::parse_extraction_response(&text) {
                Ok(detected) => {
                    if detected.should_create {
                        Ok(json!({
                            "should_create": true,
                            "name": detected.name,
                            "prompt": detected.body,
                            "cron_expression": detected.cron_expression,
                            "kind": detected.kind,
                            "confirmation": detected.confirmation,
                        }))
                    } else {
                        Ok(json!({ "should_create": false }))
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e, "cron.detect: parse error");
                    Ok(json!({ "should_create": false }))
                }
            }
        }
        Err(e) => {
            tracing::warn!(error = %e, "cron.detect: LLM error");
            Ok(json!({ "should_create": false }))
        }
    }
}

// ── Handlers ──────────────────────────────────────────────────────────────────

async fn handle_list(cron: &Arc<loom_cron::CronScheduler>) -> Result<Value, JsonRpcError> {
    match cron.list_jobs() {
        Ok(jobs) => Ok(serde_json::to_value(jobs).unwrap_or_default()),
        Err(e) => Err(err(ErrorCode::InternalError, &e.to_string())),
    }
}

async fn handle_create(
    cron: &Arc<loom_cron::CronScheduler>,
    params: &Value,
) -> Result<Value, JsonRpcError> {
    let name = get_str(params, "name")?;
    let cron_expr = get_str(params, "cron_expression")?;
    // prompt is the AI instruction (v2); accept "command" as fallback for v1 compatibility
    let prompt = params
        .get("prompt")
        .and_then(|v| v.as_str())
        .or_else(|| params.get("command").and_then(|v| v.as_str()))
        .unwrap_or("");
    if prompt.is_empty() {
        return Err(err(
            ErrorCode::InvalidRequest,
            "missing or invalid 'prompt' (AI instruction for the cron job)",
        ));
    }
    let session_mode = get_str(params, "session_mode")
        .map(|s| match s {
            "current" => SessionMode::Current,
            _ => SessionMode::Isolated,
        })
        .unwrap_or(SessionMode::Isolated);
    let timeout_secs = params
        .get("timeout_secs")
        .and_then(|v| v.as_u64())
        .unwrap_or(300)
        .clamp(1, 3600);

    match cron
        .add_job(name, cron_expr, prompt, session_mode, timeout_secs)
        .await
    {
        Ok(id) => Ok(json!({ "id": id })),
        Err(e) => Err(err(ErrorCode::InternalError, &e.to_string())),
    }
}

async fn handle_update(
    cron: &Arc<loom_cron::CronScheduler>,
    params: &Value,
) -> Result<Value, JsonRpcError> {
    let job_id = get_str(params, "id")?;
    let name = get_str(params, "name")?;
    let cron_expr = get_str(params, "cron_expression")?;
    let prompt = params
        .get("prompt")
        .and_then(|v| v.as_str())
        .or_else(|| params.get("command").and_then(|v| v.as_str()))
        .unwrap_or("");
    if prompt.is_empty() {
        return Err(err(
            ErrorCode::InvalidRequest,
            "missing or invalid 'prompt' (AI instruction for the cron job)",
        ));
    }
    let session_mode = get_str(params, "session_mode")
        .map(|s| match s {
            "current" => SessionMode::Current,
            _ => SessionMode::Isolated,
        })
        .unwrap_or(SessionMode::Isolated);
    let timeout_secs = params
        .get("timeout_secs")
        .and_then(|v| v.as_u64())
        .unwrap_or(300)
        .clamp(1, 3600);

    match cron
        .update_job(job_id, name, cron_expr, prompt, session_mode, timeout_secs)
        .await
    {
        Ok(()) => Ok(json!({ "updated": true })),
        Err(e) => Err(err(ErrorCode::InternalError, &e.to_string())),
    }
}

async fn handle_delete(
    cron: &Arc<loom_cron::CronScheduler>,
    params: &Value,
) -> Result<Value, JsonRpcError> {
    let job_id = get_str(params, "id")?;
    match cron.remove_job(job_id).await {
        Ok(()) => Ok(json!({ "deleted": true })),
        Err(e) => Err(err(ErrorCode::InternalError, &e.to_string())),
    }
}

async fn handle_pause(
    cron: &Arc<loom_cron::CronScheduler>,
    params: &Value,
) -> Result<Value, JsonRpcError> {
    let job_id = get_str(params, "id")?;
    match cron.pause_job(job_id).await {
        Ok(()) => Ok(json!({ "paused": true })),
        Err(e) => Err(err(ErrorCode::InternalError, &e.to_string())),
    }
}

async fn handle_resume(
    cron: &Arc<loom_cron::CronScheduler>,
    params: &Value,
) -> Result<Value, JsonRpcError> {
    let job_id = get_str(params, "id")?;
    match cron.resume_job(job_id).await {
        Ok(()) => Ok(json!({ "resumed": true })),
        Err(e) => Err(err(ErrorCode::InternalError, &e.to_string())),
    }
}

async fn handle_history(
    cron: &Arc<loom_cron::CronScheduler>,
    params: &Value,
) -> Result<Value, JsonRpcError> {
    let job_id = get_str(params, "id")?;
    let limit = params
        .get("limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(20)
        .clamp(1, 1000) as usize;

    match cron.get_history(job_id, limit) {
        Ok(history) => Ok(serde_json::to_value(history).unwrap_or_default()),
        Err(e) => Err(err(ErrorCode::InternalError, &e.to_string())),
    }
}

async fn handle_run_now(
    cron: &Arc<loom_cron::CronScheduler>,
    params: &Value,
) -> Result<Value, JsonRpcError> {
    let job_id = get_str(params, "id")?;
    match cron.run_now(job_id).await {
        Ok(run_id) => Ok(json!({ "run_id": run_id })),
        Err(e) => Err(err(ErrorCode::InternalError, &e.to_string())),
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn get_str<'a>(params: &'a Value, key: &str) -> Result<&'a str, JsonRpcError> {
    params.get(key).and_then(|v| v.as_str()).ok_or_else(|| {
        err(
            ErrorCode::InternalError,
            &format!("missing or invalid '{}'", key),
        )
    })
}

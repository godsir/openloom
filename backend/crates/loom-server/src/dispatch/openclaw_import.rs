//! openclaw_import.* JSON-RPC handlers — scan & import OpenClaw conversations.

use std::path::{Path, PathBuf};

use loom_types::{ErrorCode, JsonRpcError};
use serde_json::{Value, json};

use super::claude_import::{is_safe_id, mark_already_imported};
use super::err;
use crate::AppState;

pub async fn handle(state: &AppState, method: &str, p: &Value) -> Option<Result<Value, JsonRpcError>> {
    match method {
        "openclaw_import.scan" => Some(handle_scan(state).await),
        "openclaw_import.run" => Some(handle_run(state, p).await),
        _ => None,
    }
}

fn openclaw_agents_dir() -> Result<PathBuf, JsonRpcError> {
    let home = dirs::home_dir()
        .ok_or_else(|| err(ErrorCode::InternalError, "cannot resolve home directory"))?;
    Ok(home.join(".openclaw").join("agents"))
}

async fn handle_scan(state: &AppState) -> Result<Value, JsonRpcError> {
    let dir = openclaw_agents_dir()?;
    let convs = loom_import::openclaw::scan(&dir)
        .map_err(|e| err(ErrorCode::InternalError, &format!("scan failed: {e}")))?;
    let existing: Vec<String> = state
        .orchestrator
        .list_persisted_sessions()
        .await
        .into_iter()
        .map(|r| r.0)
        .collect();
    let convs = mark_already_imported(convs, &existing);
    Ok(json!({ "conversations": convs }))
}

async fn handle_run(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let ids: Vec<String> = p
        .get("ids")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    let replace = p.get("replace").and_then(|v| v.as_bool()).unwrap_or(false);
    let dir = openclaw_agents_dir()?;

    let mut imported = 0usize;
    let mut skipped = 0usize;
    let mut failed: Vec<Value> = Vec::new();

    for id in &ids {
        if !is_safe_id(id) {
            failed.push(json!({ "id": id, "reason": "invalid id" }));
            continue;
        }
        let path = match resolve_openclaw(&dir, id) {
            Some(p) => p,
            None => {
                failed.push(json!({ "id": id, "reason": "file not found" }));
                continue;
            }
        };
        match loom_import::openclaw::build_payload(&path) {
            Ok(payload) => {
                let created = payload.created_at;
                let updated = payload.updated_at;
                let count = payload.messages.len();
                let title = payload.title.clone();
                match state.orchestrator.import_session_persisted(&payload, replace).await {
                    Ok(loom_types::ImportOutcome::AlreadyExists) => skipped += 1,
                    Ok(_) => {
                        state
                            .sessions
                            .restore(
                                id.clone(),
                                created.format("%Y-%m-%d %H:%M:%S").to_string(),
                                updated.format("%Y-%m-%d %H:%M:%S").to_string(),
                                count,
                                title,
                            )
                            .await;
                        imported += 1;
                    }
                    Err(e) => failed.push(json!({ "id": id, "reason": e.to_string() })),
                }
            }
            Err(e) => failed.push(json!({ "id": id, "reason": format!("parse: {e}") })),
        }
    }

    Ok(json!({ "imported": imported, "skipped": skipped, "failed": failed }))
}

/// Find `agents/*/sessions/<id>.jsonl` (or `<id>.jsonl.reset.*`), skipping `.deleted`.
fn resolve_openclaw(agents_dir: &Path, id: &str) -> Option<PathBuf> {
    for agent in std::fs::read_dir(agents_dir).ok()?.flatten() {
        let sessions_dir = agent.path().join("sessions");
        if !sessions_dir.is_dir() {
            continue;
        }
        let exact = sessions_dir.join(format!("{id}.jsonl"));
        if exact.exists() {
            return Some(exact);
        }
        // .reset variant: <id>.jsonl.reset.<ts>Z (still contains full history)
        if let Ok(entries) = std::fs::read_dir(&sessions_dir) {
            for f in entries.flatten() {
                let name = f.file_name().to_string_lossy().to_string();
                if name.starts_with(&format!("{id}.jsonl")) && !name.contains(".deleted.") {
                    return Some(f.path());
                }
            }
        }
    }
    None
}

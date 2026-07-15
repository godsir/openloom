//! claude_import.* JSON-RPC handlers — scan & import Claude Code conversations.

use std::path::{Path, PathBuf};

use loom_import::ConversationSummary;
use loom_types::{ErrorCode, JsonRpcError};
use serde_json::{Value, json};

use super::err;
use crate::AppState;

pub async fn handle(
    state: &AppState,
    method: &str,
    p: &Value,
) -> Option<Result<Value, JsonRpcError>> {
    match method {
        "claude_import.scan" => Some(handle_scan(state).await),
        "claude_import.run" => Some(handle_run(state, p).await),
        _ => None,
    }
}

fn projects_dir() -> Result<PathBuf, JsonRpcError> {
    let home = dirs::home_dir()
        .ok_or_else(|| err(ErrorCode::InternalError, "cannot resolve home directory"))?;
    Ok(home.join(".claude").join("projects"))
}

async fn handle_scan(state: &AppState) -> Result<Value, JsonRpcError> {
    let dir = projects_dir()?;
    let convs = loom_import::scan(&dir)
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
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let replace = p.get("replace").and_then(|v| v.as_bool()).unwrap_or(false);
    let dir = projects_dir()?;

    let mut imported = 0usize;
    let mut skipped = 0usize;
    let mut failed: Vec<Value> = Vec::new();

    for id in &ids {
        // Reject ids that could escape the projects dir via path traversal
        // before touching the filesystem.
        if !is_safe_id(id) {
            failed.push(json!({ "id": id, "reason": "invalid id" }));
            continue;
        }
        // The project subfolder name is unknown from the id alone, so locate
        // the file by scanning subfolders. O(ids × subfolders) — fine for a
        // user-curated selection.
        let path = match resolve_jsonl(&dir, id) {
            Some(p) => p,
            None => {
                failed.push(json!({ "id": id, "reason": "file not found" }));
                continue;
            }
        };
        match loom_import::build_payload(&path) {
            Ok(payload) => {
                let created = payload.created_at;
                let updated = payload.updated_at;
                let count = payload.messages.len();
                let title = payload.title.clone();
                match state
                    .orchestrator
                    .import_session_persisted(&payload, replace)
                    .await
                {
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

/// Reject ids that could escape the projects dir via path traversal (`..`,
/// absolute paths, nested separators). Only a single plain file-stem-like
/// name (e.g. a UUID) is allowed — this is a filesystem-path concern; SQL
/// is parameterized regardless.
pub(super) fn is_safe_id(id: &str) -> bool {
    let comps: Vec<_> = std::path::Path::new(id).components().collect();
    !id.is_empty() && comps.len() == 1 && matches!(comps[0], std::path::Component::Normal(_))
}

/// Find `<projects_dir>/<any>/<id>.jsonl` by scanning project subfolders once.
fn resolve_jsonl(projects_dir: &Path, id: &str) -> Option<PathBuf> {
    let entries = std::fs::read_dir(projects_dir).ok()?;
    for e in entries.flatten() {
        if !e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let candidate = e.path().join(format!("{id}.jsonl"));
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

/// Mark summaries whose `session_uuid` already exists as `already_imported`.
pub(super) fn mark_already_imported(
    mut convs: Vec<ConversationSummary>,
    existing: &[String],
) -> Vec<ConversationSummary> {
    let set: std::collections::HashSet<&String> = existing.iter().collect();
    for c in &mut convs {
        c.already_imported = set.contains(&c.session_uuid);
    }
    convs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mark_flags_existing() {
        let mk = || ConversationSummary {
            session_uuid: String::new(),
            project_dir: "p".into(),
            title: None,
            first_message: None,
            message_count: 1,
            model: None,
            started_at: "x".into(),
            last_at: "x".into(),
            already_imported: false,
        };
        let mut a = mk();
        a.session_uuid = "a".into();
        let mut b = mk();
        b.session_uuid = "b".into();
        let out = mark_already_imported(vec![a, b], &["a".into()]);
        assert!(out[0].already_imported);
        assert!(!out[1].already_imported);
    }

    #[test]
    fn is_safe_id_rejects_traversal() {
        // Reject path-traversal and malformed ids
        assert!(!is_safe_id(".."));
        assert!(!is_safe_id("a/b"));
        assert!(!is_safe_id("/x"));
        assert!(!is_safe_id(""));
        assert!(!is_safe_id("../etc/foo"));
        assert!(!is_safe_id("C:\\windows"));
        // Accept a normal uuid-style stem
        assert!(is_safe_id("80c205c6-1234-5678-abcd-ef0123456789"));
        assert!(is_safe_id("session-aa"));
    }
}

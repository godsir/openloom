//! Virtual File System handlers for Write mode.
use loom_types::{ErrorCode, JsonRpcError};
use serde_json::Value;
use std::path::{Component, Path, PathBuf};

use super::err;
use crate::AppState;

pub async fn handle(_state: &AppState, method: &str, p: &Value) -> Option<Result<Value, JsonRpcError>> {
    match method {
        "vfs.read_file" => Some(handle_read_file(p)),
        "vfs.write_file" => Some(handle_write_file(p)),
        "vfs.list_directory" => Some(handle_list_directory(p)),
        "vfs.create_directory" => Some(handle_create_directory(p)),
        "vfs.rename" => Some(handle_rename(p)),
        "vfs.delete" => Some(handle_delete(p)),
        "vfs.watch_file" => Some(handle_watch_file(p)),
        "vfs.unwatch_file" => Some(handle_unwatch_file(p)),
        _ => None,
    }
}

fn resolve_path(p: &Value) -> Result<PathBuf, JsonRpcError> {
    let workspace = p.get("workspace_root").and_then(|v| v.as_str()).unwrap_or("");
    let relative = p.get("path").and_then(|v| v.as_str()).unwrap_or("");
    if workspace.is_empty() {
        return Err(err(ErrorCode::InvalidRequest, "workspace_root required"));
    }
    // Resolve the workspace root to a canonical, existing directory. Failing
    // closed here removes the previous "canonicalize failed → checks skipped"
    // bypass and guarantees `ws_canonical` is symlink-resolved.
    let ws_canonical = PathBuf::from(workspace)
        .canonicalize()
        .map_err(|_| err(ErrorCode::PermissionDenied, "invalid workspace_root"))?;
    // The client-supplied relative path may only contain normal / current-dir
    // components — no `..`, no absolute root, no drive prefix — so it can never
    // escape the workspace.
    if Path::new(relative)
        .components()
        .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir))
    {
        return Err(err(ErrorCode::PermissionDenied, "path escapes workspace"));
    }
    let full = ws_canonical.join(relative);
    // Defense in depth: re-verify containment after resolving symlinks in the
    // portion of the path that already exists (catches a symlink planted inside
    // the workspace that points outside it).
    let mut probe = full.as_path();
    let existing = loop {
        if probe.exists() {
            break probe;
        }
        match probe.parent() {
            Some(parent) => probe = parent,
            None => return Err(err(ErrorCode::PermissionDenied, "path escapes workspace")),
        }
    };
    if let Ok(existing_canonical) = existing.canonicalize()
        && !existing_canonical.starts_with(&ws_canonical)
    {
        return Err(err(ErrorCode::PermissionDenied, "path escapes workspace"));
    }
    Ok(full)
}

fn handle_read_file(p: &Value) -> Result<Value, JsonRpcError> {
    let path = resolve_path(p)?;
    match std::fs::read_to_string(&path) {
        Ok(content) => Ok(serde_json::json!({ "ok": true, "path": path.to_string_lossy(), "content": content })),
        Err(e) => Err(err(ErrorCode::InternalError, &format!("read failed: {}", e))),
    }
}

fn handle_write_file(p: &Value) -> Result<Value, JsonRpcError> {
    let path = resolve_path(p)?;
    let content = p.get("content").and_then(|v| v.as_str()).unwrap_or("");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    match std::fs::write(&path, content) {
        Ok(_) => Ok(serde_json::json!({ "ok": true, "path": path.to_string_lossy() })),
        Err(e) => Err(err(ErrorCode::InternalError, &format!("write failed: {}", e))),
    }
}

fn handle_list_directory(p: &Value) -> Result<Value, JsonRpcError> {
    let path = resolve_path(p)?;
    match std::fs::read_dir(&path) {
        Ok(entries) => {
            let list: Vec<Value> = entries.filter_map(|e| {
                let e = e.ok()?;
                let name = e.file_name().to_string_lossy().to_string();
                let is_dir = e.file_type().ok()?.is_dir();
                Some(serde_json::json!({ "name": name, "is_directory": is_dir }))
            }).collect();
            Ok(serde_json::json!({ "ok": true, "entries": list }))
        }
        Err(e) => Err(err(ErrorCode::InternalError, &format!("list failed: {}", e))),
    }
}

fn handle_create_directory(p: &Value) -> Result<Value, JsonRpcError> {
    let path = resolve_path(p)?;
    match std::fs::create_dir_all(&path) {
        Ok(_) => Ok(serde_json::json!({ "ok": true })),
        Err(e) => Err(err(ErrorCode::InternalError, &format!("mkdir failed: {}", e))),
    }
}

fn handle_rename(p: &Value) -> Result<Value, JsonRpcError> {
    let from = resolve_path(p)?;
    let new_name = p.get("new_name").and_then(|v| v.as_str()).unwrap_or("");
    // new_name must be a single normal path component — no separators, `..`, or
    // absolute prefix — so a rename cannot move the file outside its directory.
    let mut comps = Path::new(new_name).components();
    match (comps.next(), comps.next()) {
        (Some(Component::Normal(_)), None) => {}
        _ => return Err(err(ErrorCode::PermissionDenied, "invalid new_name")),
    }
    let to = from.parent().unwrap_or(&from).join(new_name);
    match std::fs::rename(&from, &to) {
        Ok(_) => Ok(serde_json::json!({ "ok": true })),
        Err(e) => Err(err(ErrorCode::InternalError, &format!("rename failed: {}", e))),
    }
}

fn handle_delete(p: &Value) -> Result<Value, JsonRpcError> {
    let path = resolve_path(p)?;
    let result = if path.is_dir() {
        std::fs::remove_dir_all(&path)
    } else {
        std::fs::remove_file(&path)
    };
    match result {
        Ok(_) => Ok(serde_json::json!({ "ok": true })),
        Err(e) => Err(err(ErrorCode::InternalError, &format!("delete failed: {}", e))),
    }
}

fn handle_watch_file(p: &Value) -> Result<Value, JsonRpcError> {
    let _path = resolve_path(p)?;
    // File watching is handled by the Electron main process via fs.watch.
    // This backend stub validates the path and returns ok.
    Ok(serde_json::json!({ "ok": true }))
}

fn handle_unwatch_file(_p: &Value) -> Result<Value, JsonRpcError> {
    Ok(serde_json::json!({ "ok": true }))
}

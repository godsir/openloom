//! LSP dispatch handlers — lsp.*

use loom_lsp::binary_available;
use loom_lsp::install_hint;
use loom_lsp::uninstall_hint;
use loom_types::{ErrorCode, JsonRpcError};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use super::err;
use crate::AppState;

/// Shared install tasks — keyed by a unique id, each holding a ring buffer
/// of captured output lines and a completion flag.
struct InstallTask {
    lines: Vec<String>,
    done: bool,
    ok: bool,
    exit_code: Option<i32>,
}

static INSTALL_TASKS: std::sync::LazyLock<Arc<Mutex<HashMap<String, InstallTask>>>> =
    std::sync::LazyLock::new(|| Arc::new(Mutex::new(HashMap::new())));

fn spawn_install(_language: String, command: String) -> String {
    let task_id = uuid::Uuid::new_v4().to_string();
    let tasks = INSTALL_TASKS.clone();
    let id = task_id.clone();

    tasks.lock().unwrap().insert(id.clone(), InstallTask {
        lines: vec![],
        done: false,
        ok: false,
        exit_code: None,
    });

    tokio::spawn(async move {
        let (shell, shell_arg) = if cfg!(windows) { ("cmd".to_string(), "/C".to_string()) } else { ("sh".to_string(), "-c".to_string()) };

        let mut child = match tokio::process::Command::new(&shell)
            .args([shell_arg.as_str(), &command])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                let mut tasks = tasks.lock().unwrap();
                if let Some(t) = tasks.get_mut(&id) {
                    t.lines.push(format!("[ERROR] failed to spawn: {e}"));
                    t.done = true;
                    t.ok = false;
                }
                return;
            }
        };

        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        use tokio::io::{AsyncBufReadExt, BufReader};

        let output_lines = Arc::new(Mutex::new(Vec::new()));

        let lines_a = output_lines.clone();
        let lines_b = output_lines.clone();

        if let Some(out) = stdout {
            let lines = lines_a.clone();
            tokio::spawn(async move {
                let mut reader = BufReader::new(out).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    let mut buf = lines.lock().unwrap();
                    buf.push(line);
                }
            });
        }

        if let Some(err) = stderr {
            let lines = lines_b.clone();
            tokio::spawn(async move {
                let mut reader = BufReader::new(err).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    let mut buf = lines.lock().unwrap();
                    buf.push(format!("[stderr] {line}"));
                }
            });
        }

        // Poll buffer every 200ms and push to shared task
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            {
                let mut buf = output_lines.lock().unwrap();
                if !buf.is_empty() {
                    let mut tasks = tasks.lock().unwrap();
                    if let Some(t) = tasks.get_mut(&id) {
                        t.lines.append(&mut std::mem::take(buf.as_mut()));
                    }
                }
            }
            match child.try_wait() {
                Ok(Some(status)) => {
                    // One final drain
                    let mut buf = output_lines.lock().unwrap();
                    if !buf.is_empty() {
                        let mut tasks = tasks.lock().unwrap();
                        if let Some(t) = tasks.get_mut(&id) {
                            t.lines.append(&mut std::mem::take(buf.as_mut()));
                        }
                    }
                    let mut tasks = tasks.lock().unwrap();
                    if let Some(t) = tasks.get_mut(&id) {
                        t.done = true;
                        t.ok = status.success();
                        t.exit_code = status.code();
                    }
                    break;
                }
                Ok(None) => continue, // still running
                Err(e) => {
                    let mut tasks = tasks.lock().unwrap();
                    if let Some(t) = tasks.get_mut(&id) {
                        t.lines.push(format!("[ERROR] {e}"));
                        t.done = true;
                        t.ok = false;
                    }
                    break;
                }
            }
        }
    });

    task_id
}

pub async fn handle(
    state: &AppState,
    method: &str,
    p: &Value,
) -> Option<Result<Value, JsonRpcError>> {
    match method {
        "lsp.list_servers" => Some(handle_lsp_list_servers(state).await),
        "lsp.diagnostics" => Some(handle_lsp_diagnostics(state, p).await),
        "lsp.completion" => Some(handle_lsp_completion(state, p).await),
        "lsp.hover" => Some(handle_lsp_hover(state, p).await),
        "lsp.definition" => Some(handle_lsp_definition(state, p).await),
        "lsp.references" => Some(handle_lsp_references(state, p).await),
        "lsp.symbols" => Some(handle_lsp_symbols(state, p).await),
        "lsp.shutdown" => Some(handle_lsp_shutdown(state, p).await),
        "lsp.shutdown_all" => Some(handle_lsp_shutdown_all(state).await),
        "lsp.supported_languages" => Some(handle_lsp_supported_languages(state).await),
        "lsp.check" => Some(handle_lsp_check(state).await),
        "lsp.install" => Some(handle_lsp_install(state, p).await),
        "lsp.uninstall" => Some(handle_lsp_uninstall(state, p).await),
        "lsp.install_status" => Some(handle_lsp_install_status(state, p).await),
        "lsp.all_diagnostics" => Some(handle_lsp_all_diagnostics(state).await),
        "lsp.start" => Some(handle_lsp_start(state, p).await),
        _ => None,
    }
}

// --- lsp.list_servers ---

async fn handle_lsp_list_servers(state: &AppState) -> Result<Value, JsonRpcError> {
    let servers = state.orchestrator.lsp_client().list_servers().await;
    Ok(json!({ "servers": servers }))
}

// --- lsp.diagnostics ---

async fn handle_lsp_diagnostics(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let file_path = p.get("file_path").and_then(|v| v.as_str()).unwrap_or("");
    if file_path.is_empty() {
        return Err(err(ErrorCode::InvalidRequest, "file_path required"));
    }
    let result = state
        .orchestrator
        .lsp_client()
        .diagnostics(file_path)
        .await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
    Ok(result)
}

// --- lsp.completion ---

async fn handle_lsp_completion(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let file_path = p.get("file_path").and_then(|v| v.as_str()).unwrap_or("");
    let line = p.get("line").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    let character = p.get("character").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    if file_path.is_empty() {
        return Err(err(ErrorCode::InvalidRequest, "file_path required"));
    }
    let result = state
        .orchestrator
        .lsp_client()
        .completion(file_path, line, character)
        .await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
    Ok(result)
}

// --- lsp.hover ---

async fn handle_lsp_hover(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let file_path = p.get("file_path").and_then(|v| v.as_str()).unwrap_or("");
    let line = p.get("line").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    let character = p.get("character").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    if file_path.is_empty() {
        return Err(err(ErrorCode::InvalidRequest, "file_path required"));
    }
    let result = state
        .orchestrator
        .lsp_client()
        .hover(file_path, line, character)
        .await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
    Ok(result)
}

// --- lsp.definition ---

async fn handle_lsp_definition(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let file_path = p.get("file_path").and_then(|v| v.as_str()).unwrap_or("");
    let line = p.get("line").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    let character = p.get("character").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    if file_path.is_empty() {
        return Err(err(ErrorCode::InvalidRequest, "file_path required"));
    }
    let result = state
        .orchestrator
        .lsp_client()
        .definition(file_path, line, character)
        .await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
    Ok(result)
}

// --- lsp.references ---

async fn handle_lsp_references(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let file_path = p.get("file_path").and_then(|v| v.as_str()).unwrap_or("");
    let line = p.get("line").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    let character = p.get("character").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    let include_decl = p
        .get("include_declaration")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    if file_path.is_empty() {
        return Err(err(ErrorCode::InvalidRequest, "file_path required"));
    }
    let result = state
        .orchestrator
        .lsp_client()
        .references(file_path, line, character, include_decl)
        .await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
    Ok(result)
}

// --- lsp.symbols ---

async fn handle_lsp_symbols(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let file_path = p.get("file_path").and_then(|v| v.as_str()).unwrap_or("");
    if file_path.is_empty() {
        return Err(err(ErrorCode::InvalidRequest, "file_path required"));
    }
    let result = state
        .orchestrator
        .lsp_client()
        .document_symbols(file_path)
        .await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
    Ok(result)
}

// --- lsp.shutdown ---

async fn handle_lsp_shutdown(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let language = p.get("language").and_then(|v| v.as_str()).unwrap_or("");
    if language.is_empty() {
        return Err(err(
            ErrorCode::InvalidRequest,
            "language required (e.g. 'rust', 'typescript')",
        ));
    }
    state
        .orchestrator
        .lsp_client()
        .shutdown(language)
        .await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
    Ok(json!({ "ok": true }))
}

// --- lsp.shutdown_all ---

async fn handle_lsp_shutdown_all(state: &AppState) -> Result<Value, JsonRpcError> {
    state
        .orchestrator
        .lsp_client()
        .shutdown_all()
        .await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
    Ok(json!({ "ok": true }))
}

// --- lsp.supported_languages ---

// --- lsp.check ---

/// Check the availability of all supported language servers.
/// Returns a list of { language, command, available: bool, running: bool, install_hint? }.
async fn handle_lsp_check(state: &AppState) -> Result<Value, JsonRpcError> {
    let client = state.orchestrator.lsp_client();
    let supported = client.supported_languages();
    let running: Vec<String> = client.list_servers().await;
    let running_set: std::collections::HashSet<&str> =
        running.iter().map(|s| s.as_str()).collect();

    let items: Vec<Value> = supported
        .iter()
        .map(|(lang, cmd)| {
            let available = binary_available(cmd);
            let hint = if !available {
                install_hint(lang, cmd)
            } else {
                None
            };
            let uninst = if available {
                uninstall_hint(lang)
            } else {
                None
            };
            json!({
                "language": lang,
                "command": cmd,
                "available": available,
                "running": running_set.contains(lang),
                "install_hint": hint.map(|(mgr, cmd)| json!({ "manager": mgr, "command": cmd })),
                "uninstall_command": uninst,
            })
        })
        .collect();

    Ok(json!({ "languages": items }))
}

// --- lsp.supported_languages ---

async fn handle_lsp_supported_languages(state: &AppState) -> Result<Value, JsonRpcError> {
    let langs = state.orchestrator.lsp_client().supported_languages();
    let list: Vec<Value> = langs
        .iter()
        .map(|(lang, cmd)| json!({ "language": lang, "command": cmd }))
        .collect();
    Ok(json!({ "languages": list }))
}

// --- lsp.all_diagnostics ---

async fn handle_lsp_all_diagnostics(state: &AppState) -> Result<Value, JsonRpcError> {
    let diags = state.orchestrator.lsp_client().all_diagnostics().await;
    let items: Vec<Value> = diags
        .into_iter()
        .map(|(lang, files)| {
            let file_list: Vec<Value> = files
                .into_iter()
                .map(|(path, count)| {
                    json!({ "file": path, "count": count })
                })
                .collect();
            let total: usize = file_list.iter().filter_map(|v| v["count"].as_u64().map(|n| n as usize)).sum();
            json!({
                "language": lang,
                "total": total,
                "files": file_list,
            })
        })
        .collect();
    Ok(json!({ "servers": items }))
}

// --- lsp.install ---

async fn handle_lsp_install(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let _ = state;
    let language = p.get("language").and_then(|v| v.as_str()).unwrap_or("");
    let command = p.get("command").and_then(|v| v.as_str()).unwrap_or("");
    if language.is_empty() || command.is_empty() {
        return Err(err(ErrorCode::InvalidRequest, "language and command required"));
    }
    let hint = install_hint(language, command)
        .ok_or_else(|| err(ErrorCode::MethodNotFound, "No install recipe for this language server"))?;
    let task_id = spawn_install(language.to_string(), hint.1.to_string());
    Ok(json!({ "task_id": task_id }))
}

// --- lsp.uninstall ---

async fn handle_lsp_uninstall(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let _ = state;
    let language = p.get("language").and_then(|v| v.as_str()).unwrap_or("");
    if language.is_empty() {
        return Err(err(ErrorCode::InvalidRequest, "language required"));
    }
    let cmd = uninstall_hint(language)
        .ok_or_else(|| err(ErrorCode::MethodNotFound, "No uninstall recipe for this language server"))?;
    let task_id = spawn_install(language.to_string(), cmd.to_string());
    Ok(json!({ "task_id": task_id }))
}

// --- lsp.install_status ---

async fn handle_lsp_install_status(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let _ = state;
    let task_id = p.get("task_id").and_then(|v| v.as_str()).unwrap_or("");
    if task_id.is_empty() {
        return Err(err(ErrorCode::InvalidRequest, "task_id required"));
    }
    let tasks = INSTALL_TASKS.lock().unwrap();
    match tasks.get(task_id) {
        Some(t) => {
            let res = json!({
                "task_id": task_id,
                "lines": t.lines,
                "done": t.done,
                "ok": t.ok,
                "exit_code": t.exit_code,
            });
            Ok(res)
        }
        None => Err(err(ErrorCode::MethodNotFound, "Task not found (may have been cleaned up)")),
    }
}

// --- lsp.start ---

async fn handle_lsp_start(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let language = p.get("language").and_then(|v| v.as_str()).unwrap_or("");
    let command = p.get("command").and_then(|v| v.as_str()).unwrap_or("");
    let args: Vec<String> = p
        .get("args")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    if language.is_empty() || command.is_empty() {
        return Err(err(
            ErrorCode::InvalidRequest,
            "language and command required",
        ));
    }
    if !binary_available(command) {
        let hint = install_hint(language, command);
        let detail = hint.map(|(mgr, cmd)| format!(" Install it via: {} — {}", mgr, cmd)).unwrap_or_default();
        return Err(err(ErrorCode::InternalError,
            &format!("'{}' not found on PATH.{}", command, detail)));
    }
    state
        .orchestrator
        .lsp_client()
        .start_custom(language, command, &args)
        .await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
    Ok(json!({ "ok": true }))
}

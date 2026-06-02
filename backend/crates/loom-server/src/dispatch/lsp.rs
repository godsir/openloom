//! LSP dispatch handlers — lsp.*

use loom_types::{ErrorCode, JsonRpcError};
use serde_json::{Value, json};

use crate::AppState;
use super::err;

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

async fn handle_lsp_supported_languages(state: &AppState) -> Result<Value, JsonRpcError> {
    let langs = state.orchestrator.lsp_client().supported_languages();
    let list: Vec<Value> = langs
        .iter()
        .map(|(lang, cmd)| json!({ "language": lang, "command": cmd }))
        .collect();
    Ok(json!({ "languages": list }))
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
    state
        .orchestrator
        .lsp_client()
        .start_custom(language, command, &args)
        .await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
    Ok(json!({ "ok": true }))
}

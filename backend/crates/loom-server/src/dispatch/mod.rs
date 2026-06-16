//! JSON-RPC 2.0 dispatch — routes incoming requests to orchestrator methods.
//!
//! Supported: system, agent, chat, completion, session, config, mcp, lsp, tools, skills, plugins.
//!
//! Each submodule exports a `handle` function returning
//! `Option<Result<Value, JsonRpcError>>`.  The main match in this file
//! delegates to each sub-handler in sequence.

mod chat;
mod completion;
mod clawhub;
mod cron;
mod kg;
mod lsp;
mod mcp;
mod model;
mod plan;
mod plugins;
pub mod session;
mod skills;
mod system;
mod tool;
mod vfs;
pub mod write_rag;

// Re-export SessionStore for crate::dispatch::SessionStore access (used by lib.rs).
pub use session::SessionStore;

use std::sync::Arc;

use loom_types::{ErrorCode, JsonRpcError, JsonRpcRequest, JsonRpcResponse};
use serde_json::{Value, json};

use crate::AppState;

// ---------------------------------------------------------------------------
// Public entry points
// ---------------------------------------------------------------------------

pub async fn dispatch_handler(state: Arc<AppState>, req: JsonRpcRequest) -> JsonRpcResponse {
    let result = dispatch_method(&state, &req).await;
    let (result_val, error_val) = match result {
        Ok(v) => (Some(v), None),
        Err(e) => (None, Some(e)),
    };
    JsonRpcResponse {
        jsonrpc: "2.0".into(),
        result: result_val,
        error: error_val,
        id: req.id,
    }
}

pub async fn dispatch_method(
    state: &AppState,
    req: &JsonRpcRequest,
) -> Result<Value, JsonRpcError> {
    let p = req.params.clone().unwrap_or(json!({}));
    let method = req.method.as_str();

    // Delegate to sub-handlers in order of cost / likelihood.
    // Each handler returns None when it does not own the method.
    if let Some(result) = chat::handle(state, method, &p).await {
        return result;
    }
    if let Some(result) = completion::handle(state, method, &p).await {
        return result;
    }
    if let Some(result) = session::handle(state, method, &p).await {
        return result;
    }
    if let Some(result) = model::handle(state, method, &p).await {
        return result;
    }
    if let Some(result) = system::handle(state, method, &p).await {
        return result;
    }
    if let Some(result) = mcp::handle(state, method, &p).await {
        return result;
    }
    if let Some(result) = lsp::handle(state, method, &p).await {
        return result;
    }
    if let Some(result) = skills::handle(state, method, &p).await {
        return result;
    }
    if let Some(result) = plugins::handle(state, method, &p).await {
        return result;
    }
    if let Some(result) = kg::handle(state, method, &p).await {
        return result;
    }
    if let Some(result) = tool::handle(state, method, &p).await {
        return result;
    }
    if let Some(result) = write_rag::handle(state, method, &p).await {
        return result;
    }
    if let Some(result) = vfs::handle(state, method, &p).await {
        return result;
    }
    if let Some(result) = cron::handle(state, method, &p).await {
        return result;
    }
    if let Some(result) = clawhub::handle(state, method, &p).await {
        return result;
    }
    if let Some(result) = plan::handle(state, method, &p).await {
        return result;
    }

    Err(err(
        ErrorCode::MethodNotFound,
        &format!("method '{}' not found", req.method),
    ))
}

// ---------------------------------------------------------------------------
// Shared helper
// ---------------------------------------------------------------------------

pub(crate) fn err(code: ErrorCode, msg: &str) -> JsonRpcError {
    JsonRpcError {
        code,
        message: msg.to_string(),
        data: None,
    }
}

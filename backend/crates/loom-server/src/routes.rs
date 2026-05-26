//! Route handlers.

use axum::{extract::State, http::StatusCode, Json};
use serde_json::json;
use std::sync::Arc;

use crate::AppState;

pub async fn health(State(state): State<Arc<AppState>>) -> (StatusCode, Json<serde_json::Value>) {
    let agents = state.orchestrator.list_agents().await;
    (StatusCode::OK, Json(json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
        "agents": agents.len(),
    })))
}

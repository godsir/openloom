// SPDX-License-Identifier: Apache-2.0
//! HTTP/WebSocket server for openLoom v2.
//!
//! Provides Axum routes for JSON-RPC dispatch over HTTP POST and WebSocket.

use std::path::PathBuf;
use std::sync::Arc;

use axum::{Json, Router, extract::State, routing::get};
use loom_core::Orchestrator;

mod credential;
mod dispatch;
mod routes;
mod ws;

pub use credential::save_credential;
pub use dispatch::SessionStore;
pub use ws::ws_handler;

/// Shared application state passed to all route handlers.
pub struct AppState {
    pub orchestrator: Arc<Orchestrator>,
    pub sessions: SessionStore,
    pub data_dir: PathBuf,
}

impl AppState {
    pub fn new(orchestrator: Arc<Orchestrator>, data_dir: PathBuf) -> Self {
        Self {
            orchestrator,
            sessions: SessionStore::default(),
            data_dir,
        }
    }
}

/// Build the Axum router with all routes.
pub fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(routes::health))
        .route("/ws", get(ws::ws_handler))
        .route("/api", axum::routing::post(dispatch_handler_http))
        .route("/sessions/:session_id/images/:file_id", get(routes::serve_session_image))
        .with_state(state)
}

/// HTTP POST /api — JSON-RPC 2.0
async fn dispatch_handler_http(
    State(state): State<Arc<AppState>>,
    Json(req): Json<loom_types::JsonRpcRequest>,
) -> Json<loom_types::JsonRpcResponse> {
    Json(dispatch::dispatch_handler(state, req).await)
}

/// Start the server on the given address.
pub async fn serve(host: &str, port: u16, orchestrator: Arc<Orchestrator>, data_dir: &std::path::Path) -> anyhow::Result<()> {
    tracing::info!(data_dir = %data_dir.display(), "loom-server starting");
    let state = Arc::new(AppState::new(orchestrator, data_dir.to_path_buf()));
    // Hydrate sessions from persisted store
    let sessions = state.orchestrator.list_persisted_sessions().await;
    for (id, created_at, message_count, title) in sessions {
        state
            .sessions
            .restore(id, created_at, message_count, title)
            .await;
    }
    let app = build_router(state);
    let addr = format!("{}:{}", host, port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    let actual_addr = listener.local_addr()?;
    tracing::info!("loom-server listening on http://{}", actual_addr);
    println!("{{\"type\":\"ready\",\"port\":{}}}", actual_addr.port());
    println!("Server started: http://{}", actual_addr);
    println!("Health check: http://{}/health", actual_addr);
    axum::serve(listener, app).await?;
    Ok(())
}

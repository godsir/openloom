// SPDX-License-Identifier: Apache-2.0
//! HTTP/WebSocket server for openLoom v2.
//!
//! Provides Axum routes for JSON-RPC dispatch over HTTP POST and WebSocket.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use axum::{Json, Router, extract::State, routing::get};
use loom_core::Orchestrator;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

mod credential;
mod dispatch;
mod routes;
mod ws;

pub use credential::{load_credentials, persist_credentials, save_key};
pub use dispatch::SessionStore;
pub use ws::ws_handler;

/// Shared application state passed to all route handlers.
pub struct AppState {
    pub orchestrator: Arc<Orchestrator>,
    pub sessions: SessionStore,
    pub data_dir: PathBuf,
    /// Global shutdown token — cancelled on SIGTERM/SIGINT.
    /// Route handlers should check this before accepting new work.
    pub shutdown_token: CancellationToken,
    /// In-memory API key store. Keys are stored as (env_name → api_key_value).
    /// This replaces the unsafe std::env::set_var approach. Keys are persisted
    /// to <data_dir>/credentials.json on every save.
    pub key_store: Arc<RwLock<HashMap<String, String>>>,
}

impl AppState {
    pub fn new(
        orchestrator: Arc<Orchestrator>,
        data_dir: PathBuf,
        shutdown_token: CancellationToken,
        key_store: Arc<RwLock<HashMap<String, String>>>,
    ) -> Self {
        Self {
            orchestrator,
            sessions: SessionStore::default(),
            data_dir,
            shutdown_token,
            key_store,
        }
    }
}

/// Build the Axum router with all routes.
pub fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(routes::health))
        .route("/ws", get(ws::ws_handler))
        .route("/api", axum::routing::post(dispatch_handler_http))
        .route(
            "/sessions/:session_id/images/:file_id",
            get(routes::serve_session_image),
        )
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
/// Returns when the shutdown token is cancelled (SIGTERM/SIGINT).
pub async fn serve(
    host: &str,
    port: u16,
    orchestrator: Arc<Orchestrator>,
    data_dir: &std::path::Path,
    shutdown_token: CancellationToken,
) -> anyhow::Result<()> {
    tracing::info!(data_dir = %data_dir.display(), "loom-server starting");
    let _ = std::fs::create_dir_all(data_dir);

    // Load persisted API keys and share them between AppState and the orchestrator.
    let loaded_keys = credential::load_credentials(data_dir).await;
    let key_store = Arc::new(RwLock::new(loaded_keys));
    orchestrator.set_key_store(key_store.clone()).await;

    let state = Arc::new(AppState::new(
        orchestrator,
        data_dir.to_path_buf(),
        shutdown_token.clone(),
        key_store,
    ));

    // Hydrate sessions from persisted store
    let sessions = state.orchestrator.list_persisted_sessions().await;
    for (id, created_at, message_count, title, updated_at) in sessions {
        let effective_updated = updated_at.unwrap_or_else(|| created_at.clone());
        state
            .sessions
            .restore(
                id.clone(),
                created_at,
                effective_updated,
                message_count,
                title,
            )
            .await;
        // Restore persisted agent binding
        let agent_name = state
            .orchestrator
            .memory_store_session_agent_name(&id)
            .await;
        if let Some(name) = agent_name {
            let _ = state.sessions.bind_agent(&id, &name).await;
        }
    }

    // Phase 3: Start background forgetting loop (hourly check, 7-day interval gate).
    state.orchestrator.spawn_forgetting_loop();

    let app = build_router(state);
    let addr = format!("{}:{}", host, port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    let actual_addr = listener.local_addr()?;
    tracing::info!("loom-server listening on http://{}", actual_addr);
    println!("{{\"type\":\"ready\",\"port\":{}}}", actual_addr.port());
    println!("Server started: http://{}", actual_addr);
    println!("Health check: http://{}/health", actual_addr);

    // Graceful shutdown: axum stops accepting new connections when the token
    // is cancelled, drains existing connections, then returns.
    tracing::info!("loom-server running — waiting for shutdown signal");
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            shutdown_token.cancelled().await;
            tracing::info!("loom-server shutdown signal received");
        })
        .await?;

    tracing::info!("loom-server shutdown complete");
    Ok(())
}

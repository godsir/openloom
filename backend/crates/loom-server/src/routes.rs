//! Route handlers.

use axum::{
    Json,
    extract::{Path, State},
    http::{StatusCode, header},
    response::IntoResponse,
};
use serde_json::json;
use std::sync::Arc;

use crate::AppState;

pub async fn health(State(state): State<Arc<AppState>>) -> (StatusCode, Json<serde_json::Value>) {
    let agents = state.orchestrator.list_agents().await;
    (
        StatusCode::OK,
        Json(json!({
            "status": "ok",
            "version": env!("CARGO_PKG_VERSION"),
            "agents": agents.len(),
        })),
    )
}

/// Serve a session image file from ~/.loom/sessions/<session_id>/images/<file_id>.
pub async fn serve_session_image(
    State(state): State<Arc<AppState>>,
    Path((session_id, file_id)): Path<(String, String)>,
) -> impl IntoResponse {
    // Prevent directory traversal
    if session_id.contains("..")
        || file_id.contains("..")
        || session_id.contains('/')
        || session_id.contains('\\')
        || file_id.contains('/')
        || file_id.contains('\\')
    {
        return (StatusCode::BAD_REQUEST, "invalid path").into_response();
    }

    let path = state
        .data_dir
        .join("sessions")
        .join(&session_id)
        .join("images")
        .join(&file_id);

    tracing::info!(data_dir = %state.data_dir.display(), session_id = %session_id, file_id = %file_id, path = %path.display(), "serving session image request");

    match tokio::fs::read(&path).await {
        Ok(data) => {
            let mime = if file_id.ends_with(".jpg") || file_id.ends_with(".jpeg") {
                "image/jpeg"
            } else if file_id.ends_with(".gif") {
                "image/gif"
            } else if file_id.ends_with(".webp") {
                "image/webp"
            } else if file_id.ends_with(".bmp") {
                "image/bmp"
            } else if file_id.ends_with(".svg") {
                "image/svg+xml"
            } else {
                "image/png"
            };
            (
                [
                    (header::CONTENT_TYPE, mime.to_string()),
                    (
                        header::CACHE_CONTROL,
                        "public, max-age=31536000, immutable".to_string(),
                    ),
                ],
                data,
            )
                .into_response()
        }
        Err(_) => {
            tracing::warn!(session_id = %session_id, file_id = %file_id, path = %path.display(), "session image not found on disk");
            (StatusCode::NOT_FOUND, "image not found").into_response()
        }
    }
}

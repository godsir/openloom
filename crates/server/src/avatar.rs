use std::path::PathBuf;
use std::sync::Arc;

use axum::Json;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{StatusCode, header};
use axum::response::Response;
use openloom_engine::Engine;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct AvatarUpload {
    pub data: String,
}

fn avatars_dir(data_dir: &std::path::Path) -> PathBuf {
    data_dir.join("avatars")
}

/// Serve user avatar: GET /api/avatar/:role
pub async fn serve_avatar(
    State(engine): State<Arc<Engine>>,
    Path(role): Path<String>,
) -> Response<Body> {
    // Only allow alphanumeric role names
    if !role
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::from("invalid role"))
            .unwrap();
    }

    let dir = avatars_dir(engine.data_dir());
    let path = dir.join(format!("{}.png", role));

    match tokio::fs::read(&path).await {
        Ok(data) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "image/png")
            .header(header::CACHE_CONTROL, "public, max-age=3600")
            .body(Body::from(data))
            .unwrap(),
        Err(_) => {
            // No custom avatar — return Loom text fallback
            let color = if role == "agent" { "1a1a1a" } else { "2f6f8f" };
            let svg = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"80\" height=\"80\" viewBox=\"0 0 80 80\"><rect width=\"80\" height=\"80\" rx=\"20\" fill=\"#fff\" stroke=\"#d0d0d0\" stroke-width=\"1\"/><text x=\"50%\" y=\"54%\" text-anchor=\"middle\" font-family=\"sans-serif\" font-size=\"36\" font-weight=\"600\" fill=\"#".to_string() + color + "\">L</text></svg>";
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "image/svg+xml")
                .header(header::CACHE_CONTROL, "public, max-age=3600")
                .body(Body::from(svg))
                .unwrap()
        }
    }
}

/// Upload user avatar: POST /api/avatar/:role
pub async fn upload_avatar(
    State(engine): State<Arc<Engine>>,
    Path(role): Path<String>,
    Json(payload): Json<AvatarUpload>,
) -> Response<Body> {
    if !role
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::from("invalid role"))
            .unwrap();
    }

    let dir = avatars_dir(engine.data_dir());
    if let Err(e) = tokio::fs::create_dir_all(&dir).await {
        return Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::from(format!("mkdir failed: {}", e)))
            .unwrap();
    }

    let data_url = payload.data;

    // Strip data URL prefix
    let b64 = if let Some(comma) = data_url.find(',') {
        &data_url[comma + 1..]
    } else {
        &data_url
    };

    use base64::Engine as _;
    let bytes = match base64::engine::general_purpose::STANDARD.decode(b64) {
        Ok(b) => b,
        Err(e) => {
            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Body::from(format!("invalid base64: {}", e)))
                .unwrap();
        }
    };

    let path = dir.join(format!("{}.png", role));
    if let Err(e) = tokio::fs::write(&path, &bytes).await {
        return Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::from(format!("write failed: {}", e)))
            .unwrap();
    }

    tracing::info!(role = %role, path = %path.display(), "avatar uploaded");
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"ok":true}"#))
        .unwrap()
}

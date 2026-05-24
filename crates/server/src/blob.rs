use axum::{Json, extract::State, response::IntoResponse};
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;

use openloom_engine::Engine;

#[derive(Deserialize)]
pub struct UploadRequest {
    pub name: String,
    #[serde(rename = "base64Data")]
    pub base64_data: String,
    #[serde(rename = "mimeType")]
    pub mime_type: String,
    #[serde(rename = "sessionPath")]
    pub session_path: Option<String>,
}

#[derive(Serialize)]
pub struct UploadItem {
    #[serde(rename = "fileId")]
    pub file_id: String,
    pub dest: String,
    pub name: String,
}

#[derive(Serialize)]
pub struct UploadResponse {
    pub uploads: Vec<UploadItem>,
}

pub async fn handle_upload_blob(
    State(_engine): State<Arc<Engine>>,
    Json(req): Json<UploadRequest>,
) -> impl IntoResponse {
    let data_dir = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("openLoom")
        .join("uploads");

    let subdir = req.session_path.as_deref().unwrap_or("_general");
    let dest_dir = data_dir.join(subdir);
    if let Err(e) = std::fs::create_dir_all(&dest_dir) {
        return Json(serde_json::json!({
            "uploads": [],
            "error": format!("failed to create upload dir: {}", e),
        }));
    }

    let file_id = Uuid::new_v4().to_string();

    // Preserve file extension from the original name
    let ext = std::path::Path::new(&req.name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("png");
    let dest_name = format!("{}.{}", file_id, ext);
    let dest_path = dest_dir.join(&dest_name);

    let data = match base64::engine::general_purpose::STANDARD.decode(&req.base64_data) {
        Ok(d) => d,
        Err(e) => {
            return Json(serde_json::json!({
                "uploads": [],
                "error": format!("base64 decode failed: {}", e),
            }));
        }
    };

    if let Err(e) = std::fs::write(&dest_path, &data) {
        return Json(serde_json::json!({
            "uploads": [],
            "error": format!("write failed: {}", e),
        }));
    }

    Json(serde_json::json!({
        "uploads": [{
            "fileId": file_id,
            "dest": dest_path.to_string_lossy().to_string(),
            "name": req.name,
        }]
    }))
}

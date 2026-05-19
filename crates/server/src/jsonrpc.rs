use axum::{Json, http::StatusCode};
use openloom_models::*;

pub async fn handle_jsonrpc(
    Json(req): Json<JsonRpcRequest>,
) -> Result<Json<JsonRpcResponse>, StatusCode> {
    let result = match req.method.as_str() {
        "system.health" => Ok(serde_json::json!({"status": "ok"})),
        "system.shutdown" => Ok(serde_json::json!({"ok": true})),
        "skill.list" => Ok(serde_json::json!({"skills": []})),
        "cache.stats" => Ok(serde_json::json!({"hit_rate": 0.0})),
        _ => Err(JsonRpcError {
            code: ErrorCode::MethodNotFound,
            message: format!("method '{}' not found", req.method),
            data: None,
        }),
    };

    match result {
        Ok(value) => Ok(Json(JsonRpcResponse {
            jsonrpc: "2.0".into(),
            result: Some(value),
            error: None,
            id: req.id,
        })),
        Err(err) => Ok(Json(JsonRpcResponse {
            jsonrpc: "2.0".into(),
            result: None,
            error: Some(err),
            id: req.id,
        })),
    }
}

use axum::{Json, extract::State, http::StatusCode};
use openloom_engine::Engine;
use openloom_models::*;
use std::sync::Arc;

use crate::dispatch;

pub async fn handle_jsonrpc(
    State(engine): State<Arc<Engine>>,
    Json(req): Json<JsonRpcRequest>,
) -> Result<Json<JsonRpcResponse>, StatusCode> {
    let result = dispatch::dispatch_method(&engine, &req.method, req.params).await;

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

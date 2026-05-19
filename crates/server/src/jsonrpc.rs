use axum::{Json, extract::State, http::StatusCode};
use openloom_engine::Engine;
use openloom_models::*;
use std::sync::Arc;

pub async fn handle_jsonrpc(
    State(engine): State<Arc<Engine>>,
    Json(req): Json<JsonRpcRequest>,
) -> Result<Json<JsonRpcResponse>, StatusCode> {
    let result = match req.method.as_str() {
        "system.health" => {
            let health = engine.health_check().await;
            Ok(serde_json::to_value(health).unwrap_or_default())
        }
        "system.shutdown" => {
            let _ = engine.shutdown().await;
            Ok(serde_json::json!({"ok": true}))
        }
        "chat.send" => {
            let params = req.params.unwrap_or_default();
            let content = params
                .get("messages")
                .and_then(|m| m.as_array())
                .and_then(|arr| arr.last())
                .and_then(|m| m.get("content"))
                .and_then(|c| c.as_str())
                .unwrap_or("");
            let session_id = params
                .get("session_id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| "default".to_string());

            let msg = ChatMessage {
                role: "user".into(),
                content: content.to_string(),
            };
            match engine.handle_message(msg, &session_id).await {
                Ok(resp) => Ok(serde_json::to_value(resp).unwrap_or_default()),
                Err(e) => Err(JsonRpcError {
                    code: ErrorCode::InternalError,
                    message: e.to_string(),
                    data: None,
                }),
            }
        }
        "skill.list" => {
            let skills: Vec<serde_json::Value> = engine
                .list_skills()
                .iter()
                .map(|s| {
                    serde_json::json!({
                        "name": s.name,
                        "description": s.description,
                        "triggers": s.triggers,
                    })
                })
                .collect();
            Ok(serde_json::json!({"skills": skills}))
        }
        "skill.invoke" => {
            let params = req.params.unwrap_or_default();
            let skill_name = params
                .get("skill_name")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let skill_params = params.get("params").cloned().unwrap_or_default();
            match engine.invoke_skill(skill_name, skill_params).await {
                Ok(result) => Ok(serde_json::json!({"result": result})),
                Err(e) => Err(JsonRpcError {
                    code: ErrorCode::SkillFailed,
                    message: e.to_string(),
                    data: None,
                }),
            }
        }
        "memory.query" => Ok(serde_json::json!({
            "events": [],
            "cognitions": [],
            "note": "Phase 2: FTS5 search integration"
        })),
        "memory.persona" => Ok(serde_json::json!({
            "summary": "Phase 2: Persona Projector integration",
            "traits": []
        })),
        "agent.status" => Ok(serde_json::json!({
            "state": "idle",
            "active_session": null,
            "model_info": {"router": "qwen3-1.7b"}
        })),
        "cache.stats" => Ok(serde_json::json!({
            "hit_rate": 0.0,
            "block_count": 0,
            "total_size_mb": 0
        })),
        "config.get" => Ok(serde_json::json!({"config": {}})),
        "config.set" => Ok(serde_json::json!({"ok": true})),
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

use openloom_engine::Engine;
use openloom_models::*;
use serde_json::Value;

pub async fn dispatch_method(
    engine: &Engine,
    method: &str,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    match method {
        "system.health" => {
            let health = engine.health_check().await;
            Ok(serde_json::to_value(health).unwrap_or_default())
        }
        "chat.send" => {
            let p = params.unwrap_or_default();
            let content = p
                .get("messages")
                .and_then(|m| m.as_array())
                .and_then(|arr| arr.last())
                .and_then(|m| m.get("content"))
                .and_then(|c| c.as_str())
                .unwrap_or("");
            let session_id = p
                .get("session_id")
                .and_then(|v| v.as_str())
                .unwrap_or("default")
                .to_string();
            let msg = ChatMessage {
                role: "user".into(),
                content: content.to_string(),
            };
            engine
                .handle_message(msg, &session_id)
                .await
                .map(|r| serde_json::to_value(r).unwrap_or_default())
                .map_err(|e| JsonRpcError {
                    code: ErrorCode::InternalError,
                    message: e.to_string(),
                    data: None,
                })
        }
        "skill.list" => {
            let skills: Vec<Value> = engine
                .list_skills()
                .iter()
                .map(|s| serde_json::json!({
                    "name": s.name,
                    "description": s.description,
                    "triggers": s.triggers,
                }))
                .collect();
            Ok(serde_json::json!({"skills": skills}))
        }
        "skill.invoke" => {
            let p = params.unwrap_or_default();
            let name = p.get("skill_name").and_then(|v| v.as_str()).unwrap_or("");
            let skill_params = p.get("params").cloned().unwrap_or_default();
            engine
                .invoke_skill(name, skill_params)
                .await
                .map(|r| serde_json::json!({"result": r}))
                .map_err(|e| JsonRpcError {
                    code: ErrorCode::SkillFailed,
                    message: e.to_string(),
                    data: None,
                })
        }
        "session.list" => {
            let sessions = engine.list_sessions().await.map_err(|e| JsonRpcError {
                code: ErrorCode::InternalError,
                message: e.to_string(),
                data: None,
            })?;
            Ok(serde_json::json!({"sessions": sessions}))
        }
        "session.create" => {
            let session = engine.create_session().await.map_err(|e| JsonRpcError {
                code: ErrorCode::InternalError,
                message: e.to_string(),
                data: None,
            })?;
            Ok(serde_json::to_value(session).unwrap_or_default())
        }
        "memory.cognitions" => {
            Ok(serde_json::json!({"cognitions": [], "note": "Query via CLI in Phase 2 Milestone B"}))
        }
        "memory.persona" => {
            Ok(serde_json::json!({"summary": "", "traits": [], "note": "Persona Projector in Milestone B"}))
        }
        "memory.query" => {
            Ok(serde_json::json!({"events": [], "cognitions": [], "note": "FTS5 search in Phase 2"}))
        }
        "agent.status" => {
            Ok(serde_json::json!({"state": "idle", "active_session": null, "model_info": {"router": "qwen3-1.7b"}}))
        }
        "cache.stats" => {
            Ok(serde_json::json!({"hit_rate": 0.0, "block_count": 0, "total_size_mb": 0}))
        }
        "system.shutdown" => Ok(serde_json::json!({"ok": true})),
        "config.get" => Ok(serde_json::json!({"config": {}})),
        "config.set" => Ok(serde_json::json!({"ok": true})),
        _ => Err(JsonRpcError {
            code: ErrorCode::MethodNotFound,
            message: format!("method '{}' not found", method),
            data: None,
        }),
    }
}

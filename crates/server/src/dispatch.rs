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
                timestamp: chrono::Utc::now(),
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
        "session.switch" => {
            let session_id = params
                .as_ref()
                .and_then(|p| p.get("session_id"))
                .and_then(|v| v.as_str())
                .unwrap_or("default")
                .to_string();
            let sessions = engine.list_sessions().await.map_err(|e| JsonRpcError {
                code: ErrorCode::InternalError,
                message: e.to_string(),
                data: None,
            })?;
            let found = sessions.iter().any(|s| s.id == session_id);
            if found {
                Ok(serde_json::json!({"session_id": session_id}))
            } else {
                let session = engine.create_session().await.map_err(|e| JsonRpcError {
                    code: ErrorCode::InternalError,
                    message: e.to_string(),
                    data: None,
                })?;
                Ok(serde_json::json!({"session_id": session.id}))
            }
        }
        "memory.cognitions" => {
            let subject = params
                .as_ref()
                .and_then(|p| p.get("subject"))
                .and_then(|v| v.as_str())
                .unwrap_or("USER");
            let limit = params
                .as_ref()
                .and_then(|p| p.get("limit"))
                .and_then(|v| v.as_u64())
                .unwrap_or(20) as usize;
            let cognitions =
                engine
                    .list_cognitions(subject, limit)
                    .await
                    .map_err(|e| JsonRpcError {
                        code: ErrorCode::InternalError,
                        message: e.to_string(),
                        data: None,
                    })?;
            let rows: Vec<serde_json::Value> = cognitions
                .iter()
                .map(|c| {
                    serde_json::json!({
                        "id": c.id,
                        "trait": c.trait_name,
                        "value": c.value,
                        "confidence": c.confidence,
                        "evidence_count": c.evidence_count,
                        "version": c.version,
                    })
                })
                .collect();
            Ok(serde_json::json!({"cognitions": rows}))
        }
        "memory.persona" => {
            let summary = engine.persona_summary().await;
            Ok(serde_json::json!({"summary": summary, "traits": []}))
        }
        "memory.query" => {
            let query = params
                .as_ref()
                .and_then(|p| p.get("query"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let limit = params
                .as_ref()
                .and_then(|p| p.get("limit"))
                .and_then(|v| v.as_u64())
                .unwrap_or(20) as usize;
            let events = engine
                .search_events(query, limit)
                .await
                .map_err(|e| JsonRpcError {
                    code: ErrorCode::InternalError,
                    message: e.to_string(),
                    data: None,
                })?;
            Ok(serde_json::json!({"events": events, "cognitions": []}))
        }
        "agent.status" => {
            let state = engine.agent_state().await;
            Ok(
                serde_json::json!({"state": state, "active_session": null, "model_info": {"router": "qwen3-1.7b"}}),
            )
        }
        "cache.stats" => {
            let stats = engine.cache_stats();
            Ok(
                serde_json::json!({"hit_rate": stats.hit_rate, "block_count": stats.block_count, "total_size_mb": stats.total_size_mb}),
            )
        }
        "system.shutdown" => {
            engine.shutdown().await.map_err(|e| JsonRpcError {
                code: ErrorCode::InternalError,
                message: e.to_string(),
                data: None,
            })?;
            Ok(serde_json::json!({"ok": true}))
        }
        "config.get" => {
            let key = params
                .as_ref()
                .and_then(|p| p.get("key"))
                .and_then(|v| v.as_str());
            let config = engine.get_config(key).await;
            Ok(serde_json::json!({"config": config}))
        }
        "config.set" => {
            let key = params
                .as_ref()
                .and_then(|p| p.get("key"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let value = params
                .as_ref()
                .and_then(|p| p.get("value"))
                .map(|v| v.to_string())
                .unwrap_or_default();
            engine
                .set_config(key, &value)
                .await
                .map_err(|e| JsonRpcError {
                    code: ErrorCode::InternalError,
                    message: e.to_string(),
                    data: None,
                })?;
            Ok(serde_json::json!({"ok": true}))
        }
        "memory.cognition_snapshots" => {
            let id = params
                .as_ref()
                .and_then(|p| p.get("cognition_id"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as i64;
            let snapshots = engine
                .cognition_snapshots(id)
                .await
                .map_err(|e| JsonRpcError {
                    code: ErrorCode::InternalError,
                    message: e.to_string(),
                    data: None,
                })?;
            Ok(serde_json::json!({"snapshots": snapshots}))
        }
        "memory.cognition_rollback" => {
            let id = params
                .as_ref()
                .and_then(|p| p.get("cognition_id"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as i64;
            let version = params
                .as_ref()
                .and_then(|p| p.get("version"))
                .and_then(|v| v.as_u64())
                .unwrap_or(1) as i64;
            engine
                .rollback_cognition(id, version)
                .await
                .map_err(|e| JsonRpcError {
                    code: ErrorCode::InternalError,
                    message: e.to_string(),
                    data: None,
                })?;
            Ok(serde_json::json!({"ok": true}))
        }
        _ => Err(JsonRpcError {
            code: ErrorCode::MethodNotFound,
            message: format!("method '{}' not found", method),
            data: None,
        }),
    }
}

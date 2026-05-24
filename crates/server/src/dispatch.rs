use openloom_engine::Engine;
use openloom_models::*;
use serde_json::Value;

pub async fn dispatch_method(
    engine: &std::sync::Arc<Engine>,
    method: &str,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    match method {
        "system.health" => {
            let health = engine.health_check().await;
            let mut result = serde_json::to_value(health).unwrap_or_default();
            // Front-end expects avatars field for settings UI
            if result.is_object() && !result.as_object().unwrap().contains_key("avatars") {
                let avatars_dir = engine.data_dir().join("avatars");
                let user_avatar = avatars_dir.join("user.png").exists();
                let agent_avatar = avatars_dir.join("agent-default.png").exists();
                result["avatars"] = serde_json::json!({"agent": agent_avatar, "user": user_avatar});
            }
            Ok(result)
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
            let model_id = p
                .get("model_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let provider = p
                .get("provider")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            // Notify frontend: agent is thinking
            let event_bus = engine.event_bus().clone();
            let _ = event_bus.send(EngineEvent::AgentStateChanged {
                old_state: openloom_models::AgentState::Idle,
                new_state: openloom_models::AgentState::Thinking,
            });

            // Parse images if present
            let images: Vec<openloom_models::ImagePart> = p
                .get("images")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter().filter_map(|img| {
                        let raw_data = img.get("data")?.as_str()?.to_string();
                        let mime_type = img.get("mime_type")?.as_str()?.to_string();
                        // Strip "data:<mime>;base64," prefix if present — engine will re-add it
                        let data = if let Some(comma_pos) = raw_data.find(',') {
                            if raw_data[..comma_pos].contains("base64") {
                                raw_data[comma_pos + 1..].to_string()
                            } else {
                                raw_data
                            }
                        } else {
                            raw_data
                        };
                        Some(openloom_models::ImagePart { data, mime_type })
                    }).collect()
                })
                .unwrap_or_default();

            if !model_id.is_empty() && !provider.is_empty() {
                // Use streaming: each token fires StreamDelta via event_bus.
                // ws.rs spawns this in a background task so the event loop stays alive.
                let _ = event_bus.send(EngineEvent::AgentStateChanged {
                    old_state: openloom_models::AgentState::Thinking,
                    new_state: openloom_models::AgentState::Acting,
                });
                let metadata = if !images.is_empty() {
                    let imgs: Vec<Value> = images.iter().map(|img| serde_json::json!({
                        "data": img.data,
                        "mimeType": img.mime_type,
                    })).collect();
                    Some(serde_json::json!({"images": imgs}).to_string())
                } else { None };
                match engine.complete_with_model_streaming_meta(&session_id, content, &images, metadata.as_deref(), model_id, provider).await {
                    Ok(()) => {
                        let _ = event_bus.send(EngineEvent::AgentStateChanged {
                            old_state: openloom_models::AgentState::Acting,
                            new_state: openloom_models::AgentState::Idle,
                        });
                        // Background: run LLM extraction every 5 user messages (fallback to rule-based)
                        let bg_engine = engine.clone();
                        let bg_sid = session_id.clone();
                        tokio::spawn(async move {
                            let working = bg_engine.get_working_memory(&bg_sid).unwrap_or_default();
                            let user_count = working.iter().filter(|m| m.role == "user").count();
                            if user_count > 0 && user_count % 5 == 0 {
                                match bg_engine.extract_cognitions_from_session(&bg_sid).await {
                                    Ok(n) if n > 0 => tracing::info!(n, session_id = %bg_sid, "bg cognition extraction: {} new traits", n),
                                    Ok(_) => tracing::debug!(session_id = %bg_sid, "bg extraction: no new cognitions"),
                                    Err(e) => tracing::warn!(error = %e, session_id = %bg_sid, "bg extraction failed"),
                                }
                            }
                        });
                        Ok(serde_json::json!({"ok": true, "streaming": true, "session_id": session_id}))
                    }
                    Err(e) => {
                        let _ = event_bus.send(EngineEvent::AgentStateChanged {
                            old_state: openloom_models::AgentState::Acting,
                            new_state: openloom_models::AgentState::Idle,
                        });
                        Err(JsonRpcError {
                            code: ErrorCode::InternalError,
                            message: e.to_string(),
                            data: None,
                        })
                    }
                }
            } else {
                // Fallback: use the router
                let msg = ChatMessage {
                    role: "user".into(),
                    content: content.to_string(),
                    timestamp: chrono::Utc::now(),
            id: None,
            seq: None,
                    metadata: None,
                };
                let mode = p
                    .get("mode")
                    .and_then(|v| v.as_str())
                    .and_then(openloom_models::Mode::from_key)
                    .unwrap_or(openloom_models::Mode::Chat);
                let model_pref = p
                    .get("model_pref")
                    .and_then(|v| v.as_str())
                    .and_then(openloom_models::ModelPreference::from_key)
                    .unwrap_or_default();
                let result = engine
                    .handle_message(msg, &session_id, mode, model_pref)
                    .await;
                let _ = event_bus.send(EngineEvent::AgentStateChanged {
                    old_state: openloom_models::AgentState::Thinking,
                    new_state: openloom_models::AgentState::Idle,
                });
                // Background: fallback cognition extraction every 5 user messages
                let fb_engine = engine.clone();
                let fb_sid = session_id.clone();
                tokio::spawn(async move {
                    let working = fb_engine.get_working_memory(&fb_sid).unwrap_or_default();
                    let user_count = working.iter().filter(|m| m.role == "user").count();
                    if user_count > 0 && user_count % 5 == 0 {
                        match fb_engine.extract_cognitions_from_session(&fb_sid).await {
                            Ok(n) if n > 0 => tracing::info!(n, session_id = %fb_sid, "fallback cognition extraction: {} new traits", n),
                            Ok(_) => tracing::debug!(session_id = %fb_sid, "fallback extraction: no new cognitions"),
                            Err(e) => tracing::warn!(error = %e, session_id = %fb_sid, "fallback extraction failed"),
                        }
                    }
                });

                result
                    .map(|r| {
                        let response = r.response.clone();
                        let _ = event_bus.send(EngineEvent::StreamEnd {
                            session_id: session_id.clone(),
                            full_response: response.clone(),
                        });
                        serde_json::to_value(r).unwrap_or_default()
                    })
                    .map_err(|e| JsonRpcError {
                        code: ErrorCode::InternalError,
                        message: e.to_string(),
                        data: None,
                    })
            }
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
        "skill.list_all" => {
            let skills: Vec<Value> = engine
                .list_all_skills()
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
            let mapped: Vec<serde_json::Value> = sessions.iter().map(|s| {
                serde_json::json!({
                    "path": s.id,
                    "title": s.title,
                    "firstMessage": "",
                    "modified": s.created_at,
                    "messageCount": s.message_count,
                    "agentId": null,
                    "agentName": null,
                    "cwd": null,
                    "permissionMode": null,
                    "pinnedAt": s.pinned_at,
                })
            }).collect();
            Ok(serde_json::json!({"sessions": mapped}))
        }
        "session.create" => {
            let session = engine.create_session().await.map_err(|e| JsonRpcError {
                code: ErrorCode::InternalError,
                message: e.to_string(),
                data: None,
            })?;
            let model_info = engine.model_info().await;
            let perm_mode = engine.permission_mode(&session.id);
            Ok(serde_json::json!({
                "session_id": session.id,
                "path": session.id,
                "agentId": "default",
                "agentName": "Loom",
                "isStreaming": false,
                "permissionMode": perm_mode,
                "accessMode": if perm_mode == "read_only" { "read_only" } else { "full" },
                "planMode": perm_mode == "read_only",
                "currentModelId": model_info.model_id,
                "currentModelProvider": model_info.backend,
                "currentModelName": model_info.display_name,
                "currentModelReasoning": false,
                "memoryEnabled": true,
            }))
        }
        "session.switch" => {
            let session_id = params
                .as_ref()
                .and_then(|p| p.get("session_id"))
                .or_else(|| params.as_ref().and_then(|p| p.get("path")))
                .and_then(|v| v.as_str())
                .unwrap_or("default")
                .to_string();
            let sessions = engine.list_sessions().await.map_err(|e| JsonRpcError {
                code: ErrorCode::InternalError,
                message: e.to_string(),
                data: None,
            })?;
            let found = sessions.iter().any(|s| s.id == session_id);
            let sid = if found {
                session_id.clone()
            } else {
                let session = engine.create_session().await.map_err(|e| JsonRpcError {
                    code: ErrorCode::InternalError,
                    message: e.to_string(),
                    data: None,
                })?;
                session.id
            };
            let model_info = engine.model_info().await;
            let perm_mode = engine.permission_mode(&sid);
            Ok(serde_json::json!({
                "session_id": sid,
                "path": sid,
                "agentId": "default",
                "agentName": "Loom",
                "isStreaming": false,
                "permissionMode": perm_mode,
                "accessMode": if perm_mode == "read_only" { "read_only" } else { "full" },
                "planMode": perm_mode == "read_only",
                "currentModelId": model_info.model_id,
                "currentModelProvider": model_info.backend,
                "currentModelName": model_info.display_name,
                "currentModelReasoning": false,
                "memoryEnabled": true,
            }))
        }
        "memory.cognitions" => {
            let subject = params
                .as_ref()
                .and_then(|p| p.get("subject"))
                .and_then(|v| v.as_str())
                .unwrap_or("USER");
            let scope = params
                .as_ref()
                .and_then(|p| p.get("scope"))
                .and_then(|v| v.as_str());
            let limit = params
                .as_ref()
                .and_then(|p| p.get("limit"))
                .and_then(|v| v.as_u64())
                .unwrap_or(20) as usize;
            let offset = params
                .as_ref()
                .and_then(|p| p.get("offset"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as usize;
            let total = engine
                    .count_cognitions(subject, scope)
                    .await
                    .map_err(|e| JsonRpcError {
                        code: ErrorCode::InternalError,
                        message: e.to_string(),
                        data: None,
                    })?;
            let cognitions =
                engine
                    .list_cognitions(subject, scope, limit, offset)
                    .await
                    .map_err(|e| JsonRpcError {
                        code: ErrorCode::InternalError,
                        message: e.to_string(),
                        data: None,
                    })?;
            let rows: Vec<serde_json::Value> = cognitions
                .into_iter().map(|c| {
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
            Ok(serde_json::json!({"cognitions": rows, "total": total}))
        }
        "memory.cognition_delete" => {
            let id = params.as_ref().and_then(|p| p.get("id")).and_then(|v| v.as_i64()).unwrap_or(0);
            if id == 0 {
                return Err(JsonRpcError { code: ErrorCode::InvalidRequest, message: "id required".into(), data: None });
            }
            engine.delete_cognition(id).await.map_err(|e| JsonRpcError {
                code: ErrorCode::InternalError,
                message: e.to_string(),
                data: None,
            })?;
            Ok(serde_json::json!({"ok": true}))
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
        "memory.record" => {
            let text = params.as_ref().and_then(|p| p.get("text")).and_then(|v| v.as_str()).unwrap_or("");
            let scope = params.as_ref().and_then(|p| p.get("session_id")).and_then(|v| v.as_str()).unwrap_or("_manual");
            if text.is_empty() {
                return Err(JsonRpcError { code: ErrorCode::InvalidRequest, message: "text required".into(), data: None });
            }
            let count = engine.extract_cognitions_with_local_model(text, scope).await.map_err(|e| JsonRpcError {
                code: ErrorCode::InternalError,
                message: e.to_string(),
                data: None,
            })?;
            Ok(serde_json::json!({"ok": true, "cognitions": count}))
        }
        "memory.record_from_session" => {
            let session_id = params.as_ref().and_then(|p| p.get("session_id")).and_then(|v| v.as_str()).unwrap_or("default");
            let count = engine.extract_cognitions_from_session(session_id).await.map_err(|e| JsonRpcError {
                code: ErrorCode::InternalError,
                message: e.to_string(),
                data: None,
            })?;
            Ok(serde_json::json!({"ok": true, "cognitions": count}))
        }
        "agent.status" => {
            let state = engine.agent_state().await;
            let model_info = engine.current_model_id();
            Ok(
                serde_json::json!({"state": state, "active_session": null, "model_info": {"router": model_info}}),
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
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            engine
                .set_config(key, value)
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
        // ── Session lifecycle ───────────────────────────────────────
        "session.archive" => {
            let sid = params.as_ref().and_then(|p| p.get("session_id")).or_else(|| params.as_ref().and_then(|p| p.get("path"))).and_then(|v| v.as_str()).unwrap_or("");
            engine.archive_session(sid).await.map_err(|e| JsonRpcError {
                code: ErrorCode::InternalError,
                message: e.to_string(),
                data: None,
            })?;
            Ok(serde_json::json!({"ok": true, "session_id": sid}))
        }
        "session.delete" => {
            let sid = params.as_ref().and_then(|p| p.get("session_id")).or_else(|| params.as_ref().and_then(|p| p.get("path"))).and_then(|v| v.as_str()).unwrap_or("");
            engine.delete_session(sid).await.map_err(|e| JsonRpcError {
                code: ErrorCode::InternalError,
                message: e.to_string(),
                data: None,
            })?;
            Ok(serde_json::json!({"ok": true, "session_id": sid}))
        }
        "session.rename" => {
            let sid = params.as_ref().and_then(|p| p.get("session_id")).or_else(|| params.as_ref().and_then(|p| p.get("path"))).and_then(|v| v.as_str()).unwrap_or("");
            let title = params.as_ref().and_then(|p| p.get("title")).and_then(|v| v.as_str()).unwrap_or("");
            engine.rename_session(sid, title).await.map_err(|e| JsonRpcError {
                code: ErrorCode::InternalError,
                message: e.to_string(),
                data: None,
            })?;
            Ok(serde_json::json!({"ok": true, "session_id": sid}))
        }
        "session.pin" => {
            let sid = params.as_ref().and_then(|p| p.get("session_id")).or_else(|| params.as_ref().and_then(|p| p.get("path"))).and_then(|v| v.as_str()).unwrap_or("");
            let pinned = params.as_ref().and_then(|p| p.get("pinned")).and_then(|v| v.as_bool()).unwrap_or(true);
            engine.pin_session(sid, pinned).await.map_err(|e| JsonRpcError {
                code: ErrorCode::InternalError,
                message: e.to_string(),
                data: None,
            })?;
            let pinned_at = if pinned {
                serde_json::Value::String(chrono::Utc::now().to_rfc3339())
            } else {
                serde_json::Value::Null
            };
            Ok(serde_json::json!({"ok": true, "session_id": sid, "pinned": pinned, "pinnedAt": pinned_at}))
        }
        "session.archived_list" => {
            let sessions = engine.list_archived_sessions().await.map_err(|e| JsonRpcError {
                code: ErrorCode::InternalError,
                message: e.to_string(),
                data: None,
            })?;
            let archived: Vec<Value> = sessions.iter().map(|s| {
                serde_json::json!({
                    "path": s.id,
                    "title": s.title,
                    "archivedAt": s.archived_at,
                    "sizeBytes": 0,
                    "agentId": null,
                    "agentName": null,
                })
            }).collect();
            Ok(serde_json::json!({"archived": archived}))
        }
        "session.restore" => {
            let sid = params.as_ref().and_then(|p| p.get("session_id")).or_else(|| params.as_ref().and_then(|p| p.get("path"))).and_then(|v| v.as_str()).unwrap_or("");
            let ok = engine.restore_session(sid).await.map_err(|e| JsonRpcError {
                code: ErrorCode::InternalError,
                message: e.to_string(),
                data: None,
            })?;
            Ok(serde_json::json!({"ok": ok, "session_id": sid}))
        }
        "session.delete_archived" => {
            let sid = params.as_ref().and_then(|p| p.get("session_id")).or_else(|| params.as_ref().and_then(|p| p.get("path"))).and_then(|v| v.as_str()).unwrap_or("");
            let ok = engine.delete_archived_session(sid).await.map_err(|e| JsonRpcError {
                code: ErrorCode::InternalError,
                message: e.to_string(),
                data: None,
            })?;
            Ok(serde_json::json!({"ok": ok, "session_id": sid}))
        }
        "session.cleanup" => {
            let days = params.as_ref().and_then(|p| p.get("maxAgeDays")).and_then(|v| v.as_u64()).unwrap_or(30) as u32;
            let deleted = engine.cleanup_archived_sessions(days).await.map_err(|e| JsonRpcError {
                code: ErrorCode::InternalError,
                message: e.to_string(),
                data: None,
            })?;
            Ok(serde_json::json!({"deleted": deleted}))
        }
        "session.messages" => {
            let sid = params.as_ref().and_then(|p| p.get("session_id")).or_else(|| params.as_ref().and_then(|p| p.get("path"))).and_then(|v| v.as_str()).unwrap_or("default");
            let messages = engine.get_working_memory(sid).unwrap_or_default();
            let msg_values: Vec<Value> = messages.iter().map(|m| {
                let mut msg = serde_json::json!({
                    "id": format!("hist-{}-{}", sid, m.timestamp.timestamp_millis()),
                    "role": m.role,
                    "content": m.content,
                    "timestamp": m.timestamp.timestamp_millis(),
                });
                if let Some(db_id) = m.id {
                    msg["dbId"] = serde_json::json!(db_id);
                }
                if let Some(seq) = m.seq {
                    msg["seq"] = serde_json::json!(seq);
                }
                if let Some(ref meta) = m.metadata {
                    if let Ok(parsed) = serde_json::from_str::<Value>(meta) {
                        if let Some(images) = parsed.get("images").and_then(|f| f.as_array()) {
                            msg["images"] = serde_json::json!(images);
                        }
                    }
                }
                msg
            }).collect();
            Ok(serde_json::json!({"messages": msg_values, "items": [], "hasMore": false, "sessionFiles": [], "todos": []}))
        }
        "session.delete_message" => {
            let p = params.unwrap_or_default();
            let sid = p.get("session_id").and_then(|v| v.as_str()).unwrap_or("default");
            let msg_id = p.get("msg_id").and_then(|v| v.as_i64()).unwrap_or(0);
            if msg_id == 0 {
                return Err(JsonRpcError {
                    code: ErrorCode::InvalidRequest,
                    message: "msg_id is required".into(),
                    data: None,
                });
            }
            let deleted = engine.delete_message(sid, msg_id).map_err(|e| JsonRpcError {
                code: ErrorCode::InternalError,
                message: e.to_string(),
                data: None,
            })?;
            Ok(serde_json::json!({"ok": deleted}))
        }
        "session.permission_mode" => {
            let p = params.unwrap_or_default();
            let session_id = p.get("session_id").and_then(|v| v.as_str()).unwrap_or("");
            let mode = engine.permission_mode(session_id);
            let default_mode = engine.permission_mode("");
            Ok(serde_json::json!({
                "mode": mode,
                "defaultMode": default_mode,
                "locked": false,
            }))
        }
        "session.set_permission_mode" => {
            let p = params.unwrap_or_default();
            let session_id = p.get("session_id").and_then(|v| v.as_str()).unwrap_or("");
            let mode = p.get("mode").and_then(|v| v.as_str()).unwrap_or("ask");
            let pending_new_session = p.get("pending_new_session").and_then(|v| v.as_bool()).unwrap_or(false);
            let normalized = engine.set_permission_mode(session_id, mode, pending_new_session);
            tracing::info!(session_id, %normalized, pending_new_session, "permission mode updated");
            Ok(serde_json::json!({
                "ok": true,
                "mode": normalized,
                "locked": false,
            }))
        }

        // ── Agent ────────────────────────────────────────────────────
        "agent.list" => {
            let state = engine.agent_state().await;
            let model = engine.current_model_id();
            // Read agent name from settings JSON if available
            let config = engine.get_config(Some("settings.agent")).await;
            let agent_name = config
                .get("default")
                .and_then(|v| v.get("name"))
                .and_then(|v| v.as_str())
                .unwrap_or("Loom")
                .to_string();
            Ok(serde_json::json!({
                "agents": [{
                    "id": "default",
                    "name": agent_name,
                    "yuan": "loom",
                    "isPrimary": true,
                    "hasAvatar": false,
                    "memoryMasterEnabled": false,
                    "chatModel": model,
                    "state": state,
                }]
            }))
        }
        "agent.switch" => {
            let _agent_id = params.as_ref().and_then(|p| p.get("agent_id")).and_then(|v| v.as_str()).unwrap_or("default");
            Ok(serde_json::json!({"agent_id": "default", "agentName": "Loom", "agentYuan": "loom"}))
        }

        // ── Commands ─────────────────────────────────────────────────
        "command.list" => {
            let skills = engine.list_skills();
            let commands: Vec<Value> = skills.iter().map(|s| {
                serde_json::json!({
                    "name": format!("/{}", s.name),
                    "description": s.description,
                    "category": "skill",
                })
            }).collect();
            Ok(serde_json::json!({"commands": commands}))
        }

        // ── Models ───────────────────────────────────────────────────
        "model.list" => {
            let config_val = engine.get_config(None).await;
            let models_arr = config_val.get("models").and_then(|m| m.as_array());
            let mut models: Vec<Value> = if let Some(arr) = models_arr {
                arr.iter().map(|m| {
                    let id = m.get("model").and_then(|v| v.as_str()).unwrap_or("");
                    let name = m.get("name").and_then(|v| v.as_str()).unwrap_or(id);
                    let provider = m.get("backend").and_then(|v| v.as_str()).unwrap_or("unknown");
                    let context_size = m.get("context_size").and_then(|v| v.as_u64()).unwrap_or(4096);
                    let image = m.get("image").and_then(|v| v.as_bool()).unwrap_or(false);
                    let video = m.get("video").and_then(|v| v.as_bool()).unwrap_or(false);
                    let reasoning = m.get("reasoning").and_then(|v| v.as_bool()).unwrap_or(false);
                    let mut input = vec!["text"];
                    if image { input.push("image"); }
                    if video { input.push("video"); }
                    serde_json::json!({
                        "id": id,
                        "name": name,
                        "provider": provider,
                        "context_size": context_size,
                        "image": image,
                        "video": video,
                        "reasoning": reasoning,
                        "input": input,
                    })
                }).collect()
            } else {
                // Fallback: single active model
                let info = engine.model_info().await;
                vec![serde_json::json!({
                    "id": info.model_id,
                    "name": info.display_name,
                    "provider": info.backend,
                    "image": false,
                    "video": false,
                    "reasoning": false,
                    "input": ["text"],
                })]
            };

            // Also include models from settings providers (custom providers)
            let settings_providers = config_val
                .get("settings").and_then(|s| s.get("providers"))
                .and_then(|p| p.as_object())
                .or_else(|| {
                    config_val
                        .get("settings").and_then(|s| s.get("general"))
                        .and_then(|g| g.get("providers"))
                        .and_then(|p| p.as_object())
                });
            if let Some(providers) = settings_providers {
                let existing_ids: std::collections::HashSet<String> = models
                    .iter()
                    .filter_map(|m| m.get("id").and_then(|v| v.as_str()).map(String::from))
                    .collect();
                for (prov_name, prov_val) in providers {
                    if prov_val.is_null() { continue; }
                    let prov_models = prov_val.get("models").and_then(|v| v.as_array());
                    if let Some(arr) = prov_models {
                        for m in arr {
                            let model_id = if m.is_string() {
                                m.as_str().unwrap_or("").to_string()
                            } else if m.is_object() {
                                m.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string()
                            } else {
                                continue;
                            };
                            if model_id.is_empty() || existing_ids.contains(&model_id) { continue; }
                            let image = m.get("image").and_then(|v| v.as_bool()).unwrap_or(false);
                            let video = m.get("video").and_then(|v| v.as_bool()).unwrap_or(false);
                            let reasoning = m.get("reasoning").and_then(|v| v.as_bool()).unwrap_or(false);
                            let name = m.get("name").and_then(|v| v.as_str()).unwrap_or(&model_id);
                            let mut input = vec!["text"];
                            if image { input.push("image"); }
                            if video { input.push("video"); }
                            models.push(serde_json::json!({
                                "id": model_id,
                                "name": name,
                                "provider": prov_name,
                                "image": image,
                                "video": video,
                                "reasoning": reasoning,
                                "input": input,
                            }));
                        }
                    }
                }
            }

            // Determine the active model
            let active_model = config_val
                .get("settings").and_then(|s| s.get("active_model"))
                .cloned()
                .or_else(|| {
                    // Fallback: derive from first typed ModelConfig
                    models_arr.and_then(|arr| arr.first()).map(|m| {
                        serde_json::json!({
                            "id": m.get("model").and_then(|v| v.as_str()).unwrap_or(""),
                            "provider": m.get("backend").and_then(|v| v.as_str()).unwrap_or(""),
                        })
                    })
                });

            Ok(serde_json::json!({"models": models, "activeModel": active_model}))
        }
        "model.switch" => {
            let p = params.unwrap_or_default();
            let model_id = p.get("model_id").or_else(|| p.get("id"))
                .and_then(|v| v.as_str()).unwrap_or("");
            let provider = p.get("provider").and_then(|v| v.as_str()).unwrap_or("");
            if model_id.is_empty() {
                return Err(JsonRpcError { code: ErrorCode::InvalidRequest, message: "model_id required".into(), data: None });
            }
            // Persist the selected model as active model in settings
            let active = serde_json::json!({
                "id": model_id,
                "provider": provider,
            });
            if let Err(e) = engine.set_config("settings.active_model", active).await {
                tracing::warn!(error = %e, "Failed to persist active_model setting");
            }
            // Also update config.models[0] with the new model so engine uses it on next request
            let provider_lower = provider.to_lowercase();
            let backend_str = match provider_lower.as_str() {
                "anthropic" => "anthropic",
                "openai" => "openai",
                "deepseek" => "deepseek",
                "lmstudio" | "lm-studio" => "lmstudio",
                "ollama" => "ollama",
                other => other,
            };
            let models_val = serde_json::json!([{
                "model": model_id,
                "name": model_id,
                "backend": backend_str,
            }]);
            if let Err(e) = engine.set_config("models", models_val).await {
                tracing::warn!(error = %e, "Failed to update models config");
            }
            Ok(serde_json::json!({"ok": true, "model_id": model_id, "provider": provider}))
        }

        // ── Providers ────────────────────────────────────────────────
        "providers.summary" => {
            let config_val = engine.get_config(None).await;
            let models_arr = config_val.get("models").and_then(|m| m.as_array());
            let mut providers = serde_json::Map::new();

            // ── 1. Read from typed config.models (Vec<ModelConfig>) ──
            if let Some(arr) = models_arr {
                for m in arr {
                    let backend = m.get("backend").and_then(|v| v.as_str()).unwrap_or("unknown");
                    let is_local = m.get("backend")
                        .and_then(|v| v.as_str())
                        .map(|b| b == "LmStudio" || b == "Ollama")
                        .unwrap_or(false);
                    let model_id = m.get("model").and_then(|v| v.as_str()).unwrap_or("");
                    let model_name = m.get("name").and_then(|v| v.as_str()).unwrap_or(model_id);
                    let context_size = m.get("context_size").and_then(|v| v.as_u64()).unwrap_or(4096);
                    let api_key_env = m.get("api_key_env").and_then(|v| v.as_str()).unwrap_or("");
                    let has_credentials = if api_key_env.is_empty() {
                        is_local
                    } else {
                        std::env::var(api_key_env).is_ok()
                    };

                    let provider_key = backend.to_lowercase();
                    let entry = providers.entry(provider_key).or_insert_with(|| {
                        serde_json::json!({
                            "type": if is_local { "local" } else { "api-key" },
                            "auth_type": if is_local { "none" } else { "api-key" },
                            "display_name": backend,
                            "base_url": m.get("base_url").and_then(|v| v.as_str()).unwrap_or(""),
                            "has_credentials": has_credentials,
                            "supports_oauth": false,
                            "models": [],
                        })
                    });
                    if let Some(obj) = entry.as_object_mut() {
                        if let Some(models_list) = obj.get_mut("models").and_then(|v| v.as_array_mut()) {
                            models_list.push(serde_json::json!({
                                "id": model_id,
                                "name": model_name,
                                "context_size": context_size,
                                "reasoning": false,
                                "image": false,
                                "video": false,
                                "input": ["text"],
                            }));
                        }
                        // Update has_credentials if any model has credentials
                        if has_credentials {
                            obj.insert("has_credentials".into(), serde_json::Value::Bool(true));
                        }
                    }
                }
            }

            // ── 2. Merge from settings providers (front-end writes here) ──
            // Front-end sends config.set { key: 'general', value: { providers: {...} } }
            // set_nested merges value's top-level keys into settings, so providers ends up
            // at settings.providers (not settings.general.providers).
            let settings_providers = config_val
                .get("settings").and_then(|s| s.get("providers"))
                .and_then(|p| p.as_object())
                .or_else(|| {
                    config_val
                        .get("settings").and_then(|s| s.get("general"))
                        .and_then(|g| g.get("providers"))
                        .and_then(|p| p.as_object())
                });
            if let Some(settings_providers) = settings_providers {
                for (prov_name, prov_val) in settings_providers {
                    // null value means "delete this provider"
                    if prov_val.is_null() {
                        providers.remove(prov_name);
                        continue;
                    }
                    let entry = providers.entry(prov_name.clone()).or_insert_with(|| {
                        serde_json::json!({
                            "type": "api-key",
                            "auth_type": "api-key",
                            "display_name": prov_name,
                            "base_url": "",
                            "has_credentials": false,
                            "supports_oauth": false,
                            "models": [],
                            "can_delete": true,
                        })
                    });
                    if let Some(obj) = entry.as_object_mut() {
                        if let Some(base_url) = prov_val.get("base_url").and_then(|v| v.as_str()) {
                            obj.insert("base_url".into(), serde_json::Value::String(base_url.to_string()));
                        }
                        if let Some(api) = prov_val.get("api").and_then(|v| v.as_str()) {
                            obj.insert("api".into(), serde_json::Value::String(api.to_string()));
                        }
                        // Check for api_key in secrets (api_key_env or has_api_key in settings)
                        let has_key = prov_val.get("has_api_key").and_then(|v| v.as_bool()).unwrap_or(false)
                            || prov_val.get("api_key_env").and_then(|v| v.as_str()).is_some();
                        if has_key {
                            if let Some(masked) = openloom_engine::secrets::get_masked(prov_name) {
                                obj.insert("api_key".into(), serde_json::Value::String(masked));
                            }
                            obj.insert("has_credentials".into(), serde_json::Value::Bool(true));
                        }
                        // Merge models from settings provider into existing list
                        #[allow(clippy::collapsible_if)]
                        if let Some(settings_models) = prov_val.get("models").and_then(|v| v.as_array()) {
                            if let Some(existing_models) = obj.get_mut("models").and_then(|v| v.as_array_mut()) {
                                for sm in settings_models {
                                    let (model_id, model_obj) = if sm.is_string() {
                                        (sm.as_str().unwrap_or("").to_string(), serde_json::json!({}))
                                    } else if sm.is_object() {
                                        let id = sm.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                                        (id, sm.clone())
                                    } else {
                                        continue;
                                    };
                                    let img = model_obj.get("image").and_then(|v| v.as_bool()).unwrap_or(false);
                                    let vid = model_obj.get("video").and_then(|v| v.as_bool()).unwrap_or(false);
                                    let mut input = vec!["text"];
                                    if img { input.push("image"); }
                                    if vid { input.push("video"); }
                                    // Check if model already exists in the list
                                    if let Some(existing) = existing_models.iter_mut().find(|m| m.get("id").and_then(|v| v.as_str()) == Some(&model_id)) {
                                        // Merge capabilities from settings into existing entry
                                        if let Some(obj) = existing.as_object_mut() {
                                            if let Some(v) = model_obj.get("image").and_then(|v| v.as_bool()) {
                                                obj.insert("image".into(), serde_json::Value::Bool(v));
                                            }
                                            if let Some(v) = model_obj.get("video").and_then(|v| v.as_bool()) {
                                                obj.insert("video".into(), serde_json::Value::Bool(v));
                                            }
                                            if let Some(v) = model_obj.get("reasoning").and_then(|v| v.as_bool()) {
                                                obj.insert("reasoning".into(), serde_json::Value::Bool(v));
                                            }
                                            if let Some(v) = model_obj.get("name").and_then(|v| v.as_str()) {
                                                obj.insert("name".into(), serde_json::Value::String(v.to_string()));
                                            }
                                            if let Some(v) = model_obj.get("context").and_then(|v| v.as_u64()) {
                                                obj.insert("context".into(), serde_json::json!(v));
                                            }
                                            if let Some(v) = model_obj.get("maxOutput").and_then(|v| v.as_u64()) {
                                                obj.insert("maxOutput".into(), serde_json::json!(v));
                                            }
                                            obj.insert("input".into(), serde_json::json!(input));
                                        }
                                    } else {
                                        // New model not in existing list — add it
                                        existing_models.push(serde_json::json!({
                                            "id": model_id,
                                            "name": model_obj.get("name").and_then(|v| v.as_str()).unwrap_or(&model_id),
                                            "image": img,
                                            "video": vid,
                                            "reasoning": model_obj.get("reasoning").and_then(|v| v.as_bool()).unwrap_or(false),
                                            "context": model_obj.get("context").and_then(|v| v.as_u64()).unwrap_or(0),
                                            "maxOutput": model_obj.get("maxOutput").and_then(|v| v.as_u64()).unwrap_or(0),
                                            "input": input,
                                        }));
                                    }
                                }
                            }
                        }
                        obj.insert("can_delete".into(), serde_json::Value::Bool(true));
                    }
                }
            }

            // ── 3. Check api_key_env for any provider that has it ──
            for (_key, entry) in providers.iter_mut() {
                if let Some(obj) = entry.as_object_mut() {
                    let has_existing = obj.get("has_credentials").and_then(|v| v.as_bool()).unwrap_or(false);
                    if !has_existing {
                        // Check if env var exists for common provider names
                        let env_var = match _key.as_str() {
                            "anthropic" => "ANTHROPIC_API_KEY",
                            "openai" => "OPENAI_API_KEY",
                            "deepseek" => "DEEPSEEK_API_KEY",
                            _ => "",
                        };
                        if !env_var.is_empty() && std::env::var(env_var).is_ok() {
                            obj.insert("has_credentials".into(), serde_json::Value::Bool(true));
                        }
                    }
                }
            }

            Ok(serde_json::json!({"providers": providers}))
        }

        // ── Provider actions ──────────────────────────────────────────
        "providers.fetch_models" => {
            let p = params.unwrap_or_default();
            let name = p.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let base_url = p.get("base_url").and_then(|v| v.as_str()).unwrap_or("");
            let api_key = p.get("api_key").and_then(|v| v.as_str()).unwrap_or("");

            // Read api_key from secrets if not provided inline
            let effective_key = if !api_key.is_empty() {
                api_key.to_string()
            } else {
                openloom_engine::secrets::get(name).unwrap_or_default()
            };

            let effective_url = if base_url.is_empty() {
                let config_val = engine.get_config(None).await;
                config_val
                    .get("settings").and_then(|s| s.get("providers"))
                    .and_then(|p| p.get(name))
                    .and_then(|v| v.get("base_url"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string()
            } else {
                base_url.to_string()
            };

            if effective_url.is_empty() {
                return Ok(serde_json::json!({"models": [], "error": "No base URL configured"}));
            }

            // Build the models endpoint URL — avoid double /v1/ if base_url already ends with /v1
            let base = effective_url.trim_end_matches('/');
            let models_url = if base.ends_with("/v1") || base.ends_with("/v1/") {
                format!("{}/models", base)
            } else {
                format!("{}/v1/models", base)
            };

            let client = reqwest::Client::new();
            let mut req = client.get(&models_url);
            if !effective_key.is_empty() {
                req = req.bearer_auth(&effective_key);
            }

            match req.timeout(std::time::Duration::from_secs(10)).send().await {
                Ok(resp) => {
                    match resp.json::<serde_json::Value>().await {
                        Ok(body) => {
                            let models = body.get("data")
                                .and_then(|d| d.as_array())
                                .map(|arr| {
                                    arr.iter().filter_map(|m| {
                                        let id = m.get("id").and_then(|v| v.as_str()).unwrap_or("");
                                        if id.is_empty() { return None; }
                                        Some(serde_json::json!({
                                            "id": id,
                                            "name": id,
                                        }))
                                    }).collect::<Vec<_>>()
                                })
                                .unwrap_or_default();
                            Ok(serde_json::json!({"models": models}))
                        }
                        Err(e) => Ok(serde_json::json!({"models": [], "error": format!("Parse error: {}", e)})),
                    }
                }
                Err(e) => Ok(serde_json::json!({"models": [], "error": format!("Request failed: {}", e)})),
            }
        }
        "providers.test" => {
            let p = params.unwrap_or_default();
            let name = p.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let base_url = p.get("base_url").and_then(|v| v.as_str()).unwrap_or("");
            let api_key = p.get("api_key").and_then(|v| v.as_str()).unwrap_or("");

            // Read api_key from secrets if not provided inline
            let effective_key = if !api_key.is_empty() {
                api_key.to_string()
            } else {
                openloom_engine::secrets::get(name).unwrap_or_default()
            };

            let effective_url = if base_url.is_empty() {
                let config_val = engine.get_config(None).await;
                config_val
                    .get("settings").and_then(|s| s.get("providers"))
                    .and_then(|p| p.get(name))
                    .and_then(|v| v.get("base_url"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string()
            } else {
                base_url.to_string()
            };

            if effective_url.is_empty() {
                return Ok(serde_json::json!({"ok": false, "error": "No base URL configured"}));
            }

            let models_url = format!("{}/v1/models", effective_url.trim_end_matches('/'));
            let client = reqwest::Client::new();
            let mut req = client.get(&models_url);
            if !effective_key.is_empty() {
                req = req.bearer_auth(&effective_key);
            }

            match req.timeout(std::time::Duration::from_secs(10)).send().await {
                Ok(resp) if resp.status().is_success() => Ok(serde_json::json!({"ok": true})),
                Ok(resp) => Ok(serde_json::json!({"ok": false, "error": format!("HTTP {}", resp.status())})),
                Err(e) => Ok(serde_json::json!({"ok": false, "error": format!("Connection failed: {}", e)})),
            }
        }

        // ── Provider API key (masked) ──────────────────────────────────
        "providers.get_api_key" => {
            let name = params.as_ref().and_then(|p| p.get("name")).and_then(|v| v.as_str()).unwrap_or("");
            let masked = openloom_engine::secrets::get_masked(name);
            Ok(serde_json::json!({
                "api_key": masked,
                "has_key": masked.is_some(),
            }))
        }

        // ── Context usage ────────────────────────────────────────────
        "context.usage" | "context_usage" => {
            let session_id = params
                .as_ref()
                .and_then(|p| p.get("sessionPath").or_else(|| p.get("session_id")))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let (used, total, percent) = engine.context_usage(session_id).await;
            Ok(serde_json::json!({"used": used, "total": total, "percent": percent}))
        }

        // ── Chat replay ──────────────────────────────────────────────
        "chat.replay" => {
            let _sid = params.as_ref().and_then(|p| p.get("session_id")).and_then(|v| v.as_str()).unwrap_or("");
            Ok(serde_json::json!({"ok": true}))
        }

        // ── Desk (Phase E) ────────────────────────────────────────────
        "desk.list" => Ok(serde_json::json!({"files": [], "notes": []})),
        "desk.create_note" => Ok(serde_json::json!({"ok": true, "id": "note-1"})),
        "desk.update_note" => Ok(serde_json::json!({"ok": true})),
        "desk.delete_item" => Ok(serde_json::json!({"ok": true})),
        "desk.watch" => Ok(serde_json::json!({"ok": true, "watcher_id": "w1"})),
        "desk.unwatch" => Ok(serde_json::json!({"ok": true})),

        // ── Memory stats (Phase B) ────────────────────────────────────
        "memory.stats" => Ok(serde_json::json!({
            "total_events": 0, "total_cognitions": 0, "db_size_bytes": 0
        })),
        "memory.recent_events" => {
            let _limit = params.as_ref().and_then(|p| p.get("limit")).and_then(|v| v.as_u64()).unwrap_or(20);
            Ok(serde_json::json!({"events": []}))
        },
        "memory.graph_snapshot" => Ok(serde_json::json!({"nodes": [], "edges": []})),

        // ── Agent CRUD (Phase C) ──────────────────────────────────────
        "agent.create" => Ok(serde_json::json!({"id": "agent-1", "name": "New Agent"})),
        "agent.delete" => Ok(serde_json::json!({"ok": true})),
        "agent.configure" => Ok(serde_json::json!({"ok": true})),
        "agent.activity_log" => Ok(serde_json::json!({"entries": []})),
        "agent.tool_policy.get" => Ok(serde_json::json!({"policies": {}})),
        "agent.tool_policy.set" => Ok(serde_json::json!({"ok": true})),

        // ── Skills management (Phase D) ────────────────────────────────
        "skill.enable" => Ok(serde_json::json!({"ok": true})),
        "skill.disable" => Ok(serde_json::json!({"ok": true})),
        "skill.info" => {
            let _name = params.as_ref().and_then(|p| p.get("name")).and_then(|v| v.as_str()).unwrap_or("");
            Ok(serde_json::json!({"name": _name, "description": "", "triggers": [], "enabled": true}))
        },

        // ── Cron / Automation (Phase C) ────────────────────────────────
        "cron.list" => Ok(serde_json::json!({"jobs": []})),
        "cron.create" => Ok(serde_json::json!({"ok": true, "id": "cron-1"})),
        "cron.delete" => Ok(serde_json::json!({"ok": true})),

        // ── Config schema (Phase F) ────────────────────────────────────
        "config.schema" => Ok(serde_json::json!({"schema": {}})),
        "session.thinking_level.set" => Ok(serde_json::json!({"ok": true})),

        // ── Session compact ─────────────────────────────────────────
        "session.compact" => {
            let session_id = params
                .as_ref()
                .and_then(|p| p.get("session_id"))
                .and_then(|v| v.as_str())
                .unwrap_or("default");
            let count = engine.compact_session(session_id).await.map_err(|e| JsonRpcError {
                code: ErrorCode::InternalError,
                message: e.to_string(),
                data: None,
            })?;
            Ok(serde_json::json!({"ok": true, "messages_replaced": count}))
        }

        // ── Chat abort ───────────────────────────────────────────────
        "chat.abort" => {
            let p = params.unwrap_or_default();
            let session_id = p.get("session_id").and_then(|v| v.as_str()).unwrap_or("default");
            let aborted = engine.abort_session(session_id);
            // Fire agent state changed so frontend clears isStreaming
            let _ = engine.event_bus().send(EngineEvent::AgentStateChanged {
                old_state: openloom_models::AgentState::Acting,
                new_state: openloom_models::AgentState::Idle,
            });
            tracing::info!(session_id, aborted, "chat.abort requested");
            Ok(serde_json::json!({"ok": true, "aborted": aborted}))
        }

        // ── Avatar ──────────────────────────────────────────────────
        "avatar.upload" => {
            let p = params.unwrap_or_default();
            let role = p.get("role").and_then(|v| v.as_str()).unwrap_or("user");
            let data_url = p.get("data").and_then(|v| v.as_str()).unwrap_or("");

            let b64 = if let Some(comma) = data_url.find(',') {
                &data_url[comma + 1..]
            } else {
                data_url
            };

            use base64::Engine as _;
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(b64)
                .map_err(|e| JsonRpcError {
                    code: ErrorCode::InvalidRequest,
                    message: format!("invalid base64: {}", e),
                    data: None,
                })?;

            let avatars_dir = engine.data_dir().join("avatars");
            std::fs::create_dir_all(&avatars_dir).map_err(|e| JsonRpcError {
                code: ErrorCode::InternalError,
                message: format!("mkdir: {}", e),
                data: None,
            })?;

            let path = avatars_dir.join(format!("{}.png", role));
            std::fs::write(&path, &bytes).map_err(|e| JsonRpcError {
                code: ErrorCode::InternalError,
                message: format!("write: {}", e),
                data: None,
            })?;

            tracing::info!(role = %role, "avatar uploaded");
            Ok(serde_json::json!({"ok": true, "role": role}))
        }
        "avatar.get" => {
            let role = params.as_ref().and_then(|p| p.get("role")).and_then(|v| v.as_str()).unwrap_or("user");
            if !role.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
                return Err(JsonRpcError { code: ErrorCode::InvalidRequest, message: "invalid role".into(), data: None });
            }
            let path = engine.data_dir().join("avatars").join(format!("{}.png", role));
            match std::fs::read(&path) {
                Ok(bytes) => {
                    use base64::Engine as _;
                    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
                    Ok(serde_json::json!({"data": format!("data:image/png;base64,{}", b64)}))
                }
                Err(_) => Ok(serde_json::json!({"data": null})),
            }
        }

        // ── File upload ──────────────────────────────────────────────
        "file.upload_blob" => {
            let p = params.unwrap_or_default();
            let name = p.get("name").and_then(|v| v.as_str()).unwrap_or("upload");
            let base64_data = p.get("base64Data").and_then(|v| v.as_str()).unwrap_or("");
            let mime_type = p.get("mimeType").and_then(|v| v.as_str()).unwrap_or("application/octet-stream");
            let _session_path = p.get("sessionPath").and_then(|v| v.as_str()).unwrap_or("");

            if base64_data.is_empty() {
                return Err(JsonRpcError {
                    code: ErrorCode::InternalError,
                    message: "base64Data is required".into(),
                    data: None,
                });
            }

            // Decode base64 — strip data URL prefix if present (data:image/png;base64,...)
            let raw_b64 = if let Some(comma_pos) = base64_data.find(',') {
                &base64_data[comma_pos + 1..]
            } else {
                base64_data
            };

            use base64::Engine as B64Engine;
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(raw_b64)
                .map_err(|e| JsonRpcError {
                    code: ErrorCode::InternalError,
                    message: format!("invalid base64: {}", e),
                    data: None,
                })?;

            // Write to a temp dir under the app data directory
            let data_dir = engine.data_dir();
            let uploads_dir = data_dir.join("uploads");
            std::fs::create_dir_all(&uploads_dir).map_err(|e| JsonRpcError {
                code: ErrorCode::InternalError,
                message: format!("failed to create uploads dir: {}", e),
                data: None,
            })?;

            let file_id = uuid::Uuid::new_v4().to_string();
            // Sanitize file name: keep extension, replace unsafe chars
            let safe_name = name
                .chars()
                .map(|c| if c.is_alphanumeric() || c == '.' || c == '-' || c == '_' { c } else { '_' })
                .collect::<String>();
            let dest_name = format!("{}_{}", &file_id[..8], safe_name);
            let dest_path = uploads_dir.join(&dest_name);

            std::fs::write(&dest_path, &bytes).map_err(|e| JsonRpcError {
                code: ErrorCode::InternalError,
                message: format!("failed to write upload: {}", e),
                data: None,
            })?;

            let dest_str = dest_path.to_string_lossy().to_string();
            tracing::info!(file_id = %file_id, dest = %dest_str, mime = %mime_type, "file.upload_blob");

            Ok(serde_json::json!({
                "uploads": [{
                    "fileId": file_id,
                    "dest": dest_str,
                    "name": safe_name,
                    "mimeType": mime_type,
                    "size": bytes.len(),
                }]
            }))
        }

        _ => Err(JsonRpcError {
            code: ErrorCode::MethodNotFound,
            message: format!("method '{}' not found", method),
            data: None,
        }),
    }
}

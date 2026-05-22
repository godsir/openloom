use super::Engine;
use crate::token_store::TokenUsageRecord;
use anyhow::Result;
use chrono::Utc;
use openloom_inference::{CompletionRequest, CompletionResponse};
use openloom_models::*;
use std::sync::atomic::Ordering;
use tokio::sync::mpsc;

impl Engine {
    pub(crate) async fn agent_loop(
        &self,
        msg: &ChatMessage,
        session_id: &str,
        mode: openloom_models::Mode,
    ) -> Result<ChatResponse> {
        self.agent_loop_inner(msg, session_id, None, mode).await
    }

    pub(crate) async fn agent_loop_streaming(
        &self,
        msg: &ChatMessage,
        session_id: &str,
        tx: mpsc::Sender<String>,
        mode: openloom_models::Mode,
    ) -> Result<ChatResponse> {
        self.agent_loop_inner(msg, session_id, Some(tx), mode).await
    }

    async fn agent_loop_inner(
        &self,
        msg: &ChatMessage,
        session_id: &str,
        tx: Option<mpsc::Sender<String>>,
        mode: openloom_models::Mode,
    ) -> Result<ChatResponse> {
        self.in_flight.fetch_add(1, Ordering::SeqCst);
        let loop_start = std::time::Instant::now();
        *self.agent_state.write().await = AgentState::Thinking;
        let _ = self.event_bus.send(EngineEvent::AgentStateChanged {
            old_state: AgentState::Idle,
            new_state: AgentState::Thinking,
        });
        self.interruptible.store(true, Ordering::SeqCst);

        let mut history: Vec<ChatMessage> = self.get_working_memory(session_id).unwrap_or_default();
        history.push(msg.clone());

        let skill_infos = self.skills.list_all();
        let tool_definitions = crate::build_tool_definitions(&skill_infos);

        let mut all_tool_messages: Vec<ChatMessage> = Vec::new();
        let mut last_response = String::new();
        let mut total_prompt_tokens = 0usize;
        let mut total_completion_tokens = 0usize;

        let (max_iterations, timeout_secs) = {
            let cfg = self.config.read().await;
            (
                cfg.agent.max_iterations.max(3),
                cfg.agent.timeout_secs.max(60),
            )
        };

        let mode_cfg = mode.config();
        let outcome = tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), async {
            for _iteration in 0..max_iterations {
                let persona_summary = self.persona.summarize().await.unwrap_or_default();
                let system_with_mode = crate::system_instruction();
                let system_with_mode = if mode_cfg.system_suffix.is_empty() {
                    system_with_mode
                } else {
                    format!("{}\n\n{}", system_with_mode, mode_cfg.system_suffix)
                };
                let messages = self.weaver.assemble_messages(
                    &system_with_mode,
                    "",
                    &persona_summary,
                    None,
                    &history,
                    self.context_max_chars,
                );

                let completion_req = CompletionRequest {
                    messages,
                    tools: tool_definitions.clone(),
                    tool_choice: None,
                    prompt: String::new(),
                    max_tokens: self.max_output_tokens,
                    temperature: 0.0,
                    ..Default::default()
                };

                let response = self.invoke_model_native(&completion_req).await?;
                total_prompt_tokens += response.prompt_tokens;
                total_completion_tokens += response.completion_tokens;

                if !response.tool_calls.is_empty() {
                    // Stream tool call markers to UI
                    if let Some(ref tx) = tx {
                        for tc in &response.tool_calls {
                            let call_json = serde_json::to_string(tc).unwrap_or_default();
                            let _ = tx.send(format!("\x01CALL\x02{}", call_json)).await;
                        }
                    }

                    *self.agent_state.write().await = AgentState::Acting;
                    let _ = self.event_bus.send(EngineEvent::AgentStateChanged {
                        old_state: AgentState::Thinking,
                        new_state: AgentState::Acting,
                    });

                    // Execute each tool call
                    for tc in &response.tool_calls {
                        let result = match self.execute_tool(tc, mode).await {
                            Ok(output) => truncate_tool_result(&output),
                            Err(e) => format!("Tool error: {}", e),
                        };

                        if let Some(ref tx) = tx {
                            let _ = tx.send(format!("\x01RESULT\x02{}", result)).await;
                        }

                        let ts = Utc::now();
                        history.push(ChatMessage {
                            role: "assistant".into(),
                            content: format!("ToolCall|{}|{}", tc.id, tc.name),
                            timestamp: ts,
                        });
                        history.push(ChatMessage {
                            role: "tool".into(),
                            content: format!("{}|{}", tc.id, result.clone()),
                            timestamp: ts,
                        });
                        all_tool_messages.push(ChatMessage {
                            role: "assistant".into(),
                            content: format!("ToolCall|{}|{}", tc.id, tc.name),
                            timestamp: ts,
                        });
                        all_tool_messages.push(ChatMessage {
                            role: "tool".into(),
                            content: format!("{}|{}", tc.id, result),
                            timestamp: ts,
                        });
                    }
                    *self.agent_state.write().await = AgentState::Thinking;
                } else {
                    last_response = response.text;
                    break;
                }
            }

            if last_response.is_empty() && !all_tool_messages.is_empty() {
                let persona_summary = self.persona.summarize().await.unwrap_or_default();
                let system_with_mode = crate::system_instruction();
                let system_with_mode = if mode_cfg.system_suffix.is_empty() {
                    system_with_mode
                } else {
                    format!("{}\n\n{}", system_with_mode, mode_cfg.system_suffix)
                };
                let messages = self.weaver.assemble_messages(
                    &system_with_mode,
                    "",
                    &persona_summary,
                    None,
                    &history,
                    self.context_max_chars,
                );
                let completion_req = CompletionRequest {
                    messages,
                    tools: tool_definitions.clone(),
                    tool_choice: None,
                    prompt: String::new(),
                    max_tokens: self.max_output_tokens,
                    temperature: 0.0,
                    ..Default::default()
                };
                let response = self.invoke_model_native(&completion_req).await?;
                total_prompt_tokens += response.prompt_tokens;
                total_completion_tokens += response.completion_tokens;
                last_response = response.text;
            }

            Ok::<_, anyhow::Error>(last_response)
        })
        .await;

        *self.agent_state.write().await = AgentState::Idle;
        let _ = self.event_bus.send(EngineEvent::AgentStateChanged {
            old_state: AgentState::Thinking,
            new_state: AgentState::Idle,
        });
        self.interruptible.store(false, Ordering::SeqCst);
        self.in_flight.fetch_sub(1, Ordering::SeqCst);

        match outcome {
            Ok(Ok(ref response)) if response.is_empty() => Err(anyhow::anyhow!(
                "Agent loop produced no response after {} iterations",
                max_iterations
            )),
            Ok(Ok(response)) => {
                let _ = self.save_all_messages(session_id, msg, &all_tool_messages, &response);

                let prompt_tokens = total_prompt_tokens;
                let completion_tokens = total_completion_tokens;
                let latency_ms = loop_start.elapsed().as_millis() as u64;
                let _ = self.event_bus.send(EngineEvent::TokenUsage {
                    session_id: session_id.to_string(),
                    model: "agent-loop".into(),
                    prompt_tokens,
                    completion_tokens,
                    cached_tokens: 0,
                    latency_ms,
                });
                let _ = self.token_store_tx.send(TokenUsageRecord {
                    session_id: session_id.to_string(),
                    model: "agent-loop".into(),
                    prompt_tokens,
                    completion_tokens,
                    cached_tokens: 0,
                    latency_ms,
                });
                Ok(ChatResponse {
                    response,
                    session_id: session_id.to_string(),
                    token_usage: TokenUsage {
                        prompt_tokens,
                        completion_tokens,
                        cached_tokens: 0,
                        latency_ms,
                    },
                })
            }
            Ok(Err(e)) => Err(e),
            Err(_elapsed) => Err(anyhow::anyhow!(
                "Agent loop timed out after {}s",
                timeout_secs
            )),
        }
    }

    pub(crate) async fn invoke_model_native(
        &self,
        req: &CompletionRequest,
    ) -> Result<CompletionResponse> {
        if let Some(ref cloud) = self.cloud {
            // Pre-flight: ensure LM Studio has a model loaded
            if cloud.provider() == ModelBackend::LmStudio {
                let _ = openloom_inference::ensure_lm_studio_model(
                    "http://localhost:1234/v1",
                    cloud.model_name(),
                    32000,
                )
                .await;
            }
            match cloud.complete(req.clone()).await {
                Ok(r) => return Ok(r),
                Err(e) => tracing::warn!("Cloud failed, trying local: {}", e),
            }
        }
        // Fallback: construct text-only request from messages
        let prompt = req
            .effective_messages()
            .iter()
            .map(|m| format!("{}: {}", m.role.as_str(), m.text_content()))
            .collect::<Vec<_>>()
            .join("\n");
        let fallback_req = CompletionRequest {
            prompt,
            max_tokens: req.max_tokens,
            temperature: req.temperature,
            ..Default::default()
        };
        if let Some(ref local) = self.local_client {
            match local.complete(fallback_req.clone()).await {
                Ok(r) => return Ok(r),
                Err(e) => tracing::warn!("Local client failed, trying inference engine: {}", e),
            }
        }
        self.inference.complete(fallback_req).await
    }

    #[allow(dead_code)]
    pub(crate) async fn invoke_model_raw(&self, prompt: &str) -> Result<String> {
        let max_tokens = self.max_output_tokens;
        if let Some(ref cloud) = self.cloud {
            match cloud
                .complete(CompletionRequest {
                    prompt: prompt.to_string(),
                    max_tokens,
                    ..Default::default()
                })
                .await
            {
                Ok(r) => return Ok(r.text),
                Err(e) => tracing::warn!("Cloud failed, trying local: {}", e),
            }
        }
        if let Some(ref local) = self.local_client {
            match local
                .complete(CompletionRequest {
                    prompt: prompt.to_string(),
                    max_tokens,
                    ..Default::default()
                })
                .await
            {
                Ok(r) => return Ok(r.text),
                Err(e) => tracing::warn!("Local client failed, trying inference engine: {}", e),
            }
        }
        self.inference
            .complete(CompletionRequest {
                prompt: prompt.to_string(),
                max_tokens,
                ..Default::default()
            })
            .await
            .map(|r| r.text)
    }

    pub(crate) async fn execute_tool(
        &self,
        call: &ToolCall,
        mode: openloom_models::Mode,
    ) -> Result<String> {
        // Reverse sanitize: model returns safe name, find actual skill by matching sanitized forms
        let tool_name = if self.skills.find_by_name(&call.name).is_some() {
            call.name.clone()
        } else {
            // Try matching by sanitizing each registered skill name
            let all = self.skills.list_all();
            all.iter()
                .find(|s| crate::sanitize_tool_name(&s.name) == call.name)
                .map(|s| s.name.clone())
                .unwrap_or_else(|| call.name.clone())
        };

        let mode_cfg = mode.config();
        if !mode_cfg.tool_scope.allows(&tool_name) {
            return Ok(format!(
                "Tool '{}' is not available in {} mode.",
                tool_name, mode_cfg.status_label
            ));
        }
        let risk = openloom_sandbox::classify_risk(&tool_name, &call.arguments);

        // Permission confirmation for risky tools (skip if --dangerously-skip-permissions)
        if !self.skip_permissions
            && matches!(
                risk,
                openloom_models::RiskLevel::Medium | openloom_models::RiskLevel::High
            )
        {
            let risk_str = format!("{:?}", risk);
            let desc = format!(
                "{}({})",
                tool_name,
                call.arguments
                    .as_object()
                    .map(|p| p
                        .iter()
                        .take(2)
                        .map(|(k, v)| format!("{}={}", k, v))
                        .collect::<Vec<_>>()
                        .join(", "))
                    .unwrap_or_default()
            );
            let req = openloom_models::PermissionRequest {
                tool_name: tool_name.clone(),
                description: desc,
                risk_level: risk_str,
            };
            let (resp_tx, resp_rx) = tokio::sync::oneshot::channel();
            if self.perm_request_tx.send((req, resp_tx)).await.is_ok() {
                match resp_rx.await {
                    Ok(true) => {} // approved, continue
                    _ => return Ok(format!("Tool '{}' denied by user.", tool_name)),
                }
            }
        }

        // Always block Forbidden-level risks regardless of permissions
        if matches!(risk, openloom_models::RiskLevel::Forbidden) {
            let msg = openloom_sandbox::risk_message(&tool_name, &call.arguments, &risk);
            return Ok(msg);
        }

        self.skills
            .invoke(&tool_name, call.arguments.clone())
            .await
            .map(|v| v.to_string())
    }
}

fn truncate_tool_result(s: &str) -> String {
    const MAX: usize = 64000;
    if s.len() <= MAX {
        return s.to_string();
    }
    format!(
        "{}\n\n[... {} chars truncated ...]",
        &s[..MAX],
        s.len() - MAX
    )
}

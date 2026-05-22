use super::Engine;
use crate::token_store::TokenUsageRecord;
use anyhow::Result;
use chrono::Utc;
use openloom_inference::CompletionRequest;
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

        let skill_list = self.build_skill_list_string();

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
                let system_with_tools = crate::system_instruction().replace("[tools]", &skill_list);
                let system_with_tools = if mode_cfg.system_suffix.is_empty() {
                    system_with_tools
                } else {
                    format!("{}\n\n{}", system_with_tools, mode_cfg.system_suffix)
                };
                let assembled =
                    self.weaver
                        .assemble_with_limit(&system_with_tools, "", &persona_summary, None, &history, self.context_max_chars);

                let response = self.invoke_model_raw(&assembled.prompt).await?;
                total_prompt_tokens += self.inference.token_count(&assembled.prompt);
                total_completion_tokens += self.inference.token_count(&response);

                if let Some(tool_call) = self.parse_tool_call(&response) {
                    // Send thinking marker — extract text before the JSON/fence block
                    if let Some(ref tx) = tx {
                        let thinking_text = if let Some(fence) = response.find("```") {
                            response[..fence].trim()
                        } else if let Some(brace) = response.find('{') {
                            response[..brace].trim()
                        } else {
                            ""
                        };
                        if !thinking_text.is_empty() {
                            let _ = tx.send(format!("\x01THINK\x02{}", thinking_text)).await;
                        }
                        let call_json = serde_json::to_string(&tool_call).unwrap_or_default();
                        let _ = tx.send(format!("\x01CALL\x02{}", call_json)).await;
                    }

                    *self.agent_state.write().await = AgentState::Acting;
                    let _ = self.event_bus.send(EngineEvent::AgentStateChanged {
                        old_state: AgentState::Thinking,
                        new_state: AgentState::Acting,
                    });

                    let result = match self.execute_tool(&tool_call, mode).await {
                        Ok(output) => truncate_tool_result(&output),
                        Err(e) => format!("Tool error: {}", e),
                    };

                    // Send result marker
                    if let Some(ref tx) = tx {
                        let _ = tx.send(format!("\x01RESULT\x02{}", result)).await;
                    }

                    let ts = Utc::now();
                    history.push(ChatMessage {
                        role: "assistant".into(),
                        content: response.clone(),
                        timestamp: ts,
                    });
                    history.push(ChatMessage {
                        role: "tool".into(),
                        content: result.clone(),
                        timestamp: ts,
                    });
                    all_tool_messages.push(ChatMessage {
                        role: "assistant".into(),
                        content: response,
                        timestamp: ts,
                    });
                    all_tool_messages.push(ChatMessage {
                        role: "tool".into(),
                        content: result,
                        timestamp: ts,
                    });
                    *self.agent_state.write().await = AgentState::Thinking;
                } else {
                    last_response = response;
                    break;
                }
            }

            if last_response.is_empty() && !all_tool_messages.is_empty() {
                let persona_summary = self.persona.summarize().await.unwrap_or_default();
                let system_with_tools = crate::system_instruction().replace("[tools]", &skill_list);
                let system_with_tools = if mode_cfg.system_suffix.is_empty() {
                    system_with_tools
                } else {
                    format!("{}\n\n{}", system_with_tools, mode_cfg.system_suffix)
                };
                let assembled =
                    self.weaver
                        .assemble_with_limit(&system_with_tools, "", &persona_summary, None, &history, self.context_max_chars);
                last_response = self.invoke_model_raw(&assembled.prompt).await?;
                total_prompt_tokens += self.inference.token_count(&assembled.prompt);
                total_completion_tokens += self.inference.token_count(&last_response);
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

    pub(crate) fn build_skill_list_string(&self) -> String {
        let skills = self.skills.all_skills();
        if skills.is_empty() {
            return "None".into();
        }
        skills
            .iter()
            .map(|s| format!("### {}\n{}", s.name(), s.context_md()))
            .collect::<Vec<_>>()
            .join("\n\n")
    }

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

    pub(crate) fn parse_tool_call(&self, response: &str) -> Option<ToolCall> {
        let trimmed = response.trim();

        // Strip markdown code fences if present
        let content = if let Some(fence_start) = trimmed.find("```") {
            let after_fence = &trimmed[fence_start + 3..];
            let body_start = after_fence.find('\n').map(|i| i + 1).unwrap_or(0);
            let body = &after_fence[body_start..];
            if let Some(fence_end) = body.find("```") {
                body[..fence_end].trim()
            } else {
                body.trim()
            }
        } else {
            trimmed
        };

        // Try each '{' as potential start of a tool call JSON object
        let mut search_from = 0;
        while let Some(brace_pos) = content[search_from..].find('{') {
            let abs_pos = search_from + brace_pos;
            let slice = &content[abs_pos..];

            let mut depth = 0;
            let mut end = 0;
            for (i, ch) in slice.char_indices() {
                match ch {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            end = i;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            if end > 0 {
                let json_str = &slice[..=end];
                if let Ok(call) = serde_json::from_str::<ToolCall>(json_str) {
                    return Some(call);
                }
            }
            search_from = abs_pos + 1;
        }

        None
    }

    pub(crate) async fn execute_tool(&self, call: &ToolCall, mode: openloom_models::Mode) -> Result<String> {
        let mode_cfg = mode.config();
        if !mode_cfg.tool_scope.allows(&call.tool) {
            return Ok(format!(
                "Tool '{}' is not available in {} mode.",
                call.tool, mode_cfg.status_label
            ));
        }
        let risk = openloom_sandbox::classify_risk(&call.tool, &call.params);

        // Permission confirmation for risky tools (skip if --dangerously-skip-permissions)
        if !self.skip_permissions
            && matches!(risk, openloom_models::RiskLevel::Medium | openloom_models::RiskLevel::High)
        {
            let risk_str = format!("{:?}", risk);
            let desc = format!("{}({})", call.tool,
                call.params.as_object()
                    .map(|p| p.iter().take(2).map(|(k,v)| format!("{}={}", k, v)).collect::<Vec<_>>().join(", "))
                    .unwrap_or_default());
            let req = openloom_models::PermissionRequest {
                tool_name: call.tool.clone(),
                description: desc,
                risk_level: risk_str,
            };
            let (resp_tx, resp_rx) = tokio::sync::oneshot::channel();
            if self.perm_request_tx.send((req, resp_tx)).await.is_ok() {
                match resp_rx.await {
                    Ok(true) => {} // approved, continue
                    _ => return Ok(format!("Tool '{}' denied by user.", call.tool)),
                }
            }
        }

        // Always block Forbidden-level risks regardless of permissions
        if matches!(risk, openloom_models::RiskLevel::Forbidden) {
            let msg = openloom_sandbox::risk_message(&call.tool, &call.params, &risk);
            return Ok(msg);
        }

        self.skills
            .invoke(&call.tool, call.params.clone())
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

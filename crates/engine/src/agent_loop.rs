use super::{Engine, SYSTEM_INSTRUCTION};
use crate::token_store::TokenUsageRecord;
use anyhow::Result;
use chrono::Utc;
use openloom_inference::CompletionRequest;
use openloom_models::*;
use std::sync::atomic::Ordering;

impl Engine {
    pub(crate) async fn agent_loop(
        &self,
        msg: &ChatMessage,
        session_id: &str,
    ) -> Result<ChatResponse> {
        self.in_flight.fetch_add(1, Ordering::SeqCst);
        let loop_start = std::time::Instant::now();
        *self.agent_state.write().await = AgentState::Thinking;
        let _ = self.event_bus.send(EngineEvent::AgentStateChanged {
            old_state: AgentState::Idle,
            new_state: AgentState::Thinking,
        });
        self.interruptible.store(true, Ordering::SeqCst);

        let mut history: Vec<ChatMessage> =
            self.get_working_memory(session_id).unwrap_or_default();
        history.push(msg.clone());

        // Build skill list string for system prompt injection
        let skill_list = self.build_skill_list_string();

        let mut all_tool_messages: Vec<ChatMessage> = Vec::new();
        let mut last_response = String::new();

        let outcome = tokio::time::timeout(std::time::Duration::from_secs(120), async {
            for _iteration in 0..3 {
                let persona_summary = self.persona.summarize().await.unwrap_or_default();
                let system_with_tools = SYSTEM_INSTRUCTION.replace("[tools]", &skill_list);
                let assembled =
                    self.weaver
                        .assemble(&system_with_tools, "", &persona_summary, None, &history);

                let response = self.invoke_model_raw(&assembled.prompt).await?;

                if let Some(tool_call) = self.parse_tool_call(&response) {
                    *self.agent_state.write().await = AgentState::Acting;
                    let _ = self.event_bus.send(EngineEvent::AgentStateChanged {
                        old_state: AgentState::Thinking,
                        new_state: AgentState::Acting,
                    });
                    let result = match self.execute_tool(&tool_call).await {
                        Ok(output) => output,
                        Err(e) => format!("Tool error: {}", e),
                    };
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
                        content: result.clone(),
                        timestamp: ts,
                    });
                    // On the last iteration, use the tool result as the response
                    if _iteration == 2 {
                        last_response = result;
                    }
                } else {
                    last_response = response;
                    break;
                }
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
                "Agent loop produced no response after 3 iterations"
            )),
            Ok(Ok(response)) => {
                let _ = self.save_all_messages(session_id, msg, &all_tool_messages, &response);

                let prompt_tokens = self.inference.token_count(&msg.content);
                let completion_tokens = self.inference.token_count(&response);
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
            Err(_elapsed) => Err(anyhow::anyhow!("Agent loop timed out after 120s")),
        }
    }

    pub(crate) fn build_skill_list_string(&self) -> String {
        let skills = self.skills.list_all();
        if skills.is_empty() {
            return "None".into();
        }
        skills
            .iter()
            .map(|s| format!("{}: {}", s.name, s.description))
            .collect::<Vec<_>>()
            .join(", ")
    }

    pub(crate) async fn invoke_model_raw(&self, prompt: &str) -> Result<String> {
        if let Some(ref cloud) = self.cloud {
            match cloud
                .complete(CompletionRequest {
                    prompt: prompt.to_string(),
                    ..Default::default()
                })
                .await
            {
                Ok(r) => return Ok(r.text),
                Err(e) => tracing::warn!("Cloud failed, falling back to local: {}", e),
            }
        }
        self.inference
            .complete(CompletionRequest {
                prompt: prompt.to_string(),
                ..Default::default()
            })
            .await
            .map(|r| r.text)
    }

    pub(crate) fn parse_tool_call(&self, response: &str) -> Option<ToolCall> {
        let trimmed = response.trim();
        if let Some(start) = trimmed.find("{\"tool\"") {
            let slice = &trimmed[start..];
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
                match serde_json::from_str::<ToolCall>(json_str) {
                    Ok(call) => return Some(call),
                    Err(e) => {
                        tracing::warn!(
                            "Failed to parse tool call JSON: {} -- raw: {}",
                            e,
                            json_str
                        );
                        return None;
                    }
                }
            }
        }
        None
    }

    pub(crate) async fn execute_tool(&self, call: &ToolCall) -> Result<String> {
        self.skills
            .invoke(&call.tool, call.params.clone())
            .await
            .map(|v| v.to_string())
    }
}

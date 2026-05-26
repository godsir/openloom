//! Agent loop — the core execution cycle: LLM call → tool dispatch → repeat.
//!
//! This is the heart of an agent turn. Given a user message, it assembles
//! the context window, calls the LLM, dispatches tool calls, and iterates
//! until the LLM produces a final text response or max iterations is hit.

use anyhow::Result;
use loom_inference::engine::CloudClient;
use loom_types::{CompletionRequest, ContentPart, Message, Role, StreamDelta};
use loom_security::check_permission;
use loom_types::SkillPermissions;
use tokio::sync::mpsc;
use tracing::info;

use crate::tool_registry::ToolRegistry;

/// The result of one agent turn.
#[derive(Debug, Clone)]
pub struct TurnResult {
    pub response: String,
    pub tool_calls_made: usize,
    pub iterations: usize,
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
}

/// Configuration for the agent loop.
pub struct AgentLoopConfig {
    /// System prompt injected at the start of every turn.
    pub system_prompt: String,
    /// Maximum LLM → tool → LLM iterations per turn.
    pub max_iterations: usize,
    /// Maximum tokens for LLM output.
    pub max_tokens: usize,
    /// Model temperature.
    pub temperature: f32,
}

impl Default for AgentLoopConfig {
    fn default() -> Self {
        Self {
            system_prompt: "You are an AI assistant with REAL access to the user's Windows machine via tools (shell runs cmd /c, file_read/write use the real filesystem). You are NOT in a sandbox — when you call a tool, it actually executes on the computer. If a user asks to create files or run commands, use the tools directly. Never apologize or suggest manual alternatives — just do it. After creating files, use file_list to verify they exist.".into(),
            max_iterations: 10,
            max_tokens: 4096,
            temperature: 0.0,
        }
    }
}

/// Execute one agent turn: user message → LLM → tools → response.
pub async fn run_agent_turn(
    client: &dyn CloudClient,
    registry: &ToolRegistry,
    history: &[Message],
    user_message: &str,
    config: &AgentLoopConfig,
) -> Result<TurnResult> {
    let mut messages = Vec::with_capacity(history.len() + 2);

    // System prompt
    messages.push(Message {
        role: Role::System,
        content: vec![ContentPart::Text { text: config.system_prompt.clone() }],
        timestamp: chrono::Utc::now(),
    });

    // Conversation history
    messages.extend_from_slice(history);

    // Current user message
    messages.push(Message::user(user_message));

    let mut tools = registry.all_definitions();
    let mut tool_calls_made = 0;
    let mut total_prompt = 0usize;
    let mut total_completion = 0usize;

    for iteration in 0..config.max_iterations {
        let request = CompletionRequest {
            messages: messages.clone(),
            tools: tools.clone(),
            tool_choice: None,
            prompt: String::new(),
            max_tokens: config.max_tokens,
            temperature: config.temperature,
            top_p: 1.0,
            stop: Vec::new(),
            stream: false,
            thinking_budget: None,
        };

        info!(iteration, tool_count = tools.len(), msg_count = messages.len(), "agent turn iteration");

        let response = client.complete(request).await?;
        total_prompt += response.prompt_tokens;
        total_completion += response.completion_tokens;

        // If the LLM returned tool calls, dispatch them
        if !response.tool_calls.is_empty() {
            info!(count = response.tool_calls.len(), names = ?response.tool_calls.iter().map(|t| &t.name).collect::<Vec<_>>(), "tool calls received");

            // Stall breaker: strip tools after 7 iterations to force final response
            if iteration >= 7 {
                tools.clear();
                messages.push(Message {
                    role: Role::System,
                    content: vec![ContentPart::Text {
                        text: "You have used many tools. Stop and give your final answer now based on what you've found. Do NOT call more tools.".into(),
                    }],
                    timestamp: chrono::Utc::now(),
                });
            } else if iteration >= 3 {
                messages.push(Message {
                    role: Role::System,
                    content: vec![ContentPart::Text {
                        text: "Consider whether you have enough information to answer. If so, respond directly without more tools.".into(),
                    }],
                    timestamp: chrono::Utc::now(),
                });
            }

            // Add assistant message with tool calls
            let assistant_content: Vec<ContentPart> = response.tool_calls.iter().map(|tc| {
                ContentPart::ToolCall {
                    id: tc.id.clone(),
                    name: tc.name.clone(),
                    arguments: tc.arguments.clone(),
                }
            }).collect();
            messages.push(Message {
                role: Role::Assistant,
                content: assistant_content,
                timestamp: chrono::Utc::now(),
            });

            // Execute each tool call
            for tc in &response.tool_calls {
                let tool_name = tc.name.clone();
                info!(name = %tool_name, "executing tool");
                // Permission check
                let perms = SkillPermissions::default();
                let (allowed, risk) = check_permission(&tool_name, &perms);
                if !allowed {
                    messages.push(Message::tool(&tc.id, &tool_name, &format!("Permission denied (risk: {:?})", risk)));
                    continue;
                }

                let (progress_tx, _progress_rx) = mpsc::unbounded_channel();
                match registry.execute(&tc.name, tc.arguments.clone(), progress_tx).await {
                    Ok(result) => {
                        tool_calls_made += 1;
                        let content = if result.is_error {
                            format!("Error: {}", result.content)
                        } else {
                            result.content
                        };
                        messages.push(Message::tool(&tc.id, &tool_name, &content));
                    }
                    Err(e) => {
                        let err_msg = format!("Tool execution failed: {}", e);
                        messages.push(Message::tool(&tc.id, &tool_name, &err_msg));
                    }
                }
            }

            // Continue loop — LLM sees tool results and may respond or call more tools
            continue;
        }

        // No tool calls — this is the final text response
        let response_text = if response.text.is_empty() {
            "[no response]".to_string()
        } else {
            response.text.clone()
        };

        return Ok(TurnResult {
            response: response_text,
            tool_calls_made,
            iterations: iteration + 1,
            prompt_tokens: total_prompt,
            completion_tokens: total_completion,
        });
    }

    Ok(TurnResult {
        response: "Agent reached maximum iterations without resolving.".into(),
        tool_calls_made,
        iterations: config.max_iterations,
        prompt_tokens: total_prompt,
        completion_tokens: total_completion,
    })
}

/// Streaming variant — yields StreamDelta events as they arrive.
pub async fn run_agent_turn_streaming(
    client: &dyn CloudClient,
    registry: &ToolRegistry,
    history: &[Message],
    user_message: &str,
    config: &AgentLoopConfig,
    delta_tx: mpsc::Sender<StreamDelta>,
) -> Result<TurnResult> {
    let mut messages = Vec::with_capacity(history.len() + 2);
    messages.push(Message {
        role: Role::System,
        content: vec![ContentPart::Text { text: config.system_prompt.clone() }],
        timestamp: chrono::Utc::now(),
    });
    messages.extend_from_slice(history);
    messages.push(Message::user(user_message));

    let mut tools = registry.all_definitions();
    let mut tool_calls_made = 0;
    let mut total_prompt = 0usize;
    let mut total_completion = 0usize;
    let mut final_text = String::new();

    for _iteration in 0..config.max_iterations {
        let request = CompletionRequest {
            messages: messages.clone(),
            tools: tools.clone(),
            tool_choice: None,
            prompt: String::new(),
            max_tokens: config.max_tokens,
            temperature: config.temperature,
            top_p: 1.0,
            stop: Vec::new(),
            stream: true,
            thinking_budget: None,
        };

        // Real streaming: use complete_stream_structured to get token-by-token deltas
        let (stream_tx, mut stream_rx) = mpsc::channel::<StreamDelta>(256);
        client.complete_stream_structured(request, stream_tx).await?;

        let mut pending_tool_calls: Vec<(usize, String, String, String)> = Vec::new();
        let mut this_text = String::new();

        while let Some(delta) = stream_rx.recv().await {
            match delta {
                StreamDelta::Text(t) => {
                    this_text.push_str(&t);
                    let _ = delta_tx.send(StreamDelta::Text(t)).await;
                }
                StreamDelta::Reasoning(t) => {
                    let _ = delta_tx.send(StreamDelta::Reasoning(t)).await;
                }
                StreamDelta::ToolCallBegin { index, id, name } => {
                    pending_tool_calls.push((index, id, name, String::new()));
                }
                StreamDelta::ToolCallArgsChunk { index, chunk } => {
                    if let Some(tc) = pending_tool_calls.iter_mut().find(|(i, _, _, _)| *i == index) {
                        tc.3.push_str(&chunk);
                    }
                }
                StreamDelta::Usage { prompt_tokens, completion_tokens, .. } => {
                    total_prompt += prompt_tokens as usize;
                    total_completion += completion_tokens as usize;
                }
            }
        }

        if !pending_tool_calls.is_empty() {
            let assistant_content: Vec<ContentPart> = pending_tool_calls.iter().map(|(_, id, name, args)| {
                ContentPart::ToolCall {
                    id: id.clone(), name: name.clone(),
                    arguments: serde_json::from_str(args).unwrap_or(serde_json::Value::String(args.clone())),
                }
            }).collect();
            messages.push(Message { role: Role::Assistant, content: assistant_content, timestamp: chrono::Utc::now() });

            for (_, tc_id, tc_name, tc_args) in &pending_tool_calls {
                let arguments = serde_json::from_str(tc_args).unwrap_or(serde_json::json!({}));
                let (progress_tx, _) = mpsc::unbounded_channel();
                match registry.execute(tc_name, arguments, progress_tx).await {
                    Ok(result) => {
                        tool_calls_made += 1;
                        let content = if result.is_error { format!("Error: {}", result.content) } else { result.content };
                        messages.push(Message::tool(tc_id, tc_name, &content));
                    }
                    Err(e) => {
                        messages.push(Message::tool(tc_id, tc_name, &format!("Tool execution failed: {}", e)));
                    }
                }
            }
            if _iteration >= 7 { tools.clear(); }
            continue;
        }

        final_text = this_text;
        break;
    }

    Ok(TurnResult { response: final_text, tool_calls_made, iterations: 1, prompt_tokens: total_prompt, completion_tokens: total_completion })
}

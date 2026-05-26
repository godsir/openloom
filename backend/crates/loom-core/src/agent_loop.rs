//! Agent loop — the core execution cycle: LLM call → tool dispatch → repeat.
//!
//! This is the heart of an agent turn. Given a user message, it assembles
//! the context window, calls the LLM, dispatches tool calls, and iterates
//! until the LLM produces a final text response or max iterations is hit.

use anyhow::Result;
use loom_context::ContextAssembler;
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
    /// Number of KV cache hits during this turn (local models only).
    pub cache_hits: u64,
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
    allowed_tools: &Option<Vec<String>>,
    disallowed_tools: &Option<Vec<String>>,
) -> Result<TurnResult> {
    client.prefix_cache_reset();
    let mut tools = registry.filtered_definitions(allowed_tools, disallowed_tools);
    let assembler = ContextAssembler::new(&config.system_prompt, 8192);
    let mut messages = assembler.build(None, history, &tools)?;
    messages.push(Message::user(user_message));
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

            // Add assistant message with tool calls + thinking (if any)
            let mut assistant_content: Vec<ContentPart> = Vec::new();
            if let Some(ref thinking) = response.thinking {
                assistant_content.push(ContentPart::Thinking { text: thinking.clone() });
            }
            for tc in &response.tool_calls {
                assistant_content.push(ContentPart::ToolCall {
                    id: tc.id.clone(),
                    name: tc.name.clone(),
                    arguments: tc.arguments.clone(),
                });
            }
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

        let cache_stats = client.prefix_cache_stats();
        return Ok(TurnResult {
            response: response_text,
            tool_calls_made,
            iterations: iteration + 1,
            prompt_tokens: total_prompt,
            completion_tokens: total_completion,
            cache_hits: cache_stats.hits,
        });
    }

    let cache_stats = client.prefix_cache_stats();
    Ok(TurnResult {
        response: "Agent reached maximum iterations without resolving.".into(),
        tool_calls_made,
        iterations: config.max_iterations,
        prompt_tokens: total_prompt,
        completion_tokens: total_completion,
        cache_hits: cache_stats.hits,
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
    allowed_tools: &Option<Vec<String>>,
    disallowed_tools: &Option<Vec<String>>,
) -> Result<TurnResult> {
    client.prefix_cache_reset();
    let mut tools = registry.filtered_definitions(allowed_tools, disallowed_tools);
    let assembler = ContextAssembler::new(&config.system_prompt, 8192);
    let mut messages = assembler.build(None, history, &tools)?;
    messages.push(Message::user(user_message));
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

        // Real streaming: use complete_stream_structured to get token-by-token deltas.
        // Use unbounded channel + select! to avoid deadlock — stream sender and
        // receiver must run concurrently.
        let (stream_tx, mut stream_rx) = mpsc::channel::<StreamDelta>(4096);
        let stream_fut = client.complete_stream_structured(request, stream_tx);
        tokio::pin!(stream_fut);
        let mut stream_done = false;

        let mut pending_tool_calls: Vec<(usize, String, String, String)> = Vec::new();
        let mut this_text = String::new();
        let mut this_thinking = String::new();

        while !stream_done {
            tokio::select! {
                r = &mut stream_fut => { r?; stream_done = true; }
                delta = stream_rx.recv() => {
                    let Some(delta) = delta else { break };
                    match delta {
                StreamDelta::Text(t) => {
                    this_text.push_str(&t);
                    let _ = delta_tx.send(StreamDelta::Text(t)).await;
                }
                StreamDelta::Reasoning(t) => {
                    this_thinking.push_str(&t);
                    let _ = delta_tx.send(StreamDelta::Reasoning(t)).await;
                }
                StreamDelta::ToolCallBegin { index, id, name } => {
                    pending_tool_calls.push((index, id.clone(), name.clone(), String::new()));
                    let _ = delta_tx.send(StreamDelta::ToolCallBegin { index, id, name }).await;
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
            }
        }


        // Drain any remaining deltas after stream completes
        while let Ok(delta) = stream_rx.try_recv() {
            match delta {
                StreamDelta::Text(t) => { this_text.push_str(&t); let _ = delta_tx.send(StreamDelta::Text(t)).await; }
                StreamDelta::Reasoning(t) => { let _ = delta_tx.send(StreamDelta::Reasoning(t)).await; }
                StreamDelta::Usage { prompt_tokens, completion_tokens, .. } => {
                    total_prompt += prompt_tokens as usize;
                    total_completion += completion_tokens as usize;
                }
                _ => {}
            }
        }

        if !pending_tool_calls.is_empty() {
            let mut assistant_content: Vec<ContentPart> = Vec::new();
            if !this_thinking.is_empty() {
                assistant_content.push(ContentPart::Thinking { text: std::mem::take(&mut this_thinking) });
            }
            for (_, id, name, args) in &pending_tool_calls {
                assistant_content.push(ContentPart::ToolCall {
                    id: id.clone(), name: name.clone(),
                    arguments: serde_json::from_str(args).unwrap_or(serde_json::Value::String(args.clone())),
                });
            }
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

    let cache_stats = client.prefix_cache_stats();
    Ok(TurnResult { response: final_text, tool_calls_made, iterations: 1, prompt_tokens: total_prompt, completion_tokens: total_completion, cache_hits: cache_stats.hits })
}

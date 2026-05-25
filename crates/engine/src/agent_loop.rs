use super::Engine;
use crate::token_store::TokenUsageRecord;
use anyhow::Result;
use chrono::Utc;
use openloom_inference::{CompletionRequest, CompletionResponse, StreamDelta};
use openloom_models::*;
use std::sync::atomic::Ordering;
use tokio::sync::mpsc;

/// Abstracts over TUI (mpsc channel) and Electron (event_bus) output mechanisms.
pub(crate) enum OutputSink {
    /// No streaming output (used by handle_message non-streaming path)
    None,
    /// TUI path: sends text tokens and tool markers via mpsc channel
    Tui(mpsc::Sender<String>),
    /// Electron path: fires StreamDelta events via event_bus
    Electron {
        event_bus: tokio::sync::broadcast::Sender<EngineEvent>,
        session_id: String,
    },
}

impl Engine {
    pub(crate) async fn agent_loop(
        &self,
        msg: &ChatMessage,
        session_id: &str,
        mode: openloom_models::Mode,
        model_pref: openloom_models::ModelPreference,
    ) -> Result<ChatResponse> {
        self.agent_loop_inner(
            msg,
            session_id,
            OutputSink::None,
            None,
            None,
            mode,
            model_pref,
        )
        .await
    }

    pub(crate) async fn agent_loop_streaming(
        &self,
        msg: &ChatMessage,
        session_id: &str,
        tx: mpsc::Sender<String>,
        mode: openloom_models::Mode,
        model_pref: openloom_models::ModelPreference,
    ) -> Result<ChatResponse> {
        self.agent_loop_inner(
            msg,
            session_id,
            OutputSink::Tui(tx),
            None,
            None,
            mode,
            model_pref,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn agent_loop_inner(
        &self,
        msg: &ChatMessage,
        session_id: &str,
        sink: OutputSink,
        system_prompt_override: Option<String>,
        model_override: Option<std::sync::Arc<dyn openloom_inference::CloudClient>>,
        mode: openloom_models::Mode,
        model_pref: openloom_models::ModelPreference,
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
        let mut tool_definitions = crate::build_tool_definitions(&skill_infos);

        // Add computer use tool when available
        if crate::computer_use::check_available() {
            let cu_tool = self.computer_use_tool_definition();
            tool_definitions.push(ToolDefinition {
                name: cu_tool["name"].as_str().unwrap_or("computer").to_string(),
                description: cu_tool["description"].as_str().unwrap_or("").to_string(),
                input_schema: cu_tool["input_schema"].clone(),
            });
        }

        let mut all_tool_messages: Vec<ChatMessage> = Vec::new();
        let mut last_response = String::new();
        let mut total_prompt_tokens = 0usize;
        let mut total_completion_tokens = 0usize;

        let (max_iterations, timeout_secs) = {
            let cfg = self.config.read().await;
            (
                cfg.agent.max_iterations.max(10),   // at least 10 tool-call rounds
                cfg.agent.timeout_secs.max(120),    // at least 2 minutes
            )
        };

        let mode_cfg = mode.config();

        // Auto-compact: if history exceeds 80% of context window, summarize the older half.
        // The summary is injected as a system message — stored history is NOT modified.
        let history_chars: usize = history.iter().map(|m| m.content.chars().count()).sum();
        let compact_threshold = (self.context_max_chars as f64 * 0.8) as usize;
        if history_chars > compact_threshold && self.context_max_chars > 0 {
            let split = history.len() / 2;
            let older: Vec<String> = history[..split]
                .iter()
                .filter(|m| m.role == "user" || m.role == "assistant")
                .map(|m| format!("[{}]: {}", m.role, m.content))
                .collect();
            let older_text = older.join("\n\n");
            if !older_text.is_empty() {
                let compact_prompt = format!(
                    "Summarize this conversation history concisely. Include key decisions, code changes, and important context. Keep under 500 characters.\n\n{}",
                    older_text
                );
                if let Ok(summary) = self.invoke_model_raw(&compact_prompt).await {
                    let summary = summary.trim().to_string();
                    // Replace older messages with a single system summary
                    let compact_msg = ChatMessage {
                        role: "system".into(),
                        content: format!("[Earlier conversation summary]\n{}", summary),
                        timestamp: chrono::Utc::now(),
                        metadata: None,
                        id: None,
                        seq: None,
                    };
                    history = vec![compact_msg]
                        .into_iter()
                        .chain(history[split..].to_vec())
                        .collect();
                    tracing::info!(
                        session_id,
                        original_msgs = history.len() + split,
                        compacted_to = history.len(),
                        "auto-compacted context for LLM call"
                    );
                }
            }
        }

        let outcome = tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), async {
            for _iteration in 0..max_iterations {
                // Check per-session abort flag (set by chat.abort)
                {
                    let flags = self.abort_flags.lock().unwrap();
                    if let Some(flag) = flags.get(session_id)
                        && flag.load(Ordering::SeqCst)
                    {
                        tracing::info!(session_id, "agent loop aborted by user");
                        break;
                    }
                }

                let persona_summary = self.persona.summarize().await.unwrap_or_default();
                let system_with_mode = if let Some(ref override_prompt) = system_prompt_override {
                    override_prompt.clone()
                } else {
                    let base = self.system_instruction();
                    if mode_cfg.system_suffix.is_empty() {
                        base
                    } else {
                        format!("{}\n\n{}", base, mode_cfg.system_suffix)
                    }
                };
                // When system_prompt_override is set, persona is already embedded in it
                let effective_persona = if system_prompt_override.is_some() {
                    ""
                } else {
                    &persona_summary
                };
                // Build a temp history for assembly: the compacted history is only for this call
                let assembly_history = if _iteration == 0 {
                    &history
                } else {
                    // On tool-use follow-ups, history was already compacted in first iteration
                    &history
                };
                let messages = self.weaver.assemble_messages(
                    &system_with_mode,
                    "",
                    effective_persona,
                    None,
                    assembly_history,
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

                // Use streaming on every iteration — structured streaming handles
                // tool calls properly, so we no longer need non-streaming fallback.
                let response = match &sink {
                    OutputSink::Tui(stream_tx) => {
                        self.invoke_model_streaming(completion_req, stream_tx.clone(), model_override.clone(), model_pref)
                            .await?
                    }
                    OutputSink::Electron { event_bus, session_id: sid } => {
                        let (stream_tx, mut stream_rx) = mpsc::channel::<String>(256);
                        let event_bus_clone = event_bus.clone();
                        let sid_clone = sid.clone();

                        let forwarder = tokio::spawn(async move {
                            while let Some(token) = stream_rx.recv().await {
                                let _ = event_bus_clone.send(EngineEvent::StreamDelta {
                                    session_id: sid_clone.clone(),
                                    delta: token,
                                });
                            }
                        });

                        let result = self.invoke_model_streaming(
                            completion_req,
                            stream_tx,
                            model_override.clone(),
                            model_pref,
                        ).await;

                        let _ = forwarder.await;

                        result?
                    }
                    OutputSink::None => {
                        self.invoke_model_native(&completion_req, model_override.clone(), model_pref).await?
                    }
                };
                total_prompt_tokens += response.prompt_tokens;
                total_completion_tokens += response.completion_tokens;

                // Emit thinking/reasoning content before any tool calls or text
                if let Some(ref thinking) = response.thinking
                    && !thinking.is_empty()
                    && let OutputSink::Electron { event_bus, session_id: sid } = &sink
                {
                    // Chunk thinking into ~100 char pieces for smooth UI animation
                    let chunks: Vec<&str> = thinking
                        .as_str()
                        .char_indices()
                        .collect::<Vec<_>>()
                        .chunks(100)
                        .map(|chunk| {
                            let start = chunk[0].0;
                            let end = chunk.last().map(|(i, c)| i + c.len_utf8()).unwrap_or(start);
                            &thinking[start..end]
                        })
                        .collect();
                    for chunk in chunks {
                        let _ = event_bus.send(EngineEvent::ThinkingDelta {
                            session_id: sid.clone(),
                            delta: chunk.to_string(),
                        });
                    }
                    let _ = event_bus.send(EngineEvent::ThinkingEnd {
                        session_id: sid.clone(),
                    });
                }

                if !response.tool_calls.is_empty() {
                    // Stream tool call markers to UI
                    if let OutputSink::Tui(tx) = &sink {
                        for tc in &response.tool_calls {
                            let call_json = serde_json::to_string(tc).unwrap_or_default();
                            let _ = tx.send(format!("\x01CALL\x02{}", call_json)).await;
                        }
                    }
                    // Electron path: ToolCallStarted events are fired below via event_bus.

                    *self.agent_state.write().await = AgentState::Acting;
                    let _ = self.event_bus.send(EngineEvent::AgentStateChanged {
                        old_state: AgentState::Thinking,
                        new_state: AgentState::Acting,
                    });

                    // Execute each tool call
                    for tc in &response.tool_calls {
                        // Notify UI: tool call started
                        let _ = self.event_bus.send(EngineEvent::ToolCallStarted {
                            session_id: session_id.to_string(),
                            call_id: tc.id.clone(),
                            name: tc.name.clone(),
                            arguments: tc.arguments.clone(),
                        });

                        let result = match self.execute_tool(tc, mode).await {
                            Ok(output) => truncate_tool_result(&output),
                            Err(e) => format!("Tool error: {}", e),
                        };

                        // Notify UI: tool call ended
                        let result_summary = if result.len() > 200 {
                            format!("{}...", &result[..200])
                        } else {
                            result.clone()
                        };
                        let _ = self.event_bus.send(EngineEvent::ToolCallEnded {
                            session_id: session_id.to_string(),
                            call_id: tc.id.clone(),
                            name: tc.name.clone(),
                            success: !result.starts_with("Tool error:"),
                            result_summary,
                        });

                        if let OutputSink::Tui(tx) = &sink {
                            let _ = tx.send(format!("\x01RESULT\x02{}", result)).await;
                        }

                        let ts = Utc::now();
                        history.push(ChatMessage {
                            role: "assistant".into(),
                            content: format!("ToolCall|{}|{}", tc.id, tc.name),
                            timestamp: ts,
                            id: None,
                            seq: None,
                            metadata: None,
                        });
                        history.push(ChatMessage {
                            role: "tool".into(),
                            content: format!("{}|{}", tc.id, result.clone()),
                            timestamp: ts,
                            id: None,
                            seq: None,
                            metadata: None,
                        });
                        all_tool_messages.push(ChatMessage {
                            role: "assistant".into(),
                            content: format!("ToolCall|{}|{}", tc.id, tc.name),
                            timestamp: ts,
                            id: None,
                            seq: None,
                            metadata: None,
                        });
                        all_tool_messages.push(ChatMessage {
                            role: "tool".into(),
                            content: format!("{}|{}", tc.id, result),
                            timestamp: ts,
                            id: None,
                            seq: None,
                            metadata: None,
                        });
                    }
                    *self.agent_state.write().await = AgentState::Thinking;
                } else {
                    last_response = response.text.clone();
                    // Text was already streamed in real-time by invoke_model_streaming
                    break;
                }
            }

            // ── Post-loop: ensure a text response reaches the user ──────────────────────
            //
            // If the loop exhausted its iterations without the model producing a text
            // response (it kept calling tools), make one final call with tool_choice=none
            // so the model is forced to summarise the gathered tool results.
            // If that also fails, synthesise a best-effort summary from raw tool output.
            if last_response.is_empty() {
                let has_tool_results = !all_tool_messages.is_empty();
                let persona_summary = self.persona.summarize().await.unwrap_or_default();
                let system_with_mode = self.system_instruction();
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
                // Force text-only response: omit tools, set tool_choice=none
                let completion_req = CompletionRequest {
                    messages,
                    tools: vec![],                              // ← no tools
                    tool_choice: Some(ToolChoice::None),        // ← force text
                    prompt: String::new(),
                    max_tokens: self.max_output_tokens,
                    temperature: 0.0,
                    ..Default::default()
                };
                // Use streaming for the forced text response
                let post_loop_result = match &sink {
                    OutputSink::Tui(stream_tx) => {
                        self.invoke_model_streaming(completion_req, stream_tx.clone(), model_override.clone(), model_pref).await
                    }
                    OutputSink::Electron { event_bus, session_id: sid } => {
                        let (stream_tx, mut stream_rx) = mpsc::channel::<String>(256);
                        let event_bus_clone = event_bus.clone();
                        let sid_clone = sid.clone();
                        let forwarder = tokio::spawn(async move {
                            while let Some(token) = stream_rx.recv().await {
                                let _ = event_bus_clone.send(EngineEvent::StreamDelta {
                                    session_id: sid_clone.clone(),
                                    delta: token,
                                });
                            }
                        });
                        let result = self.invoke_model_streaming(
                            completion_req,
                            stream_tx,
                            model_override.clone(),
                            model_pref,
                        ).await;
                        let _ = forwarder.await;
                        result
                    }
                    OutputSink::None => {
                        self.invoke_model_native(&completion_req, model_override.clone(), model_pref).await
                    }
                };
                match post_loop_result {
                    Ok(response) if !response.text.is_empty() => {
                        last_response = response.text.clone();
                        total_prompt_tokens += response.prompt_tokens;
                        total_completion_tokens += response.completion_tokens;
                        // Text was already streamed in real-time
                    }
                    _ => {
                        // Last resort: synthesise from tool results if any exist
                        if has_tool_results {
                            let summary = all_tool_messages
                                .iter()
                                .filter(|m| m.role == "tool")
                                .map(|m| {
                                    // strip the "id|" prefix from tool result format
                                    m.content
                                        .split_once('|')
                                        .map(|x| x.1)
                                        .unwrap_or(&m.content)
                                        .to_string()
                                })
                                .collect::<Vec<_>>()
                                .join("\n\n---\n\n");
                            last_response = summary.clone();
                            match &sink {
                                OutputSink::Tui(stream_tx) => {
                                    let _ = stream_tx.send(summary).await;
                                }
                                OutputSink::Electron { event_bus, session_id } => {
                                    let _ = event_bus.send(EngineEvent::StreamDelta {
                                        session_id: session_id.clone(),
                                        delta: summary,
                                    });
                                }
                                OutputSink::None => {}
                            }
                        }
                        // if still empty, the error check below surfaces it
                    }
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
                "Agent loop produced no response after {} iterations",
                max_iterations
            )),
            Ok(Ok(response)) => {
                let _ = self.save_all_messages(session_id, msg, &all_tool_messages, &response);

                let prompt_tokens = total_prompt_tokens;
                let completion_tokens = total_completion_tokens;
                let latency_ms = loop_start.elapsed().as_millis() as u64;
                let context_window = self.model_context_size().await;
                let _ = self.event_bus.send(EngineEvent::TokenUsage {
                    session_id: session_id.to_string(),
                    model: "agent-loop".into(),
                    prompt_tokens,
                    completion_tokens,
                    cached_tokens: 0,
                    latency_ms,
                    context_window,
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
        model_override: Option<std::sync::Arc<dyn openloom_inference::CloudClient>>,
        model_pref: openloom_models::ModelPreference,
    ) -> Result<CompletionResponse> {
        // If a per-request model override is provided, use it directly
        if let Some(ref client) = model_override {
            if client.provider() == ModelBackend::LmStudio {
                let _ = openloom_inference::ensure_lm_studio_model(
                    "http://localhost:1234/v1",
                    client.model_name(),
                    32000,
                )
                .await;
            }
            return client.complete(req.clone()).await;
        }

        // Respect user's model preference for ordering
        let (first, second) = match model_pref {
            openloom_models::ModelPreference::Local => (&self.local_client, &self.cloud),
            openloom_models::ModelPreference::Cloud | openloom_models::ModelPreference::Auto => {
                (&self.cloud, &self.local_client)
            }
        };

        // Try the preferred backend first
        if let Some(preferred) = first {
            if preferred.provider() == ModelBackend::LmStudio {
                let _ = openloom_inference::ensure_lm_studio_model(
                    "http://localhost:1234/v1",
                    preferred.model_name(),
                    32000,
                )
                .await;
            }
            match preferred.complete(req.clone()).await {
                Ok(r) => return Ok(r),
                Err(e) => tracing::warn!(
                    "Preferred model failed (pref={:?}), trying fallback: {}",
                    model_pref,
                    e
                ),
            }
        }

        // Try the fallback backend
        if let Some(fallback) = second {
            match fallback.complete(req.clone()).await {
                Ok(r) => return Ok(r),
                Err(e) => tracing::warn!("Fallback model failed, trying native inference: {}", e),
            }
        }

        // Final fallback: native GGUF inference (stub)
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
        self.inference.complete(fallback_req).await
    }

    /// Like `invoke_model_native` but streams tokens (including reasoning) into `tx`
    /// AND returns a `CompletionResponse` with tool_calls parsed from streaming deltas.
    ///
    /// Streaming format for tool_calls:
    ///   delta.tool_calls[i].id / .function.name / .function.arguments (accumulated)
    pub(crate) async fn invoke_model_streaming(
        &self,
        req: CompletionRequest,
        tx: mpsc::Sender<String>,
        model_override: Option<std::sync::Arc<dyn openloom_inference::CloudClient>>,
        model_pref: openloom_models::ModelPreference,
    ) -> Result<CompletionResponse> {
        // If a per-request model override is provided, use it directly (non-streaming)
        if let Some(ref client) = model_override {
            if client.provider() == ModelBackend::LmStudio {
                let _ = openloom_inference::ensure_lm_studio_model(
                    "http://localhost:1234/v1",
                    client.model_name(),
                    32000,
                )
                .await;
            }
            return client.complete(req).await;
        }

        // Tool-use path: use structured streaming to get real-time text tokens
        // while accumulating tool call chunks into complete ToolCall objects.
        let has_tools = !req.tools.is_empty();

        if has_tools {
            let (first, second) = match model_pref {
                openloom_models::ModelPreference::Local => (&self.local_client, &self.cloud),
                openloom_models::ModelPreference::Cloud | openloom_models::ModelPreference::Auto => {
                    (&self.cloud, &self.local_client)
                }
            };

            let (delta_tx, mut delta_rx) = mpsc::channel::<StreamDelta>(256);
            let user_tx = tx.clone();
            let collector = tokio::spawn(async move {
                let mut full_text = String::new();
                let mut thinking_text = String::new();
                let mut usage: Option<(usize, usize, usize)> = None;
                // Accumulate tool calls: Vec of (id, name, accumulated_args)
                let mut tool_calls_acc: Vec<(String, String, String)> = Vec::new();

                while let Some(delta) = delta_rx.recv().await {
                    match delta {
                        StreamDelta::Text(t) => {
                            full_text.push_str(&t);
                            let _ = user_tx.send(t).await;
                        }
                        StreamDelta::Reasoning(r) => {
                            thinking_text.push_str(&r);
                            let marker = format!("\x02REASONING\x02{}", r);
                            let _ = user_tx.send(marker).await;
                        }
                        StreamDelta::ToolCallBegin { index, id, name } => {
                            // Ensure vec is large enough
                            while tool_calls_acc.len() <= index {
                                tool_calls_acc.push((String::new(), String::new(), String::new()));
                            }
                            tool_calls_acc[index] = (id, name, String::new());
                        }
                        StreamDelta::ToolCallArgsChunk { index, chunk } => {
                            if let Some(entry) = tool_calls_acc.get_mut(index) {
                                entry.2.push_str(&chunk);
                            }
                        }
                        StreamDelta::Usage { prompt_tokens, completion_tokens, cached_tokens } => {
                            usage = Some((
                                prompt_tokens as usize,
                                completion_tokens as usize,
                                cached_tokens as usize,
                            ));
                        }
                    }
                }

                let tool_calls: Vec<openloom_models::ToolCall> = tool_calls_acc
                    .into_iter()
                    .filter(|(_, name, _)| !name.is_empty())
                    .map(|(id, name, args)| {
                        let arguments = serde_json::from_str(&args)
                            .unwrap_or(serde_json::Value::String(args));
                        openloom_models::ToolCall { id, name, arguments }
                    })
                    .collect();

                let thinking = if thinking_text.is_empty() { None } else { Some(thinking_text) };
                (full_text, tool_calls, usage, thinking)
            });

            let mut stream_ok = false;
            if let Some(preferred) = first {
                if preferred.provider() == ModelBackend::LmStudio {
                    let _ = openloom_inference::ensure_lm_studio_model(
                        "http://localhost:1234/v1",
                        preferred.model_name(),
                        32000,
                    )
                    .await;
                }
                if preferred.complete_stream_structured(req.clone(), delta_tx.clone()).await.is_ok() {
                    stream_ok = true;
                } else if let Some(fallback) = second {
                    stream_ok = fallback.complete_stream_structured(req.clone(), delta_tx.clone()).await.is_ok();
                }
            } else if let Some(fallback) = second {
                stream_ok = fallback.complete_stream_structured(req.clone(), delta_tx.clone()).await.is_ok();
            }

            drop(delta_tx);
            let (full_text, tool_calls, stream_usage, thinking) =
                collector.await.unwrap_or_default();

            if !stream_ok && full_text.is_empty() && tool_calls.is_empty() {
                anyhow::bail!("all model backends failed for structured streaming request");
            }

            let (prompt_tokens, completion_tokens, cached_tokens) =
                stream_usage.unwrap_or((0, 0, 0));

            return Ok(CompletionResponse {
                text: full_text,
                tool_calls,
                prompt_tokens,
                completion_tokens,
                cached_tokens,
                latency_ms: 0,
                thinking,
            });
        }

        // Text-only path: real streaming with reasoning support
        let (first, second) = match model_pref {
            openloom_models::ModelPreference::Local => (&self.local_client, &self.cloud),
            openloom_models::ModelPreference::Cloud | openloom_models::ModelPreference::Auto => {
                (&self.cloud, &self.local_client)
            }
        };

        // Collect full text + usage while forwarding to tx
        let (collect_tx, mut collect_rx) = mpsc::channel::<String>(256);
        let user_tx = tx.clone();
        let collector = tokio::spawn(async move {
            let mut full_text = String::new();
            let mut usage: Option<(usize, usize, usize)> = None;
            while let Some(token) = collect_rx.recv().await {
                if let Some(u) = token.strip_prefix("\x00USAGE:") {
                    // usage marker — parse, don't forward
                    let parts: Vec<&str> = u.split(':').collect();
                    if parts.len() == 3 {
                        usage = Some((
                            parts[0].parse().unwrap_or(0),
                            parts[1].parse().unwrap_or(0),
                            parts[2].parse().unwrap_or(0),
                        ));
                    }
                    continue;
                }
                if token.starts_with('\x02') {
                    // Reasoning marker — forward only, don't add to text
                    let _ = user_tx.send(token).await;
                    continue;
                }
                full_text.push_str(&token);
                let _ = user_tx.send(token).await;
            }
            (full_text, usage)
        });

        let mut stream_ok = false;
        if let Some(preferred) = first {
            if preferred.provider() == ModelBackend::LmStudio {
                let _ = openloom_inference::ensure_lm_studio_model(
                    "http://localhost:1234/v1",
                    preferred.model_name(),
                    32000,
                )
                .await;
            }
            if preferred.complete_stream(req.clone(), collect_tx.clone()).await.is_ok() {
                stream_ok = true;
            } else if let Some(fallback) = second {
                stream_ok = fallback.complete_stream(req.clone(), collect_tx.clone()).await.is_ok();
            }
        } else if let Some(fallback) = second {
            stream_ok = fallback.complete_stream(req.clone(), collect_tx.clone()).await.is_ok();
        }

        drop(collect_tx);
        let (full_text, stream_usage) = collector.await.unwrap_or_default();

        if !stream_ok {
            anyhow::bail!("all model backends failed for streaming request");
        }

        let (prompt_tokens, completion_tokens, cached_tokens) =
            stream_usage.unwrap_or((0, 0, 0));

        Ok(CompletionResponse {
            text: full_text,
            tool_calls: vec![],
            prompt_tokens,
            completion_tokens,
            cached_tokens,
            latency_ms: 0,
            thinking: None,
        })
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
        // Read sandbox config — default true (enabled)
        let sandbox_enabled = {
            let cfg = self.config.read().await;
            cfg.settings
                .get("sandbox")
                .and_then(|v| v.as_bool())
                .unwrap_or(true)
        };

        if sandbox_enabled {
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

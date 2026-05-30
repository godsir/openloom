//! Agent loop — the core execution cycle: LLM call → tool dispatch → repeat.
//!
//! This is the heart of an agent turn. Given a user message, it assembles
//! the context window, calls the LLM, dispatches tool calls, and iterates
//! until the LLM produces a final text response or max iterations is hit.

use anyhow::Result;
use loom_context::{AssembleOptions, ContextAssembler};
use loom_inference::engine::CloudClient;
use loom_security::check_permission;
use loom_types::SkillPermissions;
use loom_types::{CompletionRequest, ContentPart, Message, Role, StreamDelta, ToolDefinition};
use tokio::sync::mpsc;
use tracing::info;

use crate::tool_registry::ToolRegistry;

/// The result of one agent turn.
#[derive(Debug, Clone)]
pub struct TurnResult {
    pub response: String,
    /// Thinking/reasoning content (empty if model doesn't support it).
    pub thinking: String,
    /// Rich content parts for persistence (thinking + text + tool calls).
    pub content_parts: Vec<ContentPart>,
    pub tool_calls_made: usize,
    pub iterations: usize,
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    /// Estimated cached tokens from prefix hit (client-side estimate, 0 if no hit).
    pub cached_tokens: usize,
    /// Whether the most recent prefix check was a cache hit (None = not checked).
    pub kv_cache_hit: Option<bool>,
    /// Intermediate tool-call and tool-result messages for persistence.
    pub tool_messages: Vec<Message>,
    /// Token usage from auxiliary models (vision, etc.) for separate cost tracking.
    pub vision_usage: Option<crate::vision::VisionUsage>,
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
    /// If true, start with only request_tools and load real tools on demand.
    pub lazy_tools: bool,
    /// Persona text (injected into stable prefix).
    pub persona: Option<String>,
    /// Conversation summary (injected into stable prefix).
    pub summary: Option<String>,
    /// KG context text (injected into stable prefix).
    pub kg_context: Option<String>,
    /// Extended thinking token budget. None = disabled.
    pub thinking_budget: Option<usize>,
    /// Registered model configs for vision auxiliary lookup.
    pub model_configs: Vec<loom_types::ModelConfig>,
    /// Name of the currently active main model (used to look up vision capability).
    pub active_model_name: Option<String>,
}

impl Default for AgentLoopConfig {
    fn default() -> Self {
        Self {
            system_prompt: "You are an AI assistant with real access to the user's machine. To use tools (file ops, shell, search, LSP, MCP), call request_tools first with what you need. Then use the loaded tools. For simple questions, just answer directly.".into(),
            max_iterations: 30,
            max_tokens: 4096,
            temperature: 0.0,
            lazy_tools: true,
            persona: None,
            summary: None,
            kg_context: None,
            thinking_budget: None,
            model_configs: Vec::new(),
            active_model_name: None,
        }
    }
}

/// Remove all `ContentPart::Image` entries from the message list. Used after
/// vision auxiliary injects a textual `<vision-context>` so non-vision main
/// models never receive an `image_url` part they cannot deserialize.
fn strip_image_parts(messages: &mut Vec<Message>) {
    for m in messages.iter_mut() {
        m.content
            .retain(|p| !matches!(p, ContentPart::Image { .. } | ContentPart::ImageRef { .. }));
        if m.content.is_empty() {
            m.content.push(ContentPart::Text {
                text: String::new(),
            });
        }
    }
}

/// Returns true iff the named model is registered with `capabilities.vision = true`.
fn main_model_has_vision(
    model_configs: &[loom_types::ModelConfig],
    active_model_name: &Option<String>,
) -> bool {
    let Some(name) = active_model_name.as_deref() else {
        return false;
    };
    model_configs
        .iter()
        .find(|c| c.name == name)
        .map(|c| c.capabilities.vision)
        .unwrap_or(false)
}

// ── Image path detection ───────────────────────────────────────────────

/// Extract image file paths from text (Windows and Unix absolute paths with image extensions).
fn extract_image_paths(text: &str) -> Vec<String> {
    let image_exts = ["jpg", "jpeg", "png", "gif", "webp", "bmp", "svg"];
    let ext_pattern = image_exts.join("|");
    let mut paths = Vec::new();

    // Match Windows paths: D:\foo\bar.jpg or C:/foo/bar.png
    let win_re = regex::Regex::new(&format!(
        r#"[A-Za-z]:[/\\][^\s<>"|]+\.(?i)({})"#, ext_pattern
    ))
    .unwrap();
    for mat in win_re.find_iter(text) {
        paths.push(mat.as_str().to_string());
    }

    // Match Unix paths: /foo/bar.jpg
    let unix_re = regex::Regex::new(&format!(
        r#"/[^\s<>"|]+\.(?i)({})"#, ext_pattern
    ))
    .unwrap();
    for mat in unix_re.find_iter(text) {
        let path = mat.as_str().to_string();
        if !paths.contains(&path) {
            paths.push(path);
        }
    }

    paths
}

/// Load an image file and convert to ContentPart::Image with base64 data.
fn load_image_as_content_part(path: &str) -> Result<ContentPart> {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use std::path::Path;

    let file_path = Path::new(path);
    if !file_path.exists() {
        anyhow::bail!("Image file not found: {}", path);
    }

    let data = std::fs::read(file_path)?;
    let base64_data = STANDARD.encode(&data);

    // Detect media type from extension
    let ext = file_path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();

    let media_type = match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        _ => "image/jpeg", // default
    };

    Ok(ContentPart::Image {
        source_type: "base64".to_string(),
        media_type: media_type.to_string(),
        data: base64_data,
    })
}

// ── On-demand tool loading ───────────────────────────────────────────────

/// The single meta-tool sent on the first iteration. The LLM calls this
/// when it actually needs tools, so pure Q&A turns never pay the token cost
/// of full tool definitions.
fn request_tools_definition() -> ToolDefinition {
    ToolDefinition {
        name: "request_tools".into(),
        description: "MUST call this first before any file/shell/search operation. You can either describe what you need to do, or specify tool names directly. The matching tools will load and become available.".into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "reason": {"type": "string", "description": "What do you need to do? Be specific."},
                "tools": {"type": "array", "items": {"type": "string"}, "description": "Specific tool names you need, e.g. [\"file_write\", \"shell\"]"}
            }
        }),
        tags: vec![],
    }
}

/// Match tool names/descriptions against keywords in the reason string.
/// Falls back to the built-in tools if nothing matches.
fn match_tools(reason: &str, all: &[ToolDefinition]) -> Vec<ToolDefinition> {
    let r = reason.to_lowercase();
    let keywords: Vec<&str> = r.split_whitespace().filter(|w| w.len() >= 3).collect();

    let mut matched: Vec<ToolDefinition> = all
        .iter()
        .filter(|t| {
            if t.name == "request_tools" {
                return false;
            }
            let nl = t.name.to_lowercase();
            let dl = t.description.to_lowercase();
            keywords.iter().any(|kw| nl.contains(kw) || dl.contains(kw))
        })
        .cloned()
        .collect();

    // Always include the essential built-in tools as a base
    let builtins: &[&str] = &[
        "shell",
        "file_read",
        "file_write",
        "file_list",
        "content_search",
        "file_delete",
        "use_skill",
    ];
    for name in builtins {
        if !matched.iter().any(|t| t.name == *name) {
            if let Some(t) = all.iter().find(|t| t.name == *name) {
                matched.push(t.clone());
            }
        }
    }

    // Always include all MCP tools (mcp__ prefix) so the model can discover them
    for t in all {
        if t.name.starts_with("mcp__") && !matched.iter().any(|m| m.name == t.name) {
            matched.push(t.clone());
        }
    }

    matched
}

pub(crate) fn build_user_message(user_message: &str, attached_images: &[ContentPart]) -> Message {
    let mut content: Vec<ContentPart> = Vec::new();
    if !user_message.is_empty() {
        content.push(ContentPart::Text {
            text: user_message.to_string(),
        });
    }
    content.extend(attached_images.iter().cloned());
    if content.is_empty() {
        content.push(ContentPart::Text {
            text: String::new(),
        });
    }
    Message {
        role: loom_types::Role::User,
        content,
        timestamp: chrono::Utc::now(),
        usage: None,
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
    let cancel = tokio_util::sync::CancellationToken::new();
    run_agent_turn_inner(
        client,
        registry,
        history,
        user_message,
        &[],
        config,
        allowed_tools,
        disallowed_tools,
        &cancel,
    )
    .await
}

/// Execute one agent turn with attached images.
pub async fn run_agent_turn_with_images(
    client: &dyn CloudClient,
    registry: &ToolRegistry,
    history: &[Message],
    user_message: &str,
    attached_images: &[ContentPart],
    config: &AgentLoopConfig,
    allowed_tools: &Option<Vec<String>>,
    disallowed_tools: &Option<Vec<String>>,
) -> Result<TurnResult> {
    let cancel = tokio_util::sync::CancellationToken::new();
    run_agent_turn_inner(
        client,
        registry,
        history,
        user_message,
        attached_images,
        config,
        allowed_tools,
        disallowed_tools,
        &cancel,
    )
    .await
}

async fn run_agent_turn_inner(
    client: &dyn CloudClient,
    registry: &ToolRegistry,
    history: &[Message],
    user_message: &str,
    attached_images: &[ContentPart],
    config: &AgentLoopConfig,
    allowed_tools: &Option<Vec<String>>,
    disallowed_tools: &Option<Vec<String>>,
    cancel: &tokio_util::sync::CancellationToken,
) -> Result<TurnResult> {
    client.prefix_cache_reset();
    let tools = registry.filtered_definitions(allowed_tools, disallowed_tools);
    let assembler = ContextAssembler::new(&config.system_prompt, 8192);
    let opts = AssembleOptions {
        persona: config.persona.clone(),
        summary: config.summary.clone(),
        kg_context: config.kg_context.clone(),
        tool_catalog: None,
        history: history.to_vec(),
    };
    let mut messages = assembler.build(opts)?;
    // Strip images from history — they were already processed by the vision
    // model in their original turn. Only the current user message's images
    // (appended below) should trigger vision auxiliary processing.
    for msg in messages.iter_mut() {
        msg.content.retain(|part| {
            !matches!(
                part,
                ContentPart::Image { .. } | ContentPart::ImageRef { .. }
            )
        });
    }
    // Drop history messages that became empty after image stripping.
    messages.retain(|msg| !msg.content.is_empty());
    messages.push(build_user_message(user_message, attached_images));

    // Detect image file paths in user message and load them as images
    if let Some(last_msg) = messages.last_mut() {
        let image_paths = extract_image_paths(user_message);
        for path in image_paths {
            if let Ok(image_part) = load_image_as_content_part(&path) {
                last_msg.content.push(image_part);
                info!(path = %path, "loaded image from path in user message");
            }
        }
    }

    let mut vision_usage: Option<crate::vision::VisionUsage> = None;

    // Vision auxiliary: if images present and the main model has no vision
    // capability, call the vision model to produce a textual analysis and
    // strip image parts so non-vision providers never see `image_url`.
    if crate::vision::has_images(&messages) {
        let main_has_vision =
            main_model_has_vision(&config.model_configs, &config.active_model_name);
        if main_has_vision {
            info!("main model is vision-capable, skipping vision auxiliary");
        } else {
            let vision_cfg = crate::vision::load_vision_config();
            if vision_cfg.enabled {
                if let Some(vision_model) = &vision_cfg.model {
                    let images = crate::vision::extract_images(&messages);
                    if !images.is_empty() {
                        let model_configs = config.model_configs.clone();
                        let vision_fut = crate::vision::prepare_vision_context(
                            &images,
                            user_message,
                            vision_model,
                            &model_configs,
                            None, // no progress reporting in non-streaming
                        );
                        let vision_result =
                            tokio::time::timeout(std::time::Duration::from_secs(300), vision_fut)
                                .await;
                        match vision_result {
                            Ok(Ok(vresult)) => {
                                vision_usage = Some(vresult.usage);
                                messages.push(Message {
                                    role: loom_types::Role::System,
                                    content: vec![ContentPart::Text { text: vresult.context }],
                                    timestamp: chrono::Utc::now(),
                                    usage: None,
                                });
                                info!("vision auxiliary context injected");
                            }
                            Ok(Err(e)) => {
                                tracing::warn!(error = %e, "vision auxiliary failed");
                                messages.push(Message {
                                    role: loom_types::Role::System,
                                    content: vec![ContentPart::Text {
                                        text: format!(
                                            "<vision-context>\n[图像分析失败：辅助视觉模型不可用 ({}). 请明确告诉用户你没看到图片，建议稍后重试或检查视觉模型配置。]\n</vision-context>",
                                            e
                                        ),
                                    }],
                                    timestamp: chrono::Utc::now(),
                                    usage: None,
                                });
                            }
                            Err(_) => {
                                tracing::warn!("vision auxiliary timed out after 300s");
                                messages.push(Message {
                                    role: loom_types::Role::System,
                                    content: vec![ContentPart::Text {
                                        text: "<vision-context>\n[图像分析超时：辅助视觉模型 300s 内无响应。请明确告诉用户你没看到图片，建议检查视觉模型配置。]\n</vision-context>".into(),
                                    }],
                                    timestamp: chrono::Utc::now(),
                                    usage: None,
                                });
                            }
                        }
                    }
                }
            }
            // Always strip images for non-vision main model — even if aux
            // disabled or failed — to avoid 400 "unknown variant 'image_url'".
            strip_image_parts(&mut messages);
        }
    }

    let mut tool_calls_made = 0;
    let mut tool_messages: Vec<Message> = Vec::new();
    let mut total_prompt = 0usize;
    let mut total_completion = 0usize;

    for iteration in 0..config.max_iterations {
        // Check for user interruption before each iteration
        if cancel.is_cancelled() {
            info!("agent turn cancelled by user at iteration {}", iteration);
            return Ok(TurnResult {
                response: "[已中断]".into(),
                thinking: String::new(),
                tool_calls_made,
                iterations: iteration,
                prompt_tokens: total_prompt,
                completion_tokens: total_completion,
                cached_tokens: 0,
                kv_cache_hit: None,
                content_parts: vec![ContentPart::Text {
                    text: "[已中断]".into(),
                }],
                tool_messages: vec![],
                vision_usage: None,
            });
        }

        // Many providers (Gemini-compat gateways especially) reject requests
        // that combine image input with arbitrary function-calling tools, e.g.
        // "Only google search tool ... is supported for image response".
        // Only strip tools when the model can't see images natively.
        let images_in_call = crate::vision::has_images(&messages);
        let mut force_no_tools = false;
        let strip_tools_for_images = images_in_call
            && !main_model_has_vision(&config.model_configs, &config.active_model_name);

        let mut response = loop {
            let effective_tools = if strip_tools_for_images || force_no_tools {
                Vec::new()
            } else {
                tools.clone()
            };

            let request = CompletionRequest {
                messages: messages.clone(),
                tools: effective_tools,
                tool_choice: None,
                prompt: String::new(),
                max_tokens: config.max_tokens,
                temperature: config.temperature,
                top_p: 1.0,
                stop: Vec::new(),
                stream: false,
                thinking_budget: config.thinking_budget,
            };

            info!(
                iteration,
                tool_count = if strip_tools_for_images || force_no_tools {
                    0
                } else {
                    tools.len()
                },
                msg_count = messages.len(),
                images_in_call,
                strip_tools_for_images,
                force_no_tools,
                "agent turn iteration"
            );

            match client.complete(request).await {
                Ok(r) => break r,
                Err(e) => {
                    let msg = e.to_string();
                    let is_image_tool_conflict = !force_no_tools
                        && !tools.is_empty()
                        && (msg.contains("image response")
                            || msg.contains("Only google search tool"));
                    if is_image_tool_conflict {
                        tracing::warn!(
                            error = %msg,
                            "upstream rejected tools for image-response model, retrying without tools"
                        );
                        force_no_tools = true;
                        continue;
                    }
                    return Err(e);
                }
            }
        };
        total_prompt += response.prompt_tokens;
        total_completion += response.completion_tokens;

        // After image-bearing call, strip images so next iteration can use tools.
        if images_in_call {
            strip_image_parts(&mut messages);
        }

        // Local models sometimes emit tool calls as inline XML/text instead of
        // structured calls. Parse them from the text when no structured calls
        // were received. When tools are already cleared, strip the inline calls
        // so raw XML doesn't leak into the final response.
        if response.tool_calls.is_empty() && !response.text.is_empty() {
            let (cleaned, inline_tcs) = loom_inference::parse_inline_tool_calls(&response.text);
            if !inline_tcs.is_empty() {
                response.text = cleaned;
                if !tools.is_empty() {
                    response.tool_calls = inline_tcs;
                }
            }
        }

        // If the LLM returned tool calls, dispatch them
        if !response.tool_calls.is_empty() {
            info!(count = response.tool_calls.len(), names = ?response.tool_calls.iter().map(|t| &t.name).collect::<Vec<_>>(), "tool calls received");

            // Add assistant message with tool calls + thinking (if any)
            let mut assistant_content: Vec<ContentPart> = Vec::new();
            if let Some(ref thinking) = response.thinking {
                assistant_content.push(ContentPart::Thinking {
                    text: thinking.clone(),
                });
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
                usage: None,
            });
            tool_messages.push(messages.last().unwrap().clone());

            // Execute each tool call
            for tc in &response.tool_calls {
                let tool_name = tc.name.clone();
                info!(name = %tool_name, "executing tool");
                // Permission check — local personal AI allows shell and file write by default
                let perms = SkillPermissions {
                    shell: true,
                    fs_write: Some(vec![]),
                    ..Default::default()
                };
                let (allowed, risk) = check_permission(&tool_name, &perms);
                if !allowed {
                    let perm_msg = Message::tool(
                        &tc.id,
                        &tool_name,
                        format!("Permission denied (risk: {:?})", risk),
                    );
                    messages.push(perm_msg.clone());
                    tool_messages.push(perm_msg);
                    continue;
                }

                let (progress_tx, _progress_rx) = mpsc::unbounded_channel();
                match registry
                    .execute(&tc.name, tc.arguments.clone(), progress_tx)
                    .await
                {
                    Ok(result) => {
                        tool_calls_made += 1;
                        let content = if result.is_error {
                            format!("Error: {}", result.content)
                        } else {
                            result.content
                        };
                        let tool_msg = Message::tool(&tc.id, &tool_name, &content);
                        messages.push(tool_msg.clone());
                        tool_messages.push(tool_msg);
                    }
                    Err(e) => {
                        let err_msg = format!("Tool execution failed: {}", e);
                        let tool_msg = Message::tool(&tc.id, &tool_name, &err_msg);
                        messages.push(tool_msg.clone());
                        tool_messages.push(tool_msg);
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
        let thinking_text = response.thinking.unwrap_or_default();
        let mut content_parts = Vec::new();
        if !thinking_text.is_empty() {
            content_parts.push(ContentPart::Thinking {
                text: thinking_text.clone(),
            });
        }
        content_parts.push(ContentPart::Text {
            text: response_text.clone(),
        });
        for (media_type, data) in &response.images {
            content_parts.push(ContentPart::Image {
                source_type: "base64".to_string(),
                media_type: media_type.clone(),
                data: data.clone(),
            });
        }
        return Ok(TurnResult {
            response: response_text,
            thinking: thinking_text,
            content_parts,
            tool_calls_made,
            iterations: iteration + 1,
            prompt_tokens: total_prompt,
            completion_tokens: total_completion,
            cached_tokens: client.estimated_cache_tokens(),
            kv_cache_hit: client.last_cache_hit(),
            tool_messages,
            vision_usage: vision_usage.clone(),
        });
    }

    Ok(TurnResult {
        response: "Agent reached maximum iterations without resolving.".into(),
        thinking: String::new(),
        content_parts: vec![ContentPart::Text {
            text: "Agent reached maximum iterations without resolving.".into(),
        }],
        tool_calls_made,
        iterations: config.max_iterations,
        prompt_tokens: total_prompt,
        completion_tokens: total_completion,
        cached_tokens: client.estimated_cache_tokens(),
        kv_cache_hit: client.last_cache_hit(),
        tool_messages: vec![],
        vision_usage: vision_usage.clone(),
    })
}

/// Streaming variant — yields StreamDelta events as they arrive.
#[allow(clippy::too_many_arguments)]
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
    let cancel = tokio_util::sync::CancellationToken::new();
    run_agent_turn_streaming_inner(
        client,
        registry,
        history,
        user_message,
        &[],
        config,
        delta_tx,
        allowed_tools,
        disallowed_tools,
        &cancel,
    )
    .await
}

/// Execute one agent turn (streaming) with attached images.
#[allow(clippy::too_many_arguments)]
pub async fn run_agent_turn_streaming_with_images(
    client: &dyn CloudClient,
    registry: &ToolRegistry,
    history: &[Message],
    user_message: &str,
    attached_images: &[ContentPart],
    config: &AgentLoopConfig,
    delta_tx: mpsc::Sender<StreamDelta>,
    allowed_tools: &Option<Vec<String>>,
    disallowed_tools: &Option<Vec<String>>,
    cancel: &tokio_util::sync::CancellationToken,
) -> Result<TurnResult> {
    run_agent_turn_streaming_inner(
        client,
        registry,
        history,
        user_message,
        attached_images,
        config,
        delta_tx,
        allowed_tools,
        disallowed_tools,
        cancel,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn run_agent_turn_streaming_inner(
    client: &dyn CloudClient,
    registry: &ToolRegistry,
    history: &[Message],
    user_message: &str,
    attached_images: &[ContentPart],
    config: &AgentLoopConfig,
    delta_tx: mpsc::Sender<StreamDelta>,
    allowed_tools: &Option<Vec<String>>,
    disallowed_tools: &Option<Vec<String>>,
    cancel: &tokio_util::sync::CancellationToken,
) -> Result<TurnResult> {
    client.prefix_cache_reset();
    let all_tools = registry.filtered_definitions(allowed_tools, disallowed_tools);
    let mut tools = if config.lazy_tools {
        vec![request_tools_definition()]
    } else {
        all_tools.clone()
    };
    let assembler = ContextAssembler::new(&config.system_prompt, 8192);
    let opts = AssembleOptions {
        persona: config.persona.clone(),
        summary: config.summary.clone(),
        kg_context: config.kg_context.clone(),
        tool_catalog: None,
        history: history.to_vec(),
    };
    let mut messages = assembler.build(opts)?;
    // Strip images from history — they were already processed by the vision
    // model in their original turn. Only the current user message's images
    // (appended below) should trigger vision auxiliary processing.
    for msg in messages.iter_mut() {
        msg.content.retain(|part| {
            !matches!(
                part,
                ContentPart::Image { .. } | ContentPart::ImageRef { .. }
            )
        });
    }
    // Drop history messages that became empty after image stripping.
    messages.retain(|msg| !msg.content.is_empty());
    tracing::info!(
        sys_chars = config.system_prompt.len(),
        tool_count = tools.len(),
        all_tool_count = all_tools.len(),
        hist_msgs = history.len(),
        lazy = config.lazy_tools,
        "streaming turn — {} chars system prompt, {}/{} tools, {} history msgs",
        config.system_prompt.len(),
        tools.len(),
        all_tools.len(),
        history.len(),
    );
    messages.push(build_user_message(user_message, attached_images));

    // Detect image file paths in user message and load them as images
    if let Some(last_msg) = messages.last_mut() {
        let image_paths = extract_image_paths(user_message);
        for path in image_paths {
            if let Ok(image_part) = load_image_as_content_part(&path) {
                last_msg.content.push(image_part);
                tracing::info!(path = %path, "loaded image from path in user message (streaming)");
            }
        }
    }

    let mut vision_usage: Option<crate::vision::VisionUsage> = None;

    // Vision auxiliary: if images present and main model lacks vision capability,
    // call vision model, inject textual context, and strip image parts.
    let imgs_in_messages = crate::vision::has_images(&messages);
    tracing::info!(
        has_images = imgs_in_messages,
        active_model = ?config.active_model_name,
        model_config_count = config.model_configs.len(),
        "vision check: messages contain images? (streaming)"
    );
    if !config.model_configs.is_empty() {
        for mc in &config.model_configs {
            tracing::info!(
                name = %mc.name,
                vision = mc.capabilities.vision,
                "model_config (streaming)"
            );
        }
    }
    if imgs_in_messages {
        let main_has_vision =
            main_model_has_vision(&config.model_configs, &config.active_model_name);
        tracing::info!(main_has_vision, "main_model_has_vision result (streaming)");
        if main_has_vision {
            tracing::info!("main model is vision-capable, skipping vision auxiliary (streaming)");
        } else {
            let vision_cfg = crate::vision::load_vision_config();
            if vision_cfg.enabled {
                if let Some(vision_model) = &vision_cfg.model {
                    let images = crate::vision::extract_images(&messages);
                    if !images.is_empty() {
                        let _ = delta_tx
                            .send(StreamDelta::Text("\x02VISION_START\x02".into()))
                            .await;
                        let model_configs = config.model_configs.clone();
                        let (progress_tx, mut progress_rx) =
                            tokio::sync::mpsc::channel::<crate::vision::VisionBatchProgress>(8);
                        let images = images.clone();
                        let user_message = user_message.to_string();
                        let vision_model = vision_model.clone();

                        // Spawn progress forwarder
                        let delta_tx_progress = delta_tx.clone();
                        let progress_handle = tokio::spawn(async move {
                            while let Some(p) = progress_rx.recv().await {
                                // Encode result: replace newlines with \x03 for safe transport
                                let result_encoded = p.result
                                    .as_deref()
                                    .unwrap_or("")
                                    .replace('\n', "\x03");
                                let signal = format!(
                                    "\x02VISION_BATCH\x02{};{};{};{}",
                                    p.batch_index, p.total_batches, p.status, result_encoded
                                );
                                let _ = delta_tx_progress
                                    .send(StreamDelta::Text(signal))
                                    .await;
                            }
                        });

                        let vision_fut = crate::vision::prepare_vision_context(
                            &images,
                            &user_message,
                            &vision_model,
                            &model_configs,
                            Some(progress_tx),
                        );
                        let vision_result =
                            tokio::time::timeout(std::time::Duration::from_secs(300), vision_fut)
                                .await;
                        match vision_result {
                            Ok(Ok(vresult)) => {
                                // Emit vision token usage as AuxiliaryUsage delta
                                // so the orchestrator persists it under the vision model name
                                if vresult.usage.prompt_tokens > 0 || vresult.usage.completion_tokens > 0 {
                                    let _ = delta_tx.send(StreamDelta::AuxiliaryUsage {
                                        model: vresult.usage.model_name.clone(),
                                        prompt_tokens: vresult.usage.prompt_tokens as u64,
                                        completion_tokens: vresult.usage.completion_tokens as u64,
                                    }).await;
                                }
                                vision_usage = Some(vresult.usage);
                                messages.push(Message {
                                    role: loom_types::Role::System,
                                    content: vec![ContentPart::Text { text: vresult.context }],
                                    timestamp: chrono::Utc::now(),
                                    usage: None,
                                });
                                tracing::info!("vision auxiliary context injected (streaming)");
                            }
                            Ok(Err(e)) => {
                                tracing::warn!(error = %e, "vision auxiliary failed");
                                messages.push(Message {
                                    role: loom_types::Role::System,
                                    content: vec![ContentPart::Text {
                                        text: format!(
                                            "<vision-context>\n[图像分析失败：辅助视觉模型不可用 ({}). 请明确告诉用户你没看到图片，建议稍后重试或检查视觉模型配置。]\n</vision-context>",
                                            e
                                        ),
                                    }],
                                    timestamp: chrono::Utc::now(),
                                    usage: None,
                                });
                            }
                            Err(_) => {
                                tracing::warn!("vision auxiliary timed out after 300s");
                                messages.push(Message {
                                    role: loom_types::Role::System,
                                    content: vec![ContentPart::Text {
                                        text: "<vision-context>\n[图像分析超时：辅助视觉模型 300s 内无响应。请明确告诉用户你没看到图片，建议检查视觉模型配置。]\n</vision-context>".into(),
                                    }],
                                    timestamp: chrono::Utc::now(),
                                    usage: None,
                                });
                            }
                        }
                        // Clean up progress forwarder
                        progress_handle.abort();
                        let _ = delta_tx
                            .send(StreamDelta::Text("\x02VISION_DONE\x02".into()))
                            .await;
                    }
                }
            }
            // Strip images so non-vision main model never receives image_url.
            strip_image_parts(&mut messages);
        }
    }

    let mut tool_calls_made = 0;
    let mut tool_messages: Vec<Message> = Vec::new();
    let mut total_prompt = 0usize;
    let mut total_completion = 0usize;
    let mut final_text = String::new();
    let mut content_parts: Vec<ContentPart> = Vec::new();
    let mut captured_thinking = String::new();
    let mut captured_images: Vec<(String, String)> = Vec::new();

    for iteration in 0..config.max_iterations {
        // Check for user interruption before each iteration
        if cancel.is_cancelled() {
            tracing::info!("agent turn cancelled by user at iteration {}", iteration);
            let _ = delta_tx.send(StreamDelta::Text("[已中断]".into())).await;
            drop(delta_tx);
            return Ok(TurnResult {
                response: "[已中断]".into(),
                thinking: String::new(),
                tool_calls_made,
                iterations: iteration,
                prompt_tokens: total_prompt,
                completion_tokens: total_completion,
                cached_tokens: 0,
                kv_cache_hit: None,
                content_parts: vec![ContentPart::Text {
                    text: "[已中断]".into(),
                }],
                tool_messages: vec![],
                vision_usage: None,
            });
        }

        // Drop tools on iterations that carry image input ONLY when the main
        // model lacks vision capability. Vision-capable models can handle
        // image+tools combos simultaneously.
        let images_in_call = crate::vision::has_images(&messages);
        let mut force_no_tools = false;
        // Only strip tools for images when the model can't see them natively
        let strip_tools_for_images = images_in_call
            && !main_model_has_vision(&config.model_configs, &config.active_model_name);

        let mut pending_tool_calls: Vec<(usize, String, String, String)> = Vec::new();
        let mut this_text = String::new();
        let mut this_thinking = String::new();

        // Inner retry loop: some upstreams (Gemini image-gen) reject tools when
        // returning image responses with errors like "Only google search tool
        // ... is supported for image response". Detect and retry without tools.
        loop {
            let effective_tools = if strip_tools_for_images || force_no_tools {
                Vec::new()
            } else {
                tools.clone()
            };

            let request = CompletionRequest {
                messages: messages.clone(),
                tools: effective_tools,
                tool_choice: None,
                prompt: String::new(),
                max_tokens: config.max_tokens,
                temperature: config.temperature,
                top_p: 1.0,
                stop: Vec::new(),
                stream: true,
                thinking_budget: config.thinking_budget,
            };

            let (stream_tx, mut stream_rx) = mpsc::channel::<StreamDelta>(4096);
            let stream_fut = client.complete_stream_structured(request, stream_tx);
            tokio::pin!(stream_fut);

            // Buffer this attempt's output so we can discard on retry.
            let mut attempt_text = String::new();
            let mut attempt_thinking = String::new();
            let mut attempt_pending: Vec<(usize, String, String, String)> = Vec::new();
            let mut attempt_images: Vec<(String, String)> = Vec::new();
            let mut attempt_prompt_tokens: u64 = 0;
            let mut attempt_completion_tokens: u64 = 0;
            // Forwarded deltas pending — we forward incrementally; if retry
            // happens after partial forward, downstream sees concatenation.
            // Image-only models typically fail before any deltas are emitted.
            let mut forwarded_any = false;

            let stream_err: Option<anyhow::Error> = loop {
                tokio::select! {
                    biased;
                    delta = stream_rx.recv() => {
                        let Some(delta) = delta else { break None };
                        match delta {
                            StreamDelta::Text(t) => {
                                attempt_text.push_str(&t);
                                forwarded_any = true;
                                let _ = delta_tx.send(StreamDelta::Text(t)).await;
                            }
                            StreamDelta::Reasoning(t) => {
                                attempt_thinking.push_str(&t);
                                forwarded_any = true;
                                let _ = delta_tx.send(StreamDelta::Reasoning(t)).await;
                            }
                            StreamDelta::ToolCallBegin { index, id, name } => {
                                attempt_pending.push((index, id.clone(), name.clone(), String::new()));
                                forwarded_any = true;
                                let _ = delta_tx.send(StreamDelta::ToolCallBegin { index, id, name }).await;
                            }
                            StreamDelta::ToolCallArgsChunk { index, chunk } => {
                                if let Some(tc) = attempt_pending.iter_mut().find(|(i, _, _, _)| *i == index) {
                                    tc.3.push_str(&chunk);
                                }
                            }
                            StreamDelta::Usage { prompt_tokens, completion_tokens, .. } => {
                                attempt_prompt_tokens += prompt_tokens;
                                attempt_completion_tokens += completion_tokens;
                                let _ = delta_tx.send(StreamDelta::Usage { prompt_tokens, completion_tokens, cache_read_tokens: 0, cache_write_tokens: 0 }).await;
                            }
                            StreamDelta::Image { media_type, data } => {
                                attempt_images.push((media_type.clone(), data.clone()));
                                forwarded_any = true;
                                let _ = delta_tx.send(StreamDelta::Image { media_type, data }).await;
                            }
                            StreamDelta::ToolResult { call_id, tool_name, success, result } => {
                                forwarded_any = true;
                                let _ = delta_tx.send(StreamDelta::ToolResult { call_id, tool_name, success, result }).await;
                            }
                            StreamDelta::AuxiliaryUsage { .. } => {
                                // Forward auxiliary usage deltas as-is
                                let _ = delta_tx.send(delta).await;
                            }
                        }
                    }
                    r = &mut stream_fut => {
                        match r {
                            Ok(()) => {
                                while let Ok(delta) = stream_rx.try_recv() {
                                    match delta {
                                        StreamDelta::Text(t) => {
                                            attempt_text.push_str(&t);
                                            forwarded_any = true;
                                            let _ = delta_tx.send(StreamDelta::Text(t)).await;
                                        }
                                        StreamDelta::Reasoning(t) => {
                                            attempt_thinking.push_str(&t);
                                            forwarded_any = true;
                                            let _ = delta_tx.send(StreamDelta::Reasoning(t)).await;
                                        }
                                        StreamDelta::Image { media_type, data } => {
                                            attempt_images.push((media_type.clone(), data.clone()));
                                            forwarded_any = true;
                                            let _ = delta_tx.send(StreamDelta::Image { media_type, data }).await;
                                        }
                                        StreamDelta::Usage { prompt_tokens, completion_tokens, .. } => {
                                            attempt_prompt_tokens += prompt_tokens;
                                            attempt_completion_tokens += completion_tokens;
                                            let _ = delta_tx.send(StreamDelta::Usage { prompt_tokens, completion_tokens, cache_read_tokens: 0, cache_write_tokens: 0 }).await;
                                        }
                                        _ => {}
                                    }
                                }
                                break None;
                            }
                            Err(e) => break Some(e),
                        }
                    }
                }
            };

            if let Some(err) = stream_err {
                let msg = err.to_string();
                let is_image_tool_conflict = !force_no_tools
                    && !forwarded_any
                    && !tools.is_empty()
                    && (msg.contains("image response") || msg.contains("Only google search tool"));
                if is_image_tool_conflict {
                    tracing::warn!(
                        error = %msg,
                        "upstream rejected tools for image-response model, retrying without tools"
                    );
                    force_no_tools = true;
                    continue;
                }
                return Err(err);
            }

            // Commit attempt buffers to the iteration state.
            this_text.push_str(&attempt_text);
            this_thinking.push_str(&attempt_thinking);
            pending_tool_calls.extend(attempt_pending);
            for img in attempt_images {
                captured_images.push(img);
            }
            total_prompt += attempt_prompt_tokens as usize;
            total_completion += attempt_completion_tokens as usize;
            break;
        }

        // Strip image parts after the call so subsequent iterations regain
        // tools (we dropped tools above when images were present).
        if images_in_call {
            strip_image_parts(&mut messages);
        }

        // Local models sometimes emit tool calls as inline JSON/text instead of
        // structured calls. Parse them from the text when no structured calls
        // were received. When tools are already cleared, strip the inline
        // calls so raw XML doesn't leak into the final response.
        if pending_tool_calls.is_empty() && !this_text.is_empty() {
            let (cleaned, inline_tcs) = loom_inference::parse_inline_tool_calls(&this_text);
            if !inline_tcs.is_empty() {
                this_text = cleaned;
                if !tools.is_empty() {
                    for (idx, tc) in inline_tcs.into_iter().enumerate() {
                        pending_tool_calls.push((idx, tc.id, tc.name, tc.arguments.to_string()));
                    }
                }
            }
        }

        if !pending_tool_calls.is_empty() {
            let mut assistant_content: Vec<ContentPart> = Vec::new();
            if !this_thinking.is_empty() {
                assistant_content.push(ContentPart::Thinking {
                    text: std::mem::take(&mut this_thinking),
                });
            }
            for (_, id, name, args) in &pending_tool_calls {
                assistant_content.push(ContentPart::ToolCall {
                    id: id.clone(),
                    name: name.clone(),
                    arguments: serde_json::from_str(args)
                        .unwrap_or(serde_json::Value::String(args.clone())),
                });
            }
            messages.push(Message {
                role: Role::Assistant,
                content: assistant_content,
                timestamp: chrono::Utc::now(),
                usage: None,
            });
            tool_messages.push(messages.last().unwrap().clone());

            for (_, tc_id, tc_name, tc_args) in &pending_tool_calls {
                // Handle request_tools meta-tool: match and inject real tools
                if config.lazy_tools && tc_name == "request_tools" {
                    let args: serde_json::Value =
                        serde_json::from_str(tc_args).unwrap_or(serde_json::json!({}));

                    // Support both {"tools": ["file_write"]} and {"reason": "write a file"}
                    let mut matched: Vec<ToolDefinition> = Vec::new();

                    if let Some(tools_arr) = args["tools"].as_array() {
                        // Match by exact tool name
                        for t in tools_arr {
                            if let Some(name) = t.as_str() {
                                if let Some(def) = all_tools.iter().find(|d| d.name == name) {
                                    if !matched.iter().any(|m| m.name == name) {
                                        matched.push(def.clone());
                                    }
                                }
                            }
                        }
                    }

                    // Also try matching by reason if provided
                    let reason = args["reason"].as_str().unwrap_or("");
                    if !reason.is_empty() {
                        let reason_matched = match_tools(reason, &all_tools);
                        for t in reason_matched {
                            if !matched.iter().any(|m| m.name == t.name) {
                                matched.push(t);
                            }
                        }
                    }

                    // Fallback: if nothing matched, load all tools
                    if matched.is_empty() {
                        matched = all_tools.iter()
                            .filter(|t| t.name != "request_tools")
                            .cloned()
                            .collect();
                    }

                    let names: Vec<&str> = matched.iter().map(|t| t.name.as_str()).collect();
                    tracing::info!(%reason, ?names, "request_tools matched");
                    let content = format!("Tools loaded: {}", names.join(", "));
                    messages.push(Message::tool(tc_id, tc_name, &content));
                    tool_messages.push(messages.last().unwrap().clone());
                    tools = matched;
                    tool_calls_made += 1;
                    continue;
                }

                // Permission check (same as non-streaming path)
                let perms = loom_types::SkillPermissions {
                    shell: true,
                    fs_write: Some(vec![]),
                    ..Default::default()
                };
                let (allowed, risk) = check_permission(tc_name, &perms);
                if !allowed {
                    let perm_msg = Message::tool(
                        tc_id,
                        tc_name,
                        format!("Permission denied (risk: {:?})", risk),
                    );
                    messages.push(perm_msg.clone());
                    tool_messages.push(perm_msg);
                    let _ = delta_tx
                        .send(StreamDelta::ToolResult {
                            call_id: tc_id.clone(),
                            tool_name: tc_name.clone(),
                            success: false,
                            result: Some(format!("Permission denied (risk: {:?})", risk)),
                        })
                        .await;
                    continue;
                }

                let arguments = serde_json::from_str(tc_args).unwrap_or(serde_json::json!({}));
                let (progress_tx, _) = mpsc::unbounded_channel();
                match registry.execute(tc_name, arguments, progress_tx).await {
                    Ok(result) => {
                        tool_calls_made += 1;
                        let success = !result.is_error;
                        let content = if result.is_error {
                            format!("Error: {}", result.content)
                        } else {
                            result.content
                        };
                        let tool_msg = Message::tool(tc_id, tc_name, &content);
                        messages.push(tool_msg.clone());
                        tool_messages.push(tool_msg);
                        let _ = delta_tx
                            .send(StreamDelta::ToolResult {
                                call_id: tc_id.clone(),
                                tool_name: tc_name.clone(),
                                success,
                                result: Some(content),
                            })
                            .await;
                    }
                    Err(e) => {
                        let err_msg = format!("Tool execution failed: {}", e);
                        let tool_msg = Message::tool(tc_id, tc_name, &err_msg);
                        messages.push(tool_msg.clone());
                        tool_messages.push(tool_msg);
                        let _ = delta_tx
                            .send(StreamDelta::ToolResult {
                                call_id: tc_id.clone(),
                                tool_name: tc_name.clone(),
                                success: false,
                                result: Some(err_msg),
                            })
                            .await;
                    }
                }
            }
            continue;
        }

        final_text = this_text;
        captured_thinking = std::mem::take(&mut this_thinking);
        content_parts.clear();
        if !captured_thinking.is_empty() {
            content_parts.push(ContentPart::Thinking {
                text: captured_thinking.clone(),
            });
        }
        content_parts.push(ContentPart::Text {
            text: final_text.clone(),
        });
        break;
    }

    // Append captured images to content_parts
    for (media_type, data) in &captured_images {
        content_parts.push(ContentPart::Image {
            source_type: "base64".to_string(),
            media_type: media_type.clone(),
            data: data.clone(),
        });
    }

    Ok(TurnResult {
        response: final_text,
        thinking: captured_thinking,
        content_parts,
        tool_calls_made,
        iterations: 1,
        prompt_tokens: total_prompt,
        completion_tokens: total_completion,
        cached_tokens: client.estimated_cache_tokens(),
        kv_cache_hit: client.last_cache_hit(),
        tool_messages,
        vision_usage: vision_usage.clone(),
    })
}

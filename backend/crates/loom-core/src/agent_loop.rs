//! Agent loop — the core execution cycle: LLM call → tool dispatch → repeat.
//!
//! This is the heart of an agent turn. Given a user message, it assembles
//! the context window, calls the LLM, dispatches tool calls, and iterates
//! until the LLM produces a final text response or max iterations is hit.

use anyhow::Result;
use loom_context::{AssembleOptions, ContextAssembler};
use loom_inference::engine::CloudClient;
use loom_memory::TodoStore;
use loom_security::check_permission;
use loom_types::SkillPermissions;
use loom_types::{
    CompactionConfig, CompletionRequest, CompletionResponse, ContentPart, Message, Role, StreamDelta,
    ToolDefinition,
};
use loom_types::StopReason;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::sync::mpsc;
use tracing::info;
use tracing::debug;

use crate::event_bus::EventBus;
use crate::tool_context::ToolContext;
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
    /// Cache read tokens reported by the provider (Anthropic: cache_read_input_tokens).
    pub cache_read_tokens: usize,
    /// Cache write tokens reported by the provider (Anthropic: cache_creation_input_tokens).
    pub cache_write_tokens: usize,
    /// Intermediate tool-call and tool-result messages for persistence.
    pub tool_messages: Vec<Message>,
    /// Token usage from auxiliary models (vision, etc.) for separate cost tracking.
    pub vision_usage: Option<crate::vision::VisionUsage>,
    /// Why the turn stopped — used by frontend to show/hide Continue button.
    pub stop_reason: StopReason,
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
    /// Workspace path for file operations. Relative paths will be resolved against this.
    pub workspace_path: Option<String>,
    /// Cumulative prompt token budget — stops the loop when exceeded (0 = disabled).
    /// Default 0 (disabled); set to e.g. 96000 for ~80% of a 120K context window.
    pub max_prompt_budget: usize,
    /// Current model context window in tokens. None = unknown (fallback to max_prompt_budget/8192).
    /// Used by mid-turn safety truncation (90% ceiling) and BudgetExhausted (current-window) checks.
    pub context_window: Option<usize>,
    /// Number of messages already summarized; passed to AssembleOptions for layered assembly.
    pub summary_at_count: usize,
    /// Default tool-call permissions applied to every turn.
    /// Set to `SkillPermissions::default()` for zero trust (deny shell, deny file writes);
    /// set shell=true / fs_write=Some(vec![]) to restore the old open-everything behaviour.
    pub default_permissions: SkillPermissions,
    /// Session ID for hook context.
    pub session_id: String,
    /// Agent ID for hook context.
    pub agent_id: String,
    /// In-memory API key store for resolving API keys in auxiliary models (vision, etc.).
    pub key_store: Option<Arc<RwLock<HashMap<String, String>>>>,
    /// Base data directory (~/.loom) for resolving file-based resources.
    pub loom_dir: Option<std::path::PathBuf>,
    /// Permission mode: "operate" | "ask" | "read_only"
    pub permission_mode: String,
    /// When `selected_skills` is non-empty this flag is effectively ignored:
    /// skill body is already in the system prompt so all tools are exposed
    /// upfront so the LLM can act on the skill instructions immediately.
    pub cc_dispatch: bool,
    /// Names of skills already injected into the system prompt.
    /// When non-empty, lazy_tools is bypassed so the LLM can act on skill
    /// instructions without first calling request_tools.
    pub selected_skills: Vec<String>,
    /// Union of `allowed_tools` from all active skills.  When `Some`, the
    /// agent loop filters tool definitions to only include tools in this
    /// set before sending them to the model.  `None` means no skill-level
    /// tool restriction is active.
    pub skill_tool_allowlist: Option<Vec<String>>,
    /// Number of available (installable but not yet selected) skills.
    /// Used by the request_tools handler to decide whether to soft-intercept
    /// web_search requests.
    pub available_skill_count: usize,
    /// Event bus for publishing permission requests (for "ask" mode)
    pub event_bus: Option<EventBus>,
    /// Pending permission approvals keyed by call_id
    #[allow(clippy::type_complexity)]
    pub pending_permissions:
        Option<Arc<RwLock<HashMap<String, tokio::sync::oneshot::Sender<loom_types::PermissionResponse>>>>>,
    /// Tools that the user has chosen to auto-approve for the rest of this session.
    /// Keyed by tool name. Only used in "ask" permission mode.
    pub session_approved_tools: Arc<std::sync::Mutex<HashSet<String>>>,
    /// Optional sandbox guard for file/path access control.
    /// When None, no sandbox checks are performed (backward compatible).
    pub sandbox: Option<Arc<loom_security::sandbox::SandboxGuard>>,
    /// Optional todo store for session-scoped todo list management.
    pub todo_store: Option<Arc<TodoStore>>,
    /// Compaction configuration for mid-turn history compression.
    pub compaction_config: CompactionConfig,
    /// Dynamic context injected as additional system messages AFTER the stable
    /// prefix. Contains frequently-changing content (skills, KG context,
    /// available skills list, workspace path) that would otherwise invalidate
    /// the KV-cache on every turn.
    pub dynamic_context: Option<String>,
    /// Formatted todo list to inject into system prompt each turn. None = empty list (skip injection).
    pub todo_context: Option<String>,
    /// Continuation note injected when the previous turn was cancelled by the user.
    /// Tells the LLM the user's latest message is a correction/follow-up and it should
    /// continue from where it left off rather than starting fresh.
    pub continuation_note: Option<String>,
    /// Steering queue: the GUI can push guidance messages here mid-turn.
    /// Each message is drained at the top of every iteration and injected
    /// as a System message so the LLM can adapt without canceling the turn.
    pub steering_queue: Option<Arc<RwLock<Vec<String>>>>,
    /// Few-shot examples injected as additional system messages after the
    /// stable prefix but before dynamic_context. Each entry becomes a separate
    /// System message. Empty vec (default) disables injection.
    pub few_shots: Vec<String>,
}

impl AgentLoopConfig {
    /// Effective context window in tokens.
    ///
    /// When the model's `context_size` is unknown we fall back to 100 K which
    /// matches modern model defaults — this avoids silently disabling
    /// summarisation / mid-turn compaction.
    pub fn effective_context_window(&self) -> usize {
        self.context_window
            .filter(|w| *w > 0)
            .or_else(|| {
                self.model_configs
                    .iter()
                    .find(|c| Some(c.name.as_str()) == self.active_model_name.as_deref())
                    .map(|c| c.context_size)
                    .filter(|s| *s > 0)
            })
            .unwrap_or(100_000)
    }
}

/// Default system prompt that ships with openLoom.
/// This is the content written to `~/.loom/Loom.md` on first startup,
/// allowing users to discover and customise the agent's behaviour.
pub const DEFAULT_SYSTEM_PROMPT: &str = concat!(
    "你是 openLoom，一个运行在用户本机、拥有真实系统访问能力的 AI 助手。\n",
    "\n",
    "## 核心原则\n",
    "- 简洁直接：用最短的话把事说清楚，不要客套废话。\n",
    "- 行动优先：能直接动手解决的问题就动手，不要只给建议。\n",
    "- 诚实透明：不确定的事明确告知，不要编造。操作前说明风险。\n",
    "- 上下文感知：关注当前工作区路径，理解用户的文件结构和项目背景。\n",
    "\n",
    "## 工具使用\n",
    "你拥有以下工具类别，通过 request_tools 按需加载：\n",
    "- 文件操作：读取、写入、编辑、删除文件，列出目录\n",
    "- Shell：执行终端命令（需关注工作区路径）\n",
    "- 搜索：全文搜索文件内容，支持正则\n",
    "- 网络：搜索网页、抓取 URL 内容\n",
    "- LSP：代码诊断、补全、跳转定义、查找引用（支持 30+ 语言）\n",
    "- MCP：通过 MCP 协议连接外部工具服务\n",
    "- 技能：加载用户导入的技能模块\n",
    "\n",
    "工具使用规则：\n",
    "1. 简单问题直接回答，不要无意义地调用工具。\n",
    "2. 需要工具时先调 request_tools 告知需要哪些工具，加载后再使用。\n",
    "3. 批量操作用 shell 一次性完成，不要逐个文件处理。\n",
    "4. 修改文件前先读文件确认当前内容，修改后展示 diff。\n",
    "5. 长时间操作说明进度，出错时说明原因和恢复方案。\n",
    "\n",
    "## 子代理\n",
    "需要并行处理多个独立子任务时，可以派生子代理并发执行。\n",
    "子代理独立运行，完成后汇总结果。\n",
    "\n",
    "## 知识图谱\n",
    "对话中的重要实体和关系会被自动提取到知识图谱。\n",
    "长期记忆中存储了你的历史交互和用户偏好，会作为上下文注入。\n",
    "\n",
    "## 安全性边界\n",
    "- 操作受权限模式限制（Operate/Ask/Read Only/Plan）。被拒绝的操作不要反复尝试。\n",
    "- 不要尝试绕过安全限制或访问敏感系统文件。\n",
    "- 不确定安全性的操作应先说明风险再执行。\n",
    "\n",
    "## 响应格式\n",
    "- 代码修改后建议用 diff 格式展示（```diff）。\n",
    "- 代码块标注语言类型（```rust、```python 等）。\n",
    "- 列表、分步骤的结构化输出优于大段文字。\n",
    "\n",
    "## 错误恢复\n",
    "- 工具调用失败时分析错误信息，尝试替代方案而非重复相同调用。\n",
    "- 连续失败后向用户说明并征求意见，不要无限重试。\n",
    "\n",
    "## 迭代限制\n",
    "- 每轮对话约 100 次迭代上限。复杂任务应合理规划步骤，避免循环中浪费迭代。\n",
    "- 接近限制时优先给出阶段性结论。\n",
);

impl Default for AgentLoopConfig {
    fn default() -> Self {
        Self {
            system_prompt: DEFAULT_SYSTEM_PROMPT.to_string(),
            max_iterations: 100,
            max_tokens: 4096,
            temperature: 0.0,
            lazy_tools: true,
            persona: None,
            summary: None,
            kg_context: None,
            thinking_budget: None,
            model_configs: Vec::new(),
            active_model_name: None,
            workspace_path: None,
            max_prompt_budget: 0,
            context_window: None,
            summary_at_count: 0,
            default_permissions: SkillPermissions::default(),
            session_id: String::new(),
            agent_id: String::new(),
            key_store: None,
            loom_dir: None,
            permission_mode: "operate".to_string(),
            cc_dispatch: true,
            selected_skills: Vec::new(),
            skill_tool_allowlist: None,
            available_skill_count: 0,
            event_bus: None,
            pending_permissions: None,
            session_approved_tools: Arc::new(std::sync::Mutex::new(HashSet::new())),
            sandbox: None,
            todo_store: None,
            compaction_config: CompactionConfig::default(),
            dynamic_context: None,
            todo_context: None,
            continuation_note: None,
            steering_queue: None,
            few_shots: Vec::new(),
        }
    }
}

/// Remove all `ContentPart::Image` entries from the message list. Used after
/// vision auxiliary injects a textual `<vision-context>` so non-vision main
/// models never receive an `image_url` part they cannot deserialize.
fn strip_image_parts(messages: &mut [Message]) {
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
        r#"[A-Za-z]:[/\\][^\s<>"|]+\.(?i)({})"#,
        ext_pattern
    ))
    .unwrap();
    for mat in win_re.find_iter(text) {
        paths.push(mat.as_str().to_string());
    }

    // Match Unix paths: /foo/bar.jpg
    let unix_re = regex::Regex::new(&format!(r#"/[^\s<>"|]+\.(?i)({})"#, ext_pattern)).unwrap();
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
    use base64::{Engine, engine::general_purpose::STANDARD};
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
        "file_edit",
        "file_glob",
        "file_find",
        "file_list",
        "content_search",
        "file_delete",
        "use_skill",
        "ask_user",
        "system_info",
        "token_usage",
        "memory_search",
        "todo_write",
        "todo_list",
    ];
    for name in builtins {
        if !matched.iter().any(|t| t.name == *name)
            && let Some(t) = all.iter().find(|t| t.name == *name)
        {
            matched.push(t.clone());
        }
    }

    matched
}

/// Sanitize a message sequence before sending to the API.
///
/// Removes two classes of invalid messages that cause HTTP 400:
/// 1. `tool` messages not immediately preceded by an `assistant` message that
///    contains at least one `ToolCall` part — these are "orphaned" tool results
///    that arise when the context assembler truncates the assistant turn away
///    while keeping the tool-result turn.
/// 2. `user` or `assistant` messages whose text content is entirely empty
///    (e.g. DB records written with an empty string before the structured
///    content format was adopted).
///
/// The system message (first message) is always preserved unchanged.
fn sanitize_message_sequence(messages: &mut Vec<Message>) {
    // Pass 1: collect indices of orphaned tool messages.
    // Instead of only checking the immediate predecessor (which fails when a
    // single assistant message issues N > 1 tool calls, producing N contiguous
    // `tool` messages), scan backwards past other `tool` messages to find the
    // nearest non-`tool` predecessor.
    let mut orphaned_tool: std::collections::HashSet<usize> = std::collections::HashSet::new();
    for i in 0..messages.len() {
        if messages[i].role == loom_types::Role::Tool {
            // Scan backwards to the nearest non-tool message.
            let mut valid = false;
            let mut j = i;
            while j > 0 {
                j -= 1;
                if messages[j].role == loom_types::Role::Tool {
                    // Another tool message — keep scanning past it.
                    // If the preceding assistant issued all these tools,
                    // the entire contiguous run is valid.
                    continue;
                }
                // Found a non-tool predecessor.
                valid = messages[j].role == loom_types::Role::Assistant
                    && messages[j]
                        .content
                        .iter()
                        .any(|p| matches!(p, ContentPart::ToolCall { .. }));
                break;
            }
            if !valid {
                orphaned_tool.insert(i);
            }
        }
    }

    // Pass 2: mark assistant messages whose *all* following tool-result
    // messages are orphaned.  If every tool message between this assistant
    // and the next non-tool message is orphaned, the assistant's ToolCall
    // parts have no matching tool_results and cause HTTP 400.
    let mut orphaned_assistant: std::collections::HashSet<usize> = std::collections::HashSet::new();
    for i in 0..messages.len() {
        if messages[i].role == loom_types::Role::Assistant
            && messages[i]
                .content
                .iter()
                .any(|p| matches!(p, ContentPart::ToolCall { .. }))
        {
            // Collect the run of consecutive tool messages immediately
            // following this assistant.
            let mut tool_run: Vec<usize> = Vec::new();
            let mut k = i + 1;
            while k < messages.len() && messages[k].role == loom_types::Role::Tool {
                tool_run.push(k);
                k += 1;
            }
            // If the assistant has tool_calls but zero tool messages follow
            // at all, or every following tool message is orphaned, the
            // assistant is orphaned too.
            if tool_run.is_empty()
                || tool_run.iter().all(|idx| orphaned_tool.contains(idx))
            {
                orphaned_assistant.insert(i);
            }
        }
    }

    // Pass 3: retain only valid messages.
    let mut idx = 0usize;
    messages.retain(|msg| {
        let keep = if orphaned_tool.contains(&idx) {
            tracing::warn!(
                role = ?msg.role,
                idx,
                "sanitize_message_sequence: dropping orphaned tool message"
            );
            false
        } else if orphaned_assistant.contains(&idx) {
            tracing::warn!(
                role = ?msg.role,
                idx,
                "sanitize_message_sequence: dropping orphaned assistant message (tool_calls without results)"
            );
            false
        } else {
            true
        };
        idx += 1;
        keep
    });
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

/// Execution-time enforcement of the agent's tool policy. Mirrors
/// `ToolRegistry::filtered_definitions` + the skill allowlist so a model cannot
/// run a tool that was filtered out of its visible set (by emitting the call
/// name directly, or via `request_tools`). An explicit `disallowed_tools` entry
/// is an absolute veto; `request_tools` is exempt from the positive allowlists
/// so lazy tool-loading always works.
fn tool_execution_denied(
    tool_name: &str,
    allowed_tools: &Option<Vec<String>>,
    disallowed_tools: &Option<Vec<String>>,
    skill_tool_allowlist: &Option<Vec<String>>,
) -> bool {
    if let Some(deny) = disallowed_tools
        && deny.iter().any(|t| t.as_str() == tool_name)
    {
        return true;
    }
    if tool_name == "request_tools"
        || tool_name == "todo_write"
        || tool_name == "todo_list"
    {
        return false;
    }
    if let Some(allow) = allowed_tools
        && !allow.iter().any(|t| t.as_str() == tool_name)
    {
        return true;
    }
    if let Some(allowlist) = skill_tool_allowlist
        && !allowlist.iter().any(|t| t.as_str() == tool_name)
    {
        return true;
    }
    false
}

/// Build synthetic ToolCall ContentParts from tool_messages so that
/// sanitize_message_sequence doesn't orphan the tool results on the next turn.
/// This is needed for interrupted turns (UserCancelled / BudgetExhausted /
/// MaxIterations) where the final assistant content_parts would otherwise only
/// contain a text message like "[已中断]", causing all tool results to be dropped
/// as orphaned.
fn build_toolcall_parts(tool_messages: &[Message]) -> Vec<ContentPart> {
    let mut parts = Vec::new();
    for msg in tool_messages {
        for part in &msg.content {
            if let ContentPart::ToolResult { tool_call_id, name, .. } = part {
                parts.push(ContentPart::ToolCall {
                    id: tool_call_id.clone(),
                    name: name.clone(),
                    arguments: serde_json::json!({}),
                });
            }
        }
    }
    parts
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

/// Execute one agent turn driven by a caller-supplied cancellation token.
///
/// Unlike [`run_agent_turn`], which mints a throwaway token internally, this
/// threads a real `CancellationToken` into the turn so the orchestrator (or a
/// parent agent pool) can interrupt it via `kill`/`shutdown`/`stop_session`.
#[allow(clippy::too_many_arguments)]
pub async fn run_agent_turn_with_cancel(
    client: &dyn CloudClient,
    registry: &ToolRegistry,
    history: &[Message],
    user_message: &str,
    config: &AgentLoopConfig,
    allowed_tools: &Option<Vec<String>>,
    disallowed_tools: &Option<Vec<String>>,
    cancel: &tokio_util::sync::CancellationToken,
) -> Result<TurnResult> {
    run_agent_turn_inner(
        client,
        registry,
        history,
        user_message,
        &[],
        config,
        allowed_tools,
        disallowed_tools,
        cancel,
    )
    .await
}

/// Execute one agent turn with attached images.
#[allow(clippy::too_many_arguments)]
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

#[allow(clippy::too_many_arguments)]
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
    let mut tools = registry.filtered_definitions(allowed_tools, disallowed_tools);
    // If active skills restrict tools, filter to the union of their allowed_tools.
    if let Some(ref allowlist) = config.skill_tool_allowlist {
        let allowed_set: HashSet<&str> = allowlist.iter().map(|s| s.as_str()).collect();
        tools.retain(|t| allowed_set.contains(t.name.as_str()));
    }
    let keep_recent_pct = 0.25_f32;
    let cw = config.effective_context_window();
    let history_budget = (cw as f32 * keep_recent_pct) as usize;
    let assembler = ContextAssembler::new(&config.system_prompt, history_budget);
    let opts = AssembleOptions {
        persona: config.persona.clone(),
        summary: config.summary.clone(),
        kg_context: config.kg_context.clone(),
        tool_catalog: None,
        history: history.to_vec(),
        summary_at_count: config.summary_at_count,
    };
    let mut messages = assembler.build(opts)?;
    // Inject few-shot examples as additional system messages after the stable
    // prefix (index 0) — each shot becomes a separate System message.
    if !config.few_shots.is_empty() {
        for (i, shot) in config.few_shots.iter().enumerate() {
            messages.insert(
                1 + i,
                Message {
                    role: Role::System,
                    content: vec![ContentPart::Text { text: shot.clone() }],
                    timestamp: chrono::Utc::now(),
                    usage: None,
                },
            );
        }
    }
    // Inject dynamic context (skills, KG, workspace) as additional system
    // messages AFTER the stable prefix and few-shot examples — keeping
    // frequently-changing content out of the KV-cache so cache hit rates
    // stay high across turns.
    if let Some(ref dc) = config.dynamic_context {
        if !dc.is_empty() {
            let insert_pos = 1 + config.few_shots.len();
            messages.insert(
                insert_pos,
                Message {
                    role: Role::System,
                    content: vec![ContentPart::Text { text: dc.clone() }],
                    timestamp: chrono::Utc::now(),
                    usage: None,
                },
            );
        }
    }
    // Inject todo context (after dynamic_context, before user/assistant history)
    if let Some(ref tc) = config.todo_context {
        if !tc.is_empty() {
            let insert_pos = 1 + config.few_shots.len() + config.dynamic_context.as_ref().map_or(0, |dc| if dc.is_empty() { 0 } else { 1 });
            messages.insert(
                insert_pos,
                Message {
                    role: Role::System,
                    content: vec![ContentPart::Text { text: tc.clone() }],
                    timestamp: chrono::Utc::now(),
                    usage: None,
                },
            );
        }
    }
    // Inject continuation note — tells LLM the user cancelled and is now giving follow-up
    if let Some(ref note) = config.continuation_note {
        if !note.is_empty() {
            let insert_pos = 1 + config.few_shots.len() + config.dynamic_context.as_ref().map_or(0, |dc| if dc.is_empty() { 0 } else { 1 })
                + config.todo_context.as_ref().map_or(0, |tc| if tc.is_empty() { 0 } else { 1 });
            messages.insert(
                insert_pos,
                Message {
                    role: Role::System,
                    content: vec![ContentPart::Text { text: note.clone() }],
                    timestamp: chrono::Utc::now(),
                    usage: None,
                },
            );
        }
    }
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
    // Drop history messages that became empty (or only contain empty text) after image stripping.
    messages.retain(|msg| {
        !msg.content.is_empty()
            && msg.content.iter().any(|p| match p {
                ContentPart::Text { text } => !text.is_empty(),
                _ => true,
            })
    });
    // Remove orphaned tool messages (tool result without a preceding assistant+tool_call).
    sanitize_message_sequence(&mut messages);
    messages.push(build_user_message(user_message, attached_images));

    // Detect image file paths in user message and load them as images
    if let Some(last_msg) = messages.last_mut() {
        let image_paths = extract_image_paths(user_message);
        for path in image_paths {
            if let Ok(image_part) = load_image_as_content_part(&path) {
                last_msg.content.push(image_part);
                debug!(path = %path, "loaded image from path in user message");
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
            if vision_cfg.enabled
                && let Some(vision_model) = &vision_cfg.model
            {
                // Resolve ImageRef back to base64 for vision model
                let images = crate::vision::extract_images_from_messages(
                    &messages,
                    config.loom_dir.as_deref(),
                );
                if !images.is_empty() {
                    let model_configs = config.model_configs.clone();
                    let ks = config
                        .key_store
                        .clone()
                        .unwrap_or_else(|| Arc::new(RwLock::new(HashMap::new())));
                    let vision_fut = crate::vision::prepare_vision_context(
                        &images,
                        user_message,
                        vision_model,
                        &model_configs,
                        &ks,
                        None, // no progress reporting in non-streaming
                    );
                    let vision_result =
                        tokio::time::timeout(std::time::Duration::from_secs(300), vision_fut).await;
                    match vision_result {
                        Ok(Ok(vresult)) => {
                            vision_usage = Some(vresult.usage);
                            messages.insert(messages.len().saturating_sub(1), Message {
                                role: loom_types::Role::System,
                                content: vec![ContentPart::Text {
                                    text: vresult.context,
                                }],
                                timestamp: chrono::Utc::now(),
                                usage: None,
                            });
                            info!("vision auxiliary context injected");
                        }
                        Ok(Err(e)) => {
                            tracing::warn!(error = %e, "vision auxiliary failed");
                            messages.insert(messages.len().saturating_sub(1), Message {
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
                            messages.insert(messages.len().saturating_sub(1), Message {
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
            // Always strip images for non-vision main model — even if aux
            // disabled or failed — to avoid 400 "unknown variant 'image_url'".
            strip_image_parts(&mut messages);
        }
    }

    let mut tool_calls_made = 0;
    let mut tool_messages: Vec<Message> = Vec::new();
    let mut total_prompt = 0usize;
    let mut total_completion = 0usize;
    let mut total_cache_read = 0usize;
    // NOTE: cache_write_tokens is always 0 for non-streaming because
    // CompletionResponse does not yet separate cache_read/cache_write.
    // The streaming path correctly accumulates via StreamDelta::Usage.
    let total_cache_write = 0usize;

    // Create tool context with workspace path for file operations
    let tool_context = ToolContext {
        workspace_path: config.workspace_path.clone(),
        sandbox: None,
        recently_read: Arc::new(std::sync::Mutex::new(HashMap::new())),
        session_id: Some(config.session_id.clone()),
        todo_store: config.todo_store.clone(),
        event_bus: config.event_bus.clone(),
    };

    // ── Prefix digest: compute SHA256 fingerprint of stable prefix ──
    let digest = assembler.compute_prefix_digest(&AssembleOptions {
        persona: config.persona.clone(),
        summary: config.summary.clone(),
        kg_context: config.kg_context.clone(),
        tool_catalog: None,
        history: vec![],
        summary_at_count: 0,
    });
    client.set_prefix_digest(Some(digest.clone()));
    tracing::info!(
        prefix_hash = %&digest.combined_hash[..12],
        prefix_tokens = digest.prefix_token_count,
        "prefix digest computed — estimated cache savings: {} tokens",
        digest.prefix_token_count,
    );

    let mut safety_truncation_count: u32 = 0;

    for iteration in 0..config.max_iterations {
        // Drain steering queue: inject any GUI-provided guidance messages
        if let Some(ref queue) = config.steering_queue {
            let mut msgs = queue.write().await;
            while let Some(msg) = msgs.pop() {
                messages.push(Message {
                    role: Role::System,
                    content: vec![ContentPart::Text { text: format!("[用户指引] {}", msg) }],
                    timestamp: chrono::Utc::now(),
                    usage: None,
                });
            }
        }
        // Token budget check: stop if CURRENT window tokens exceed the budget
        // (was cumulative total_prompt, which falsely tripped after N iterations).
        let current_window_tokens: usize = if config.max_prompt_budget > 0 {
            let bpe = loom_context::bpe();
            messages.iter().map(|m| loom_context::message_tokens(m, bpe)).sum()
        } else { 0 };
        if config.max_prompt_budget > 0 && current_window_tokens > config.max_prompt_budget {
            info!(
                iteration,
                total_prompt,
                budget = config.max_prompt_budget,
                "token budget exceeded"
            );

            return Ok(TurnResult {
                response: format!(
                    "任务进行中（已用 {} tokens，达到预算上限）。输入「继续」以接着执行。",
                    total_prompt
                ),
                thinking: String::new(),
                tool_calls_made,
                iterations: iteration,
                prompt_tokens: total_prompt,
                completion_tokens: total_completion,
                cached_tokens: 0,
                cache_read_tokens: 0,
                cache_write_tokens: 0,
                kv_cache_hit: None,
                content_parts: {
                    let mut parts = build_toolcall_parts(&tool_messages);
                    parts.push(ContentPart::Text {
                        text: format!(
                            "任务进行中（已用 {} tokens）。输入「继续」以接着执行。",
                            total_prompt
                        ),
                    });
                    parts
                },
                tool_messages,
                vision_usage: None,
                stop_reason: StopReason::BudgetExhausted,
            });
        }
        // Mid-turn safety: check token usage against context window ceiling.
        // When compaction is enabled, try truncation first. When disabled, stop
        // immediately if tokens exceed the ceiling to avoid LLM HTTP 400 errors.
        if !messages.is_empty() {
            let bpe = loom_context::bpe();
            let total_tokens: usize = messages.iter()
                .map(|m| loom_context::message_tokens(m, bpe))
                .sum();
            let cw = config.effective_context_window().max(config.max_prompt_budget);
            let ceiling = (cw as f32 * 0.9) as usize;
            if total_tokens > ceiling {
                if config.compaction_config.enabled {
                    safety_truncation_count += 1;
                    let before = total_tokens;
                    messages = loom_context::mid_turn_safety_truncate(
                        &messages,
                        config.compaction_config.max_tool_output_chars,
                    );
                    let after: usize = messages.iter().map(|m| loom_context::message_tokens(m, bpe)).sum();
                    tracing::info!(iteration, before, after, count = safety_truncation_count, "mid-turn safety truncation applied");
                    // If safety truncation fired 3+ times this turn, or post-truncation
                    // tokens still exceed 85% of the context window, stop and show ContinueButton
                    // so the user can click to continue with a fresh context window (which also
                    // triggers pre-turn LLM summarization at the 80% threshold).
                    let critical_ceiling = (cw as f32 * 0.85) as usize;
                    if safety_truncation_count >= 3 || after > critical_ceiling {
                    info!(
                        iteration,
                        after,
                        count = safety_truncation_count,
                        "safety truncation repeated — stopping for user to continue"
                    );
                    return Ok(TurnResult {
                        response: format!(
                            "任务进行中（已用 {} tokens，达上下文上限）。输入「继续」以接着执行。",
                            after
                        ),
                        thinking: String::new(),
                        tool_calls_made,
                        iterations: iteration,
                        prompt_tokens: total_prompt,
                        completion_tokens: total_completion,
                        cached_tokens: 0,
                        cache_read_tokens: 0,
                        cache_write_tokens: 0,
                        kv_cache_hit: None,
                        content_parts: {
                            let mut parts = build_toolcall_parts(&tool_messages);
                            parts.push(ContentPart::Text {
                                text: format!(
                                    "任务进行中（已用 {} tokens，达上下文上限）。输入「继续」以接着执行。",
                                    after
                                ),
                            });
                            parts
                        },
                        tool_messages,
                        vision_usage: None,
                        stop_reason: StopReason::BudgetExhausted,
                    });
                }
            } else {
                // Compaction disabled but tokens exceed ceiling — stop immediately
                // to avoid LLM HTTP 400 errors from context window overflow.
                info!(
                    iteration,
                    total_tokens,
                    ceiling,
                    "token ceiling exceeded (compaction disabled) — stopping for user to continue"
                );
                return Ok(TurnResult {
                    response: format!(
                        "任务进行中（已用 {} tokens，达上下文上限）。输入「继续」以接着执行。",
                        total_tokens
                    ),
                    thinking: String::new(),
                    tool_calls_made,
                    iterations: iteration,
                    prompt_tokens: total_prompt,
                    completion_tokens: total_completion,
                    cached_tokens: 0,
                    cache_read_tokens: 0,
                    cache_write_tokens: 0,
                    kv_cache_hit: None,
                    content_parts: {
                        let mut parts = build_toolcall_parts(&tool_messages);
                        parts.push(ContentPart::Text {
                            text: format!(
                                "任务进行中（已用 {} tokens，达上下文上限）。输入「继续」以接着执行。",
                                total_tokens
                            ),
                        });
                        parts
                    },
                    tool_messages,
                    vision_usage: None,
                    stop_reason: StopReason::BudgetExhausted,
                });
            }
        }
        }
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
                cache_read_tokens: 0,
                cache_write_tokens: 0,
                kv_cache_hit: None,
                content_parts: {
                    let mut parts = build_toolcall_parts(&tool_messages);
                    parts.push(ContentPart::Text { text: "[已中断]".into() });
                    parts
                },
                tool_messages,
                vision_usage: None,
                stop_reason: StopReason::UserCancelled,
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

            // Transient error retry counter scoped to this LLM attempt
            let mut transient_retries: u32 = 0;

            let inner_result: Option<CompletionResponse> = loop {
                match client.complete(request.clone()).await {
                    Ok(r) => break Some(r),
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
                            break None;
                        }
                        // Retry transient errors with exponential backoff
                        if is_transient_error(&msg) && transient_retries < 3 {
                            transient_retries += 1;
                            let delay_ms = 1000 * 2u64.pow(transient_retries - 1);
                            tracing::warn!(
                                retry = transient_retries,
                                delay_ms,
                                error = %msg,
                                "transient LLM error, retrying with backoff"
                            );
                            tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                            continue;
                        }
                        return Err(e);
                    }
                }
            };
            match inner_result {
                Some(r) => break r,
                None => continue, // restart outer loop with force_no_tools = true
            }
        };
        total_prompt += response.prompt_tokens;
        total_completion += response.completion_tokens;
        total_cache_read += response.cached_tokens;

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

            // Storm breaker: track consecutive identical tool calls
            let mut storm_tracker: HashMap<String, u32> = HashMap::new();
            let mut last_storm_key: Option<String> = None;

            // Execute each tool call
            for tc in &response.tool_calls {
                let tool_name = tc.name.clone();
                info!(name = %tool_name, "executing tool");
                // Permission check using configured defaults (or merged skill permissions)
                let perms = config.default_permissions.clone();
                let (mut allowed, risk) = check_permission(&tool_name, &perms);

                // Enforce the agent's allow/deny tool policy at execution time,
                // not just in the visible tool set the model was shown.
                if allowed
                    && tool_execution_denied(
                        &tool_name,
                        allowed_tools,
                        disallowed_tools,
                        &config.skill_tool_allowlist,
                    )
                {
                    allowed = false;
                }

                // "ask" mode: for medium/high risk tools, request user approval
                if allowed && config.permission_mode == "ask" && risk != loom_types::RiskLevel::Low
                {
                    allowed =
                        request_user_approval(&tc.id, &tool_name, &tc.arguments, &risk, config)
                            .await;
                }



                if !allowed {
                    let reason = match config.permission_mode.as_str() {
                        "plan" => format!(
                            "【规划模式】当前处于 Plan 模式，不允许执行 {} 操作。\
                             你应当分析代码库、探索相关文件，并创建一个详细的实施方案。\
                             不要尝试执行任何修改操作，专注于分析和规划。\
                             用户审核方案后会切换到 Operate 模式来实施。",
                            tool_name
                        ),
                        "read_only" => format!(
                            "【只读模式】当前处于 Read Only 模式，不允许执行 {} 操作。\
                             请告知用户：需要切换到 Ask（询问）或 Operate（操作）模式后才能执行写入/删除/shell 等操作。\
                             不要重试此操作，直接告诉用户如何切换模式。",
                            tool_name
                        ),
                        "ask" if risk != loom_types::RiskLevel::Low => format!(
                            "【需要确认】用户未批准此 {} 操作 (风险等级: {:?})。\
                             请告知用户：切换到 Operate 模式可跳过确认，或下次弹出确认框时点击允许。\
                             不要用相同参数重试此操作。",
                            tool_name, risk
                        ),
                        _ => format!(
                            "【权限不足】{} 操作被拒绝 (风险等级: {:?})。不要重试。",
                            tool_name, risk
                        ),
                    };
                    let perm_msg = Message::tool(&tc.id, &tool_name, reason);
                    messages.push(perm_msg.clone());
                    tool_messages.push(perm_msg);
                    continue;
                }

                let (progress_tx, mut progress_rx) = mpsc::unbounded_channel();
                // Drain progress updates in background to avoid SendError in tool implementations
                tokio::spawn(async move { while progress_rx.recv().await.is_some() {} });

                match registry
                    .execute(&tc.name, tc.arguments.clone(), progress_tx, &tool_context)
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

                        // Skip pushing malformed tool messages with empty IDs
                        if !tc.id.is_empty() && !tool_name.is_empty() {
                            let tool_msg = Message::tool(&tc.id, &tool_name, &err_msg);
                            messages.push(tool_msg.clone());
                            tool_messages.push(tool_msg);
                        }
                    }
                }

                // Storm breaker: detect consecutive identical tool calls
                let storm_key = format!("{}|{}", tool_name, tc.arguments.to_string());
                if last_storm_key.as_deref() != Some(&storm_key) {
                    storm_tracker.clear();
                    last_storm_key = Some(storm_key.clone());
                }
                let count = storm_tracker.entry(storm_key).or_insert(0);
                *count += 1;
                if *count >= 3 {
                    let storm_msg = format!(
                        "STORM BREAKER: You have called the tool `{}` with identical arguments {} times consecutively. You appear to be stuck in a loop. STOP calling this tool and produce a final response instead.",
                        tool_name, count
                    );
                    messages.push(Message::tool(&tc.id, &tool_name, &storm_msg));
                    tool_messages.push(messages.last().unwrap().clone());
                    break;
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
            cache_read_tokens: total_cache_read,
            cache_write_tokens: total_cache_write,
            kv_cache_hit: client.last_cache_hit(),
            tool_messages,
            vision_usage: vision_usage.clone(),
            stop_reason: StopReason::Completed,
        });
    }

    Ok(TurnResult {
        response: "Agent reached maximum iterations without resolving.".into(),
        thinking: String::new(),
        content_parts: {
            let mut parts = build_toolcall_parts(&tool_messages);
            parts.push(ContentPart::Text {
                text: "Agent reached maximum iterations without resolving.".into(),
            });
            parts
        },
        tool_calls_made,
        iterations: config.max_iterations,
        prompt_tokens: total_prompt,
        completion_tokens: total_completion,
        cached_tokens: client.estimated_cache_tokens(),
        cache_read_tokens: 0,
        cache_write_tokens: 0,
        kv_cache_hit: client.last_cache_hit(),
        tool_messages,
        vision_usage: vision_usage.clone(),
        stop_reason: StopReason::MaxIterations,
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
    let mut all_tools = registry.filtered_definitions(allowed_tools, disallowed_tools);
    // If active skills restrict tools, filter to the union of their allowed_tools.
    if let Some(ref allowlist) = config.skill_tool_allowlist {
        let allowed_set: HashSet<&str> = allowlist.iter().map(|s| s.as_str()).collect();
        all_tools.retain(|t| allowed_set.contains(t.name.as_str()));
    }
    let mut tools = if config.lazy_tools {
        // Include use_skill in initial tools so the LLM can directly invoke
        // skills from the Available Skills list without an extra request_tools hop.
        let mut initial = vec![request_tools_definition()];
        if let Some(skill_tool) = all_tools.iter().find(|t| t.name == "use_skill") {
            initial.push(skill_tool.clone());
        }
        initial
    } else {
        all_tools.clone()
    };
    let keep_recent_pct = 0.25_f32;
    let cw = config.effective_context_window();
    let history_budget = (cw as f32 * keep_recent_pct) as usize;
    let assembler = ContextAssembler::new(&config.system_prompt, history_budget);
    let opts = AssembleOptions {
        persona: config.persona.clone(),
        summary: config.summary.clone(),
        kg_context: config.kg_context.clone(),
        tool_catalog: None,
        history: history.to_vec(),
        summary_at_count: config.summary_at_count,
    };
    let mut messages = assembler.build(opts)?;
    // Inject few-shot examples as additional system messages after the stable
    // prefix (index 0) — each shot becomes a separate System message.
    if !config.few_shots.is_empty() {
        for (i, shot) in config.few_shots.iter().enumerate() {
            messages.insert(
                1 + i,
                Message {
                    role: Role::System,
                    content: vec![ContentPart::Text { text: shot.clone() }],
                    timestamp: chrono::Utc::now(),
                    usage: None,
                },
            );
        }
    }
    // Inject dynamic context (skills, KG, workspace) as additional system
    // messages AFTER the stable prefix and few-shot examples — keeping
    // frequently-changing content out of the KV-cache so cache hit rates
    // stay high across turns.
    if let Some(ref dc) = config.dynamic_context {
        if !dc.is_empty() {
            let insert_pos = 1 + config.few_shots.len();
            messages.insert(
                insert_pos,
                Message {
                    role: Role::System,
                    content: vec![ContentPart::Text { text: dc.clone() }],
                    timestamp: chrono::Utc::now(),
                    usage: None,
                },
            );
        }
    }
    // Inject continuation note — tells LLM the user cancelled and is now giving follow-up
    if let Some(ref note) = config.continuation_note {
        if !note.is_empty() {
            let insert_pos = 1 + config.few_shots.len() + config.dynamic_context.as_ref().map_or(0, |dc| if dc.is_empty() { 0 } else { 1 })
                + config.todo_context.as_ref().map_or(0, |tc| if tc.is_empty() { 0 } else { 1 });
            messages.insert(
                insert_pos,
                Message {
                    role: Role::System,
                    content: vec![ContentPart::Text { text: note.clone() }],
                    timestamp: chrono::Utc::now(),
                    usage: None,
                },
            );
        }
    }
    // Inject todo context (after dynamic_context, before user/assistant history)
    if let Some(ref tc) = config.todo_context {
        if !tc.is_empty() {
            let insert_pos = 1 + config.few_shots.len() + config.dynamic_context.as_ref().map_or(0, |dc| if dc.is_empty() { 0 } else { 1 });
            messages.insert(
                insert_pos,
                Message {
                    role: Role::System,
                    content: vec![ContentPart::Text { text: tc.clone() }],
                    timestamp: chrono::Utc::now(),
                    usage: None,
                },
            );
        }
    }
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
    // Drop history messages that became empty (or only contain empty text) after image stripping.
    messages.retain(|msg| {
        !msg.content.is_empty()
            && msg.content.iter().any(|p| match p {
                ContentPart::Text { text } => !text.is_empty(),
                _ => true,
            })
    });
    // Remove orphaned tool messages (tool result without a preceding assistant+tool_call).
    sanitize_message_sequence(&mut messages);
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
                tracing::debug!(path = %path, "loaded image from path in user message (streaming)");
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
            if vision_cfg.enabled
                && let Some(vision_model) = &vision_cfg.model
            {
                // Resolve ImageRef back to base64 for vision model
                let images = crate::vision::extract_images_from_messages(
                    &messages,
                    config.loom_dir.as_deref(),
                );
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
                            let result_encoded =
                                p.result.as_deref().unwrap_or("").replace('\n', "\x03");
                            let signal = format!(
                                "\x02VISION_BATCH\x02{};{};{};{}",
                                p.batch_index, p.total_batches, p.status, result_encoded
                            );
                            let _ = delta_tx_progress.send(StreamDelta::Text(signal)).await;
                        }
                    });

                    let ks = config
                        .key_store
                        .clone()
                        .unwrap_or_else(|| Arc::new(RwLock::new(HashMap::new())));
                    let vision_fut = crate::vision::prepare_vision_context(
                        &images,
                        &user_message,
                        &vision_model,
                        &model_configs,
                        &ks,
                        Some(progress_tx),
                    );
                    let vision_result =
                        tokio::time::timeout(std::time::Duration::from_secs(300), vision_fut).await;
                    match vision_result {
                        Ok(Ok(vresult)) => {
                            // Emit vision token usage as AuxiliaryUsage delta
                            // so the orchestrator persists it under the vision model name
                            if vresult.usage.prompt_tokens > 0
                                || vresult.usage.completion_tokens > 0
                            {
                                let _ = delta_tx
                                    .send(StreamDelta::AuxiliaryUsage {
                                        model: vresult.usage.model_name.clone(),
                                        prompt_tokens: vresult.usage.prompt_tokens as u64,
                                        completion_tokens: vresult.usage.completion_tokens as u64,
                                    })
                                    .await;
                            }
                            vision_usage = Some(vresult.usage);
                            messages.insert(messages.len().saturating_sub(1), Message {
                                role: loom_types::Role::System,
                                content: vec![ContentPart::Text {
                                    text: vresult.context,
                                }],
                                timestamp: chrono::Utc::now(),
                                usage: None,
                            });
                            tracing::info!("vision auxiliary context injected (streaming)");
                        }
                        Ok(Err(e)) => {
                            tracing::warn!(error = %e, "vision auxiliary failed");
                            messages.insert(messages.len().saturating_sub(1), Message {
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
                            messages.insert(messages.len().saturating_sub(1), Message {
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
            // Strip images so non-vision main model never receives image_url.
            strip_image_parts(&mut messages);
        }
    }

    let mut tool_calls_made = 0;
    let mut tool_messages: Vec<Message> = Vec::new();
    let mut total_prompt = 0usize;
    let mut total_completion = 0usize;
    let mut total_cache_read = 0usize;
    let mut total_cache_write = 0usize;
    let mut final_text = String::new();
    let mut content_parts: Vec<ContentPart> = Vec::new();
    let mut captured_thinking = String::new();
    let mut captured_images: Vec<(String, String)> = Vec::new();
    let mut completed_iterations = 0usize;

    // Create tool context with workspace path for file operations
    let tool_context = ToolContext {
        workspace_path: config.workspace_path.clone(),
        sandbox: None,
        recently_read: Arc::new(std::sync::Mutex::new(HashMap::new())),
        session_id: Some(config.session_id.clone()),
        todo_store: config.todo_store.clone(),
        event_bus: config.event_bus.clone(),
    };

    // ── Prefix digest (streaming): compute SHA256 fingerprint of stable prefix ──
    let digest = assembler.compute_prefix_digest(&AssembleOptions {
        persona: config.persona.clone(),
        summary: config.summary.clone(),
        kg_context: config.kg_context.clone(),
        tool_catalog: None,
        history: vec![],
        summary_at_count: 0,
    });
    client.set_prefix_digest(Some(digest.clone()));
    tracing::info!(
        prefix_hash = %&digest.combined_hash[..12],
        prefix_tokens = digest.prefix_token_count,
        "prefix digest computed (streaming) — estimated cache savings: {} tokens",
        digest.prefix_token_count,
    );

    let mut safety_truncation_count: u32 = 0;

    for iteration in 0..config.max_iterations {
        // Drain steering queue: inject any GUI-provided guidance messages
        if let Some(ref queue) = config.steering_queue {
            let mut msgs = queue.write().await;
            while let Some(msg) = msgs.pop() {
                messages.push(Message {
                    role: Role::System,
                    content: vec![ContentPart::Text { text: format!("[用户指引] {}", msg) }],
                    timestamp: chrono::Utc::now(),
                    usage: None,
                });
            }
        }
        // Token budget check: stop if CURRENT window tokens exceed the budget
        // (was cumulative total_prompt, which falsely tripped after N iterations).
        let current_window_tokens: usize = if config.max_prompt_budget > 0 {
            let bpe = loom_context::bpe();
            messages.iter().map(|m| loom_context::message_tokens(m, bpe)).sum()
        } else { 0 };
        if config.max_prompt_budget > 0 && current_window_tokens > config.max_prompt_budget {
            tracing::info!(
                iteration,
                total_prompt,
                budget = config.max_prompt_budget,
                "token budget exceeded (streaming)"
            );

            let msg = format!(
                "任务进行中（已用 {} tokens，达到预算上限）。输入「继续」以接着执行。",
                total_prompt
            );
            let _ = delta_tx.send(StreamDelta::Text(msg.clone())).await;
            drop(delta_tx);
            return Ok(TurnResult {
                response: msg,
                thinking: String::new(),
                tool_calls_made,
                iterations: iteration,
                prompt_tokens: total_prompt,
                completion_tokens: total_completion,
                cached_tokens: 0,
                cache_read_tokens: 0,
                cache_write_tokens: 0,
                kv_cache_hit: None,
                content_parts: {
                    let mut parts = build_toolcall_parts(&tool_messages);
                    parts.push(ContentPart::Text {
                        text: "任务进行中，已达预算上限。".into(),
                    });
                    parts
                },
                tool_messages,
                vision_usage: None,
                stop_reason: StopReason::BudgetExhausted,
            });
        }
        // Mid-turn safety: check token usage against context window ceiling (streaming).
        if !messages.is_empty() {
            let bpe = loom_context::bpe();
            let total_tokens: usize = messages.iter()
                .map(|m| loom_context::message_tokens(m, bpe))
                .sum();
            let cw = config.effective_context_window().max(config.max_prompt_budget);
            let ceiling = (cw as f32 * 0.9) as usize;
            if total_tokens > ceiling {
                if config.compaction_config.enabled {
                safety_truncation_count += 1;
                let before = total_tokens;
                messages = loom_context::mid_turn_safety_truncate(
                    &messages,
                    config.compaction_config.max_tool_output_chars,
                );
                let after: usize = messages.iter().map(|m| loom_context::message_tokens(m, bpe)).sum();
                tracing::info!(iteration, before, after, count = safety_truncation_count, "mid-turn safety truncation applied (streaming)");
                // If safety truncation fired 3+ times this turn, or post-truncation
                // tokens still exceed 85% of the context window, stop and show ContinueButton.
                let critical_ceiling = (cw as f32 * 0.85) as usize;
                if safety_truncation_count >= 3 || after > critical_ceiling {
                    tracing::info!(
                        iteration,
                        after,
                        count = safety_truncation_count,
                        "safety truncation repeated — stopping for user to continue (streaming)"
                    );
                    let msg = format!(
                        "任务进行中（已用 {} tokens，达上下文上限）。输入「继续」以接着执行。",
                        after
                    );
                    let _ = delta_tx.send(StreamDelta::Text(msg.clone())).await;
                    drop(delta_tx);
                    return Ok(TurnResult {
                        response: msg,
                        thinking: String::new(),
                        tool_calls_made,
                        iterations: iteration,
                        prompt_tokens: total_prompt,
                        completion_tokens: total_completion,
                        cached_tokens: 0,
                        cache_read_tokens: 0,
                        cache_write_tokens: 0,
                        kv_cache_hit: None,
                        content_parts: {
                            let mut parts = build_toolcall_parts(&tool_messages);
                            parts.push(ContentPart::Text {
                                text: "任务进行中（已达上下文上限）。输入「继续」以接着执行。".into(),
                            });
                            parts
                        },
                        tool_messages,
                        vision_usage: None,
                        stop_reason: StopReason::BudgetExhausted,
                    });
                }
            } else {
                // Compaction disabled but tokens exceed ceiling — stop immediately (streaming).
                tracing::info!(
                    iteration,
                    total_tokens,
                    ceiling,
                    "token ceiling exceeded (compaction disabled, streaming) — stopping"
                );
                let msg = format!(
                    "任务进行中（已用 {} tokens，达上下文上限）。输入「继续」以接着执行。",
                    total_tokens
                );
                let _ = delta_tx.send(StreamDelta::Text(msg.clone())).await;
                drop(delta_tx);
                return Ok(TurnResult {
                    response: msg,
                    thinking: String::new(),
                    tool_calls_made,
                    iterations: iteration,
                    prompt_tokens: total_prompt,
                    completion_tokens: total_completion,
                    cached_tokens: 0,
                    cache_read_tokens: 0,
                    cache_write_tokens: 0,
                    kv_cache_hit: None,
                    content_parts: {
                        let mut parts = build_toolcall_parts(&tool_messages);
                        parts.push(ContentPart::Text {
                            text: "任务进行中（已达上下文上限）。输入「继续」以接着执行。".into(),
                        });
                        parts
                    },
                    tool_messages,
                    vision_usage: None,
                    stop_reason: StopReason::BudgetExhausted,
                });
            }
        }
        }
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
                cache_read_tokens: 0,
                cache_write_tokens: 0,
                kv_cache_hit: None,
                content_parts: {
                    let mut parts = build_toolcall_parts(&tool_messages);
                    parts.push(ContentPart::Text { text: "[已中断]".into() });
                    parts
                },
                tool_messages,
                vision_usage: None,
                stop_reason: StopReason::UserCancelled,
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
        // Declared OUTSIDE the retry loop below: the transient-error handler
        // retries via an unlabeled `continue` on that loop, so declaring the
        // counter inside it would reset it to 0 every retry and defeat the
        // `< 3` cap (unbounded retry). Here it persists across retries of this
        // LLM call, matching the non-streaming path.
        let mut transient_retries: u32 = 0;

        // Inner retry loop: some upstreams (Gemini image-gen) reject tools when
        // returning image responses with errors like "Only google search tool
        // ... is supported for image response". Detect and retry without tools.
        // Also retries transient errors (rate limiting, 5xx, network) with backoff.
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
            let mut attempt_cache_read_tokens: u64 = 0;
            let mut attempt_cache_write_tokens: u64 = 0;
            // Forwarded deltas pending — we forward incrementally; if retry
            // happens after partial forward, downstream sees concatenation.
            // Image-only models typically fail before any deltas are emitted.
            let mut forwarded_any = false;

            let stream_err: Option<anyhow::Error> = loop {
                tokio::select! {
                    biased;
                    _ = cancel.cancelled() => {
                        tracing::info!("agent turn cancelled during LLM stream");
                        drop(stream_rx);
                        // stream_fut is a pinned future that doesn't need explicit drop
                        let _ = delta_tx.send(StreamDelta::Text("[已中断]".into())).await;
                        drop(delta_tx);
                        return Ok(TurnResult {
                            response: "[已中断]".into(),
                            thinking: attempt_thinking,
                            tool_calls_made,
                            iterations: iteration,
                            prompt_tokens: total_prompt + attempt_prompt_tokens as usize,
                            completion_tokens: total_completion + attempt_completion_tokens as usize,
                            cached_tokens: 0,
                            cache_read_tokens: attempt_cache_read_tokens as usize,
                            cache_write_tokens: attempt_cache_write_tokens as usize,
                            kv_cache_hit: None,
                            content_parts: {
                                let mut parts = build_toolcall_parts(&tool_messages);
                                parts.push(ContentPart::Text { text: "[已中断]".into() });
                                parts
                            },
                            tool_messages,
                            vision_usage: None,
                            stop_reason: StopReason::UserCancelled,
                        });
                    }
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
                                let _ = delta_tx.send(StreamDelta::ToolCallBegin { index, id, name }).await;
                            }
                            StreamDelta::ToolCallArgsChunk { index, chunk } => {
                                if let Some(tc) = attempt_pending.iter_mut().find(|(i, _, _, _)| *i == index) {
                                    tc.3.push_str(&chunk);
                                }
                            }
                            StreamDelta::Usage { prompt_tokens, completion_tokens, cache_read_tokens, cache_write_tokens } => {
                                attempt_prompt_tokens += prompt_tokens;
                                attempt_completion_tokens += completion_tokens;
                                attempt_cache_read_tokens += cache_read_tokens;
                                attempt_cache_write_tokens += cache_write_tokens;
                                let _ = delta_tx.send(StreamDelta::Usage { prompt_tokens, completion_tokens, cache_read_tokens, cache_write_tokens }).await;
                            }
                            StreamDelta::Image { media_type, data } => {
                                attempt_images.push((media_type.clone(), data.clone()));
                                let _ = delta_tx.send(StreamDelta::Image { media_type, data }).await;
                            }
                            StreamDelta::ToolResult { call_id, tool_name, success, result, structured_content } => {
                                forwarded_any = true;
                                let _ = delta_tx.send(StreamDelta::ToolResult { call_id, tool_name, success, result, structured_content }).await;
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
                                // Drain remaining deltas with short timeout to catch racing sends.
                                #[allow(clippy::while_let_loop)]
                                loop {
                                    match tokio::time::timeout(
                                        std::time::Duration::from_millis(50),
                                        stream_rx.recv(),
                                    )
                                    .await
                                    {
                                        Ok(Some(delta)) => match delta {
                                            StreamDelta::Text(t) => {
                                                attempt_text.push_str(&t);
                                                let _ = delta_tx.send(StreamDelta::Text(t)).await;
                                            }
                                            StreamDelta::Reasoning(t) => {
                                                attempt_thinking.push_str(&t);
                                                let _ = delta_tx.send(StreamDelta::Reasoning(t)).await;
                                            }
                                            StreamDelta::Image { media_type, data } => {
                                                attempt_images.push((media_type.clone(), data.clone()));
                                                let _ = delta_tx
                                                    .send(StreamDelta::Image { media_type, data })
                                                    .await;
                                            }
                                            StreamDelta::Usage {
                                                prompt_tokens,
                                                completion_tokens,
                                                cache_read_tokens,
                                                cache_write_tokens,
                                            } => {
                                                attempt_prompt_tokens += prompt_tokens;
                                                attempt_completion_tokens += completion_tokens;
                                                attempt_cache_read_tokens += cache_read_tokens;
                                                attempt_cache_write_tokens += cache_write_tokens;
                                                let _ = delta_tx
                                                    .send(StreamDelta::Usage {
                                                        prompt_tokens,
                                                        completion_tokens,
                                                        cache_read_tokens,
                                                        cache_write_tokens,
                                                    })
                                                    .await;
                                            }
                                            _ => {}
                                        },
                                        _ => break,
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
                // Retry transient errors with exponential backoff, but only
                // when no visible content has been forwarded yet.
                if !forwarded_any && is_transient_error(&msg) && transient_retries < 3 {
                    transient_retries += 1;
                    let delay_ms = 1000 * 2u64.pow(transient_retries - 1);
                    tracing::warn!(
                        retry = transient_retries,
                        delay_ms,
                        error = %msg,
                        "transient LLM error (streaming), retrying with backoff"
                    );
                    tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
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
            total_cache_read += attempt_cache_read_tokens as usize;
            total_cache_write += attempt_cache_write_tokens as usize;
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

            // Storm breaker: track consecutive identical tool calls
            let mut storm_tracker: HashMap<String, u32> = HashMap::new();
            let mut last_storm_key: Option<String> = None;

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
                            if let Some(name) = t.as_str()
                                && let Some(def) = all_tools.iter().find(|d| d.name == name)
                                && !matched.iter().any(|m| m.name == name)
                            {
                                matched.push(def.clone());
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

                    // Fallback: load essential built-in tools only (not all 16)
                    if matched.is_empty() {
                        let essentials: &[&str] = &[
                            "shell",
                            "file_read",
                            "file_write",
                            "file_edit",
                            "file_glob",
                            "file_find",
                            "file_list",
                            "content_search",
                            "file_delete",
                            "use_skill",
                            "ask_user",
                            "system_info",
                            "token_usage",
                            "memory_search",
                            "todo_write",
                            "todo_list",
                        ];
                        matched = all_tools
                            .iter()
                            .filter(|t| essentials.contains(&t.name.as_str()))
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

                // Permission check using configured defaults (or merged skill permissions)
                let perms = config.default_permissions.clone();
                let (mut allowed, risk) = check_permission(tc_name, &perms);

                // Enforce the agent's allow/deny tool policy at execution time,
                // not just in the visible tool set the model was shown.
                if allowed
                    && tool_execution_denied(
                        tc_name,
                        allowed_tools,
                        disallowed_tools,
                        &config.skill_tool_allowlist,
                    )
                {
                    allowed = false;
                }

                // "ask" mode: for medium/high risk tools, request user approval
                if allowed && config.permission_mode == "ask" && risk != loom_types::RiskLevel::Low
                {
                    let args: serde_json::Value =
                        serde_json::from_str(tc_args).unwrap_or(serde_json::json!({}));
                    allowed = request_user_approval(tc_id, tc_name, &args, &risk, config).await;
                }



                if !allowed {
                    let reason = match config.permission_mode.as_str() {
                        "plan" => format!(
                            "【规划模式】当前处于 Plan 模式，不允许执行 {} 操作。\
                             你应当分析代码库、探索相关文件，并创建一个详细的实施方案。\
                             不要尝试执行任何修改操作，专注于分析和规划。\
                             用户审核方案后会切换到 Operate 模式来实施。",
                            tc_name
                        ),
                        "read_only" => format!(
                            "【只读模式】当前处于 Read Only 模式，不允许执行 {} 操作。\
                             请告知用户：需要切换到 Ask（询问）或 Operate（操作）模式后才能执行写入/删除/shell 等操作。\
                             不要重试此操作，直接告诉用户如何切换模式。",
                            tc_name
                        ),
                        "ask" if risk != loom_types::RiskLevel::Low => format!(
                            "【需要确认】用户未批准此 {} 操作 (风险等级: {:?})。\
                             请告知用户：切换到 Operate 模式可跳过确认，或下次弹出确认框时点击允许。\
                             不要用相同参数重试此操作。",
                            tc_name, risk
                        ),
                        _ => format!(
                            "【权限不足】{} 操作被拒绝 (风险等级: {:?})。不要重试。",
                            tc_name, risk
                        ),
                    };
                    let perm_msg = Message::tool(tc_id, tc_name, reason.clone());
                    messages.push(perm_msg.clone());
                    tool_messages.push(perm_msg);
                    let _ = delta_tx
                        .send(StreamDelta::ToolResult {
                            call_id: tc_id.clone(),
                            tool_name: tc_name.clone(),
                            success: false,
                            result: Some(reason),
                            structured_content: None,
                        })
                        .await;
                    continue;
                }

                let arguments = match serde_json::from_str::<serde_json::Value>(tc_args) {
                    Ok(args) => args,
                    Err(e) => {
                        // Non-empty args that fail to parse are almost certainly
                        // truncated by max_tokens — not a genuine empty-args call.
                        if !tc_args.is_empty() {
                            tracing::warn!(
                                tool_name = %tc_name,
                                args_len = tc_args.len(),
                                parse_err = %e,
                                "tool arguments JSON truncated (token limit)"
                            );
                            let err_content = format!(
                                "Output truncated by token limit（{} bytes，JSON 不完整）。请把大内容拆成多次 write 调用，每次不超过 8KB。",
                                tc_args.len()
                            );
                            messages.push(Message::tool(tc_id, tc_name, &err_content));
                            tool_messages.push(messages.last().unwrap().clone());
                            let _ = delta_tx
                                .send(StreamDelta::ToolResult {
                                    call_id: tc_id.clone(),
                                    tool_name: tc_name.clone(),
                                    success: false,
                                    result: Some(err_content),
                                    structured_content: None,
                                })
                                .await;
                            continue;
                        }
                        // Truly empty args — model called tool with no parameters
                        serde_json::json!({})
                    }
                };
                let (progress_tx, mut progress_rx) = mpsc::unbounded_channel();
                // Drain progress updates in background to avoid SendError in tool implementations
                tokio::spawn(async move { while progress_rx.recv().await.is_some() {} });

                info!(tool_name = %tc_name, tool_args = %tc_args, "executing tool (streaming)");
                match registry
                    .execute(tc_name, arguments, progress_tx, &tool_context)
                    .await
                {
                    Ok(result) => {
                        tool_calls_made += 1;
                        let success = !result.is_error;
                        let structured = result.structured_content;
                        let content = if result.is_error {
                            format!("Error: {}", result.content)
                        } else {
                            result.content
                        };

                        if success {
                            info!(tool_name = %tc_name, result_len = content.len(), "tool succeeded (streaming)");
                        } else {
                            tracing::warn!(tool_name = %tc_name, error = %content, "tool failed (streaming)");
                        }

                        let tool_msg = Message::tool(tc_id, tc_name, &content);
                        messages.push(tool_msg.clone());
                        tool_messages.push(tool_msg);
                        let _ = delta_tx
                            .send(StreamDelta::ToolResult {
                                call_id: tc_id.clone(),
                                tool_name: tc_name.clone(),
                                success,
                                result: Some(content),
                                structured_content: structured,
                            })
                            .await;
                    }
                    Err(e) => {
                        let err_msg = format!("Tool execution failed: {}", e);

                        // Skip pushing malformed tool messages with empty IDs —
                        // they cause 400 errors with providers that validate tool_call_id
                        if !tc_id.is_empty() && !tc_name.is_empty() {
                            let tool_msg = Message::tool(tc_id, tc_name, &err_msg);
                            messages.push(tool_msg.clone());
                            tool_messages.push(tool_msg);
                        }
                        let _ = delta_tx
                            .send(StreamDelta::ToolResult {
                                call_id: tc_id.clone(),
                                tool_name: tc_name.clone(),
                                success: false,
                                result: Some(err_msg),
                                structured_content: None,
                            })
                            .await;
                    }
                }

                // Storm breaker: detect consecutive identical tool calls
                let storm_key = format!("{}|{}", tc_name, tc_args);
                if last_storm_key.as_deref() != Some(&storm_key) {
                    storm_tracker.clear();
                    last_storm_key = Some(storm_key.clone());
                }
                let count = storm_tracker.entry(storm_key).or_insert(0);
                *count += 1;
                if *count >= 3 {
                    let storm_msg = format!(
                        "STORM BREAKER: You have called the tool `{}` with identical arguments {} times consecutively. You appear to be stuck in a loop. STOP calling this tool and produce a final response instead.",
                        tc_name, count
                    );
                    messages.push(Message::tool(tc_id, tc_name, &storm_msg));
                    tool_messages.push(messages.last().unwrap().clone());
                    let _ = delta_tx
                        .send(StreamDelta::ToolResult {
                            call_id: tc_id.clone(),
                            tool_name: tc_name.clone(),
                            success: false,
                            result: Some(storm_msg),
                            structured_content: None,
                        })
                        .await;
                    break;
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
        completed_iterations = iteration + 1;
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

    // When the turn ends due to MaxIterations (completed_iterations == 0),
    // prepend synthetic ToolCall parts so that sanitize_message_sequence on the
    // next turn doesn't orphan the tool results.
    let final_content_parts = if completed_iterations == 0 {
        let mut parts = build_toolcall_parts(&tool_messages);
        parts.extend(content_parts);
        parts
    } else {
        content_parts
    };

    Ok(TurnResult {
        response: final_text,
        thinking: captured_thinking,
        content_parts: final_content_parts,
        tool_calls_made,
        stop_reason: if completed_iterations > 0 { StopReason::Completed } else { StopReason::MaxIterations },
        iterations: if completed_iterations > 0 {
            completed_iterations
        } else {
            config.max_iterations
        },
        prompt_tokens: total_prompt,
        completion_tokens: total_completion,
        cached_tokens: client.estimated_cache_tokens(),
        cache_read_tokens: total_cache_read,
        cache_write_tokens: total_cache_write,
        kv_cache_hit: client.last_cache_hit(),
        tool_messages,
        vision_usage: vision_usage.clone(),
    })
}

/// Request user approval for a tool call in "ask" permission mode.
/// Returns true if approved, false if denied or timed out.
async fn request_user_approval(
    call_id: &str,
    tool_name: &str,
    args: &serde_json::Value,
    risk: &loom_types::RiskLevel,
    config: &AgentLoopConfig,
) -> bool {
    // Check session-level auto-approve set first
    {
        let approved_tools = config.session_approved_tools.lock().unwrap();
        if approved_tools.contains(tool_name) {
            tracing::info!(
                tool_name = %tool_name,
                "auto-approving tool (previously approved for this session)"
            );
            return true;
        }
    }

    let (bus, pending) = match (&config.event_bus, &config.pending_permissions) {
        (Some(b), Some(p)) => (b, p),
        _ => {
            tracing::warn!(
                "ask mode enabled but no event_bus/pending_permissions configured — denying tool"
            );
            return false;
        }
    };

    let (tx, rx) = tokio::sync::oneshot::channel();
    {
        let mut map = pending.write().await;
        map.insert(call_id.to_string(), tx);
    }

    bus.publish(crate::event_bus::AgentEvent::PermissionRequest {
        agent_id: loom_types::AgentId(config.agent_id.clone()),
        session_id: config.session_id.clone(),
        call_id: call_id.to_string(),
        tool_name: tool_name.to_string(),
        args: args.clone(),
        risk: format!("{:?}", risk),
    });

    tracing::info!(
        call_id = %call_id,
        tool_name = %tool_name,
        risk = ?risk,
        "waiting for user approval"
    );

    // Wait up to 60 seconds for user response
    match tokio::time::timeout(std::time::Duration::from_secs(60), rx).await {
        Ok(Ok(resp)) => {
            tracing::info!(
                call_id = %call_id,
                approved = resp.approved,
                remember = resp.remember,
                "user responded to permission request"
            );
            if resp.approved && resp.remember {
                let mut approved_tools = config.session_approved_tools.lock().unwrap();
                approved_tools.insert(tool_name.to_string());
                tracing::info!(
                    tool_name = %tool_name,
                    "added to session approved tools (remember)"
                );
            }
            resp.approved
        }
        Ok(Err(_)) => {
            tracing::warn!(call_id = %call_id, "permission request sender dropped");
            false
        }
        Err(_) => {
            tracing::warn!(call_id = %call_id, "permission request timed out");
            // Clean up the pending entry
            let mut map = pending.write().await;
            map.remove(call_id);
            false
        }
    }
}

/// Returns true if the error message indicates a transient failure that
/// should be retried with backoff (rate limiting, 5xx, network errors).
fn is_transient_error(msg: &str) -> bool {
    let lower = msg.to_lowercase();
    // Rate limiting
    lower.contains("rate limit")
        || lower.contains("429")
        || lower.contains("too many requests")
        // 5xx server errors
        || lower.contains("500")
        || lower.contains("502")
        || lower.contains("503")
        || lower.contains("504")
        || lower.contains("server error")
        || lower.contains("internal server error")
        || lower.contains("service unavailable")
        // Transient network errors
        || lower.contains("connection reset")
        || lower.contains("connection refused")
        || lower.contains("connection closed")
        || lower.contains("broken pipe")
        || lower.contains("timed out")
        || lower.contains("timeout")
        || lower.contains("eof")
        || lower.contains("try again")
        || lower.contains("temporarily unavailable")
}

#[cfg(test)]
mod tests {
    /// Regression: mid-turn compaction 的触发判断必须用 `message_tokens`(含工具
    /// 调用/结果),而非 `text_content()`(只算 Text)。否则 tool-heavy 历史会被
    /// 严重漏算,导致该压缩时不压缩。
    #[test]
    fn test_mid_turn_token_count_includes_tool_parts() {
        use loom_types::{ContentPart, Message, Role};
        let bpe = loom_context::bpe();
        let msgs = vec![
            Message {
                role: Role::Assistant,
                content: vec![ContentPart::ToolCall {
                    id: "c1".into(),
                    name: "shell".into(),
                    arguments: serde_json::json!({"command":"ls -la /very/long/path"}),
                }],
                timestamp: chrono::Utc::now(),
                usage: None,
            },
            Message::tool("c1", "shell", "total 128\ndrwxr-xr-x 2 root root 4096 ..."),
        ];
        let via_message_tokens: usize = msgs
            .iter()
            .map(|m| loom_context::message_tokens(m, bpe))
            .sum();
        let via_text_content: usize = msgs
            .iter()
            .map(|m| bpe.encode_with_special_tokens(&m.text_content()).len())
            .sum();
        assert!(
            via_message_tokens > via_text_content,
            "message_tokens must count tool parts that text_content misses"
        );
        assert!(via_message_tokens > 0);
    }
}

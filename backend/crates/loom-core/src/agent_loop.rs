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
use loom_types::StopReason;
use loom_types::{
    CompactionConfig, CompletionRequest, CompletionResponse, ContentPart, Message, Role,
    StreamDelta, ToolDefinition,
};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::sync::mpsc;
use tracing::debug;
use tracing::info;

use crate::event_bus::EventBus;
use crate::tool_context::ToolContext;
use crate::tool_registry::ToolRegistry;

/// Maximum number of in-turn continuations when the provider truncates output
/// at the token ceiling (finish_reason == "length"). Keeps the reply seamless
/// (like Codex) while bounding runaway continuation loops. Beyond this the turn
/// ends with StopReason::Length so the orchestrator can fall back to auto-continue.
const MAX_TRUNCATION_CONTINUATIONS: usize = 5;

/// Checkpoint / progress tracking for long-running agent turns.
/// Recorded each iteration so auto-continue can tell the LLM what's been done.
#[derive(Debug, Clone, Default)]
pub struct ProgressCheckpoint {
    pub completed_steps: Vec<String>,
    pub tool_calls_executed: usize,
    pub files_touched: Vec<String>,
}

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
    /// Context window size used for this turn (from config.effective_context_window()).
    pub context_window: usize,
    /// Intermediate tool-call and tool-result messages for persistence.
    pub tool_messages: Vec<Message>,
    /// Token usage from auxiliary models (vision, etc.) for separate cost tracking.
    pub vision_usage: Option<crate::vision::VisionUsage>,
    /// Why the turn stopped — used by frontend to show/hide Continue button.
    pub stop_reason: StopReason,
    pub progress: ProgressCheckpoint,
}

/// Configuration for the agent loop.
#[derive(Clone)]
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
    pub pending_permissions: Option<
        Arc<RwLock<HashMap<String, tokio::sync::oneshot::Sender<loom_types::PermissionResponse>>>>,
    >,
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
    pub steering_queue: Option<Arc<RwLock<Vec<crate::event_bus::SteeringItem>>>>,
    /// Few-shot examples injected as additional system messages after the
    /// stable prefix but before dynamic_context. Each entry becomes a separate
    /// System message. Empty vec (default) disables injection.
    pub few_shots: Vec<String>,
    pub progress_checkpoint: Option<ProgressCheckpoint>,
}

impl AgentLoopConfig {
    /// 是否对该轮活跃模型启用精简模式（从 active model config 读 compact_mode）。
    /// 不写死 backend：任何模型勾选 compact_mode 都生效。无活跃模型时返回 false。
    pub fn compact_mode(&self) -> bool {
        self.model_configs
            .iter()
            .find(|c| Some(c.name.as_str()) == self.active_model_name.as_deref())
            .map(|c| c.compact_mode)
            .unwrap_or(false)
    }

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

    /// Pick the best tokenizer for the active model based on model name and backend.
    pub fn tokenizer_for_active_model(&self) -> loom_context::TokenizerId {
        let backend = self
            .model_configs
            .iter()
            .find(|c| Some(c.name.as_str()) == self.active_model_name.as_deref())
            .map(|c| c.backend.clone())
            .unwrap_or_default();
        let model_name = self.active_model_name.as_deref().unwrap_or("");
        loom_context::tokenizer_for_model(model_name, backend)
    }
}

/// Default system prompt that ships with openLoom.
/// This is the content written to `~/.loom/Loom.md` on first startup,
/// allowing users to discover and customise the agent's behaviour.
pub const DEFAULT_SYSTEM_PROMPT: &str = concat!(
    "你是 openLoom，一个运行在本地系统上的 AI 编程助手。你有真实的文件系统访问、Shell 执行、代码分析和网络搜索能力。\n",
    "\n",
    "## 身份\n",
    "- 你是一名资深软件工程师——思维缜密，行动果断。\n",
    "- 直接、简洁、有温度。用代码和事实说话。\n",
    "- 有主见：当你有更好方案时，说出来。\n",
    "\n",
    "## 工作流（读 → 改 → 查 → 完成）\n",
    "每次代码修改遵循标准流程，不要绕圈子：\n",
    "1. file_read 读取目标文件\n",
    "2. file_edit （或 file_write）直接修改\n",
    "3. 用 ```diff 展示变更\n",
    "4. 结束。不需要反复验证、不需要多余的总结。\n",
    "\n",
    "## 文件编辑规则\n",
    "- 编辑前先 file_read。不要凭记忆或猜测。\n",
    "- 用 file_edit 做精确替换。old_string 必须在文件中唯一匹配（含缩进和换行）。\n",
    "- 匹配失败时：扩大 old_string 上下文使其唯一，不要放弃去写文字方案。\n",
    "- 不要用 shell 写文件（echo/cat >）。\n",
    "- 新文件用 file_write；已有文件用 file_edit。\n",
    "- 写入大文件：内容超过约 8000 字符时分块——先 file_write 第一部分，再用 file_write(append=true) 逐块追加，避免单次输出超限被截断。\n",
    "- 每次只改和任务相关的代码。\n",
    "\n",
    "## Shell 规则\n",
    "- 用 && 或 ; 把多条命令合并。不要每个文件单独开一个 shell。\n",
    "- 注意输出长度——大文件用 head/tail/wc 控制。\n",
    "- 长时间命令用 process_spawn 后台跑。\n",
    "- 不做危险操作（rm -rf /、格盘、改系统配置），除非用户明确要求。\n",
    "\n",
    "## 搜索策略\n",
    "- 按文件名：file_glob（**/*.rs）或 file_find（按名称）\n",
    "- 按内容：content_search（底层 rg，支持正则）\n",
    "- 查最新信息：web_search\n",
    "- 先小范围试再扩大，避免海量结果。\n",
    "\n",
    "## 工具调用原则\n",
    "- 直接调用工具——工具定义已在请求中，无需任何前置步骤。\n",
    "- 失败时读错误信息理解原因，调整参数重试 1-2 次。不要同参数无限重试。\n",
    "- 连续 3 次失败后告知用户并征求意见。\n",
    "- 子代理（spawn_agent）：多个独立文件可并行分析和修改。prompt 要具体明确。\n",
    "- 复杂任务用 todo_write 跟踪进度。\n",
    "\n",
    "## 响应格式\n",
    "- 修改代码后用 ```diff 展示变更，标注文件路径。\n",
    "- 代码块标注语言（```rust、```python、```ts 等）。\n",
    "- 结构化输出优于大段文字。文件名用反引号包裹。\n",
    "\n",
    "## 限制\n",
    "- 每轮最多 100 次迭代。合理规划。\n",
    "- 工具输出可能被截断，关键部分用 file_read 补读。\n",
    "- 遵守权限模式。被拒绝的操作不要换方式重试。\n",
    "- 不确定的事明确告知，不编造。\n",
);

impl Default for AgentLoopConfig {
    fn default() -> Self {
        Self {
            system_prompt: DEFAULT_SYSTEM_PROMPT.to_string(),
            max_iterations: 100,
            max_tokens: 4096,
            temperature: 0.0,
            lazy_tools: false,
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
            few_shots: vec![
                // Canonical example: read → edit → diff → done
                concat!(
                    "<example>\n",
                    "用户: 把 src/main.rs 里的端口从 8080 改成 3000\n",
                    "\n",
                    "助手:\n",
                    "1. file_read(file_path=\"src/main.rs\") → 看到第12行 `const PORT: u16 = 8080;`\n",
                    "2. file_edit(path=\"src/main.rs\", edits=[{old_string:\"const PORT: u16 = 8080;\", new_string:\"const PORT: u16 = 3000;\"}])\n",
                    "3. 展示 diff:\n",
                    "```diff\n",
                    "-const PORT: u16 = 8080;\n",
                    "+const PORT: u16 = 3000;\n",
                    "```\n",
                    "4. 完成。不需要再读一遍文件验证——工具已确认写入成功。\n",
                    "</example>",
                ).to_string(),
            ],
            progress_checkpoint: None,
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
        "memory_remember",
        "todo_write",
        "todo_list",
        "process_spawn",
        "process_wait",
        "process_peek",
        "process_kill",
        "monitor",
        "monitor_wait",
        "monitor_peek",
        "monitor_list",
        "monitor_kill",
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
            if tool_run.is_empty() || tool_run.iter().all(|idx| orphaned_tool.contains(idx)) {
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
    if tool_name == "request_tools" || tool_name == "todo_write" || tool_name == "todo_list" {
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
            if let ContentPart::ToolResult {
                tool_call_id, name, ..
            } = part
            {
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

/// Build a recovery instruction for a tool call whose argument JSON could not be
/// parsed. When `truncated` is true the cause is the output token ceiling cutting
/// the call off mid-arguments — recoverable by chunking the work (file_write has
/// an `append` flag exactly for this). Otherwise it's a genuine malformed call.
fn tool_args_recovery_guidance(tool_name: &str, truncated: bool) -> String {
    if !truncated {
        return format!(
            "工具 `{}` 的参数不是合法 JSON，无法解析。请检查参数格式后重新调用。",
            tool_name
        );
    }
    match tool_name {
        "file_write" | "write_file" => {
            "你的 file_write 调用因输出达到长度上限被截断——文件内容太大，无法一次发送。\n\
             请分块写入（不要重复已写入的部分）：\n\
             1. 先 file_write(path, <第一部分，约 8000 字符以内>)\n\
             2. 再用 file_write(path, <下一部分>, append=true) 逐块追加，直到文件完整\n\
             每次只发送一部分内容，避免再次被截断。"
                .to_string()
        }
        "file_edit" => {
            "你的 file_edit 调用因输出达到长度上限被截断。\n\
             请拆分编辑：每次只提交一部分 edits（或缩短 newText），分多次调用完成。"
                .to_string()
        }
        _ => format!(
            "你的 `{}` 调用的参数在输出长度上限处被截断，无法解析。\n\
             请用更短的参数重新调用，或把操作拆分成多次调用。",
            tool_name
        ),
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

// ── Slim: window-aware graduated context slimming ──────────────────────────

/// Per-turn slim plan: the tier plus whether tools ride the text protocol
/// (compact catalog in the system prefix, no JSON schemas in the request —
/// inline-JSON calls are still parsed and executed server-side).
#[derive(Debug, Clone, Copy)]
struct SlimPlan {
    level: crate::slim::SlimLevel,
    text_tool_protocol: bool,
}

fn plan_slim(config: &AgentLoopConfig) -> SlimPlan {
    let level = crate::slim::slim_level(config.compact_mode(), config.effective_context_window());
    let function_calling = config
        .model_configs
        .iter()
        .find(|c| Some(c.name.as_str()) == config.active_model_name.as_deref())
        .map(|c| c.capabilities.function_calling)
        .unwrap_or(false);
    SlimPlan {
        level,
        text_tool_protocol: level == crate::slim::SlimLevel::Slim && !function_calling,
    }
}

/// Tier-conditional filtering/minification of the tool list. This list is the
/// EXECUTION set — inline calls dispatch against it even when the request
/// carries no schemas (text-protocol mode).
fn prepare_tools(
    registry: &ToolRegistry,
    config: &AgentLoopConfig,
    allowed_tools: &Option<Vec<String>>,
    disallowed_tools: &Option<Vec<String>>,
    plan: &SlimPlan,
) -> Vec<loom_types::ToolDefinition> {
    let mut tools = registry.filtered_definitions(allowed_tools, disallowed_tools);
    // If active skills restrict tools, filter to the union of their allowed_tools.
    if let Some(ref allowlist) = config.skill_tool_allowlist {
        let allowed_set: HashSet<&str> = allowlist.iter().map(|s| s.as_str()).collect();
        tools.retain(|t| allowed_set.contains(t.name.as_str()));
    }
    match plan.level {
        crate::slim::SlimLevel::Full => {}
        crate::slim::SlimLevel::Slim => {
            tools.retain(|t| crate::slim::CORE_SLIM_TOOLS.contains(&t.name.as_str()));
            tools = tools.iter().map(crate::slim::minify_tool_definition).collect();
        }
        // Tiny（含手动 compact_mode）：不挂工具（省掉全套工具 schema）
        crate::slim::SlimLevel::Tiny => tools.clear(),
    }
    tools
}

/// Slim-aware persona for both assembly and prefix-digest computation —
/// the digest must hash what the prefix actually contains.
fn slim_persona(config: &AgentLoopConfig, level: crate::slim::SlimLevel) -> Option<String> {
    match level {
        crate::slim::SlimLevel::Full => config.persona.clone(),
        crate::slim::SlimLevel::Slim => config
            .persona
            .as_ref()
            .map(|p| crate::slim::truncate_chars(p, 500)),
        crate::slim::SlimLevel::Tiny => None,
    }
}

/// Text-protocol catalog for both assembly and prefix-digest computation.
fn slim_tool_catalog(plan: &SlimPlan, tools: &[loom_types::ToolDefinition]) -> Option<String> {
    if plan.text_tool_protocol && !tools.is_empty() {
        Some(crate::slim::build_text_tool_catalog(tools))
    } else {
        None
    }
}

fn system_msg(text: String) -> Message {
    Message {
        role: Role::System,
        content: vec![ContentPart::Text { text }],
        timestamp: chrono::Utc::now(),
        usage: None,
    }
}

/// Assemble the turn's messages: stable prefix (assembler) + tier-aware
/// injections (few-shots / dynamic context / todo / continuation) placed
/// after index 0 with a running cursor — immune to level-conditional gaps
/// that the old per-block insert_pos arithmetic was fragile against.
fn assemble_turn_messages(
    config: &AgentLoopConfig,
    history: &[Message],
    plan: &SlimPlan,
    tools: &[loom_types::ToolDefinition],
) -> Result<Vec<Message>> {
    let cw = config.effective_context_window();
    let history_budget = (cw as f32 * 0.25) as usize;
    let assembler = ContextAssembler::new(&config.system_prompt, history_budget);
    let opts = AssembleOptions {
        persona: slim_persona(config, plan.level),
        summary: config.summary.clone(),
        kg_context: config.kg_context.clone(),
        tool_catalog: slim_tool_catalog(plan, tools),
        history: history.to_vec(),
        summary_at_count: config.summary_at_count,
    };
    let mut messages = assembler.build(opts)?;

    let mut pos = 1usize;
    // Few-shot examples — Full only (they are the most expendable).
    if plan.level == crate::slim::SlimLevel::Full {
        for shot in &config.few_shots {
            messages.insert(pos, system_msg(shot.clone()));
            pos += 1;
        }
    }
    // Dynamic context (skills, KG, workspace) — Full intact, Slim truncated,
    // Tiny dropped. Kept out of the stable prefix for KV-cache stability.
    if let Some(ref dc) = config.dynamic_context
        && !dc.is_empty()
    {
        let text = match plan.level {
            crate::slim::SlimLevel::Full => dc.clone(),
            crate::slim::SlimLevel::Slim => crate::slim::truncate_chars(dc, 1500),
            crate::slim::SlimLevel::Tiny => String::new(),
        };
        if !text.is_empty() {
            messages.insert(pos, system_msg(text));
            pos += 1;
        }
    }
    // Todo context — small and steers continuation; kept except Tiny.
    if let Some(ref tc) = config.todo_context
        && !tc.is_empty()
        && plan.level != crate::slim::SlimLevel::Tiny
    {
        messages.insert(pos, system_msg(tc.clone()));
        pos += 1;
    }
    // Continuation note — always kept (short, semantically vital).
    if let Some(ref note) = config.continuation_note
        && !note.is_empty()
    {
        messages.insert(pos, system_msg(note.clone()));
    }
    Ok(messages)
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
    let plan = plan_slim(config);
    let tools = prepare_tools(registry, config, allowed_tools, disallowed_tools, &plan);
    let cw = config.effective_context_window();
    let mut messages = assemble_turn_messages(config, history, &plan, &tools)?;
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
            let vision_cfg = match config.loom_dir.as_deref() {
                Some(dir) => crate::vision::load_vision_config_from(dir),
                None => crate::vision::load_vision_config(),
            };
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
                            messages.insert(
                                messages.len().saturating_sub(1),
                                Message {
                                    role: loom_types::Role::System,
                                    content: vec![ContentPart::Text {
                                        text: vresult.context,
                                    }],
                                    timestamp: chrono::Utc::now(),
                                    usage: None,
                                },
                            );
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
    // In-turn truncation continuations (see the streaming path for the full story).
    let mut continuations = 0usize;
    let mut accumulated_text = String::new();

    // Create tool context with workspace path for file operations
    let tool_context = ToolContext {
        workspace_path: config.workspace_path.clone(),
        sandbox: config.sandbox.clone(),
        recently_read: Arc::new(std::sync::Mutex::new(HashMap::new())),
        session_id: Some(config.session_id.clone()),
        todo_store: config.todo_store.clone(),
        event_bus: config.event_bus.clone(),
        cancel_token: Some(cancel.clone()),
    };

    // ── Prefix digest: compute SHA256 fingerprint of stable prefix ──
    let digest = {
        let digest_assembler =
            ContextAssembler::new(&config.system_prompt, (cw as f32 * 0.25) as usize);
        digest_assembler.compute_prefix_digest(&AssembleOptions {
            persona: slim_persona(config, plan.level),
            summary: config.summary.clone(),
            kg_context: config.kg_context.clone(),
            tool_catalog: slim_tool_catalog(&plan, &tools),
            history: vec![],
            summary_at_count: 0,
        })
    };
    client.set_prefix_digest(Some(digest.clone()));
    tracing::info!(
        prefix_hash = %&digest.combined_hash[..12],
        prefix_tokens = digest.prefix_token_count,
        "prefix digest computed — estimated cache savings: {} tokens",
        digest.prefix_token_count,
    );

    let mut safety_truncation_count: u32 = 0;

    for iteration in 0..config.max_iterations {
        // Steering queue drain DISABLED: queued items are now held until the turn
        // ends, then auto-sent as a new user message by the frontend
        // (autoSendPendingSteering in stream-buffer.ts). Previously items were
        // injected as System messages each iteration, which caused
        // them to disappear from the queue panel mid-turn instead of being
        // delivered as a follow-up message after the turn completes.
        let _steering_consumed: Vec<crate::event_bus::SteeringItem> = Vec::new();
        // Token budget check: stop if CURRENT window tokens exceed the budget
        // (was cumulative total_prompt, which falsely tripped after N iterations).
        let current_window_tokens: usize = if config.max_prompt_budget > 0 {
            let tid = config.tokenizer_for_active_model();
            messages
                .iter()
                .map(|m| loom_context::message_tokens_with_id(m, tid))
                .sum()
        } else {
            0
        };
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
                progress: ProgressCheckpoint::default(),
                context_window: config.effective_context_window(),
                stop_reason: StopReason::BudgetExhausted,
            });
        }
        // Mid-turn safety: check token usage against context window ceiling.
        // When compaction is enabled, try truncation first. When disabled, stop
        // immediately if tokens exceed the ceiling to avoid LLM HTTP 400 errors.
        if !messages.is_empty() {
            let tid = config.tokenizer_for_active_model();
            let total_tokens: usize = messages
                .iter()
                .map(|m| loom_context::message_tokens_with_id(m, tid))
                .sum();
            let cw = config
                .effective_context_window()
                .max(config.max_prompt_budget);
            let ceiling = (cw as f32 * 0.9) as usize;
            if total_tokens > ceiling {
                if config.compaction_config.enabled {
                    safety_truncation_count += 1;
                    let before = total_tokens;
                    // LLM semantic compression first
                    if config.compaction_config.use_llm_summarization {
                        llm_compress_large_outputs(
                            &mut messages,
                            &config.compaction_config,
                            client,
                        )
                        .await;
                    }
                    // Character truncation as safety net
                    messages = loom_context::mid_turn_safety_truncate(
                        &messages,
                        config.compaction_config.max_tool_output_chars,
                    );
                    let after: usize = messages
                        .iter()
                        .map(|m| loom_context::message_tokens_with_id(m, tid))
                        .sum();
                    tracing::info!(
                        iteration,
                        before,
                        after,
                        count = safety_truncation_count,
                        "mid-turn safety truncation applied"
                    );
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
                            progress: ProgressCheckpoint::default(),
                            context_window: config.effective_context_window(),
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
                        progress: ProgressCheckpoint::default(),
                        context_window: config.effective_context_window(),
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
                    parts.push(ContentPart::Text {
                        text: "[已中断]".into(),
                    });
                    parts
                },
                tool_messages,
                vision_usage: None,
                progress: ProgressCheckpoint::default(),
                context_window: config.effective_context_window(),
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

        // Pre-flight window guard: count the assembled prompt with the real
        // tokenizer and trim (history → injected dynamic context → oversized
        // tool outputs) instead of dying on a provider-side context-overflow.
        let preflight = crate::slim::preflight_trim(
            &mut messages,
            if plan.text_tool_protocol { &[] } else { &tools },
            cw,
            config.max_tokens,
            config.tokenizer_for_active_model(),
        );
        if !preflight.is_clean() {
            sanitize_message_sequence(&mut messages);
            tracing::info!(?preflight, "pre-flight prompt trim applied");
        }

        let mut response = loop {
            let effective_tools = if strip_tools_for_images || force_no_tools || plan.text_tool_protocol {
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

                let (progress_tx, mut progress_rx) =
                    mpsc::unbounded_channel::<loom_types::ToolProgress>();
                // Forward progress updates to EventBus as ToolOutput events for real-time display
                {
                    let eb = tool_context.event_bus.clone();
                    let aid = config.agent_id.clone();
                    let cid = tc.id.clone();
                    let tn = tc.name.clone();
                    let sid = config.session_id.clone();
                    tokio::spawn(async move {
                        while let Some(p) = progress_rx.recv().await {
                            if let Some(ref bus) = eb {
                                bus.publish(crate::event_bus::AgentEvent::ToolOutput {
                                    agent_id: loom_types::AgentId(aid.clone()),
                                    call_id: cid.clone(),
                                    tool_name: tn.clone(),
                                    line: p.message.clone(),
                                    stream: "stdout".to_string(),
                                    session_id: sid.clone(),
                                });
                            }
                        }
                    });
                }

                // Wrap tool execution with cancel check — if user clicks stop
                // while a tool is running, we break out immediately.
                let tc_name_owned = tc.name.clone();
                let tool_exec_result = tokio::select! {
                    result = registry.execute(&tc.name, tc.arguments.clone(), progress_tx, &tool_context) => result,
                    _ = cancel.cancelled() => {
                        tracing::info!("tool execution cancelled by user: {}", tc_name_owned);
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
                            progress: ProgressCheckpoint::default(),
                            context_window: config.effective_context_window(),
                            stop_reason: StopReason::UserCancelled,
                        });
                    }
                };
                match tool_exec_result {
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
                let storm_key = format!("{}|{}", tool_name, tc.arguments);
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

        // No tool calls — this is the final text response.
        // Truncation continuation: the model hit its output ceiling mid-reply.
        // Feed the partial text back and continue in-turn (bounded), mirroring
        // the streaming path so sub-agent replies aren't silently cut off.
        if response.truncated
            && !response.text.is_empty()
            && continuations < MAX_TRUNCATION_CONTINUATIONS
        {
            continuations += 1;
            tracing::info!(
                iteration,
                continuations,
                chars = response.text.len(),
                "output truncated at token ceiling — continuing in-turn (non-streaming)"
            );
            accumulated_text.push_str(&response.text);
            messages.push(Message {
                role: Role::Assistant,
                content: vec![ContentPart::Text {
                    text: response.text.clone(),
                }],
                timestamp: chrono::Utc::now(),
                usage: None,
            });
            messages.push(Message {
                role: Role::System,
                content: vec![ContentPart::Text {
                    text: "你上一条回复因输出长度限制被截断。请从截断处无缝继续，直接输出后续内容，不要重复已输出的部分，不要添加任何说明、道歉或前缀。".into(),
                }],
                timestamp: chrono::Utc::now(),
                usage: None,
            });
            continue;
        }

        let mut full_text = accumulated_text.clone();
        full_text.push_str(&response.text);
        let response_text = if full_text.is_empty() {
            "[no response]".to_string()
        } else {
            full_text
        };
        let ended_truncated = response.truncated;
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
            progress: ProgressCheckpoint::default(),
            context_window: config.effective_context_window(),
            stop_reason: if ended_truncated {
                StopReason::Length
            } else {
                StopReason::Completed
            },
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
        progress: ProgressCheckpoint::default(),
        context_window: config.effective_context_window(),
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
    let plan = plan_slim(config);
    let all_tools = prepare_tools(registry, config, allowed_tools, disallowed_tools, &plan);
    let mut tools = if config.lazy_tools && !plan.text_tool_protocol {
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
    let cw = config.effective_context_window();
    let mut messages = assemble_turn_messages(config, history, &plan, &tools)?;
    // 小窗口提示（一次性，走 thinking 通道不污染正文）
    if plan.level != crate::slim::SlimLevel::Full {
        let note = match plan.level {
            crate::slim::SlimLevel::Slim if plan.text_tool_protocol => {
                format!("[小窗口模式：已精简上下文，{} 个核心工具以文本协议提供]", tools.len())
            }
            crate::slim::SlimLevel::Slim => {
                format!("[小窗口模式：已精简上下文，保留 {} 个核心工具]", tools.len())
            }
            _ => "[精简模式：仅保留核心对话能力]".to_string(),
        };
        let _ = delta_tx.send(StreamDelta::Reasoning(note)).await;
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
            let vision_cfg = match config.loom_dir.as_deref() {
                Some(dir) => crate::vision::load_vision_config_from(dir),
                None => crate::vision::load_vision_config(),
            };
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
                            messages.insert(
                                messages.len().saturating_sub(1),
                                Message {
                                    role: loom_types::Role::System,
                                    content: vec![ContentPart::Text {
                                        text: vresult.context,
                                    }],
                                    timestamp: chrono::Utc::now(),
                                    usage: None,
                                },
                            );
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
    let mut progress = ProgressCheckpoint::default();
    let mut content_parts: Vec<ContentPart> = Vec::new();
    let mut captured_thinking = String::new();
    let mut captured_images: Vec<(String, String)> = Vec::new();
    let mut completed_iterations = 0usize;
    // In-turn truncation continuations: when the provider hits its output token
    // ceiling we feed the partial reply back and continue on the same stream.
    let mut continuations = 0usize;
    let mut ended_truncated = false;

    // Create tool context with workspace path for file operations
    let tool_context = ToolContext {
        workspace_path: config.workspace_path.clone(),
        sandbox: config.sandbox.clone(),
        recently_read: Arc::new(std::sync::Mutex::new(HashMap::new())),
        session_id: Some(config.session_id.clone()),
        todo_store: config.todo_store.clone(),
        event_bus: config.event_bus.clone(),
        cancel_token: Some(cancel.clone()),
    };

    // ── Prefix digest (streaming): compute SHA256 fingerprint of stable prefix ──
    let digest = {
        let digest_assembler =
            ContextAssembler::new(&config.system_prompt, (cw as f32 * 0.25) as usize);
        digest_assembler.compute_prefix_digest(&AssembleOptions {
            persona: slim_persona(config, plan.level),
            summary: config.summary.clone(),
            kg_context: config.kg_context.clone(),
            tool_catalog: slim_tool_catalog(&plan, &tools),
            history: vec![],
            summary_at_count: 0,
        })
    };
    client.set_prefix_digest(Some(digest.clone()));
    tracing::info!(
        prefix_hash = %&digest.combined_hash[..12],
        prefix_tokens = digest.prefix_token_count,
        "prefix digest computed (streaming) — estimated cache savings: {} tokens",
        digest.prefix_token_count,
    );

    let mut safety_truncation_count: u32 = 0;

    for iteration in 0..config.max_iterations {
        // Steering queue drain DISABLED: queued items are now held until the turn
        // ends, then auto-sent as a new user message by the frontend
        // (autoSendPendingSteering in stream-buffer.ts). Previously items were
        // injected as System messages each iteration, which caused
        // them to disappear from the queue panel mid-turn instead of being
        // delivered as a follow-up message after the turn completes.
        let _steering_consumed: Vec<crate::event_bus::SteeringItem> = Vec::new();
        // Token budget check: stop if CURRENT window tokens exceed the budget
        // (was cumulative total_prompt, which falsely tripped after N iterations).
        let current_window_tokens: usize = if config.max_prompt_budget > 0 {
            let tid = config.tokenizer_for_active_model();
            messages
                .iter()
                .map(|m| loom_context::message_tokens_with_id(m, tid))
                .sum()
        } else {
            0
        };
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
                progress: ProgressCheckpoint::default(),
                context_window: config.effective_context_window(),
                stop_reason: StopReason::BudgetExhausted,
            });
        }
        // Mid-turn safety: check token usage against context window ceiling (streaming).
        if !messages.is_empty() {
            let tid = config.tokenizer_for_active_model();
            let total_tokens: usize = messages
                .iter()
                .map(|m| loom_context::message_tokens_with_id(m, tid))
                .sum();
            let cw = config
                .effective_context_window()
                .max(config.max_prompt_budget);
            let ceiling = (cw as f32 * 0.9) as usize;
            if total_tokens > ceiling {
                if config.compaction_config.enabled {
                    safety_truncation_count += 1;
                    let before = total_tokens;
                    messages = loom_context::mid_turn_safety_truncate(
                        &messages,
                        config.compaction_config.max_tool_output_chars,
                    );
                    let after: usize = messages
                        .iter()
                        .map(|m| loom_context::message_tokens_with_id(m, tid))
                        .sum();
                    tracing::info!(
                        iteration,
                        before,
                        after,
                        count = safety_truncation_count,
                        "mid-turn safety truncation applied (streaming)"
                    );
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
                                    text: "任务进行中（已达上下文上限）。输入「继续」以接着执行。"
                                        .into(),
                                });
                                parts
                            },
                            tool_messages,
                            vision_usage: None,
                            progress: ProgressCheckpoint::default(),
                            context_window: config.effective_context_window(),
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
                                text: "任务进行中（已达上下文上限）。输入「继续」以接着执行。"
                                    .into(),
                            });
                            parts
                        },
                        tool_messages,
                        vision_usage: None,
                        progress: ProgressCheckpoint::default(),
                        context_window: config.effective_context_window(),
                        stop_reason: StopReason::BudgetExhausted,
                    });
                }
            }
        }
        // Check for user interruption before each iteration
        if cancel.is_cancelled() {
            tracing::info!("agent turn cancelled by user at iteration {}", iteration);
            let interrupted = "[已中断]";
            let partial = &final_text;
            let response = if partial.is_empty() {
                interrupted.to_string()
            } else {
                format!("{}\n\n{}", partial, interrupted)
            };
            let _ = delta_tx.send(StreamDelta::Text(response.clone())).await;
            drop(delta_tx);
            return Ok(TurnResult {
                response,
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
                    if !final_text.is_empty() {
                        parts.push(ContentPart::Text { text: final_text });
                    }
                    parts.push(ContentPart::Text {
                        text: interrupted.into(),
                    });
                    parts
                },
                tool_messages,
                vision_usage: None,
                progress: ProgressCheckpoint::default(),
                context_window: config.effective_context_window(),
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

        // Pre-flight window guard: count the assembled prompt with the real
        // tokenizer and trim (history → injected dynamic context → oversized
        // tool outputs) instead of dying on a provider-side context-overflow.
        let preflight = crate::slim::preflight_trim(
            &mut messages,
            if plan.text_tool_protocol { &[] } else { &tools },
            cw,
            config.max_tokens,
            config.tokenizer_for_active_model(),
        );
        if !preflight.is_clean() {
            sanitize_message_sequence(&mut messages);
            tracing::info!(?preflight, "pre-flight prompt trim applied");
            let _ = delta_tx
                .send(StreamDelta::Reasoning(format!(
                    "[上下文预检：{}]",
                    preflight.describe()
                )))
                .await;
        }

        let mut pending_tool_calls: Vec<(usize, String, String, String)> = Vec::new();
        let mut this_text = String::new();
        let mut this_thinking = String::new();
        // Set by StreamDelta::Finish when the provider hit the output token ceiling.
        let mut this_truncated = false;
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
            let effective_tools = if strip_tools_for_images || force_no_tools || plan.text_tool_protocol {
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
                        // Keep the partial text that was already received; append
                        // the interruption marker so the user sees what was
                        // generated before stopping and the next turn has
                        // continuous context.
                        let interrupted = "[已中断]";
                        let response = if attempt_text.is_empty() { interrupted.to_string() } else { format!("{}\n\n{}", attempt_text, interrupted) };
                        let _ = delta_tx.send(StreamDelta::Text(response.clone())).await;
                        drop(delta_tx);
                        return Ok(TurnResult {
                            response,
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
                                if !attempt_text.is_empty() {
                                    parts.push(ContentPart::Text { text: attempt_text });
                                }
                                parts.push(ContentPart::Text { text: interrupted.into() });
                                parts
                            },
                            tool_messages,
                            vision_usage: None,
                            progress: ProgressCheckpoint::default(),
                context_window: config.effective_context_window(),
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
                            StreamDelta::Finish { truncated } => {
                                // Terminal truncation signal — consumed here, NOT
                                // forwarded to the frontend; drives in-turn continue.
                                this_truncated = truncated;
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
                                            StreamDelta::Finish { truncated } => {
                                                this_truncated = truncated;
                                            }
                                            // Tool-call deltas can also race into the drain
                                            // window (observed: the final args chunk arriving
                                            // after stream_fut completed, dropping the closing
                                            // `}` and corrupting the JSON). Accumulate them
                                            // exactly like the main loop does.
                                            StreamDelta::ToolCallBegin { index, id, name } => {
                                                attempt_pending.push((
                                                    index,
                                                    id.clone(),
                                                    name.clone(),
                                                    String::new(),
                                                ));
                                                let _ = delta_tx
                                                    .send(StreamDelta::ToolCallBegin {
                                                        index,
                                                        id,
                                                        name,
                                                    })
                                                    .await;
                                            }
                                            StreamDelta::ToolCallArgsChunk { index, chunk } => {
                                                if let Some(tc) = attempt_pending
                                                    .iter_mut()
                                                    .find(|(i, _, _, _)| *i == index)
                                                {
                                                    tc.3.push_str(&chunk);
                                                }
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
                            "memory_remember",
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
                                truncated = this_truncated,
                                "tool arguments JSON unparseable (truncation or malformed)"
                            );
                            let err_content = tool_args_recovery_guidance(tc_name, this_truncated);
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
                let (progress_tx, mut progress_rx) =
                    mpsc::unbounded_channel::<loom_types::ToolProgress>();
                // Forward progress updates to EventBus as ToolOutput events for real-time display
                {
                    let eb = tool_context.event_bus.clone();
                    let aid = config.agent_id.clone();
                    let cid = tc_id.clone();
                    let tn = tc_name.clone();
                    let sid = config.session_id.clone();
                    tokio::spawn(async move {
                        while let Some(p) = progress_rx.recv().await {
                            if let Some(ref bus) = eb {
                                bus.publish(crate::event_bus::AgentEvent::ToolOutput {
                                    agent_id: loom_types::AgentId(aid.clone()),
                                    call_id: cid.clone(),
                                    tool_name: tn.clone(),
                                    line: p.message.clone(),
                                    stream: "stdout".to_string(),
                                    session_id: sid.clone(),
                                });
                            }
                        }
                    });
                }

                info!(tool_name = %tc_name, tool_args = %tc_args, "executing tool (streaming)");
                // Capture file path before arguments is moved into execute
                let maybe_file_path: Option<String> =
                    if matches!(tc_name.as_str(), "file_write" | "file_edit") {
                        arguments
                            .get("file_path")
                            .or(arguments.get("path"))
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string())
                    } else {
                        None
                    };
                // Clone tc_name for use after tokio::select! (which moves it)
                let tc_name_for_log = tc_name.clone();
                // Wrap tool execution with cancel check — if user clicks stop
                // while a tool is running, we break out immediately.
                let tool_exec_result = tokio::select! {
                    result = registry.execute(tc_name, arguments, progress_tx, &tool_context) => result,
                    _ = cancel.cancelled() => {
                        tracing::info!("tool execution cancelled by user: {}", tc_name_for_log);
                        let interrupted = "[已中断]";
                        let response = if this_text.is_empty() { interrupted.to_string() } else { format!("{}\n\n{}", this_text, interrupted) };
                        let _ = delta_tx.send(StreamDelta::Text(response.clone())).await;
                        drop(delta_tx);
                        return Ok(TurnResult {
                            response,
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
                                if !this_text.is_empty() {
                                    parts.push(ContentPart::Text { text: this_text });
                                }
                                parts.push(ContentPart::Text { text: interrupted.into() });
                                parts
                            },
                            tool_messages,
                            vision_usage: None,
                            progress: ProgressCheckpoint::default(),
                            context_window: config.effective_context_window(),
                            stop_reason: StopReason::UserCancelled,
                        });
                    }
                };
                // Restore tc_name for subsequent use
                let tc_name = tc_name_for_log;
                match tool_exec_result {
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

                        let tool_msg = Message::tool(tc_id.clone(), tc_name.clone(), &content);
                        messages.push(tool_msg.clone());
                        tool_messages.push(tool_msg);

                        // Record progress for auto-continue checkpoint
                        let step_summary = content.chars().take(150).collect::<String>();
                        progress
                            .completed_steps
                            .push(format!("{} -> {}", tc_name, step_summary));
                        if progress.completed_steps.len() > 50 {
                            progress.completed_steps.remove(0);
                        }
                        progress.tool_calls_executed += 1;
                        // Track file writes/edits
                        if let Some(ref path) = maybe_file_path
                            && !progress.files_touched.contains(path) {
                                progress.files_touched.push(path.clone());
                            }

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
                            let tool_msg = Message::tool(tc_id.clone(), tc_name.clone(), &err_msg);
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
                    messages.push(Message::tool(tc_id.clone(), tc_name.clone(), &storm_msg));
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

        // ── Truncation continuation ─────────────────────────────────────────
        // The provider hit its output token ceiling mid-reply (finish_reason ==
        // "length" / stop_reason == "max_tokens"). Rather than end the turn with
        // a cut-off response, feed the partial text back and let the model keep
        // going — on the same open delta channel, so the user sees one seamless
        // stream (like Codex) instead of a silent stop.
        if this_truncated && !this_text.is_empty() && continuations < MAX_TRUNCATION_CONTINUATIONS {
            continuations += 1;
            tracing::info!(
                iteration,
                continuations,
                chars = this_text.len(),
                "output truncated at token ceiling — continuing in-turn"
            );
            final_text.push_str(&this_text);
            if !this_thinking.is_empty() {
                captured_thinking.push_str(&this_thinking);
                this_thinking.clear();
            }
            // Echo the partial assistant text so the model knows where it stopped.
            messages.push(Message {
                role: Role::Assistant,
                content: vec![ContentPart::Text {
                    text: this_text.clone(),
                }],
                timestamp: chrono::Utc::now(),
                usage: None,
            });
            messages.push(Message {
                role: Role::System,
                content: vec![ContentPart::Text {
                    text: "你上一条回复因输出长度限制被截断。请从截断处无缝继续，直接输出后续内容，不要重复已输出的部分，不要添加任何说明、道歉或前缀。".into(),
                }],
                timestamp: chrono::Utc::now(),
                usage: None,
            });
            continue;
        }

        // Normal terminal (natural end, or continuation budget exhausted).
        ended_truncated = this_truncated;
        final_text.push_str(&this_text);
        if !this_thinking.is_empty() {
            captured_thinking.push_str(&this_thinking);
        }
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
        context_window: config.effective_context_window(),
        stop_reason: if completed_iterations > 0 {
            if ended_truncated {
                StopReason::Length
            } else {
                StopReason::Completed
            }
        } else {
            StopReason::MaxIterations
        },
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
        progress,
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

/// LLM semantic compression for large tool outputs.
/// Falls back to None on failure — caller should apply char-level truncation.
pub(crate) async fn llm_compress_tool_output(
    tool_name: &str,
    output: &str,
    cfg: &CompactionConfig,
    client: &dyn CloudClient,
) -> Option<String> {
    use loom_types::CompletionRequest;
    if output.len() <= cfg.semantic_compress_min_chars || cfg.semantic_compress_min_chars == 0 {
        return None;
    }
    let prompt = format!(
        "请将以下工具 `{tool_name}` 的输出压缩到约 {target} 字符以内。\
         必须保留：所有错误消息(error/warning/failed)、文件路径、IP 地址和端口号、\
         数值结果、代码片段、JSON 结构。不要编造任何信息。\n\n原始输出:\n{output}",
        target = cfg.semantic_compress_target_chars,
    );
    let request = CompletionRequest {
        prompt,
        max_tokens: 512,
        temperature: 0.0,
        ..Default::default()
    };
    match tokio::time::timeout(
        std::time::Duration::from_millis(cfg.summarization_timeout_ms),
        client.complete(request),
    )
    .await
    {
        Ok(Ok(resp)) if !resp.text.is_empty() => {
            tracing::info!(
                tool_name,
                before = output.len(),
                after = resp.text.len(),
                "tool output semantically compressed"
            );
            Some(resp.text)
        }
        Ok(Ok(_)) => {
            tracing::warn!(tool_name, "LLM compression returned empty");
            None
        }
        Ok(Err(e)) => {
            tracing::warn!(tool_name, error = %e, "LLM compression failed");
            None
        }
        Err(_) => {
            tracing::warn!(tool_name, "LLM compression timed out");
            None
        }
    }
}

/// Iterate messages and LLM-compress oversized tool outputs. Falls back to char truncation.
async fn llm_compress_large_outputs(
    messages: &mut [Message],
    cfg: &CompactionConfig,
    client: &dyn CloudClient,
) {
    if cfg.semantic_compress_min_chars == 0 || !cfg.use_llm_summarization {
        return;
    }
    let mut compressed = 0usize;
    for msg in messages.iter_mut() {
        let is_file_read = msg
            .content
            .iter()
            .any(|p| matches!(p, ContentPart::ToolResult { name, .. } if name == "file_read"));
        if is_file_read {
            continue;
        }
        for part in &mut msg.content {
            match part {
                ContentPart::ToolResult { result, name, .. } => {
                    if result.len() <= cfg.semantic_compress_min_chars {
                        continue;
                    }
                    if let Some(c) = llm_compress_tool_output(name, result, cfg, client).await {
                        *result = c;
                        compressed += 1;
                    }
                }
                ContentPart::Text { text } => {
                    if text.len() <= cfg.semantic_compress_min_chars {
                        continue;
                    }
                    if let Some(c) = llm_compress_tool_output("unknown", text, cfg, client).await {
                        *text = c;
                        compressed += 1;
                    }
                }
                _ => {}
            }
        }
    }
    if compressed > 0 {
        tracing::info!(compressed, "LLM semantic compression applied");
    }
}

#[cfg(test)]
mod compact_mode_tests {
    use super::*;
    use loom_types::{ModelConfig, ModelBackend};

    fn cfg_with(compact: bool) -> AgentLoopConfig {
        let mc = ModelConfig {
            name: "local".into(),
            backend: ModelBackend::Ollama,
            context_size: 8192,
            compact_mode: compact,
            ..ModelConfig::default()
        };
        AgentLoopConfig {
            model_configs: vec![mc],
            active_model_name: Some("local".into()),
            ..AgentLoopConfig::default()
        }
    }

    #[test]
    fn compact_mode_reads_active_model() {
        assert!(cfg_with(true).compact_mode());
        assert!(!cfg_with(false).compact_mode());
    }

    #[test]
    fn compact_mode_false_when_no_active_model() {
        let mut c = cfg_with(true);
        c.active_model_name = None;
        assert!(!c.compact_mode());
    }
}

#[cfg(test)]
mod tests {
    /// Regression: mid-turn compaction 的触发判断必须用 `message_tokens`(含工具
    /// 调用/结果),而非 `text_content()`(只算 Text)。否则 tool-heavy 历史会被
    /// 严重漏算,导致该压缩时不压缩。
    #[test]
    fn test_mid_turn_token_count_includes_tool_parts() {
        use loom_types::{ContentPart, Message, Role};
        let tid = loom_context::TokenizerId::Cl100k;
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
            .map(|m| loom_context::message_tokens_with_id(m, tid))
            .sum();
        let via_text_content: usize = msgs
            .iter()
            .map(|m| {
                tid.get()
                    .encode_with_special_tokens(&m.text_content())
                    .len()
            })
            .sum();
        assert!(
            via_message_tokens > via_text_content,
            "message_tokens must count tool parts that text_content misses"
        );
        assert!(via_message_tokens > 0);
    }
}

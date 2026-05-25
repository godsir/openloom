use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

// === Phase 0 保留 ===

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum ModelType {
    #[default]
    Router,
    Summarizer,
    Reasoning,
}

// === Phase 0 扩展 ===

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum ModelBackend {
    #[default]
    LmStudio,
    Anthropic,
    OpenAI,
    DeepSeek,
    Ollama,
}

impl ModelBackend {
    pub fn name(&self) -> &'static str {
        match self {
            ModelBackend::LmStudio => "LmStudio",
            ModelBackend::Anthropic => "Anthropic",
            ModelBackend::OpenAI => "OpenAI",
            ModelBackend::DeepSeek => "DeepSeek",
            ModelBackend::Ollama => "Ollama",
        }
    }

    pub fn is_cloud_capable(&self) -> bool {
        !self.is_local_inference()
    }

    pub fn is_local_inference(&self) -> bool {
        matches!(self, ModelBackend::LmStudio | ModelBackend::Ollama)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub name: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub model_type: ModelType,
    #[serde(default)]
    pub backend: ModelBackend,
    pub path: Option<String>,
    #[serde(default = "default_context_size")]
    pub context_size: usize,
    #[serde(default)]
    pub max_output_tokens: Option<usize>,
    #[serde(default)]
    pub n_gpu_layers: usize,
    pub api_key_env: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
}

fn default_context_size() -> usize {
    4096
}

impl ModelConfig {
    pub fn effective_max_output(&self) -> usize {
        self.max_output_tokens.unwrap_or(self.context_size / 2)
    }
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            model: None,
            model_type: ModelType::Router,
            backend: ModelBackend::default(),
            path: None,
            context_size: default_context_size(),
            max_output_tokens: None,
            n_gpu_layers: 0,
            api_key_env: None,
            base_url: None,
        }
    }
}

// === Persona provider ===

#[async_trait::async_trait]
pub trait PersonaProvider: Send + Sync {
    async fn summarize(&self) -> anyhow::Result<String>;
    fn invalidate(&self);
}

pub struct NoopPersonaProvider;

#[async_trait::async_trait]
impl PersonaProvider for NoopPersonaProvider {
    async fn summarize(&self) -> anyhow::Result<String> {
        Ok(String::new())
    }
    fn invalidate(&self) {}
}

// === Skill Permissions (shared by skills + sandbox crates) ===

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SkillPermissions {
    #[serde(default)]
    pub fs_read: Option<Vec<String>>,
    #[serde(default)]
    pub fs_write: Option<Vec<String>>,
    #[serde(default)]
    pub network: Option<Vec<String>>,
    #[serde(default)]
    pub shell: bool,
    #[serde(default)]
    pub subprocess: bool,
}

// === Router types ===

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Intent {
    Chat,
    FileOperation,
    WebSearch,
    CodeAssist,
    Schedule,
    Question,
    Other,
}

impl std::fmt::Display for Intent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Intent::Chat => write!(f, "chat"),
            Intent::FileOperation => write!(f, "file_operation"),
            Intent::WebSearch => write!(f, "web_search"),
            Intent::CodeAssist => write!(f, "code_assist"),
            Intent::Schedule => write!(f, "schedule"),
            Intent::Question => write!(f, "question"),
            Intent::Other => write!(f, "other"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TargetModel {
    Local,
    None,
    Cloud,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassifyOutput {
    pub intent: Intent,
    pub complexity: f32,
    pub skill_match: Option<String>,
    pub confidence: f32,
    pub cache_hit: bool,
    pub target_model: TargetModel,
    #[serde(default)]
    pub route_reason: String,
}

// === Engine types ===

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    #[serde(default = "Utc::now")]
    pub timestamp: DateTime<Utc>,
    #[serde(default)]
    pub metadata: Option<String>,
    #[serde(default)]
    pub id: Option<i64>,
    #[serde(default)]
    pub seq: Option<i64>,
}

// === Native Tool Calling Types (aligned with OpenCode) ===

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

impl Role {
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::Tool => "tool",
        }
    }
}

/// Lightweight image descriptor for constructing multimodal messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImagePart {
    pub data: String,
    #[serde(rename = "mime_type")]
    pub mime_type: String,
}

/// A content part within a message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentPart {
    Text {
        text: String,
    },
    Image {
        source_type: String,
        media_type: String,
        data: String,
    },
    ToolCall {
        id: String,
        name: String,
        arguments: serde_json::Value,
    },
    ToolResult {
        tool_call_id: String,
        name: String,
        result: String,
    },
}

/// A structured message with role-separated content parts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: Vec<ContentPart>,
    #[serde(skip)]
    pub timestamp: DateTime<Utc>,
}

impl Message {
    pub fn user(text: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: vec![ContentPart::Text { text: text.into() }],
            timestamp: Utc::now(),
        }
    }

    pub fn user_with_images(text: impl Into<String>, images: &[ImagePart]) -> Self {
        let mut parts: Vec<ContentPart> = Vec::new();
        // Text MUST come before images in the content array, otherwise llama.cpp's
        // chat template processing may fail to insert image marker tokens, causing
        // "number of bitmaps (N) does not match number of markers (0)" errors.
        let t = text.into();
        parts.push(ContentPart::Text { text: t });
        for img in images {
            parts.push(ContentPart::Image {
                source_type: "base64".into(),
                media_type: img.mime_type.clone(),
                data: img.data.clone(),
            });
        }
        Self {
            role: Role::User,
            content: parts,
            timestamp: Utc::now(),
        }
    }

    pub fn assistant(text: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: vec![ContentPart::Text { text: text.into() }],
            timestamp: Utc::now(),
        }
    }

    pub fn tool(
        tool_call_id: impl Into<String>,
        name: impl Into<String>,
        result: impl Into<String>,
    ) -> Self {
        Self {
            role: Role::Tool,
            content: vec![ContentPart::ToolResult {
                tool_call_id: tool_call_id.into(),
                name: name.into(),
                result: result.into(),
            }],
            timestamp: Utc::now(),
        }
    }

    /// Extract text content from this message.
    pub fn text_content(&self) -> String {
        self.content
            .iter()
            .filter_map(|p| match p {
                ContentPart::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Extract tool calls from this message.
    pub fn tool_calls(&self) -> Vec<&ContentPart> {
        self.content
            .iter()
            .filter(|p| matches!(p, ContentPart::ToolCall { .. }))
            .collect()
    }

    /// Convert legacy ChatMessage to new Message.
    pub fn from_legacy(msg: &ChatMessage) -> Self {
        let role = match msg.role.as_str() {
            "system" => Role::System,
            "assistant" => Role::Assistant,
            _ => Role::User,
        };
        if msg.role == "tool" {
            // Tool content is "tc_id|result" format; extract id and result
            let (tc_id, result) = if let Some(pos) = msg.content.find('|') {
                (
                    msg.content[..pos].to_string(),
                    msg.content[pos + 1..].to_string(),
                )
            } else {
                ("tool_legacy".into(), msg.content.clone())
            };
            return Self {
                role: Role::Tool,
                content: vec![ContentPart::ToolResult {
                    tool_call_id: tc_id,
                    name: "unknown".into(),
                    result,
                }],
                timestamp: msg.timestamp,
            };
        }
        // Assistant messages starting with "ToolCall|" have structured tool call info
        if msg.role == "assistant" && msg.content.starts_with("ToolCall|") {
            let parts: Vec<&str> = msg.content.splitn(4, '|').collect();
            let tc_id = parts
                .get(1)
                .map(|s| s.to_string())
                .unwrap_or_else(|| "tool_legacy".into());
            let tc_name = parts
                .get(2)
                .map(|s| s.to_string())
                .unwrap_or_else(|| "unknown".into());
            return Self {
                role: Role::Assistant,
                content: vec![ContentPart::ToolCall {
                    id: tc_id,
                    name: tc_name,
                    arguments: serde_json::Value::Object(Default::default()),
                }],
                timestamp: msg.timestamp,
            };
        }
        Self {
            role,
            content: vec![ContentPart::Text {
                text: msg.content.clone(),
            }],
            timestamp: msg.timestamp,
        }
    }
}

/// Tool definition sent to the API in the `tools` parameter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// A parsed tool call extracted from the model response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolChoice {
    Auto,
    None,
    Required,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub response: String,
    pub session_id: String,
    pub token_usage: TokenUsage,
}

/// A permission request sent from the engine to the UI for user approval.
#[derive(Debug, Clone)]
pub struct PermissionRequest {
    pub tool_name: String,
    pub description: String,
    pub risk_level: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TokenUsage {
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    #[serde(default)]
    pub cached_tokens: usize,
    #[serde(default)]
    pub latency_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: String,
    pub created_at: DateTime<Utc>,
    pub message_count: usize,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub pinned_at: Option<String>,
    #[serde(default)]
    pub archived_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    pub status: String,
    pub uptime: u64,
    pub gpu_info: GpuInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AgentState {
    Idle,
    Thinking,
    Acting,
}

// === Mode system ===

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Chat,
    Plan,
    #[default]
    Code,
    Assistant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolScope {
    None,
    ReadOnly,
    Selective,
    Full,
}

pub struct ModeConfig {
    pub agent_loop: bool,
    pub tool_scope: ToolScope,
    pub system_suffix: &'static str,
    pub status_label: &'static str,
}

const READ_ONLY_TOOLS: &[&str] = &["file_read", "file_search", "content_search", "web_browser"];

const SELECTIVE_TOOLS: &[&str] = &[
    "file_read",
    "file_search",
    "content_search",
    "web_browser",
    "schedule_reminder",
];

impl ToolScope {
    pub fn allows(&self, tool_name: &str) -> bool {
        match self {
            ToolScope::None => false,
            ToolScope::ReadOnly => READ_ONLY_TOOLS.contains(&tool_name),
            ToolScope::Selective => SELECTIVE_TOOLS.contains(&tool_name) || tool_name.contains(':'),
            ToolScope::Full => true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ModelPreference {
    #[default]
    Auto,
    Local,
    Cloud,
}

impl ModelPreference {
    pub fn from_key(s: &str) -> Option<ModelPreference> {
        match s.to_lowercase().as_str() {
            "auto" => Some(ModelPreference::Auto),
            "local" => Some(ModelPreference::Local),
            "cloud" => Some(ModelPreference::Cloud),
            _ => None,
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            ModelPreference::Auto => "auto (cloud first, local fallback)",
            ModelPreference::Local => "local only",
            ModelPreference::Cloud => "cloud first",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThinkingLevel {
    None,
    Low,
    #[default]
    Medium,
    High,
    Max,
}

impl ThinkingLevel {
    pub fn budget_tokens(&self) -> Option<usize> {
        match self {
            ThinkingLevel::None => None,
            ThinkingLevel::Low => Some(1024),
            ThinkingLevel::Medium => Some(4096),
            ThinkingLevel::High => Some(16384),
            ThinkingLevel::Max => Some(65536),
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            ThinkingLevel::None => "none",
            ThinkingLevel::Low => "low",
            ThinkingLevel::Medium => "mid",
            ThinkingLevel::High => "high",
            ThinkingLevel::Max => "max",
        }
    }

    pub fn from_key(s: &str) -> Option<ThinkingLevel> {
        match s.to_lowercase().as_str() {
            "none" | "off" | "0" => Some(ThinkingLevel::None),
            "low" | "lo" | "1" => Some(ThinkingLevel::Low),
            "auto" | "mid" | "medium" | "2" => Some(ThinkingLevel::Medium),
            "high" | "hi" | "3" => Some(ThinkingLevel::High),
            "xhigh" | "max" | "full" | "4" => Some(ThinkingLevel::Max),
            _ => None,
        }
    }

    pub fn next(&self) -> ThinkingLevel {
        match self {
            ThinkingLevel::None => ThinkingLevel::Low,
            ThinkingLevel::Low => ThinkingLevel::Medium,
            ThinkingLevel::Medium => ThinkingLevel::High,
            ThinkingLevel::High => ThinkingLevel::Max,
            ThinkingLevel::Max => ThinkingLevel::None,
        }
    }
}

impl Mode {
    pub fn config(&self) -> ModeConfig {
        match self {
            Mode::Chat => ModeConfig {
                agent_loop: false,
                tool_scope: ToolScope::None,
                system_suffix: "Respond concisely. Do not invoke tools or generate code unless explicitly asked.",
                status_label: "chat",
            },
            Mode::Plan => ModeConfig {
                agent_loop: true,
                tool_scope: ToolScope::ReadOnly,
                system_suffix: "You are in Plan mode. Analyze code, explore architecture, propose solutions. Do NOT modify any files. Output plans, diagrams, and recommendations only.",
                status_label: "plan",
            },
            Mode::Code => ModeConfig {
                agent_loop: true,
                tool_scope: ToolScope::Full,
                system_suffix: "",
                status_label: "code",
            },
            Mode::Assistant => ModeConfig {
                agent_loop: true,
                tool_scope: ToolScope::Selective,
                system_suffix: "You are a general-purpose assistant. You can search, read files, write notes and memories, and invoke skills. Do NOT modify code files or execute shell commands.",
                status_label: "asst",
            },
        }
    }

    pub fn from_key(s: &str) -> Option<Mode> {
        match s.to_lowercase().as_str() {
            "chat" => Some(Mode::Chat),
            "plan" => Some(Mode::Plan),
            "code" => Some(Mode::Code),
            "assistant" | "asst" => Some(Mode::Assistant),
            _ => None,
        }
    }

    pub fn next(&self) -> Mode {
        match self {
            Mode::Chat => Mode::Plan,
            Mode::Plan => Mode::Code,
            Mode::Code => Mode::Assistant,
            Mode::Assistant => Mode::Chat,
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            Mode::Chat => "Pure conversation, no tools",
            Mode::Plan => "Read-only exploration, no file modifications",
            Mode::Code => "Full agent loop with tool calling",
            Mode::Assistant => "General assistant, read + memory + skills",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemEvent {
    pub event_type: String,
    pub payload: Value,
}

// === GPU info ===

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuInfo {
    pub vendor: String,
    pub vram_mb: u64,
    pub supported: bool,
}

// === Engine events ===

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineEvent {
    CognitionUpdated {
        trait_name: String,
        old_value: String,
        new_value: String,
        confidence: f64,
    },
    AgentStateChanged {
        old_state: AgentState,
        new_state: AgentState,
    },
    StreamDelta {
        session_id: String,
        delta: String,
    },
    StreamEnd {
        session_id: String,
        full_response: String,
    },
    TokenUsage {
        session_id: String,
        model: String,
        prompt_tokens: usize,
        completion_tokens: usize,
        cached_tokens: usize,
        latency_ms: u64,
        context_window: usize,
    },
    Error {
        code: ErrorCode,
        message: String,
        subsystem: String,
    },
    HeartbeatTick {
        idle_minutes: u64,
        event_count: usize,
        suggested_action: Option<String>,
    },
    PermissionRequired {
        tool: String,
        params: serde_json::Value,
        risk_level: RiskLevel,
    },
    ToolCallStarted {
        session_id: String,
        call_id: String,
        name: String,
        arguments: serde_json::Value,
    },
    ToolCallEnded {
        session_id: String,
        call_id: String,
        name: String,
        success: bool,
        result_summary: String,
    },
    ThinkingDelta {
        session_id: String,
        delta: String,
    },
    ThinkingEnd {
        session_id: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Forbidden,
}

// === JSON-RPC types ===

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub method: String,
    pub params: Option<Value>,
    pub id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
    pub id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: ErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum ErrorCode {
    #[serde(rename = "-32700")]
    ParseError = -32700,
    #[serde(rename = "-32600")]
    InvalidRequest = -32600,
    #[serde(rename = "-32601")]
    MethodNotFound = -32601,
    #[serde(rename = "-32603")]
    InternalError = -32603,
    #[serde(rename = "-32000")]
    ModelUnavailable = -32000,
    #[serde(rename = "-32001")]
    SkillFailed = -32001,
    #[serde(rename = "-32002")]
    PermissionDenied = -32002,
    #[serde(rename = "-32003")]
    Timeout = -32003,
}

// === Config types ===

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachePrefs {
    #[serde(default = "default_block_size")]
    pub block_size: usize,
    #[serde(default = "default_max_blocks")]
    pub max_blocks: usize,
    #[serde(default = "default_cache_budget_mb")]
    pub total_budget_mb: usize,
}

fn default_block_size() -> usize {
    1024
}
fn default_max_blocks() -> usize {
    32
}
fn default_cache_budget_mb() -> usize {
    5120
}

impl Default for CachePrefs {
    fn default() -> Self {
        Self {
            block_size: default_block_size(),
            max_blocks: default_max_blocks(),
            total_budget_mb: default_cache_budget_mb(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPrefs {
    #[serde(default = "default_max_iterations")]
    pub max_iterations: usize,
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
}

fn default_max_iterations() -> usize {
    3
}
fn default_timeout_secs() -> u64 {
    120
}

impl Default for AgentPrefs {
    fn default() -> Self {
        Self {
            max_iterations: default_max_iterations(),
            timeout_secs: default_timeout_secs(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonaPrefs {
    #[serde(default = "default_top_n")]
    pub top_n: usize,
    #[serde(default = "default_recency_days")]
    pub recency_decay_days: u32,
}

fn default_top_n() -> usize {
    5
}
fn default_recency_days() -> u32 {
    30
}

impl Default for PersonaPrefs {
    fn default() -> Self {
        Self {
            top_n: default_top_n(),
            recency_decay_days: default_recency_days(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    #[serde(default = "default_rate_limit_ms")]
    pub min_interval_ms: u64,
}

fn default_rate_limit_ms() -> u64 {
    100
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            min_interval_ms: default_rate_limit_ms(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    #[serde(default)]
    pub models: Vec<ModelConfig>,
    #[serde(default)]
    pub router: RouterPrefs,
    #[serde(default)]
    pub server: ServerPrefs,
    #[serde(default)]
    pub storage: StoragePrefs,
    #[serde(default)]
    pub logging: LoggingPrefs,
    #[serde(default)]
    pub cache: CachePrefs,
    #[serde(default)]
    pub agent: AgentPrefs,
    #[serde(default)]
    pub persona: PersonaPrefs,
    #[serde(default)]
    pub rate_limit: RateLimitConfig,
    #[serde(default)]
    pub settings: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouterPrefs {
    #[serde(default = "default_keyword_threshold")]
    pub keyword_threshold: f32,
    #[serde(default = "default_fallback_threshold")]
    pub fallback_threshold: f32,
}

fn default_keyword_threshold() -> f32 {
    0.85
}
fn default_fallback_threshold() -> f32 {
    0.7
}

impl Default for RouterPrefs {
    fn default() -> Self {
        Self {
            keyword_threshold: default_keyword_threshold(),
            fallback_threshold: default_fallback_threshold(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerPrefs {
    #[serde(default = "default_host")]
    pub host: String,
}

fn default_host() -> String {
    "127.0.0.1".into()
}

impl Default for ServerPrefs {
    fn default() -> Self {
        Self {
            host: default_host(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StoragePrefs {
    #[serde(default)]
    pub data_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingPrefs {
    #[serde(default = "default_log_level")]
    pub level: String,
    #[serde(default)]
    pub log_content: bool,
    #[serde(default)]
    pub dir: Option<String>,
}

fn default_log_level() -> String {
    "INFO".into()
}

impl Default for LoggingPrefs {
    fn default() -> Self {
        Self {
            level: default_log_level(),
            log_content: false,
            dir: None,
        }
    }
}

/// Recursively set a dot-separated path inside a JSON value, creating intermediate objects.
fn set_json_path(root: &mut serde_json::Value, path: &str, value: serde_json::Value) {
    let parts: Vec<&str> = path.split('.').collect();
    if parts.is_empty() {
        return;
    }

    // Walk to the parent, creating intermediate objects as needed
    let mut current = root;
    for part in &parts[..parts.len() - 1] {
        if current.is_null() || !current.is_object() {
            *current = serde_json::Value::Object(serde_json::Map::new());
        }
        current = current
            .as_object_mut()
            .unwrap()
            .entry((*part).to_string())
            .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
    }

    // Insert the leaf value
    if current.is_null() || !current.is_object() {
        *current = serde_json::Value::Object(serde_json::Map::new());
    }
    if let Some(obj) = current.as_object_mut() {
        obj.insert(parts.last().unwrap().to_string(), value);
    }
}

/// Deep-merge `overlay` into `base`: for object values, recursively merge; otherwise overlay wins.
fn deep_merge(mut base: serde_json::Value, overlay: serde_json::Value) -> serde_json::Value {
    match (&mut base, overlay) {
        (serde_json::Value::Object(base_map), serde_json::Value::Object(overlay_map)) => {
            for (k, v) in overlay_map {
                let merged = if let Some(existing) = base_map.get(&k) {
                    deep_merge(existing.clone(), v)
                } else {
                    v
                };
                base_map.insert(k, merged);
            }
            base
        }
        (_, overlay) => overlay,
    }
}

impl AppConfig {
    pub fn get_nested(&self, key: &str) -> Option<serde_json::Value> {
        let value = serde_json::to_value(self).ok()?;
        let parts: Vec<&str> = key.split('.').collect();
        let mut current = &value;
        for part in parts {
            current = current.get(part)?;
        }
        Some(current.clone())
    }

    #[allow(clippy::collapsible_if, clippy::collapsible_match)]
    pub fn set_nested(&mut self, key: &str, value: serde_json::Value) -> anyhow::Result<()> {
        // Handle `settings.*` keys: write into the free-form settings JSON at arbitrary depth
        if key.starts_with("settings.") || key == "settings" {
            let sub_path = key.strip_prefix("settings.").unwrap_or("");
            if self.settings.is_null() {
                self.settings = serde_json::Value::Object(serde_json::Map::new());
            }
            if sub_path.is_empty() {
                // Replace entire settings
                self.settings = value;
            } else {
                set_json_path(&mut self.settings, sub_path, value);
            }
            return Ok(());
        }

        // Handle `general` key: front-end sends `config.set { key: 'general', value: {...} }`
        // Map it into the settings JSON for round-trip fidelity with deep merge
        if key == "general" {
            if self.settings.is_null() {
                self.settings = serde_json::Value::Object(serde_json::Map::new());
            }
            if let Some(map) = self.settings.as_object_mut() {
                if let serde_json::Value::Object(obj) = value {
                    for (k, v) in obj {
                        let merged = if let Some(existing) = map.get(&k) {
                            deep_merge(existing.clone(), v)
                        } else {
                            v
                        };
                        map.insert(k, merged);
                    }
                } else {
                    map.insert("general".into(), value);
                }
            }
            return Ok(());
        }

        // Typed 2-part keys remain unchanged
        let parts: Vec<&str> = key.split('.').collect();
        if parts.len() == 1 {
            // Handle top-level keys like "models"
            if parts[0] == "models" {
                if let Ok(parsed) = serde_json::from_value::<Vec<ModelConfig>>(value.clone()) {
                    self.models = parsed;
                }
                // Don't mirror into settings — typed field is authoritative
                return Ok(());
            }
        }
        if parts.len() != 2 {
            // Fall back to settings JSON for unknown multi-part keys
            if self.settings.is_null() {
                self.settings = serde_json::Value::Object(serde_json::Map::new());
            }
            set_json_path(&mut self.settings, key, value);
            return Ok(());
        }
        match parts[0] {
            "server" => {
                if parts[1] == "host" {
                    if let serde_json::Value::String(s) = value {
                        self.server.host = s;
                    }
                }
            }
            "router" => match parts[1] {
                "keyword_threshold" => {
                    if let serde_json::Value::Number(n) = &value {
                        self.router.keyword_threshold = n.as_f64().unwrap_or(0.85) as f32;
                    }
                }
                "fallback_threshold" => {
                    if let serde_json::Value::Number(n) = &value {
                        self.router.fallback_threshold = n.as_f64().unwrap_or(0.7) as f32;
                    }
                }
                _ => {}
            },
            "agent" => match parts[1] {
                "max_iterations" => {
                    if let serde_json::Value::Number(n) = &value {
                        self.agent.max_iterations = n.as_u64().unwrap_or(3) as usize;
                    }
                }
                "timeout_secs" => {
                    if let serde_json::Value::Number(n) = &value {
                        self.agent.timeout_secs = n.as_u64().unwrap_or(120);
                    }
                }
                _ => {}
            },
            "persona" => match parts[1] {
                "top_n" => {
                    if let serde_json::Value::Number(n) = &value {
                        self.persona.top_n = n.as_u64().unwrap_or(5) as usize;
                    }
                }
                "recency_decay_days" => {
                    if let serde_json::Value::Number(n) = &value {
                        self.persona.recency_decay_days = n.as_u64().unwrap_or(30) as u32;
                    }
                }
                _ => {}
            },
            "rate_limit" => {
                if parts[1] == "min_interval_ms" {
                    if let serde_json::Value::Number(n) = &value {
                        self.rate_limit.min_interval_ms = n.as_u64().unwrap_or(100);
                    }
                }
            }
            "cache" => match parts[1] {
                "block_size" => {
                    if let serde_json::Value::Number(n) = &value {
                        self.cache.block_size = n.as_u64().unwrap_or(1024) as usize;
                    }
                }
                "max_blocks" => {
                    if let serde_json::Value::Number(n) = &value {
                        self.cache.max_blocks = n.as_u64().unwrap_or(32) as usize;
                    }
                }
                "total_budget_mb" => {
                    if let serde_json::Value::Number(n) = &value {
                        self.cache.total_budget_mb = n.as_u64().unwrap_or(5120) as usize;
                    }
                }
                _ => {}
            },
            "logging" => {
                if parts[1] == "level" {
                    if let serde_json::Value::String(s) = value {
                        self.logging.level = s;
                    }
                }
            }
            _ => {
                // Unknown section: write into settings JSON
                if self.settings.is_null() {
                    self.settings = serde_json::Value::Object(serde_json::Map::new());
                }
                set_json_path(&mut self.settings, key, value);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_config_defaults() {
        let config = AppConfig::default();
        assert_eq!(config.router.keyword_threshold, 0.85);
        assert_eq!(config.router.fallback_threshold, 0.7);
        assert_eq!(config.server.host, "127.0.0.1");
        assert_eq!(config.cache.block_size, 1024);
        assert_eq!(config.agent.max_iterations, 3);
        assert_eq!(config.persona.top_n, 5);
        assert_eq!(config.rate_limit.min_interval_ms, 100);
    }

    #[test]
    fn test_intent_display() {
        assert_eq!(format!("{}", Intent::Chat), "chat");
        assert_eq!(format!("{}", Intent::FileOperation), "file_operation");
    }

    #[test]
    fn test_jsonrpc_error_code_serde() {
        let err = JsonRpcError {
            code: ErrorCode::MethodNotFound,
            message: "not found".into(),
            data: None,
        };
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("-32601"));
        let decoded: JsonRpcError = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.code, ErrorCode::MethodNotFound);
    }

    #[test]
    fn test_engine_event_snake_case() {
        let event = EngineEvent::CognitionUpdated {
            trait_name: "risk".into(),
            old_value: "low".into(),
            new_value: "high".into(),
            confidence: 0.9,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("cognition_updated"));
    }

    #[test]
    fn test_chat_response_serialization() {
        let resp = ChatResponse {
            response: "hello".into(),
            session_id: "s1".into(),
            token_usage: TokenUsage {
                prompt_tokens: 10,
                completion_tokens: 5,
                cached_tokens: 0,
                latency_ms: 0,
            },
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("session_id"));
        let decoded: ChatResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.response, "hello");
    }

    #[test]
    fn test_model_config_default() {
        let config = ModelConfig::default();
        assert_eq!(config.n_gpu_layers, 0);
        assert_eq!(config.backend, ModelBackend::LmStudio);
        assert!(config.api_key_env.is_none());
    }

    #[test]
    fn test_token_usage_backward_compat() {
        let json = r#"{"prompt_tokens":10,"completion_tokens":5}"#;
        let decoded: TokenUsage = serde_json::from_str(json).unwrap();
        assert_eq!(decoded.prompt_tokens, 10);
        assert_eq!(decoded.cached_tokens, 0);
        assert_eq!(decoded.latency_ms, 0);
    }

    #[test]
    fn test_classify_output_route_reason_default() {
        let co = ClassifyOutput {
            intent: Intent::Chat,
            complexity: 0.3,
            skill_match: None,
            confidence: 0.9,
            cache_hit: false,
            target_model: TargetModel::Local,
            route_reason: "keyword_match".into(),
        };
        let json = serde_json::to_string(&co).unwrap();
        assert!(json.contains("route_reason"));
    }

    #[test]
    fn test_agent_state_changed_uses_enum() {
        let event = EngineEvent::AgentStateChanged {
            old_state: AgentState::Idle,
            new_state: AgentState::Thinking,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("agent_state_changed"));
        assert!(json.contains("idle"));
        assert!(json.contains("thinking"));
        let decoded: EngineEvent = serde_json::from_str(&json).unwrap();
        match decoded {
            EngineEvent::AgentStateChanged {
                old_state,
                new_state,
            } => {
                assert_eq!(old_state, AgentState::Idle);
                assert_eq!(new_state, AgentState::Thinking);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_token_usage_event_has_new_fields() {
        let event = EngineEvent::TokenUsage {
            session_id: "s1".into(),
            model: "test".into(),
            prompt_tokens: 10,
            completion_tokens: 5,
            cached_tokens: 2,
            latency_ms: 150,
            context_window: 200_000,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("cached_tokens"));
        assert!(json.contains("latency_ms"));
        assert!(json.contains("context_window"));
        assert!(json.contains("200000"));
    }

    #[test]
    fn test_get_nested_key() {
        let config = AppConfig::default();
        let result = config.get_nested("server.host");
        assert!(result.is_some());
        assert_eq!(result.unwrap(), serde_json::json!("127.0.0.1"));
    }

    #[test]
    fn test_set_nested_key() {
        let mut config = AppConfig::default();
        config.set_nested("server.host", serde_json::json!("0.0.0.0")).unwrap();
        assert_eq!(config.server.host, "0.0.0.0");
        config.set_nested("agent.max_iterations", serde_json::json!(5)).unwrap();
        assert_eq!(config.agent.max_iterations, 5);
    }

    #[test]
    fn test_set_nested_settings_path() {
        let mut config = AppConfig::default();
        config.set_nested("settings.agent.default.name", serde_json::json!("MyAgent")).unwrap();
        assert_eq!(config.settings["agent"]["default"]["name"], serde_json::json!("MyAgent"));
    }

    #[test]
    fn test_set_nested_general_key() {
        let mut config = AppConfig::default();
        config.set_nested("general", serde_json::json!({"locale": "zh-CN", "theme": "dark"})).unwrap();
        assert_eq!(config.settings["locale"], serde_json::json!("zh-CN"));
        assert_eq!(config.settings["theme"], serde_json::json!("dark"));
    }
}

#[cfg(test)]
mod mode_tests {
    use super::*;

    #[test]
    fn test_chat_mode_config() {
        let cfg = Mode::Chat.config();
        assert!(!cfg.agent_loop);
        assert_eq!(cfg.tool_scope, ToolScope::None);
        assert_eq!(cfg.status_label, "chat");
    }

    #[test]
    fn test_plan_mode_config() {
        let cfg = Mode::Plan.config();
        assert!(cfg.agent_loop);
        assert_eq!(cfg.tool_scope, ToolScope::ReadOnly);
        assert_eq!(cfg.status_label, "plan");
    }

    #[test]
    fn test_code_mode_config() {
        let cfg = Mode::Code.config();
        assert!(cfg.agent_loop);
        assert_eq!(cfg.tool_scope, ToolScope::Full);
        assert_eq!(cfg.status_label, "code");
    }

    #[test]
    fn test_assistant_mode_config() {
        let cfg = Mode::Assistant.config();
        assert!(cfg.agent_loop);
        assert_eq!(cfg.tool_scope, ToolScope::Selective);
        assert_eq!(cfg.status_label, "asst");
    }

    #[test]
    fn test_default_mode_is_code() {
        assert_eq!(Mode::default(), Mode::Code);
    }

    #[test]
    fn test_mode_from_str() {
        assert_eq!(Mode::from_key("chat"), Some(Mode::Chat));
        assert_eq!(Mode::from_key("plan"), Some(Mode::Plan));
        assert_eq!(Mode::from_key("code"), Some(Mode::Code));
        assert_eq!(Mode::from_key("assistant"), Some(Mode::Assistant));
        assert_eq!(Mode::from_key("asst"), Some(Mode::Assistant));
        assert_eq!(Mode::from_key("unknown"), None);
    }

    #[test]
    fn test_tool_scope_allows() {
        assert!(!ToolScope::None.allows("file_read"));
        assert!(ToolScope::ReadOnly.allows("file_read"));
        assert!(ToolScope::ReadOnly.allows("content_search"));
        assert!(!ToolScope::ReadOnly.allows("file_write"));
        assert!(!ToolScope::ReadOnly.allows("shell"));
        assert!(ToolScope::Selective.allows("file_read"));
        assert!(ToolScope::Selective.allows("schedule_reminder"));
        assert!(!ToolScope::Selective.allows("shell"));
        assert!(!ToolScope::Selective.allows("file_write"));
        assert!(ToolScope::Full.allows("shell"));
        assert!(ToolScope::Full.allows("file_write"));
    }

    #[test]
    fn test_mode_next() {
        assert_eq!(Mode::Chat.next(), Mode::Plan);
        assert_eq!(Mode::Plan.next(), Mode::Code);
        assert_eq!(Mode::Code.next(), Mode::Assistant);
        assert_eq!(Mode::Assistant.next(), Mode::Chat);
    }
}

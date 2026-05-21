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
    LlamaCpp,
    Anthropic,
    OpenAI,
    DeepSeek,
    LmStudio,
    Ollama,
}

impl ModelBackend {
    pub fn name(&self) -> &'static str {
        match self {
            ModelBackend::LlamaCpp => "LlamaCpp",
            ModelBackend::Anthropic => "Anthropic",
            ModelBackend::OpenAI => "OpenAI",
            ModelBackend::DeepSeek => "DeepSeek",
            ModelBackend::LmStudio => "LmStudio",
            ModelBackend::Ollama => "Ollama",
        }
    }

    pub fn is_cloud_capable(&self) -> bool {
        !matches!(self, ModelBackend::LlamaCpp)
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub response: String,
    pub session_id: String,
    pub token_usage: TokenUsage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub tool: String,
    pub params: serde_json::Value,
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
    TokenUsage {
        session_id: String,
        model: String,
        prompt_tokens: usize,
        completion_tokens: usize,
        cached_tokens: usize,
        latency_ms: u64,
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
    pub fn set_nested(&mut self, key: &str, value: &str) -> anyhow::Result<()> {
        let parts: Vec<&str> = key.split('.').collect();
        if parts.len() != 2 {
            anyhow::bail!("key must be 'section.field' format");
        }
        let json_value: serde_json::Value = serde_json::from_str(value)
            .unwrap_or_else(|_| serde_json::Value::String(value.to_string()));
        match parts[0] {
            "server" => {
                if parts[1] == "host" {
                    if let serde_json::Value::String(s) = json_value {
                        self.server.host = s;
                    }
                }
            }
            "router" => match parts[1] {
                "keyword_threshold" => {
                    if let serde_json::Value::Number(n) = &json_value {
                        self.router.keyword_threshold = n.as_f64().unwrap_or(0.85) as f32;
                    }
                }
                "fallback_threshold" => {
                    if let serde_json::Value::Number(n) = &json_value {
                        self.router.fallback_threshold = n.as_f64().unwrap_or(0.7) as f32;
                    }
                }
                _ => {}
            },
            "agent" => match parts[1] {
                "max_iterations" => {
                    if let serde_json::Value::Number(n) = &json_value {
                        self.agent.max_iterations = n.as_u64().unwrap_or(3) as usize;
                    }
                }
                "timeout_secs" => {
                    if let serde_json::Value::Number(n) = &json_value {
                        self.agent.timeout_secs = n.as_u64().unwrap_or(120);
                    }
                }
                _ => {}
            },
            "persona" => match parts[1] {
                "top_n" => {
                    if let serde_json::Value::Number(n) = &json_value {
                        self.persona.top_n = n.as_u64().unwrap_or(5) as usize;
                    }
                }
                "recency_decay_days" => {
                    if let serde_json::Value::Number(n) = &json_value {
                        self.persona.recency_decay_days = n.as_u64().unwrap_or(30) as u32;
                    }
                }
                _ => {}
            },
            "rate_limit" => {
                if parts[1] == "min_interval_ms" {
                    if let serde_json::Value::Number(n) = &json_value {
                        self.rate_limit.min_interval_ms = n.as_u64().unwrap_or(100);
                    }
                }
            }
            "cache" => match parts[1] {
                "block_size" => {
                    if let serde_json::Value::Number(n) = &json_value {
                        self.cache.block_size = n.as_u64().unwrap_or(1024) as usize;
                    }
                }
                "max_blocks" => {
                    if let serde_json::Value::Number(n) = &json_value {
                        self.cache.max_blocks = n.as_u64().unwrap_or(32) as usize;
                    }
                }
                "total_budget_mb" => {
                    if let serde_json::Value::Number(n) = &json_value {
                        self.cache.total_budget_mb = n.as_u64().unwrap_or(5120) as usize;
                    }
                }
                _ => {}
            },
            "logging" => {
                if parts[1] == "level" {
                    if let serde_json::Value::String(s) = json_value {
                        self.logging.level = s;
                    }
                }
            }
            _ => {}
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
        assert_eq!(config.backend, ModelBackend::LlamaCpp);
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
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("cached_tokens"));
        assert!(json.contains("latency_ms"));
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
        config.set_nested("server.host", "\"0.0.0.0\"").unwrap();
        assert_eq!(config.server.host, "0.0.0.0");
        config.set_nested("agent.max_iterations", "5").unwrap();
        assert_eq!(config.agent.max_iterations, 5);
    }
}

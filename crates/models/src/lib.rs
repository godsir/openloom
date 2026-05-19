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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub name: String,
    #[serde(default)]
    pub model_type: ModelType,
    #[serde(default)]
    pub backend: ModelBackend,
    pub path: Option<String>,
    #[serde(default = "default_context_size")]
    pub context_size: usize,
    #[serde(default)]
    pub n_gpu_layers: usize,
    pub api_key_env: Option<String>,
}

fn default_context_size() -> usize {
    4096
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            model_type: ModelType::Router,
            backend: ModelBackend::default(),
            path: None,
            context_size: default_context_size(),
            n_gpu_layers: 0,
            api_key_env: None,
        }
    }
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassifyOutput {
    pub intent: Intent,
    pub complexity: f32,
    pub skill_match: Option<String>,
    pub confidence: f32,
    pub cache_hit: bool,
    pub target_model: TargetModel,
}

// === Engine types ===

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub response: String,
    pub session_id: String,
    pub token_usage: TokenUsage,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TokenUsage {
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
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
        old_state: String,
        new_state: String,
    },
    TokenUsage {
        session_id: String,
        model: String,
        prompt_tokens: usize,
        completion_tokens: usize,
    },
    Error {
        code: ErrorCode,
        message: String,
        subsystem: String,
    },
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
    pub data_dir: Option<String>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_config_defaults() {
        let config = AppConfig::default();
        assert_eq!(config.router.keyword_threshold, 0.85);
        assert_eq!(config.router.fallback_threshold, 0.7);
        assert_eq!(config.server.host, "127.0.0.1");
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
}

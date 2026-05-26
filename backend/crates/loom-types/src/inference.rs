//! Inference request/response types.
//!
//! Consumers: loom-core (agent loop), loom-inference (provider dispatch), loom-server

use serde::{Deserialize, Serialize};

use crate::message::Message;
use crate::tool::{ToolChoice, ToolDefinition};
use crate::tool::ToolCall;

/// Token usage statistics for a completion.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TokenUsage {
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    #[serde(default)]
    pub cached_tokens: usize,
    #[serde(default)]
    pub latency_ms: u64,
}

/// Chat completion request sent to the inference provider.
///
/// Consumers: loom-core (agent loop), loom-inference (provider dispatch)
#[derive(Debug, Clone)]
pub struct CompletionRequest {
    pub messages: Vec<Message>,
    pub tools: Vec<ToolDefinition>,
    pub tool_choice: Option<ToolChoice>,
    pub prompt: String,
    pub max_tokens: usize,
    pub temperature: f32,
    pub top_p: f32,
    pub stop: Vec<String>,
    pub stream: bool,
    pub thinking_budget: Option<usize>,
}

impl Default for CompletionRequest {
    fn default() -> Self {
        Self {
            messages: Vec::new(),
            tools: Vec::new(),
            tool_choice: None,
            prompt: String::new(),
            max_tokens: 4096,
            temperature: 0.7,
            top_p: 1.0,
            stop: Vec::new(),
            stream: false,
            thinking_budget: None,
        }
    }
}

impl CompletionRequest {
    /// Get the effective messages array: if messages is non-empty use it,
    /// otherwise convert the legacy flat prompt into a single user message.
    pub fn effective_messages(&self) -> Vec<Message> {
        if !self.messages.is_empty() {
            self.messages.clone()
        } else if !self.prompt.is_empty() {
            vec![Message::user(&self.prompt)]
        } else {
            vec![]
        }
    }
}

/// Chat completion response from the inference provider.
#[derive(Debug, Clone)]
pub struct CompletionResponse {
    pub text: String,
    pub tool_calls: Vec<ToolCall>,
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub cached_tokens: usize,
    pub latency_ms: u64,
    pub thinking: Option<String>,
}

/// Structured streaming delta emitted during true streaming with tool support.
#[derive(Debug, Clone)]
pub enum StreamDelta {
    Text(String),
    Reasoning(String),
    ToolCallBegin { index: usize, id: String, name: String },
    ToolCallArgsChunk { index: usize, chunk: String },
    Usage { prompt_tokens: u64, completion_tokens: u64, cache_read_tokens: u64, cache_write_tokens: u64 },
}

/// Engine health status response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    pub status: String,
    pub uptime: u64,
    pub gpu_info: GpuInfo,
}

/// GPU hardware information.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GpuInfo {
    pub vendor: String,
    pub vram_mb: u64,
    pub supported: bool,
}

/// Chat response returned from the engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub response: String,
    pub session_id: String,
    pub token_usage: TokenUsage,
}

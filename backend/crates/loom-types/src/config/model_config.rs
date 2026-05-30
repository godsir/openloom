//! Model backend and configuration types.
//!
//! Consumers: loom-core, loom-inference, loom-server, loom-cli

use serde::{Deserialize, Serialize};

/// The inference backend for a model.
///
/// Consumers: loom-inference (provider dispatch), loom-core (engine config), loom-server (model.* RPC)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum ModelBackend {
    #[default]
    LmStudio,
    Anthropic,
    OpenAI,
    DeepSeek,
    Ollama,
    Custom,
}

impl ModelBackend {
    pub fn name(&self) -> &'static str {
        match self {
            ModelBackend::LmStudio => "LmStudio",
            ModelBackend::Anthropic => "Anthropic",
            ModelBackend::OpenAI => "OpenAI",
            ModelBackend::DeepSeek => "DeepSeek",
            ModelBackend::Ollama => "Ollama",
            ModelBackend::Custom => "Custom",
        }
    }

    pub fn is_local_inference(&self) -> bool {
        matches!(self, ModelBackend::LmStudio | ModelBackend::Ollama)
    }
}

/// Classification of model purpose.
///
/// Consumers: loom-core (router), loom-server (model list)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum ModelType {
    #[default]
    Router,
    Summarizer,
    Reasoning,
}

/// Per-model configuration entry.
///
/// Consumers: loom-inference (provider dispatch), loom-core (Engine), loom-server (model.* RPC)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub name: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub model_type: ModelType,
    #[serde(default)]
    pub backend: ModelBackend,
    #[serde(default)]
    pub backend_label: Option<String>,
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
    #[serde(default)]
    pub capabilities: ModelCapabilities,
    #[serde(default)]
    pub api_format: Option<String>,
    /// USD per 1M input (prompt) tokens. 0 = not priced / local model.
    #[serde(default)]
    pub input_price: f64,
    /// USD per 1M output (completion) tokens.
    #[serde(default)]
    pub output_price: f64,
    /// USD per 1M cache-read (prompt cache hit) tokens.
    #[serde(default)]
    pub cache_read_price: f64,
    /// USD per 1M cache-write tokens.
    #[serde(default)]
    pub cache_write_price: f64,
}

/// Model capability flags.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelCapabilities {
    #[serde(default)]
    pub vision: bool,
    #[serde(default)]
    pub reasoning: bool,
    #[serde(default)]
    pub function_calling: bool,
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
            backend_label: None,
            path: None,
            context_size: default_context_size(),
            max_output_tokens: None,
            n_gpu_layers: 0,
            api_key_env: None,
            base_url: None,
            capabilities: ModelCapabilities::default(),
            api_format: None,
            input_price: 0.0,
            output_price: 0.0,
            cache_read_price: 0.0,
            cache_write_price: 0.0,
        }
    }
}

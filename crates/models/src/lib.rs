use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ModelType {
    Router,
    Summarizer,
    Reasoning,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub name: String,
    pub model_type: ModelType,
    pub path: Option<String>,
    pub context_size: usize,
}

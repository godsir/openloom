// SPDX-License-Identifier: Apache-2.0
//! Inference engine — provider dispatch for Anthropic, OpenAI, DeepSeek, LM Studio, Ollama.

pub mod engine;
pub mod anthropic;
pub mod openai;
pub mod cache;

pub use engine::{CloudClient, InferenceEngine};
pub use anthropic::AnthropicClient;
pub use openai::{create_cloud_client, ensure_lm_studio_model, OpenAIClient};

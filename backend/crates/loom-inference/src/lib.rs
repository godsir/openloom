// SPDX-License-Identifier: Apache-2.0
//! Inference engine — provider dispatch for Anthropic, OpenAI, DeepSeek, LM Studio, Ollama.

pub mod anthropic;
pub mod cache;
pub mod engine;
pub mod openai;

pub use anthropic::AnthropicClient;
pub use engine::{CloudClient, InferenceEngine, unload_local_model};
pub use openai::{OpenAIClient, create_cloud_client, ensure_lm_studio_model, parse_inline_tool_calls};

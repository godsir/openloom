//! Local inference context-window probing.
//!
//! Local models (LM Studio / Ollama) default to `context_size = 100_000` when
//! the user never configures one, which silently disables every window guard
//! (summarizer 80%, mid-turn 90%, history budget) and ends in provider-side
//! context-overflow errors. This module asks the local server for the real
//! context length, caches the answer per process, and lets the orchestrator
//! fall back to a conservative window when probing fails.

use std::collections::HashMap;
use std::sync::RwLock;

use loom_types::config::model_config::{ModelBackend, ModelConfig};

/// Conservative fallback when a local model's real window cannot be determined.
pub const LOCAL_FALLBACK_CONTEXT: usize = 8_192;

/// Per-process cache of probe results. `None` means "probed and failed" —
/// cached too, so a dead endpoint doesn't cost a 3s timeout on every turn.
#[derive(Default)]
pub struct ContextProbe {
    cache: RwLock<HashMap<String, Option<usize>>>,
}

impl ContextProbe {
    pub fn new() -> Self {
        Self::default()
    }

    /// Resolve the real context window for a local model config, or None when
    /// the server can't tell us. Callers decide the fallback.
    pub async fn context_window(&self, config: &ModelConfig) -> Option<usize> {
        let key = format!(
            "{}|{}|{}",
            config.backend.name(),
            config.base_url.as_deref().unwrap_or(""),
            config.model.as_deref().unwrap_or(&config.name)
        );
        if let Some(hit) = self.cache.read().unwrap().get(&key) {
            return *hit;
        }
        let probed = probe(config).await;
        if probed.is_some() {
            tracing::info!(model = %config.name, window = probed, "detected local model context window");
        }
        self.cache.write().unwrap().insert(key, probed);
        probed
    }
}

/// Probe the local server for the model's context length.
async fn probe(config: &ModelConfig) -> Option<usize> {
    let client = loom_inference::engine::build_http_client();
    let base = config.base_url.as_deref().unwrap_or("");
    let root = base
        .trim_end_matches('/')
        .strip_suffix("/v1")
        .unwrap_or(base.trim_end_matches('/'))
        .to_string();
    let model_id = config.model.as_deref().unwrap_or(&config.name);
    match config.backend {
        ModelBackend::LmStudio => probe_lmstudio(&client, &root, model_id).await,
        ModelBackend::Ollama => probe_ollama(&client, &root, model_id).await,
        _ => None,
    }
}

/// LM Studio (>= 0.3.6): GET {root}/api/v0/models
/// Response: { "data": [ { "id": "...", "max_context_length": 32768,
///   "loaded_instances": [ { "id": "...", "context_length": 4096 } ] } ] }
/// Prefer the loaded instance's actual context_length over the model's max.
async fn probe_lmstudio(
    client: &reqwest::Client,
    root: &str,
    model_id: &str,
) -> Option<usize> {
    let url = format!("{root}/api/v0/models");
    let resp = client
        .get(&url)
        .timeout(std::time::Duration::from_secs(3))
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let json: serde_json::Value = resp.json().await.ok()?;
    let data = json["data"].as_array()?;
    // Match by model id; fall back to the single entry when unambiguous.
    let entry = data
        .iter()
        .find(|m| m["id"].as_str() == Some(model_id))
        .or(if data.len() == 1 { data.first() } else { None })?;
    let loaded_ctx = entry["loaded_instances"]
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|i| i["context_length"].as_u64());
    let max_ctx = entry["max_context_length"].as_u64();
    loaded_ctx
        .or(max_ctx)
        .map(|v| v as usize)
        .filter(|v| *v >= 1024)
}

/// Ollama: POST {root}/api/show { "name": model }
/// Response: { "model_info": { "llama.context_length": 4096, ... } }
/// The architecture prefix varies — take the first key ending in
/// ".context_length".
async fn probe_ollama(client: &reqwest::Client, root: &str, model_id: &str) -> Option<usize> {
    let url = format!("{root}/api/show");
    let resp = client
        .post(&url)
        .json(&serde_json::json!({ "name": model_id }))
        .timeout(std::time::Duration::from_secs(3))
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let json: serde_json::Value = resp.json().await.ok()?;
    let info = json["model_info"].as_object()?;
    info.iter()
        .find(|(k, _)| k.ends_with(".context_length"))
        .and_then(|(_, v)| v.as_u64())
        .map(|v| v as usize)
        .filter(|v| *v >= 1024)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_lmstudio_prefers_loaded_instance() {
        let json = serde_json::json!({
            "data": [{
                "id": "qwen3-8b",
                "max_context_length": 32768,
                "loaded_instances": [{"id": "qwen3-8b", "context_length": 4096}]
            }]
        });
        let data = json["data"].as_array().unwrap();
        let entry = data
            .iter()
            .find(|m| m["id"].as_str() == Some("qwen3-8b"))
            .unwrap();
        let loaded = entry["loaded_instances"]
            .as_array()
            .and_then(|a| a.first())
            .and_then(|i| i["context_length"].as_u64());
        assert_eq!(loaded, Some(4096));
    }

    #[test]
    fn parse_ollama_context_length_key() {
        let json = serde_json::json!({
            "model_info": {
                "llama.architecture": "qwen2",
                "qwen2.context_length": 32768
            }
        });
        let info = json["model_info"].as_object().unwrap();
        let v = info
            .iter()
            .find(|(k, _)| k.ends_with(".context_length"))
            .and_then(|(_, v)| v.as_u64());
        assert_eq!(v, Some(32768));
    }

    #[test]
    fn v1_suffix_stripped_from_base_url() {
        let base = "http://localhost:1234/v1/";
        let root = base
            .trim_end_matches('/')
            .strip_suffix("/v1")
            .unwrap_or(base.trim_end_matches('/'));
        assert_eq!(root, "http://localhost:1234");
    }
}

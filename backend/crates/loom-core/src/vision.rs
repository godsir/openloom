//! Vision auxiliary — processes images via a vision-capable model when the
//! main model lacks vision capabilities.

use anyhow::Result;
use base64::Engine;
use chrono::Utc;
use loom_inference::engine::CloudClient;
use loom_inference::openai::OpenAIClient;
use loom_types::{CompletionRequest, ContentPart, Message, ModelBackend, Role};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Deserialize, Default)]
pub struct VisionConfig {
    pub enabled: bool,
    pub model: Option<String>,
}

const VISION_PROMPT: &str = r#"Analyze this image for another text-only model. Return a concise note with these exact sections:
image_overview: fixed basic description of what the image is.
visible_text: important OCR or readable text.
objects_and_layout: important objects, positions, counts, and relationships.
charts_or_data: chart/table/data details if present; otherwise say none.
user_request: restate the user's request in one short sentence.
user_request_answer: answer the user's request using the image when possible.
evidence: the visual evidence supporting that answer.
uncertainty: anything unclear, hidden, or guessed.

Do not mention that you are a tool or a separate model. Output the note as plain text, no Markdown fences, no JSON."#;

pub fn load_vision_config() -> VisionConfig {
    let home = dirs::home_dir()
        .unwrap_or_default()
        .join(".loom")
        .join("vision.json");
    std::fs::read_to_string(&home)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn has_images(messages: &[Message]) -> bool {
    messages.iter().any(|m| {
        m.content.iter().any(|part| {
            matches!(
                part,
                ContentPart::Image { .. } | ContentPart::ImageRef { .. }
            )
        })
    })
}

pub fn extract_images_from_messages(
    messages: &[Message],
    loom_dir: Option<&std::path::Path>,
) -> Vec<(String, String, String)> {
    let mut images = Vec::new();
    for msg in messages.iter().rev() {
        for part in &msg.content {
            match part {
                ContentPart::Image {
                    source_type: st,
                    media_type: mt,
                    data: d,
                } => {
                    images.push((st.clone(), mt.clone(), d.clone()));
                }
                ContentPart::ImageRef {
                    media_type: mt,
                    file_id: fid,
                } => {
                    // Reconstruct file path: loom_dir/sessions/*/images/fid
                    let mut found = false;
                    if let Some(base) = loom_dir
                        && let Ok(sessions_dir) = std::fs::read_dir(base.join("sessions"))
                    {
                        for entry in sessions_dir.flatten() {
                            let img_path = entry.path().join("images").join(fid);
                            if img_path.exists() {
                                if let Ok(data) = std::fs::read(&img_path) {
                                    let encoded =
                                        base64::engine::general_purpose::STANDARD.encode(&data);
                                    images.push(("base64".to_string(), mt.clone(), encoded));
                                    found = true;
                                }
                                break;
                            }
                        }
                    }
                    if !found {
                        tracing::warn!(file_id = %fid, "could not resolve ImageRef to file on disk");
                    }
                }
                _ => {}
            }
        }
        if !images.is_empty() {
            break;
        }
    }
    images
}

/// Progress event for vision batch processing
#[derive(Debug, Clone)]
pub struct VisionBatchProgress {
    pub batch_index: usize,
    pub total_batches: usize,
    pub status: String, // "running", "done", "error"
    pub result: Option<String>,
}

/// Token usage from the vision auxiliary model.
#[derive(Debug, Clone, Default)]
pub struct VisionUsage {
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub model_name: String,
}

/// Result of the vision auxiliary processing.
pub struct VisionResult {
    pub context: String,
    pub usage: VisionUsage,
}

pub async fn prepare_vision_context(
    images: &[(String, String, String)],
    user_text: &str,
    vision_model_name: &str,
    model_configs: &[loom_types::ModelConfig],
    key_store: &Arc<RwLock<HashMap<String, String>>>,
    progress_tx: Option<tokio::sync::mpsc::Sender<VisionBatchProgress>>,
) -> Result<VisionResult> {
    let config = model_configs
        .iter()
        .find(|c| c.name == vision_model_name)
        .ok_or_else(|| {
            anyhow::anyhow!("Vision model '{}' not found in configs", vision_model_name)
        })?;

    let guard = key_store.read().await;
    let api_key = config
        .api_key_env
        .as_deref()
        .and_then(|raw| {
            if let Some(val) = guard.get(raw) {
                return Some(val.clone());
            }
            let looks_like_env_var = !raw.is_empty()
                && raw
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_');
            if !looks_like_env_var && !raw.is_empty() {
                Some(raw.to_string())
            } else {
                std::env::var(raw).ok().filter(|v| !v.is_empty())
            }
        })
        .or_else(|| match config.backend {
            ModelBackend::DeepSeek => guard
                .get("DEEPSEEK_API_KEY")
                .cloned()
                .or_else(|| std::env::var("DEEPSEEK_API_KEY").ok()),
            ModelBackend::OpenAI => guard
                .get("OPENAI_API_KEY")
                .cloned()
                .or_else(|| std::env::var("OPENAI_API_KEY").ok()),
            ModelBackend::Anthropic => guard
                .get("ANTHROPIC_API_KEY")
                .cloned()
                .or_else(|| std::env::var("ANTHROPIC_API_KEY").ok()),
            ModelBackend::Custom | ModelBackend::LmStudio | ModelBackend::Ollama => guard
                .get("OPENLOOM_API_KEY")
                .cloned()
                .or_else(|| std::env::var("OPENLOOM_API_KEY").ok()),
        })
        .unwrap_or_default();
    drop(guard);

    if api_key.is_empty() {
        anyhow::bail!(
            "Vision model '{}' has no API key: api_key_env is not set.\n\
             Fix: Settings → Models → Edit '{}' → set 'API Key 环境变量' to the env var name that holds the key (e.g. BAILIAN_API_KEY), then Save.",
            vision_model_name,
            vision_model_name,
        );
    }

    tracing::info!(
        vision_model = %vision_model_name,
        backend = %config.backend.name(),
        api_key_env = %config.api_key_env.as_deref().unwrap_or("<unset>"),
        api_key_len = api_key.len(),
        image_count = images.len(),
        "vision auxiliary calling provider"
    );

    let base_url = config
        .base_url
        .clone()
        .unwrap_or_else(|| match config.backend {
            ModelBackend::Anthropic => "https://api.anthropic.com".into(),
            ModelBackend::OpenAI => "https://api.openai.com/v1".into(),
            ModelBackend::DeepSeek => "https://api.deepseek.com/v1".into(),
            _ => "http://localhost:1234/v1".into(),
        });

    let model_id = config
        .model
        .clone()
        .unwrap_or_else(|| vision_model_name.to_string());

    let request_line = if user_text.trim().is_empty() {
        "(no explicit text request)".to_string()
    } else {
        user_text.trim().to_string()
    };

    // Process images in sequential batches of 3 to avoid rate limits.
    const BATCH_SIZE: usize = 3;
    let batches: Vec<&[(String, String, String)]> = images.chunks(BATCH_SIZE).collect();
    let batch_count = batches.len();

    tracing::info!(
        image_count = images.len(),
        batch_count,
        batch_size = BATCH_SIZE,
        "vision: processing sequentially in batches of {}",
        BATCH_SIZE
    );

    let mut analyses = Vec::new();
    let mut total_vision_prompt = 0usize;
    let mut total_vision_completion = 0usize;
    for (batch_idx, batch) in batches.iter().enumerate() {
        // Report batch start
        if let Some(ref tx) = progress_tx {
            let _ = tx
                .send(VisionBatchProgress {
                    batch_index: batch_idx,
                    total_batches: batch_count,
                    status: "running".to_string(),
                    result: None,
                })
                .await;
        }

        let client = OpenAIClient::new(api_key.clone(), model_id.clone(), base_url.clone(), false);

        let mut content_parts: Vec<ContentPart> = Vec::new();
        if batch_count > 1 {
            content_parts.push(ContentPart::Text {
                text: format!(
                    "{}\n\nUser request:\n{}\n\n(This is batch {}/{} containing {} image(s))",
                    VISION_PROMPT,
                    request_line,
                    batch_idx + 1,
                    batch_count,
                    batch.len()
                ),
            });
        } else {
            content_parts.push(ContentPart::Text {
                text: format!("{}\n\nUser request:\n{}", VISION_PROMPT, request_line),
            });
        }
        for (source_type, media_type, data) in batch.iter() {
            content_parts.push(ContentPart::Image {
                source_type: source_type.clone(),
                media_type: media_type.clone(),
                data: data.clone(),
            });
        }

        let messages = vec![Message {
            role: Role::User,
            content: content_parts,
            timestamp: Utc::now(),
            usage: None,
        }];

        let request = CompletionRequest {
            messages,
            tools: Vec::new(),
            tool_choice: None,
            prompt: String::new(),
            max_tokens: 2048,
            temperature: 0.0,
            top_p: 1.0,
            stop: Vec::new(),
            stream: false,
            thinking_budget: None,
        };

        match client.complete(request).await {
            Ok(response) => {
                total_vision_prompt += response.prompt_tokens;
                total_vision_completion += response.completion_tokens;
                let text = response.text.trim().to_string();
                let entry = if batch_count > 1 {
                    format!("[Batch {}/{}]\n{}", batch_idx + 1, batch_count, text)
                } else {
                    text.clone()
                };
                analyses.push(entry);

                // Report batch done
                if let Some(ref tx) = progress_tx {
                    let _ = tx
                        .send(VisionBatchProgress {
                            batch_index: batch_idx,
                            total_batches: batch_count,
                            status: "done".to_string(),
                            result: Some(text),
                        })
                        .await;
                }
            }
            Err(e) => {
                tracing::warn!(batch = batch_idx + 1, error = %e, "vision batch failed");
                let error_msg = format!(
                    "[Batch {}/{}] (analysis failed: {})",
                    batch_idx + 1,
                    batch_count,
                    e
                );
                analyses.push(error_msg.clone());

                // Report batch error
                if let Some(ref tx) = progress_tx {
                    let _ = tx
                        .send(VisionBatchProgress {
                            batch_index: batch_idx,
                            total_batches: batch_count,
                            status: "error".to_string(),
                            result: Some(error_msg),
                        })
                        .await;
                }
            }
        }
    }

    let combined = analyses.join("\n\n");
    let context = format!("<vision-context>\n{}\n</vision-context>", combined);
    Ok(VisionResult {
        context,
        usage: VisionUsage {
            prompt_tokens: total_vision_prompt,
            completion_tokens: total_vision_completion,
            model_name: vision_model_name.to_string(),
        },
    })
}

//! Vision auxiliary — processes images via a vision-capable model when the
//! main model lacks vision capabilities.

use anyhow::Result;
use loom_inference::engine::CloudClient;
use loom_inference::openai::OpenAIClient;
use loom_types::{CompletionRequest, ContentPart, Message, ModelBackend, Role};
use chrono::Utc;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct VisionConfig {
    pub enabled: bool,
    pub model: Option<String>,
}

impl Default for VisionConfig {
    fn default() -> Self {
        Self { enabled: false, model: None }
    }
}

const VISION_PROMPT: &str = r#"Analyze this image in detail. Respond in JSON with these fields:
- "image_overview": Brief description of what the image shows
- "visible_text": Any text/code visible in the image (OCR)
- "objects_and_layout": Key objects, their positions and relationships
- "charts_or_data": Any charts, tables, or structured data
- "user_context": What the user likely wants to know about this image

Be concise but thorough. Respond ONLY with valid JSON."#;

pub fn load_vision_config() -> VisionConfig {
    let home = dirs::home_dir().unwrap_or_default().join(".loom").join("vision.json");
    std::fs::read_to_string(&home)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn has_images(messages: &[Message]) -> bool {
    messages.iter().any(|m| {
        m.content.iter().any(|part| matches!(part, ContentPart::Image { .. }))
    })
}

pub fn extract_images(messages: &[Message]) -> Vec<(String, String, String)> {
    let mut images = Vec::new();
    for msg in messages.iter().rev() {
        for part in &msg.content {
            if let ContentPart::Image { source_type, media_type, data } = part {
                images.push((source_type.clone(), media_type.clone(), data.clone()));
            }
        }
        if !images.is_empty() {
            break;
        }
    }
    images
}

pub async fn prepare_vision_context(
    images: &[(String, String, String)],
    user_text: &str,
    vision_model_name: &str,
    model_configs: &[loom_types::ModelConfig],
) -> Result<String> {
    let config = model_configs
        .iter()
        .find(|c| c.name == vision_model_name)
        .ok_or_else(|| anyhow::anyhow!("Vision model '{}' not found in configs", vision_model_name))?;

    let api_key = config.api_key_env.as_deref()
        .and_then(|env| std::env::var(env).ok())
        .unwrap_or_default();

    let base_url = config.base_url.clone().unwrap_or_else(|| match config.backend {
        ModelBackend::Anthropic => "https://api.anthropic.com".into(),
        ModelBackend::OpenAI => "https://api.openai.com/v1".into(),
        ModelBackend::DeepSeek => "https://api.deepseek.com/v1".into(),
        _ => "http://localhost:1234/v1".into(),
    });

    let model_id = config.model.clone().unwrap_or_else(|| vision_model_name.to_string());

    let client = OpenAIClient::new(api_key, model_id, base_url, false);

    let mut content_parts: Vec<ContentPart> = Vec::new();
    content_parts.push(ContentPart::Text {
        text: format!("{}\n\nUser's message: {}", VISION_PROMPT, user_text),
    });
    for (source_type, media_type, data) in images {
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

    let response = client.complete(request).await?;
    let analysis = response.text.trim().to_string();

    Ok(format!("<vision-context>\n{}\n</vision-context>", analysis))
}

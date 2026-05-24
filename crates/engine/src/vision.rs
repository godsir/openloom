//! Vision auxiliary bridge: when the target model is text-only, send images
//! to a separate vision-capable model and inject textual descriptions into the
//! prompt so the text model can "see" the images.
//!
//! Architecture (Phase 1): stateless free functions, no caching, sequential
//! image processing. Configuration lives at `settings.models.models.vision`.

use anyhow::Result;
use openloom_inference::create_cloud_client;
use openloom_models::{ContentPart, ImagePart, Message, ModelBackend, ModelConfig};
use serde_json::Value;

const VISION_PROMPT_TEMPLATE: &str = r#"Analyze this image for another text-only model.
Return a concise note with these exact sections in JSON format.
Output only the JSON object, no markdown wrapping:

{
  "image_overview": "fixed basic description of what the image is",
  "visible_text": "important OCR or readable text",
  "objects_and_layout": "important objects, positions, counts, and relationships",
  "charts_or_data": "chart/table/data details if present; otherwise 'none'",
  "user_request": "restate the user's request in one short sentence",
  "user_request_answer": "answer the user's request using the image when possible",
  "evidence": "the visual evidence supporting that answer",
  "uncertainty": "anything unclear, hidden, or guessed"
}

Do not mention that you are a tool or a separate model.
Do not wrap the JSON in ``` fences.

User request:
{prompt}"#;

// ── Public API ──

/// Decide whether to intercept images and use auxiliary vision.
///
/// Returns true when ALL of:
/// - There are images to process
/// - Vision auxiliary is enabled in settings
/// - A valid vision model is configured
/// - The target backend is local (LmStudio/Ollama — no native image support)
pub fn should_use_auxiliary_vision(
    settings: &Value,
    target_backend: &ModelBackend,
    images: &[ImagePart],
) -> bool {
    if images.is_empty() {
        return false;
    }

    // Only local backends need the vision bridge; cloud APIs handle images natively
    if !target_backend.is_local_inference() {
        return false;
    }

    let models = settings
        .get("models")
        .and_then(|m| m.get("models"));

    let enabled = models
        .and_then(|m| m.get("vision_enabled"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if !enabled {
        return false;
    }

    let vision_ref = models.and_then(|m| m.get("vision"));
    let has_id = vision_ref
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .map(|s| !s.is_empty())
        .unwrap_or(false);
    let has_provider = vision_ref
        .and_then(|v| v.get("provider"))
        .and_then(|v| v.as_str())
        .map(|s| !s.is_empty())
        .unwrap_or(false);

    has_id && has_provider
}

/// Resolve the vision auxiliary model from settings into a ModelConfig.
/// Returns None if no vision model is configured or resolution fails.
pub fn resolve_vision_model_config(settings: &Value) -> Option<ModelConfig> {
    let models = settings.get("models").and_then(|m| m.get("models"))?;
    let vision_ref = models.get("vision")?;

    let id = vision_ref.get("id")?.as_str()?;
    let provider = vision_ref
        .get("provider")
        .and_then(|v| v.as_str())
        .unwrap_or("openai");

    if id.is_empty() {
        return None;
    }

    super::Engine::find_model_in_settings(settings, id, provider)
}

/// Main entry point. For each image, calls the vision auxiliary model,
/// parses the JSON response, and returns formatted <vision-context> XML text.
///
/// Returns Ok(Some(text)) on success, Ok(None) if vision is not configured,
/// Err(...) if the vision model call itself failed.
pub async fn prepare_vision_context(
    settings: &Value,
    prompt: &str,
    images: &[ImagePart],
) -> Result<Option<String>> {
    let model_cfg = match resolve_vision_model_config(settings) {
        Some(cfg) => cfg,
        None => return Ok(None),
    };

    let client = create_cloud_client(&model_cfg)?;

    let mut notes: Vec<String> = Vec::with_capacity(images.len());

    for (i, img) in images.iter().enumerate() {
        let vision_prompt = VISION_PROMPT_TEMPLATE.replace("{prompt}", prompt);
        let msg = build_vision_request(&vision_prompt, img);

        let req = openloom_inference::CompletionRequest {
            messages: vec![msg],
            max_tokens: 4096,
            ..Default::default()
        };

        match client.complete(req).await {
            Ok(resp) => {
                let note = if let Some(json) = extract_json_object(&resp.text) {
                    format_vision_note(i + 1, &json)
                } else {
                    format_fallback_vision_note(i + 1, &resp.text)
                };
                notes.push(note);
            }
            Err(e) => {
                tracing::warn!(error = %e, image_index = i, "vision auxiliary model call failed");
                notes.push(format!(
                    "image_{}: [analysis unavailable: {}]",
                    i + 1,
                    e
                ));
            }
        }
    }

    if notes.is_empty() {
        return Ok(None);
    }

    let combined = format!(
        "<vision-context>\n{}\n</vision-context>",
        notes.join("\n\n")
    );

    Ok(Some(combined))
}

// ── Build the vision request message ──

fn build_vision_request(prompt: &str, img: &ImagePart) -> Message {
    let parts = vec![
        ContentPart::Image {
            source_type: "base64".into(),
            media_type: img.mime_type.clone(),
            data: img.data.clone(),
        },
        ContentPart::Text {
            text: prompt.to_string(),
        },
    ];
    Message {
        role: openloom_models::Role::User,
        content: parts,
        timestamp: chrono::Utc::now(),
    }
}

// ── JSON extraction (mirrors openhanako's extractJsonObject) ──

/// Extract a JSON object from model response text.
/// Handles ```json fences, raw JSON, and partial JSON (first `{` to last `}`).
fn extract_json_object(text: &str) -> Option<Value> {
    // Try extracting from ```json...``` or ```...``` fences
    if let Some(inner) = extract_fenced_json(text)
        && let Ok(val) = serde_json::from_str::<Value>(&inner)
        && val.is_object()
    {
        return Some(val);
    }

    // Try parsing the whole trimmed text as JSON
    let trimmed = text.trim();
    if let Ok(val) = serde_json::from_str::<Value>(trimmed)
        && val.is_object()
    {
        return Some(val);
    }

    // Find first `{` and last `}`, try that slice
    let start = trimmed.find('{')?;
    let end = trimmed.rfind('}')?;
    if end > start {
        let slice = &trimmed[start..=end];
        if let Ok(val) = serde_json::from_str::<Value>(slice)
            && val.is_object()
        {
            return Some(val);
        }
    }

    None
}

fn extract_fenced_json(text: &str) -> Option<String> {
    let trimmed = text.trim();

    // Find ```json or ``` block
    let fence_start = trimmed.find("```")?;
    let after_fence = &trimmed[fence_start + 3..];
    // Skip optional language tag like "json"
    let content_start = after_fence.strip_prefix("json").unwrap_or(after_fence);
    let content_start = content_start.trim_start();
    // Find closing ```
    let fence_end = content_start.find("```")?;
    let inner = content_start[..fence_end].trim().to_string();

    if inner.is_empty() {
        return None;
    }
    Some(inner)
}

// ── Output formatting ──

/// Format a parsed JSON analysis into a <vision-context> note block.
fn format_vision_note(index: usize, analysis: &Value) -> String {
    let overview = str_or_none(analysis, "image_overview");
    let visible_text = str_or_none(analysis, "visible_text");
    let objects = str_or_none(analysis, "objects_and_layout");
    let charts = str_or_none(analysis, "charts_or_data");
    let request = str_or_none(analysis, "user_request");
    let answer = str_or_none(analysis, "user_request_answer");
    let evidence = str_or_none(analysis, "evidence");
    let uncertainty = str_or_none(analysis, "uncertainty");

    format!(
        "image_{index}:\n  image_overview: {overview}\n  visible_text: {visible_text}\n  \
         objects_and_layout: {objects}\n  charts_or_data: {charts}\n  \
         user_request: {request}\n  user_request_answer: {answer}\n  \
         evidence: {evidence}\n  uncertainty: {uncertainty}"
    )
}

/// Format a fallback note when JSON parsing fails.
/// Includes a raw excerpt so the text model still gets some information.
fn format_fallback_vision_note(index: usize, raw_response: &str) -> String {
    let excerpt: String = raw_response.chars().take(800).collect();
    let truncated = if raw_response.chars().count() > 800 {
        format!("{}...[truncated]", excerpt)
    } else {
        excerpt
    };
    format!(
        "image_{index}:\n  [structured analysis unavailable — raw response excerpt]\n  \
         raw_excerpt: {truncated}"
    )
}

fn str_or_none(val: &Value, key: &str) -> String {
    match val.get(key).and_then(|v| v.as_str()) {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => "none".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── should_use_auxiliary_vision ──

    #[test]
    fn test_no_images_returns_false() {
        let settings = serde_json::json!({});
        assert!(!should_use_auxiliary_vision(
            &settings,
            &ModelBackend::LmStudio,
            &[]
        ));
    }

    #[test]
    fn test_cloud_backend_returns_false() {
        let settings = enabled_vision_settings();
        let images = vec![test_image()];
        for backend in &[ModelBackend::Anthropic, ModelBackend::OpenAI, ModelBackend::DeepSeek] {
            assert!(
                !should_use_auxiliary_vision(&settings, backend, &images),
                "cloud backend {backend:?} should not use auxiliary vision"
            );
        }
    }

    #[test]
    fn test_local_backend_with_vision_enabled_returns_true() {
        let settings = enabled_vision_settings();
        let images = vec![test_image()];
        assert!(should_use_auxiliary_vision(
            &settings,
            &ModelBackend::LmStudio,
            &images
        ));
    }

    #[test]
    fn test_vision_disabled_returns_false() {
        let settings = serde_json::json!({
            "models": {
                "models": {
                    "vision_enabled": false,
                    "vision": {"id": "gpt-4o", "provider": "openai"}
                }
            }
        });
        let images = vec![test_image()];
        assert!(!should_use_auxiliary_vision(
            &settings,
            &ModelBackend::LmStudio,
            &images
        ));
    }

    #[test]
    fn test_no_vision_model_configured_returns_false() {
        let settings = serde_json::json!({
            "models": {
                "models": {
                    "vision_enabled": true
                }
            }
        });
        let images = vec![test_image()];
        assert!(!should_use_auxiliary_vision(
            &settings,
            &ModelBackend::LmStudio,
            &images
        ));
    }

    // ── JSON extraction ──

    #[test]
    fn test_extract_clean_json() {
        let json = r#"{"image_overview": "A cat", "visible_text": "none"}"#;
        let result = extract_json_object(json);
        assert!(result.is_some());
        let val = result.unwrap();
        assert_eq!(val["image_overview"], "A cat");
    }

    #[test]
    fn test_extract_fenced_json() {
        let text = "Here is my analysis:\n```json\n{\"image_overview\": \"A dog\"}\n```\nHope this helps!";
        let result = extract_json_object(text);
        assert!(result.is_some());
        let val = result.unwrap();
        assert_eq!(val["image_overview"], "A dog");
    }

    #[test]
    fn test_extract_partial_json() {
        let text = "Prefix text {\"key\": \"value\"} suffix text";
        let result = extract_json_object(text);
        assert!(result.is_some());
        let val = result.unwrap();
        assert_eq!(val["key"], "value");
    }

    #[test]
    fn test_extract_invalid_returns_none() {
        assert!(extract_json_object("just plain text").is_none());
        assert!(extract_json_object("").is_none());
    }

    #[test]
    fn test_extract_array_returns_none() {
        assert!(extract_json_object("[1, 2, 3]").is_none());
    }

    // ── Formatting ──

    #[test]
    fn test_format_vision_note_includes_all_fields() {
        let analysis = serde_json::json!({
            "image_overview": "A sunset photo",
            "visible_text": "none",
            "objects_and_layout": "Sun centered, ocean below",
            "charts_or_data": "none",
            "user_request": "What is this?",
            "user_request_answer": "A sunset over the ocean",
            "evidence": "Orange sky, water reflection",
            "uncertainty": "none"
        });
        let note = format_vision_note(1, &analysis);
        assert!(note.contains("image_1:"));
        assert!(note.contains("A sunset photo"));
        assert!(note.contains("A sunset over the ocean"));
    }

    #[test]
    fn test_format_vision_note_empty_fields_default_to_none() {
        let analysis = serde_json::json!({"image_overview": "", "visible_text": null});
        let note = format_vision_note(1, &analysis);
        assert!(note.contains("image_overview: none"));
        assert!(note.contains("visible_text: none"));
    }

    #[test]
    fn test_format_fallback_vision_note() {
        let note = format_fallback_vision_note(2, "some raw model output");
        assert!(note.contains("image_2:"));
        assert!(note.contains("[structured analysis unavailable"));
        assert!(note.contains("raw model output"));
    }

    // ── Helpers ──

    fn enabled_vision_settings() -> Value {
        serde_json::json!({
            "models": {
                "models": {
                    "vision_enabled": true,
                    "vision": {"id": "gpt-4o", "provider": "openai"}
                }
            }
        })
    }

    fn test_image() -> ImagePart {
        ImagePart {
            data: "iVBORw0KGgo=".into(),
            mime_type: "image/png".into(),
        }
    }
}

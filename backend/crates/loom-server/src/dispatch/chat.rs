//! Chat dispatch handlers — chat.send / chat.stop

use base64::Engine;
use loom_types::{ContentPart, ErrorCode, JsonRpcError};
use serde_json::{Value, json};

use super::err;
use crate::AppState;

pub async fn handle(
    state: &AppState,
    method: &str,
    p: &Value,
) -> Option<Result<Value, JsonRpcError>> {
    match method {
        "chat.send" => Some(handle_chat_send(state, p).await),
        "chat.stop" => Some(handle_chat_stop(state, p).await),
        _ => None,
    }
}

async fn handle_chat_send(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let content = p.get("content").and_then(|v| v.as_str()).unwrap_or("");
    let attached_files_count = p
        .get("attached_files")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    tracing::info!(
        content_len = content.len(),
        attached_files_count,
        session_id = %p.get("session_id").and_then(|v| v.as_str()).unwrap_or("default"),
        "[dispatch] chat.send received"
    );
    let attached_images = parse_attached_images(p);
    if !attached_images.is_empty() {
        tracing::info!(
            image_count = attached_images.len(),
            "parsed attached images"
        );
    }
    // Read text-based non-image files and inject their content
    let file_contents = parse_attached_file_contents(p);
    let combined_content = if file_contents.is_empty() {
        content.to_string()
    } else {
        let mut combined = content.to_string();
        combined.push_str(&file_contents);
        combined
    };
    if combined_content.is_empty() && attached_images.is_empty() {
        return Err(err(ErrorCode::InvalidRequest, "content required"));
    }
    let session_id = p
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or("default");

    // Optional per-message model override
    let model_override = p.get("model").and_then(|v| v.as_str());

    // Thinking level: off/auto/low/medium/high → budget
    let thinking_level = p
        .get("thinking_level")
        .and_then(|v| v.as_str())
        .unwrap_or("off");
    let thinking_budget: Option<usize> = match thinking_level {
        "low" => Some(2048),
        "medium" | "mid" => Some(8192),
        "high" => Some(32768),
        "auto" => Some(16384),
        _ => None, // "off" or unknown
    };

    // If model is explicitly provided and differs from active, switch it
    if let Some(model_name) = model_override {
        let active = state.orchestrator.active_model_name().await;
        if active.as_deref() != Some(model_name) {
            let _ = state.orchestrator.model_config_set_active(model_name).await;
        }
    }

    // Parse selected skills
    let selected_skills: Vec<String> = p
        .get("skills")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    tracing::info!(
        session_id = %p.get("session_id").and_then(|v| v.as_str()).unwrap_or("default"),
        skill_count = selected_skills.len(),
        skills = ?selected_skills,
        "[dispatch] chat.send selected_skills"
    );

    // Resolve agent config for this session
    let config_name = state
        .sessions
        .get_bound_agent(session_id)
        .await
        .unwrap_or_else(|| "default".to_string());
    let mut agent_config = state
        .orchestrator
        .agent_config_get(&config_name)
        .await
        .unwrap_or_default();

    // Parse permission mode
    let permission_mode = p
        .get("permission_mode")
        .and_then(|v| v.as_str())
        .unwrap_or("operate");

    // cc_dispatch override — can be set per-request or inherited from agent config
    let cc_dispatch = p
        .get("cc_dispatch")
        .and_then(|v| v.as_bool())
        .unwrap_or(agent_config.cc_dispatch);

    // Apply cc_dispatch override from RPC params
    agent_config.cc_dispatch = cc_dispatch;

    let skip_user_message = p
        .get("skip_user_message")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let result = state
        .orchestrator
        .process_message_with_config(
            &combined_content,
            session_id,
            &agent_config,
            thinking_budget,
            attached_images,
            selected_skills,
            permission_mode,
            skip_user_message,
        )
        .await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
    if !skip_user_message {
        state
            .sessions
            .add_message(session_id, "user", content)
            .await;
    }
    state
        .sessions
        .add_message(session_id, "assistant", &result.response)
        .await;
    Ok(json!({
        "response": result.response,
        "session_id": session_id,
        "tool_calls": result.tool_calls_made,
        "iterations": result.iterations,
        "tokens": result.prompt_tokens + result.completion_tokens,
    }))
}

async fn handle_chat_stop(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let session_id = p
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or("default");
    let killed = state
        .orchestrator
        .stop_session(session_id)
        .await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
    Ok(json!({ "ok": true, "killed": killed }))
}

// ---------------------------------------------------------------------------
// Helper: parse attached images
// ---------------------------------------------------------------------------

/// Parse attached_files from frontend JSON-RPC params into ContentPart::Image items.
/// Handles both data URL thumbnails (pasted images) and file paths (picked files).
fn parse_attached_images(p: &Value) -> Vec<ContentPart> {
    let files = p
        .get("attached_files")
        .and_then(|v| v.as_array())
        .map(|a| a.as_slice())
        .unwrap_or(&[]);

    tracing::info!(file_count = files.len(), "parse_attached_images: entry");

    let mut parts = Vec::new();
    for file in files {
        let mime_type = file
            .get("mime_type")
            .and_then(|v| v.as_str())
            .unwrap_or("image/png");

        let name = file.get("name").and_then(|v| v.as_str()).unwrap_or("?");
        let has_thumb = file
            .get("thumbnail")
            .and_then(|v| v.as_str())
            .map(|s| !s.is_empty())
            .unwrap_or(false);
        let has_path = file
            .get("path")
            .and_then(|v| v.as_str())
            .map(|s| !s.is_empty())
            .unwrap_or(false);

        tracing::info!(%name, %mime_type, has_thumb, has_path, "parse_attached_images: processing file");

        if !mime_type.starts_with("image/") {
            tracing::debug!(%name, %mime_type, "skipped non-image file");
            continue;
        }

        let data = if let Some(thumb) = file.get("thumbnail").and_then(|v| v.as_str()) {
            if thumb.is_empty() {
                tracing::warn!(%name, "empty thumbnail");
                continue;
            }
            // data URL format: "data:image/png;base64,XXXX"
            if let Some(comma) = thumb.find(',') {
                thumb[comma + 1..].to_string()
            } else {
                thumb.to_string()
            }
        } else if let Some(ref path) = file.get("path").and_then(|v| v.as_str()) {
            if path.is_empty() {
                tracing::warn!(%name, "empty thumbnail and empty path, skipping image");
                continue;
            }
            match std::fs::read(path) {
                Ok(bytes) => base64::engine::general_purpose::STANDARD.encode(&bytes),
                Err(e) => {
                    tracing::warn!(path = %path, error = %e, "failed to read image file");
                    continue;
                }
            }
        } else {
            tracing::warn!(%name, %mime_type, has_thumb, has_path, "no thumbnail or path for image, skipping");
            continue;
        };

        if data.is_empty() {
            continue;
        }

        parts.push(ContentPart::Image {
            source_type: "base64".to_string(),
            media_type: mime_type.to_string(),
            data,
        });
    }

    parts
}

/// Text-based file extensions whose contents can be injected into the prompt.
fn is_text_extension(name: &str) -> bool {
    let lower = name.to_lowercase();
    let text_exts = [
        ".txt",
        ".md",
        ".rs",
        ".py",
        ".js",
        ".ts",
        ".tsx",
        ".jsx",
        ".go",
        ".java",
        ".c",
        ".cpp",
        ".h",
        ".hpp",
        ".cs",
        ".rb",
        ".php",
        ".swift",
        ".kt",
        ".scala",
        ".sh",
        ".bash",
        ".zsh",
        ".fish",
        ".ps1",
        ".bat",
        ".json",
        ".yaml",
        ".yml",
        ".toml",
        ".ini",
        ".cfg",
        ".conf",
        ".xml",
        ".html",
        ".css",
        ".scss",
        ".less",
        ".svelte",
        ".vue",
        ".sql",
        ".graphql",
        ".proto",
        ".env",
        ".lock",
        ".dockerfile",
        ".makefile",
        ".log",
        ".csv",
        ".tsv",
        ".r",
        ".m",
        ".lua",
        ".pl",
        ".ex",
        ".exs",
        ".erl",
        ".hrl",
        ".zig",
        ".nim",
        ".v",
        ".dart",
        ".jl",
    ];
    text_exts.iter().any(|ext| lower.ends_with(ext))
        || lower.contains("makefile")
        || lower.contains("dockerfile")
}

/// Read text-based non-image attached files and return formatted content for the prompt.
///
/// Prefers the `content` field (file content read in the renderer via FileReader,
/// which works in Electron's sandbox mode where `File.path` is unavailable).
/// Falls back to reading from `path` on disk for backward compatibility.
fn parse_attached_file_contents(p: &Value) -> String {
    let files = p
        .get("attached_files")
        .and_then(|v| v.as_array())
        .map(|a| a.as_slice())
        .unwrap_or(&[]);

    let mut result = String::new();

    for file in files {
        let mime_type = file.get("mime_type").and_then(|v| v.as_str()).unwrap_or("");
        let name = file.get("name").and_then(|v| v.as_str()).unwrap_or("?");

        // Images are handled by parse_attached_images separately
        if mime_type.starts_with("image/") {
            continue;
        }

        // ── Primary path: content was read in the renderer (sandbox-safe) ──
        if let Some(content) = file.get("content").and_then(|v| v.as_str()) {
            if content.is_empty() {
                tracing::warn!(%name, "attached file has empty content from frontend");
                continue;
            }
            result.push_str(&format!(
                "\n\n<attached_file name=\"{name}\">\n{content}\n</attached_file>",
            ));
            tracing::info!(%name, len = content.len(), "attached file content injected (from frontend)");
            continue;
        }

        // ── Fallback: read file from disk path (legacy / non-sandbox) ──
        let path = file.get("path").and_then(|v| v.as_str()).unwrap_or("");
        if path.is_empty() {
            tracing::warn!(%name, "attached file has no content and no path — skipping");
            continue;
        }

        // Size check: skip files larger than 500 KB
        let metadata = match std::fs::metadata(path) {
            Ok(m) => m,
            Err(_) => {
                tracing::warn!(%name, %path, "attached file not found on disk");
                continue;
            }
        };
        if metadata.len() > 512_000 {
            tracing::warn!(%name, size = metadata.len(), "attached file too large (>500KB), skipping");
            continue;
        }

        // Only read text-based files (by MIME type or extension)
        let is_text = mime_type.starts_with("text/")
            || mime_type == "application/json"
            || mime_type.contains("xml")
            || mime_type.contains("javascript")
            || mime_type.contains("typescript")
            || is_text_extension(name);

        if !is_text {
            tracing::debug!(%name, %mime_type, "skipping non-text attached file");
            continue;
        }

        match std::fs::read_to_string(path) {
            Ok(content) => {
                if content.is_empty() {
                    continue;
                }
                result.push_str(&format!(
                    "\n\n<attached_file name=\"{name}\">\n{content}\n</attached_file>",
                ));
                tracing::info!(%name, len = content.len(), "attached file content injected (from disk)");
            }
            Err(e) => {
                tracing::warn!(%name, %path, error = %e, "failed to read attached file from disk (possibly wrong encoding for non-UTF-8 file)");
            }
        }
    }

    result
}

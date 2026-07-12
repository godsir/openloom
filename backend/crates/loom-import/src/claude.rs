//! Claude Code JSONL parsing + openLoom message mapping.

use std::path::Path;

use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Utc};
use loom_types::{ContentPart, ImportPayload, Message, Role, TokenUsage};
use serde_json::Value;

/// Lightweight metadata for one Claude Code conversation (scan pass).
#[derive(Debug, Clone, serde::Serialize)]
pub struct ConversationSummary {
    pub session_uuid: String,
    pub project_dir: String,
    pub title: Option<String>,
    pub first_message: Option<String>,
    pub message_count: usize,
    pub model: Option<String>,
    /// RFC3339 first-message timestamp.
    pub started_at: String,
    /// RFC3339 last-message timestamp.
    pub last_at: String,
    /// Filled by the caller (scan never sets this).
    pub already_imported: bool,
}

/// Scan every `*.jsonl` under `projects_dir/<project_dir>/`. Reads only the
/// metadata needed to list/preview conversations — does not parse full content.
pub fn scan(projects_dir: &Path) -> Result<Vec<ConversationSummary>> {
    let mut out = Vec::new();
    if !projects_dir.exists() {
        return Ok(out);
    }
    for entry in std::fs::read_dir(projects_dir)
        .with_context(|| format!("read {}", projects_dir.display()))?
    {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let project_dir = entry.file_name().to_string_lossy().to_string();
        for f in std::fs::read_dir(entry.path())? {
            let f = f?;
            if !f.file_type()?.is_file() {
                continue;
            }
            let path = f.path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_string();
            match scan_one(&path, &stem, &project_dir) {
                Ok(Some(s)) => out.push(s),
                Ok(None) => {}
                Err(e) => tracing::warn!(path = %path.display(), err = %e, "scan: skipping jsonl"),
            }
        }
    }
    Ok(out)
}

fn scan_one(
    path: &Path,
    session_uuid: &str,
    project_dir: &str,
) -> Result<Option<ConversationSummary>> {
    let file = std::fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    let reader = std::io::BufReader::new(file);

    let mut title: Option<String> = None;
    let mut first_message: Option<String> = None;
    let mut message_count = 0usize;
    let mut model: Option<String> = None;
    let mut started_at: Option<String> = None;
    let mut last_at: Option<String> = None;

    use std::io::BufRead;
    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let obj: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let is_sidechain = obj
            .get("isSidechain")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if is_sidechain {
            continue;
        }
        let ty = obj.get("type").and_then(|v| v.as_str()).unwrap_or("");
        let ts = obj
            .get("timestamp")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        match ty {
            "ai-title" => {
                if let Some(t) = obj.get("aiTitle").and_then(|v| v.as_str()) {
                    title = Some(t.to_string()); // last one wins
                }
            }
            "user" | "assistant" => {
                message_count += 1;
                if ty == "assistant"
                    && let Some(m) = obj
                        .get("message")
                        .and_then(|m| m.get("model"))
                        .and_then(|v| v.as_str())
                {
                    model = Some(m.to_string());
                }
                if ty == "user"
                    && first_message.is_none()
                    && let Some(content) = obj.get("message").and_then(|m| m.get("content"))
                {
                    first_message = Some(extract_text(content));
                }
                if let Some(ref t) = ts {
                    if started_at.is_none() {
                        started_at = Some(t.clone());
                    }
                    last_at = Some(t.clone());
                }
            }
            _ => {}
        }
    }

    let Some(started_at) = started_at else {
        return Ok(None); // no real messages
    };
    Ok(Some(ConversationSummary {
        session_uuid: session_uuid.to_string(),
        project_dir: project_dir.to_string(),
        title,
        first_message,
        message_count,
        model,
        started_at,
        last_at: last_at.unwrap_or_default(),
        already_imported: false,
    }))
}

/// Extract readable text from a Claude Code `message.content` value (string or block array).
fn extract_text(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(arr) => arr
            .iter()
            .filter_map(|b| {
                if b.get("type").and_then(|v| v.as_str()) == Some("text") {
                    b.get("text").and_then(|v| v.as_str()).map(str::to_string)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
    }
}

/// Parse one Claude Code JSONL into an [`ImportPayload`] ready to persist.
pub fn build_payload(jsonl_path: &Path) -> Result<ImportPayload> {
    let file = std::fs::File::open(jsonl_path)
        .with_context(|| format!("open {}", jsonl_path.display()))?;
    let reader = std::io::BufReader::new(file);

    let id = jsonl_path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow!("missing file stem: {}", jsonl_path.display()))?
        .to_string();

    let mut title: Option<String> = None;
    let mut messages: Vec<Message> = Vec::new();
    let mut created_at: Option<DateTime<Utc>> = None;
    let mut updated_at: Option<DateTime<Utc>> = None;
    let mut workspace_path: Option<String> = None;

    use std::io::BufRead;
    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let obj: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => {
                tracing::warn!(path = %jsonl_path.display(), "build: skipping unparseable line");
                continue;
            }
        };
        if obj
            .get("isSidechain")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            continue;
        }
        let ts = obj
            .get("timestamp")
            .and_then(|v| v.as_str())
            .and_then(parse_ts);
        if let Some(t) = ts {
            if created_at.is_none() {
                created_at = Some(t);
            }
            updated_at = Some(t);
        }
        if let Some(cwd) = obj.get("cwd").and_then(|v| v.as_str())
            && !cwd.is_empty()
        {
            workspace_path = Some(cwd.to_string());
        }
        let ty = obj.get("type").and_then(|v| v.as_str()).unwrap_or("");
        match ty {
            "ai-title" => {
                if let Some(t) = obj.get("aiTitle").and_then(|v| v.as_str()) {
                    title = Some(t.to_string());
                }
            }
            "user" => {
                if let Some(msg) = obj.get("message")
                    && let Some(m) = map_user_message(msg, ts)
                {
                    messages.push(m);
                }
            }
            "assistant" => {
                if let Some(msg) = obj.get("message")
                    && let Some(m) = map_assistant_message(msg, ts)
                {
                    messages.push(m);
                }
            }
            _ => {}
        }
    }

    let created_at = created_at
        .ok_or_else(|| anyhow!("no messages with timestamps in {}", jsonl_path.display()))?;
    Ok(ImportPayload {
        id,
        created_at,
        updated_at: updated_at.unwrap_or(created_at),
        title: title.or_else(|| messages.first().map(|m| m.text_content())),
        workspace_path,
        messages,
    })
}

fn map_user_message(msg: &Value, ts: Option<DateTime<Utc>>) -> Option<Message> {
    let content_val = msg.get("content")?;
    let parts = match content_val {
        Value::String(s) => vec![ContentPart::Text { text: s.clone() }],
        Value::Array(arr) => arr
            .iter()
            .filter_map(|b| {
                match b.get("type").and_then(|v| v.as_str()) {
                    Some("text") => {
                        b.get("text")
                            .and_then(|v| v.as_str())
                            .map(|t| ContentPart::Text {
                                text: t.to_string(),
                            })
                    }
                    Some("tool_result") => {
                        let id = b
                            .get("tool_use_id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let result =
                            extract_text(&b.get("content").cloned().unwrap_or(Value::Null));
                        Some(ContentPart::ToolResult {
                            tool_call_id: id,
                            name: String::new(),
                            result,
                        })
                    }
                    _ => None, // images etc. skipped
                }
            })
            .collect::<Vec<_>>(),
        _ => return None,
    };
    if parts.is_empty() {
        return None;
    }
    Some(Message {
        role: Role::User,
        content: parts,
        timestamp: ts.unwrap_or_else(Utc::now),
        usage: None,
    })
}

fn map_assistant_message(msg: &Value, ts: Option<DateTime<Utc>>) -> Option<Message> {
    let arr = msg.get("content")?.as_array()?;
    let parts: Vec<ContentPart> = arr
        .iter()
        .filter_map(|b| {
            match b.get("type").and_then(|v| v.as_str()) {
                Some("text") => b
                    .get("text")
                    .and_then(|v| v.as_str())
                    .map(|t| ContentPart::Text {
                        text: t.to_string(),
                    }),
                Some("thinking") => {
                    b.get("thinking")
                        .and_then(|v| v.as_str())
                        .map(|t| ContentPart::Thinking {
                            text: t.to_string(),
                        })
                }
                Some("tool_use") => {
                    let id = b
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let name = b
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let arguments = b.get("input").cloned().unwrap_or(Value::Null);
                    Some(ContentPart::ToolCall {
                        id,
                        name,
                        arguments,
                    })
                }
                _ => None, // images etc. skipped
            }
        })
        .collect();
    if parts.is_empty() {
        return None;
    }
    let usage = msg.get("usage").map(|u| TokenUsage {
        prompt_tokens: u.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
        completion_tokens: u.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
        model: msg
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        cached_tokens: 0, // recomputed by persistence layer (cache_read+cache_write)
        cache_read_tokens: u
            .get("cache_read_input_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize,
        cache_write_tokens: u
            .get("cache_creation_input_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize,
        context_window: 0,
        latency_ms: 0,
    });
    Some(Message {
        role: Role::Assistant,
        content: parts,
        timestamp: ts.unwrap_or_else(Utc::now),
        usage,
    })
}

fn parse_ts(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

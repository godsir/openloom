//! Claude Code JSONL parsing + openLoom message mapping.

use std::path::Path;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde_json::Value;

/// Lightweight metadata for one Claude Code conversation (scan pass).
#[derive(Debug, Clone)]
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

// (build_payload is added in Task 3.)
#[allow(unused)]
fn parse_ts(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

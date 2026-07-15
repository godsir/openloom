//! Codex CLI rollout JSONL parsing + openLoom message mapping.
//!
//! Format: `~/.codex/sessions/YYYY/MM/DD/rollout-<ts>-<uuid>.jsonl` (+ archived_sessions).
//! Each line: `{"timestamp","type","payload"}`. `type` ∈ session_meta | response_item | ...
//! `response_item.payload.type` ∈ message | function_call | function_call_output | reasoning.
//! Harness-injected user messages (text starting with `<`) are filtered out.

use std::path::Path;

use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Utc};
use loom_types::{ContentPart, ImportPayload, Message, Role, TokenUsage};
use serde_json::Value;

use crate::ConversationSummary;

/// Scan `~/.codex` for rollout-*.jsonl under sessions/ + archived_sessions/.
pub fn scan(root: &Path) -> Result<Vec<ConversationSummary>> {
    let mut out = Vec::new();
    if !root.exists() {
        return Ok(out);
    }
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for sub in ["sessions", "archived_sessions"] {
        let dir = root.join(sub);
        if dir.exists() {
            collect_jsonl(&dir, &mut out, &mut seen)?;
        }
    }
    Ok(out)
}

fn collect_jsonl(
    dir: &Path,
    out: &mut Vec<ConversationSummary>,
    seen: &mut std::collections::HashSet<String>,
) -> Result<()> {
    for entry in std::fs::read_dir(dir).with_context(|| format!("read {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_jsonl(&path, out, seen)?;
        } else if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
            match scan_one(&path) {
                Ok(Some(s)) => {
                    if seen.insert(s.session_uuid.clone()) {
                        out.push(s);
                    }
                }
                Ok(None) => {}
                Err(e) => tracing::warn!(path = %path.display(), err = %e, "codex scan: skip"),
            }
        }
    }
    Ok(())
}

fn scan_one(path: &Path) -> Result<Option<ConversationSummary>> {
    let file = std::fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    let reader = std::io::BufReader::new(file);

    let mut id: Option<String> = None;
    let mut project_dir = String::new();
    let mut message_count = 0usize;
    let mut first_message: Option<String> = None;
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
        let ty = obj.get("type").and_then(|v| v.as_str()).unwrap_or("");
        let ts = obj.get("timestamp").and_then(|v| v.as_str()).map(str::to_string);
        let payload = obj.get("payload").cloned().unwrap_or(Value::Null);
        match ty {
            "session_meta" => {
                if id.is_none()
                    && let Some(sid) = payload
                        .get("session_id")
                        .and_then(|v| v.as_str())
                        .or_else(|| payload.get("id").and_then(|v| v.as_str()))
                {
                    id = Some(sid.to_string());
                }
                if let Some(cwd) = payload.get("cwd").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
                    project_dir = cwd.to_string();
                }
                if model.is_none()
                    && let Some(m) = payload.get("model").and_then(|v| v.as_str())
                {
                    model = Some(m.to_string());
                }
            }
            "response_item" => {
                let pty = payload.get("type").and_then(|v| v.as_str()).unwrap_or("");
                if pty == "message" {
                    let role = payload.get("role").and_then(|v| v.as_str()).unwrap_or("");
                    if role == "user" || role == "assistant" {
                        message_count += 1;
                        if role == "user" && first_message.is_none() {
                            let text = extract_message_text(&payload, role);
                            if !text.is_empty() && !text.starts_with('<') {
                                first_message = Some(text);
                            }
                        }
                        if let Some(ref t) = ts {
                            if started_at.is_none() {
                                started_at = Some(t.clone());
                            }
                            last_at = Some(t.clone());
                        }
                    }
                }
            }
            _ => {}
        }
    }

    let Some(id) = id else { return Ok(None) };
    let Some(started_at) = started_at else { return Ok(None) };
    Ok(Some(ConversationSummary {
        session_uuid: id,
        project_dir,
        title: None,
        first_message,
        message_count,
        model,
        started_at,
        last_at: last_at.unwrap_or_default(),
        already_imported: false,
    }))
}

/// user/developer → input_text; assistant → output_text.
fn extract_message_text(payload: &Value, role: &str) -> String {
    let block_ty = if role == "assistant" { "output_text" } else { "input_text" };
    match payload.get("content") {
        Some(Value::Array(arr)) => arr
            .iter()
            .filter_map(|b| {
                if b.get("type").and_then(|v| v.as_str()) == Some(block_ty) {
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

/// Parse one Codex rollout jsonl into an [`ImportPayload`].
pub fn build_payload(path: &Path) -> Result<ImportPayload> {
    let file = std::fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    let reader = std::io::BufReader::new(file);

    let mut id: Option<String> = None;
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
            Err(_) => continue,
        };
        let ty = obj.get("type").and_then(|v| v.as_str()).unwrap_or("");
        let ts = obj.get("timestamp").and_then(|v| v.as_str()).and_then(parse_ts);
        let payload = obj.get("payload").cloned().unwrap_or(Value::Null);
        match ty {
            "session_meta" => {
                if id.is_none()
                    && let Some(sid) = payload
                        .get("session_id")
                        .and_then(|v| v.as_str())
                        .or_else(|| payload.get("id").and_then(|v| v.as_str()))
                {
                    id = Some(sid.to_string());
                }
                if let Some(cwd) = payload.get("cwd").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
                    workspace_path = Some(cwd.to_string());
                }
            }
            "response_item" => {
                let pty = payload.get("type").and_then(|v| v.as_str()).unwrap_or("");
                match pty {
                    "message" => {
                        let role = payload.get("role").and_then(|v| v.as_str()).unwrap_or("");
                        if let Some(m) = map_message(&payload, role, ts) {
                            if let Some(t) = ts {
                                if created_at.is_none() {
                                    created_at = Some(t);
                                }
                                updated_at = Some(t);
                            }
                            messages.push(m);
                        }
                    }
                    "reasoning" => {
                        if let Some(m) = map_reasoning(&payload, ts) {
                            messages.push(m);
                        }
                    }
                    "function_call" => {
                        if let Some(m) = map_function_call(&payload, ts) {
                            messages.push(m);
                        }
                    }
                    "function_call_output" => {
                        if let Some(m) = map_function_call_output(&payload, ts) {
                            messages.push(m);
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    let id = id.ok_or_else(|| anyhow!("no session_meta id in {}", path.display()))?;
    let created_at = created_at
        .ok_or_else(|| anyhow!("no messages with timestamps in {}", path.display()))?;
    Ok(ImportPayload {
        id,
        created_at,
        updated_at: updated_at.unwrap_or(created_at),
        title: messages
            .iter()
            .find(|m| m.role == Role::User)
            .map(|m| m.text_content())
            .filter(|t| !t.is_empty())
            .or_else(|| Some("未命名".to_string())),
        workspace_path,
        messages,
    })
}

fn map_message(payload: &Value, role: &str, ts: Option<DateTime<Utc>>) -> Option<Message> {
    let block_ty = if role == "assistant" { "output_text" } else { "input_text" };
    let content = payload.get("content")?;
    let parts: Vec<ContentPart> = match content {
        Value::Array(arr) => arr
            .iter()
            .filter_map(|b| {
                if b.get("type").and_then(|v| v.as_str()) == Some(block_ty) {
                    b.get("text").and_then(|v| v.as_str()).map(|t| ContentPart::Text { text: t.to_string() })
                } else {
                    None
                }
            })
            .collect(),
        _ => return None,
    };
    if parts.is_empty() {
        return None;
    }
    // Filter harness-injected user messages (<user_instructions> etc.)
    if role == "user"
        && let Some(ContentPart::Text { text }) = parts.first()
        && text.starts_with('<')
    {
        return None;
    }
    let (r, usage) = if role == "assistant" {
        let usage = payload.get("usage").map(|u| TokenUsage {
            prompt_tokens: u.get("input").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
            completion_tokens: u.get("output").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
            model: String::new(),
            cached_tokens: 0,
            cache_read_tokens: u.get("cacheRead").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
            cache_write_tokens: u.get("cacheWrite").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
            context_window: 0,
            latency_ms: 0,
        });
        (Role::Assistant, usage)
    } else {
        (Role::User, None)
    };
    Some(Message {
        role: r,
        content: parts,
        timestamp: ts.unwrap_or_else(Utc::now),
        usage,
    })
}

/// reasoning payload.summary[] = [{type:summary_text, text}].
fn map_reasoning(payload: &Value, ts: Option<DateTime<Utc>>) -> Option<Message> {
    let text = match payload.get("summary") {
        Some(Value::Array(arr)) => arr
            .iter()
            .filter_map(|b| {
                if b.get("type").and_then(|v| v.as_str()) == Some("summary_text") {
                    b.get("text").and_then(|v| v.as_str()).map(str::to_string)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => return None,
    };
    if text.is_empty() {
        return None;
    }
    Some(Message {
        role: Role::Assistant,
        content: vec![ContentPart::Thinking { text }],
        timestamp: ts.unwrap_or_else(Utc::now),
        usage: None,
    })
}

fn map_function_call(payload: &Value, ts: Option<DateTime<Utc>>) -> Option<Message> {
    let name = payload.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let call_id = payload
        .get("call_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let arguments = match payload.get("arguments") {
        Some(Value::String(s)) => serde_json::from_str(s).unwrap_or(Value::Null),
        Some(v) => v.clone(),
        None => Value::Null,
    };
    Some(Message {
        role: Role::Assistant,
        content: vec![ContentPart::ToolCall { id: call_id, name, arguments }],
        timestamp: ts.unwrap_or_else(Utc::now),
        usage: None,
    })
}

/// Tool results map to a user-role message carrying a ToolResult part (matches
/// Claude import's mapping of Anthropic tool_result blocks).
fn map_function_call_output(payload: &Value, ts: Option<DateTime<Utc>>) -> Option<Message> {
    let call_id = payload
        .get("call_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let output = payload
        .get("output")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    Some(Message {
        role: Role::User,
        content: vec![ContentPart::ToolResult {
            tool_call_id: call_id,
            name: String::new(),
            result: output,
        }],
        timestamp: ts.unwrap_or_else(Utc::now),
        usage: None,
    })
}

fn parse_ts(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

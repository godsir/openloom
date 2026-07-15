//! OpenClaw session JSONL parsing + openLoom message mapping.
//!
//! Format: `~/.openclaw/agents/<agentId>/sessions/<session-id>.jsonl`
//! Each line is an event with a `type` field. `session` (line 1) → {version,id,timestamp,cwd};
//! `message` → `message.{role, content[], usage?, model?}`. content blocks: text/thinking/toolCall.
//! role=toolResult carries toolCallId/toolName/content[]. Skips `.deleted.*` files.
//! Timestamps may be RFC3339 strings or epoch-ms (number/string) — parsed defensively.

use std::path::Path;

use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Utc};
use loom_types::{ContentPart, ImportPayload, Message, Role, TokenUsage};
use serde_json::Value;

use crate::ConversationSummary;

/// Scan `~/.openclaw/agents` for `<agent>/sessions/*.jsonl` (skipping .deleted).
pub fn scan(agents_dir: &Path) -> Result<Vec<ConversationSummary>> {
    let mut out = Vec::new();
    if !agents_dir.exists() {
        return Ok(out);
    }
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for agent in std::fs::read_dir(agents_dir).with_context(|| format!("read {}", agents_dir.display()))? {
        let agent = agent?;
        let sessions_dir = agent.path().join("sessions");
        if !sessions_dir.is_dir() {
            continue;
        }
        for f in std::fs::read_dir(&sessions_dir)? {
            let f = f?;
            let path = f.path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if name.contains(".deleted.") {
                continue;
            }
            match scan_one(&path) {
                Ok(Some(s)) => {
                    if seen.insert(s.session_uuid.clone()) {
                        out.push(s);
                    }
                }
                Ok(None) => {}
                Err(e) => tracing::warn!(path = %path.display(), err = %e, "openclaw scan: skip"),
            }
        }
    }
    Ok(out)
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
        match ty {
            "session" => {
                if id.is_none()
                    && let Some(sid) = obj.get("id").and_then(|v| v.as_str())
                {
                    id = Some(sid.to_string());
                }
                if let Some(cwd) = obj.get("cwd").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
                    project_dir = cwd.to_string();
                }
                if let Some(t) = parse_ts_value(obj.get("timestamp")) {
                    let s = t.to_rfc3339();
                    if started_at.is_none() {
                        started_at = Some(s.clone());
                    }
                    last_at = Some(s);
                }
            }
            "message" => {
                let msg = match obj.get("message") {
                    Some(m) => m,
                    None => continue,
                };
                let role = msg.get("role").and_then(|v| v.as_str()).unwrap_or("");
                if role == "user" || role == "assistant" {
                    message_count += 1;
                    if role == "user" && first_message.is_none() {
                        if let Some(text) = extract_content_text(msg.get("content")) {
                            if !text.is_empty() {
                                first_message = Some(text);
                            }
                        }
                    }
                    if model.is_none()
                        && let Some(m) = msg.get("model").and_then(|v| v.as_str())
                    {
                        model = Some(m.to_string());
                    }
                    if let Some(t) = parse_ts_value(msg.get("timestamp")).or_else(|| parse_ts_value(obj.get("timestamp"))) {
                        let s = t.to_rfc3339();
                        if started_at.is_none() {
                            started_at = Some(s.clone());
                        }
                        last_at = Some(s);
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

fn extract_content_text(content: Option<&Value>) -> Option<String> {
    match content? {
        Value::Array(arr) => {
            let t: String = arr
                .iter()
                .filter_map(|b| {
                    if b.get("type").and_then(|v| v.as_str()) == Some("text") {
                        b.get("text").and_then(|v| v.as_str()).map(str::to_string)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join("");
            Some(t)
        }
        Value::String(s) => Some(s.clone()),
        _ => None,
    }
}

fn parse_ts_value(v: Option<&Value>) -> Option<DateTime<Utc>> {
    match v {
        Some(Value::String(s)) => parse_ts(s),
        Some(Value::Number(n)) => n.as_i64().and_then(|ms| DateTime::from_timestamp_millis(ms)),
        _ => None,
    }
}

fn parse_ts(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
        .or_else(|| s.parse::<i64>().ok().and_then(|ms| DateTime::from_timestamp_millis(ms)))
}

/// Parse one OpenClaw session jsonl into an [`ImportPayload`].
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
        match ty {
            "session" => {
                if id.is_none()
                    && let Some(sid) = obj.get("id").and_then(|v| v.as_str())
                {
                    id = Some(sid.to_string());
                }
                if let Some(cwd) = obj.get("cwd").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
                    workspace_path = Some(cwd.to_string());
                }
            }
            "message" => {
                let msg = match obj.get("message") {
                    Some(m) => m,
                    None => continue,
                };
                let role = msg.get("role").and_then(|v| v.as_str()).unwrap_or("");
                let ts = parse_ts_value(msg.get("timestamp")).or_else(|| parse_ts_value(obj.get("timestamp")));
                match role {
                    "user" => {
                        if let Some(m) = map_user(msg, ts) {
                            if let Some(t) = ts {
                                if created_at.is_none() { created_at = Some(t); }
                                updated_at = Some(t);
                            }
                            messages.push(m);
                        }
                    }
                    "assistant" => {
                        if let Some(m) = map_assistant(msg, ts) {
                            if let Some(t) = ts {
                                if created_at.is_none() { created_at = Some(t); }
                                updated_at = Some(t);
                            }
                            messages.push(m);
                        }
                    }
                    "toolResult" => {
                        if let Some(m) = map_tool_result(msg, ts) {
                            messages.push(m);
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    let id = id.ok_or_else(|| anyhow!("no session id in {}", path.display()))?;
    let created_at = created_at.ok_or_else(|| anyhow!("no messages with timestamps in {}", path.display()))?;
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

fn map_user(msg: &Value, ts: Option<DateTime<Utc>>) -> Option<Message> {
    let parts = map_content_parts(msg.get("content"))?;
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

fn map_assistant(msg: &Value, ts: Option<DateTime<Utc>>) -> Option<Message> {
    let parts = map_content_parts(msg.get("content"))?;
    if parts.is_empty() {
        return None;
    }
    let usage = msg.get("usage").map(|u| TokenUsage {
        prompt_tokens: u.get("input").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
        completion_tokens: u.get("output").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
        model: msg.get("model").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        cached_tokens: 0,
        cache_read_tokens: u.get("cacheRead").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
        cache_write_tokens: u.get("cacheWrite").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
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

fn map_tool_result(msg: &Value, ts: Option<DateTime<Utc>>) -> Option<Message> {
    let tool_call_id = msg.get("toolCallId").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let name = msg.get("toolName").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let result = extract_content_text(msg.get("content")).unwrap_or_default();
    Some(Message {
        role: Role::User,
        content: vec![ContentPart::ToolResult { tool_call_id, name, result }],
        timestamp: ts.unwrap_or_else(Utc::now),
        usage: None,
    })
}

fn map_content_parts(content: Option<&Value>) -> Option<Vec<ContentPart>> {
    match content? {
        Value::Array(arr) => {
            let parts: Vec<ContentPart> = arr
                .iter()
                .filter_map(|b| match b.get("type").and_then(|v| v.as_str()) {
                    Some("text") => b.get("text").and_then(|v| v.as_str()).map(|t| ContentPart::Text { text: t.to_string() }),
                    Some("thinking") => b.get("thinking").and_then(|v| v.as_str()).map(|t| ContentPart::Thinking { text: t.to_string() }),
                    Some("toolCall") => {
                        let id = b.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let name = b.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let arguments = b.get("arguments").cloned().unwrap_or(Value::Null);
                        Some(ContentPart::ToolCall { id, name, arguments })
                    }
                    _ => None,
                })
                .collect();
            Some(parts)
        }
        Value::String(s) => Some(vec![ContentPart::Text { text: s.clone() }]),
        _ => None,
    }
}

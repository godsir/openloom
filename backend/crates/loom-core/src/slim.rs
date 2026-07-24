//! Graduated context slimming for small-window models.
//!
//! Replaces the binary compact_mode with a window-aware profile so local
//! models keep as much capability as their context window can afford:
//!
//! - **Full** (>= 32K): everything as today.
//! - **Slim** (8K..32K): core tool whitelist with minified schemas, persona
//!   truncated to 500 chars, dynamic context truncated to 1500 chars,
//!   few-shots dropped. Models without native function calling get a compact
//!   text tool catalog instead of JSON schemas — inline-JSON calls are still
//!   parsed and executed server-side, so slim local models KEEP tool use.
//! - **Tiny** (< 8K, or manual compact_mode): the previous compact behaviour —
//!   schemas / persona / few-shots / dynamic context / todo all stripped.
//!
//! Also home to the pre-flight prompt guard: before each LLM call the
//! assembled prompt is token-counted with the model's real tokenizer and
//! trimmed (history → injected dynamic context → tool outputs) instead of
//! dying on a provider-side context-overflow 400.

use loom_types::{ContentPart, Message, Role, ToolDefinition};

// Re-export the model-aware token counter used by the preflight guard.
use loom_context::{TokenizerId, message_tokens_with_id};

/// Graduated slimming tier for one agent turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlimLevel {
    Full,
    Slim,
    Tiny,
}

pub const SLIM_WINDOW_MAX: usize = 32_768;
pub const TINY_WINDOW_MAX: usize = 8_192;

/// Compute the slim tier. Manual compact_mode maps to Tiny (preserves the
/// existing semantics of the user-facing checkbox).
pub fn slim_level(compact_mode: bool, context_window: usize) -> SlimLevel {
    if compact_mode {
        return SlimLevel::Tiny;
    }
    if context_window < TINY_WINDOW_MAX {
        SlimLevel::Tiny
    } else if context_window < SLIM_WINDOW_MAX {
        SlimLevel::Slim
    } else {
        SlimLevel::Full
    }
}

/// Core tools kept in Slim mode — enough for real file/shell work without
/// the full catalog cost (the full set costs thousands of tokens in schemas).
pub const CORE_SLIM_TOOLS: &[&str] =
    &["file_read", "file_edit", "file_write", "shell", "file_glob"];

/// One-line description + JSON schema with verbose keys stripped
/// (descriptions/examples/defaults inside properties are the token bulk).
pub fn minify_tool_definition(def: &ToolDefinition) -> ToolDefinition {
    let mut out = def.clone();
    out.description = out
        .description
        .lines()
        .next()
        .unwrap_or("")
        .chars()
        .take(80)
        .collect();
    strip_schema_verbose(&mut out.input_schema);
    out
}

fn strip_schema_verbose(v: &mut serde_json::Value) {
    match v {
        serde_json::Value::Object(map) => {
            // Keep `type` / `properties` / `required` / `items` / `enum`;
            // drop the prose that inflates the schema.
            for key in ["description", "examples", "default", "title"] {
                map.remove(key);
            }
            for val in map.values_mut() {
                strip_schema_verbose(val);
            }
        }
        serde_json::Value::Array(arr) => {
            for val in arr.iter_mut() {
                strip_schema_verbose(val);
            }
        }
        _ => {}
    }
}

/// Compact text tool catalog for the text-protocol fallback (~150-250 tokens).
/// The call format matches `parse_inline_tool_calls` on the server:
/// `{"tool": "<name>", "arguments": {...}}`.
pub fn build_text_tool_catalog(defs: &[ToolDefinition]) -> String {
    let mut out = String::from(
        "可用工具（以 JSON 调用，格式 {\"tool\": \"名称\", \"arguments\": {参数}}）：\n",
    );
    for d in defs {
        let params: Vec<String> = d
            .input_schema
            .get("properties")
            .and_then(|p| p.as_object())
            .map(|o| o.keys().cloned().collect())
            .unwrap_or_default();
        let desc = d.description.lines().next().unwrap_or("");
        out.push_str(&format!("- {}({}) — {}\n", d.name, params.join(", "), desc));
    }
    out
}

/// Char-boundary truncation used for persona / dynamic context in Slim mode.
pub fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let head: String = s.chars().take(max).collect();
    format!("{head}…")
}

// ── Pre-flight prompt guard ────────────────────────────────────────────────

/// What the preflight guard trimmed, for logging and the user-facing notice.
#[derive(Debug, Default, Clone, Copy)]
pub struct PreflightReport {
    pub history_dropped: usize,
    pub dynamic_dropped: usize,
    pub tool_outputs_truncated: usize,
}

impl PreflightReport {
    pub fn is_clean(&self) -> bool {
        self.history_dropped == 0 && self.dynamic_dropped == 0 && self.tool_outputs_truncated == 0
    }

    pub fn describe(&self) -> String {
        let mut parts = Vec::new();
        if self.history_dropped > 0 {
            parts.push(format!("丢弃{}条旧历史", self.history_dropped));
        }
        if self.dynamic_dropped > 0 {
            parts.push(format!("丢弃{}段动态上下文", self.dynamic_dropped));
        }
        if self.tool_outputs_truncated > 0 {
            parts.push(format!("截断{}条超长工具输出", self.tool_outputs_truncated));
        }
        parts.join("，")
    }
}

/// Estimated total request tokens: every message (text + tool parts) plus the
/// serialized tool schemas, using the model's real tokenizer.
pub fn estimate_request_tokens(
    messages: &[Message],
    tools: &[ToolDefinition],
    tid: TokenizerId,
) -> usize {
    let bpe = tid.get();
    let mut total: usize = messages.iter().map(|m| message_tokens_with_id(m, tid)).sum();
    if !tools.is_empty() {
        let schema_json = serde_json::to_string(tools).unwrap_or_default();
        total += bpe.encode_with_special_tokens(&schema_json).len();
    }
    total
}

/// Hard cap for a single tool result after preflight truncation.
const PREFLIGHT_TOOL_OUTPUT_MAX_CHARS: usize = 800;

/// Trim the assembled prompt to fit `context_window - max_output - 256`.
/// Trim order (least valuable first relative to the current turn):
///
/// 1. oldest history messages (never the final message — that's the live input)
/// 2. injected System messages after index 0 (few-shots / dynamic / todo / notes)
/// 3. oversized tool outputs (head+tail kept)
///
/// Returns a report; when non-empty the caller should re-run the message
/// sanitizer to drop orphaned tool calls/results.
pub fn preflight_trim(
    messages: &mut Vec<Message>,
    tools: &[ToolDefinition],
    context_window: usize,
    max_output: usize,
    tid: TokenizerId,
) -> PreflightReport {
    let mut report = PreflightReport::default();
    let budget = context_window.saturating_sub(max_output + 256);
    if budget == 0 {
        return report;
    }
    let mut total = estimate_request_tokens(messages, tools, tid);
    if total <= budget {
        return report;
    }

    // 1. Oldest history (non-System) first. Index 0 is the stable system
    // prefix; injected System messages after it are handled in step 2.
    while total > budget && messages.len() > 1 {
        let Some(i) = messages.iter().position(|m| m.role != Role::System) else {
            break;
        };
        if i >= messages.len() - 1 {
            break; // never drop the live input / newest message
        }
        let cost = message_tokens_with_id(&messages[i], tid);
        messages.remove(i);
        report.history_dropped += 1;
        total = total.saturating_sub(cost);
    }

    // 2. Injected System messages after index 0 (dynamic context etc.).
    while total > budget && messages.len() > 1 {
        let Some(i) = messages
            .iter()
            .enumerate()
            .skip(1)
            .find(|(_, m)| m.role == Role::System)
            .map(|(i, _)| i)
        else {
            break;
        };
        if i >= messages.len() - 1 {
            break;
        }
        let cost = message_tokens_with_id(&messages[i], tid);
        messages.remove(i);
        report.dynamic_dropped += 1;
        total = total.saturating_sub(cost);
    }

    // 3. Oversized tool outputs (head 3/4 + tail 1/4).
    if total > budget {
        for msg in messages.iter_mut() {
            for part in msg.content.iter_mut() {
                if let ContentPart::ToolResult { result, .. } = part {
                    let len = result.chars().count();
                    if len > PREFLIGHT_TOOL_OUTPUT_MAX_CHARS {
                        let head_len = PREFLIGHT_TOOL_OUTPUT_MAX_CHARS * 3 / 4;
                        let tail_len = PREFLIGHT_TOOL_OUTPUT_MAX_CHARS - head_len;
                        let head: String = result.chars().take(head_len).collect();
                        let tail: String = result.chars().skip(len - tail_len).collect();
                        *result = format!("{head}\n...[已截断]...\n{tail}");
                        report.tool_outputs_truncated += 1;
                    }
                }
            }
        }
    }

    report
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(name: &str, desc: &str, params: &[&str]) -> ToolDefinition {
        let props = params
            .iter()
            .map(|p| {
                (
                    p.to_string(),
                    serde_json::json!({
                        "type": "string",
                        "description": format!("{p} 的详细说明，很长很长的描述文本"),
                        "examples": ["a", "b"],
                    }),
                )
            })
            .collect::<serde_json::Map<_, _>>();
        ToolDefinition {
            name: name.into(),
            description: desc.into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": props,
                "required": params,
            }),
            tags: vec![],
        }
    }

    fn text_msg(role: Role, text: &str) -> Message {
        Message {
            role,
            content: vec![ContentPart::Text {
                text: text.to_string(),
            }],
            timestamp: chrono::Utc::now(),
            usage: None,
        }
    }

    #[test]
    fn slim_level_tiers() {
        assert_eq!(slim_level(false, 100_000), SlimLevel::Full);
        assert_eq!(slim_level(false, 32_768), SlimLevel::Full);
        assert_eq!(slim_level(false, 16_384), SlimLevel::Slim);
        assert_eq!(slim_level(false, 8_192), SlimLevel::Slim);
        assert_eq!(slim_level(false, 4_096), SlimLevel::Tiny);
        // 手动精简 = Tiny，无论窗口多大
        assert_eq!(slim_level(true, 100_000), SlimLevel::Tiny);
    }

    #[test]
    fn minify_strips_verbose_schema_keys() {
        let d = def("file_read", "读取文件内容。\n第二行详细说明会被丢弃。", &["path"]);
        let m = minify_tool_definition(&d);
        assert_eq!(m.description, "读取文件内容。");
        let path = &m.input_schema["properties"]["path"];
        assert!(path.get("description").is_none());
        assert!(path.get("examples").is_none());
        assert_eq!(path["type"], "string");
        // required 保留
        assert_eq!(m.input_schema["required"][0], "path");
    }

    #[test]
    fn catalog_compact_and_parseable_shape() {
        let defs = vec![def("file_read", "读取文件", &["path"]), def("shell", "执行命令", &["command"])];
        let c = build_text_tool_catalog(&defs);
        assert!(c.contains("file_read(path)"));
        assert!(c.contains("shell(command)"));
        assert!(c.contains("{\"tool\""));
        assert!(c.chars().count() < 400);
    }

    #[test]
    fn preflight_clean_when_fits() {
        let mut msgs = vec![text_msg(Role::System, "system"), text_msg(Role::User, "hi")];
        let r = preflight_trim(&mut msgs, &[], 100_000, 4096, TokenizerId::Cl100k);
        assert!(r.is_clean());
        assert_eq!(msgs.len(), 2);
    }

    #[test]
    fn preflight_drops_oldest_history_first() {
        let mut msgs = vec![text_msg(Role::System, "system prompt")];
        // 50 条大历史消息
        for i in 0..50 {
            let mut s = "长".repeat(200);
            s.push_str(&i.to_string());
            msgs.push(text_msg(Role::User, &s));
        }
        msgs.push(text_msg(Role::User, "当前问题"));
        let r = preflight_trim(&mut msgs, &[], 4_096, 2_048, TokenizerId::Cl100k);
        assert!(r.history_dropped > 0);
        // 最后一条（当前输入）必须保留
        let last = msgs.last().unwrap();
        match &last.content[0] {
            ContentPart::Text { text } => assert_eq!(text, "当前问题"),
            _ => panic!("unexpected part"),
        }
    }

    #[test]
    fn preflight_truncates_tool_outputs() {
        // 工具结果作为最后一条消息（mid-turn 场景）：不可丢弃，只能截断。
        // 用 CJK 文本避免 BPE 高压缩导致估算失真
        let big = "长".repeat(5000);
        let mut msgs = vec![
            text_msg(Role::System, &"系统提示".repeat(100)),
            Message {
                role: Role::Assistant,
                content: vec![ContentPart::ToolResult {
                    tool_call_id: "1".into(),
                    name: "shell".into(),
                    result: big,
                }],
                timestamp: chrono::Utc::now(),
                usage: None,
            },
        ];
        let r = preflight_trim(&mut msgs, &[], 8_192, 6_000, TokenizerId::Cl100k);
        assert!(r.tool_outputs_truncated > 0);
        let tool_len = match &msgs[1].content[0] {
            ContentPart::ToolResult { result, .. } => result.chars().count(),
            _ => panic!("unexpected part"),
        };
        assert!(tool_len <= PREFLIGHT_TOOL_OUTPUT_MAX_CHARS + 20);
    }

    #[test]
    fn truncate_chars_boundary_safe() {
        let s = "中文字符串测试".repeat(100);
        let t = truncate_chars(&s, 50);
        assert!(t.chars().count() <= 51);
    }
}

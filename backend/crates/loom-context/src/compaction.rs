//! Session compaction: heuristic + LLM-based history compression.
//!
//! When a conversation grows too large, this module compresses the oldest
//! messages while preserving critical context (errors, file paths, decisions).

use anyhow::Result;
use loom_types::{CompactionConfig, ContentPart, Message, Role};
use tiktoken_rs::CoreBPE;

use crate::bpe;

/// The result of a compaction pass.
#[derive(Debug, Clone)]
pub struct CompactionResult {
    pub compacted_history: Vec<Message>,
    pub tokens_before: usize,
    pub tokens_after: usize,
    pub savings_pct: f64,
    pub items_compacted: usize,
    pub strategies_used: Vec<CompactionStrategy>,
    pub tool_outputs_truncated: usize,
    pub base64_payloads_elided: usize,
    pub repetitive_loops_collapsed: usize,
    pub llm_summarization_performed: bool,
    pub summary_text: String,
}

/// Compaction strategies applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompactionStrategy {
    HeuristicTruncation,
    Base64Elision,
    RepetitiveLoopCollapse,
    LLMSummarization,
}

/// Compact conversation history using the configured strategies.
///
/// Partitioning: the most recent `config.keep_recent_messages` messages are
/// preserved as-is. The older block goes through heuristic compaction first,
/// then optionally LLM summarization if still over budget.
pub fn compact_history(
    history: &[Message],
    config: &CompactionConfig,
    _llm_client: Option<&dyn std::any::Any>, // LLM summarization deferred
) -> Result<CompactionResult> {
    let bpe = bpe();
    let tokens_before = count_tokens(history, bpe);

    if !config.enabled {
        return Ok(CompactionResult {
            compacted_history: history.to_vec(),
            tokens_before,
            tokens_after: tokens_before,
            savings_pct: 0.0,
            items_compacted: 0,
            strategies_used: vec![],
            tool_outputs_truncated: 0,
            base64_payloads_elided: 0,
            repetitive_loops_collapsed: 0,
            llm_summarization_performed: false,
            summary_text: String::new(),
        });
    }

    // Step 1: Partition
    let split_idx = history.len().saturating_sub(config.keep_recent_messages);
    let (old, recent) = history.split_at(split_idx);

    let mut items_compacted = 0usize;
    let mut tool_outputs_truncated = 0usize;
    let mut base64_payloads_elided = 0usize;
    let mut repetitive_loops_collapsed = 0usize;
    let mut strategies = Vec::new();

    // Step 2: Heuristic compaction on the old block
    let mut old_compacted: Vec<Message> = Vec::new();
    for msg in old {
        let is_tool_result = msg.role == Role::Tool;
        let (compacted_msg, truncations, elisions) =
            apply_heuristic_compaction(msg, config, is_tool_result);
        old_compacted.push(compacted_msg);
        if truncations > 0 {
            tool_outputs_truncated += truncations;
            if !strategies.contains(&CompactionStrategy::HeuristicTruncation) {
                strategies.push(CompactionStrategy::HeuristicTruncation);
            }
        }
        if elisions > 0 {
            base64_payloads_elided += elisions;
            if !strategies.contains(&CompactionStrategy::Base64Elision) {
                strategies.push(CompactionStrategy::Base64Elision);
            }
        }
        items_compacted += 1;
    }

    // Step 3: Collapse repetitive loops
    let (old_compacted, loops_collapsed) =
        collapse_repetitive_loops(&old_compacted, 3);
    if loops_collapsed > 0 {
        repetitive_loops_collapsed += loops_collapsed;
        strategies.push(CompactionStrategy::RepetitiveLoopCollapse);
    }

    // Step 4: Reassemble
    let mut compacted_history = old_compacted;
    compacted_history.extend(recent.to_vec());

    let tokens_after = count_tokens(&compacted_history, bpe);
    let savings_pct = if tokens_before > 0 {
        (tokens_before as f64 - tokens_after as f64) / tokens_before as f64
    } else {
        0.0
    };

    Ok(CompactionResult {
        compacted_history,
        tokens_before,
        tokens_after,
        savings_pct,
        items_compacted,
        strategies_used: strategies,
        tool_outputs_truncated,
        base64_payloads_elided,
        repetitive_loops_collapsed,
        llm_summarization_performed: false, // LLM summarization deferred to future phase
        summary_text: String::new(),
    })
}

/// Apply heuristic compaction to a single message.
fn apply_heuristic_compaction(
    msg: &Message,
    config: &CompactionConfig,
    is_tool_result: bool,
) -> (Message, usize, usize) {
    let mut new_msg = msg.clone();
    let mut truncations = 0;
    let mut elisions = 0;

    // Never truncate file_read results (user's source code)
    let is_file_read = msg.content.iter().any(|p| matches!(p, ContentPart::ToolResult { name, .. } if name == "file_read"));
    if is_file_read {
        return (msg.clone(), 0, elisions);
    }

    let keep_head = 500;
    let keep_tail = 200;

    for part in &mut new_msg.content {
        match part {
            ContentPart::Text { text } => {
                // Elide base64 data URIs in text
                if text.contains(";base64,") {
                    let new_text = elide_base64_in_text(text);
                    if new_text.len() < text.len() {
                        elisions += 1;
                    }
                    *text = new_text;
                }

                // Truncate long tool outputs (but not if they contain signals)
                if is_tool_result
                    && text.len() > config.max_tool_output_chars
                    && !has_signal_markers(text)
                {
                    let head: String = text.chars().take(keep_head).collect();
                    let tail: String = text.chars().rev().take(keep_tail).collect::<Vec<_>>().into_iter().rev().collect();
                    let truncated = text.len().saturating_sub(keep_head + keep_tail);
                    *text = format!(
                        "{}...\n[truncated {} chars]\n...{}",
                        head, truncated, tail
                    );
                    truncations += 1;
                }
            }
            ContentPart::Image { data, .. } => {
                let byte_len = data.len();
                *part = ContentPart::Text {
                    text: format!("[base64 image, {} bytes]", byte_len),
                };
                elisions += 1;
            }
            ContentPart::ToolResult { result, .. } => {
                // Elide base64 in tool results
                if result.contains(";base64,") {
                    let new_result = elide_base64_in_text(result);
                    if new_result.len() < result.len() {
                        elisions += 1;
                    }
                    *result = new_result;
                }

                // Truncate long tool results
                if result.len() > config.max_tool_output_chars
                    && !has_signal_markers(result)
                {
                    let head: String = result.chars().take(keep_head).collect();
                    let tail: String = result.chars().rev().take(keep_tail).collect::<Vec<_>>().into_iter().rev().collect();
                    let truncated = result.len().saturating_sub(keep_head + keep_tail);
                    *result = format!(
                        "{}...\n[truncated {} chars]\n...{}",
                        head, truncated, tail
                    );
                    truncations += 1;
                }
            }
            _ => {}
        }
    }

    (new_msg, truncations, elisions)
}

/// Check if text contains signal markers that prevent truncation.
fn has_signal_markers(text: &str) -> bool {
    // Only genuine error/diagnostic markers count as "signal" worth preserving
    // verbatim. The previous `'/' && '.'` / `:\\` path heuristics matched almost
    // any output containing a path, URL or version string ("v1.0"), so nearly
    // all tool output was treated as signal and truncation became a no-op.
    let lower = text.to_lowercase();
    lower.contains("error:")
        || lower.contains("error]")
        || lower.contains("failed")
        || lower.contains("panic")
        || lower.contains("exception")
        || lower.contains("traceback")
        || lower.contains("warning:")
}

/// Replace base64 data URIs with a compact placeholder.
fn elide_base64_in_text(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut remaining = text;
    while let Some(pos) = remaining.find(";base64,") {
        result.push_str(&remaining[..pos]);
        let data_start = pos + ";base64,".len();
        let data = &remaining[data_start..];
        // Find the end of the base64 chunk (up to next whitespace, quote, or end)
        let end = data.find(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == '>')
            .unwrap_or(data.len());
        let byte_len = data[..end].len();
        result.push_str(&format!(";base64,[{} bytes]", byte_len));
        remaining = &data[end..];
    }
    result.push_str(remaining);
    result
}

/// Detect and collapse repetitive tool-call loops.
///
/// Scans consecutive (ToolCall, ToolResult) pairs. If the same tool is called
/// with identical arguments >= threshold times consecutively, keeps the first
/// pair and replaces subsequent repeats with a summary message.
fn collapse_repetitive_loops(
    messages: &[Message],
    threshold: usize,
) -> (Vec<Message>, usize) {
    if messages.len() < 2 * threshold {
        return (messages.to_vec(), 0);
    }

    let mut result: Vec<Message> = Vec::new();
    let mut loops_collapsed = 0usize;
    let mut i = 0;

    // Identify (assistant+tool_call, tool+tool_result) pairs
    let pairs: Vec<(usize, String, String)> = messages
        .windows(2)
        .enumerate()
        .filter_map(|(idx, w)| {
            let tool_name = extract_tool_call_name(&w[0]);
            let tool_args = extract_tool_call_args(&w[0]);
            if let (Some(name), Some(args)) = (tool_name, tool_args) {
                if w[1].role == Role::Tool {
                    Some((idx, name, args))
                } else {
                    None
                }
            } else {
                None
            }
        })
        .collect();

    while i < messages.len() {
        // Check for consecutive repetitive pairs starting at i
        let anchor_pair_idx = pairs.iter().position(|p| p.0 == i);
        let mut repeat_count = if anchor_pair_idx.is_some() { 1usize } else { 0usize };

        if repeat_count > 0 {
            let anchor = &pairs[anchor_pair_idx.unwrap()];
            // Count consecutive identical pairs
            let mut next_idx = i + 2; // next pair position
            while next_idx + 1 < messages.len() {
                if let Some(next_pair) = pairs.iter().find(|p| p.0 == next_idx) {
                    if next_pair.1 == anchor.1 && next_pair.2 == anchor.2 {
                        repeat_count += 1;
                        next_idx += 2;
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }
        }

        if repeat_count >= threshold {
            // Keep first pair, collapse the rest
            result.push(messages[i].clone()); // assistant tool_call
            if i + 1 < messages.len() {
                result.push(messages[i + 1].clone()); // tool result (first)
            }
            loops_collapsed += repeat_count - 1;

            let first_tool_name = extract_tool_call_name(&messages[i]).unwrap_or_default();
            result.push(Message {
                role: Role::System,
                content: vec![ContentPart::Text {
                    text: format!(
                        "[Repetitive calls to {} suppressed — called {} more times with same arguments]",
                        first_tool_name, repeat_count - 1
                    ),
                }],
                timestamp: chrono::Utc::now(),
                usage: None,
            });
            i += repeat_count * 2;
        } else {
            result.push(messages[i].clone());
            i += 1;
        }
    }

    (result, loops_collapsed)
}

fn extract_tool_call_name(msg: &Message) -> Option<String> {
    if msg.role != Role::Assistant {
        return None;
    }
    for part in &msg.content {
        if let ContentPart::ToolCall { name, .. } = part {
            return Some(name.clone());
        }
    }
    None
}

fn extract_tool_call_args(msg: &Message) -> Option<String> {
    if msg.role != Role::Assistant {
        return None;
    }
    for part in &msg.content {
        if let ContentPart::ToolCall { arguments, .. } = part {
            return Some(arguments.to_string());
        }
    }
    None
}

// LLM summarization is deferred to a future phase.
// The architecture is in place (CompactionConfig.use_llm_summarization,
// the llm_client parameter, and the strategy selection at lines 54-64
// of this file), but the actual LLM call requires:
// 1. A helper to build an auxiliary CloudClient from the orchestrator
// 2. A prompt template (see design document Section 2.2)
// 3. JSON response parsing + reformatting into a System message
//
// When implemented, llm_summarize() will:
// - Take the old history block + a CloudClient
// - Build the structured summary prompt
// - Call client.complete() with temperature=0
// - Parse the JSON response
// - Return the summary text for injection into a System message

/// Count estimated tokens in a message slice using tiktoken.
///
/// Delegates to [`crate::message_tokens`] per message so the estimate counts the
/// full serialized content — tool-call arguments, tool results, and thinking
/// blocks — not just concatenated `Text` parts. This keeps compaction's
/// accounting consistent with `ContextAssembler::truncate_history` and prevents
/// undercounting tool-heavy history.
fn count_tokens(messages: &[Message], bpe: &CoreBPE) -> usize {
    messages.iter().map(|m| crate::message_tokens(m, bpe)).sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use loom_types::Message;
    use loom_types::Role;

    fn make_tool_call(name: &str, args: &str) -> Message {
        Message {
            role: Role::Assistant,
            content: vec![ContentPart::ToolCall {
                id: "call_1".into(),
                name: name.into(),
                arguments: serde_json::from_str(args).unwrap_or_default(),
            }],
            timestamp: chrono::Utc::now(),
            usage: None,
        }
    }

    fn make_tool_result(result: &str) -> Message {
        Message {
            role: Role::Tool,
            content: vec![ContentPart::Text {
                text: result.into(),
            }],
            timestamp: chrono::Utc::now(),
            usage: None,
        }
    }

    fn make_text_msg(role: Role, text: &str) -> Message {
        Message {
            role,
            content: vec![ContentPart::Text { text: text.into() }],
            timestamp: chrono::Utc::now(),
            usage: None,
        }
    }

    #[test]
    fn test_elide_base64() {
        let text = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAUA";
        let result = elide_base64_in_text(text);
        assert!(result.contains("["));
        assert!(result.contains("bytes"));
        assert!(!result.contains("iVBORw0KGgo")); // base64 content removed
    }

    #[test]
    fn test_truncation_respects_signals() {
        let error_text = format!("output start\n{}\nError: something broke\n{}",
            "x".repeat(2000), "y".repeat(2000));
        let msg = make_tool_result(&error_text);
        let config = CompactionConfig::default();
        let (compacted, truncations, _) = apply_heuristic_compaction(&msg, &config, true);
        assert_eq!(truncations, 0, "Error-containing text should not be truncated");
        assert_eq!(compacted.content.len(), msg.content.len());
    }

    #[test]
    fn test_truncation_on_long_output() {
        let long_text = "a".repeat(5000);
        let msg = make_tool_result(&long_text);
        let config = CompactionConfig {
            max_tool_output_chars: 2000,
            ..Default::default()
        };
        let (compacted, truncations, _) = apply_heuristic_compaction(&msg, &config, true);
        assert_eq!(truncations, 1);
        if let ContentPart::Text { text } = &compacted.content[0] {
            assert!(text.contains("[truncated"));
            assert!(text.len() < long_text.len());
        }
    }

    #[test]
    fn test_collapse_repetitive_loops() {
        let msgs = vec![
            make_tool_call("bash", r#"{"command":"ls"}"#),
            make_tool_result("file1 file2"),
            make_tool_call("bash", r#"{"command":"ls"}"#),
            make_tool_result("file1 file2"),
            make_tool_call("bash", r#"{"command":"ls"}"#),
            make_tool_result("file1 file2"),
        ];
        let (compacted, collapsed) = collapse_repetitive_loops(&msgs, 3);
        assert_eq!(collapsed, 2);
        // First pair kept, third pair collapsed
        assert!(compacted.len() < msgs.len());
    }

    #[test]
    fn test_no_collapse_below_threshold() {
        let msgs = vec![
            make_tool_call("bash", r#"{"command":"ls"}"#),
            make_tool_result("file1"),
            make_tool_call("bash", r#"{"command":"ls"}"#),
            make_tool_result("file1"),
        ];
        let (compacted, collapsed) = collapse_repetitive_loops(&msgs, 3);
        assert_eq!(collapsed, 0);
        assert_eq!(compacted.len(), msgs.len());
    }

    #[test]
    fn test_compact_disabled() {
        let msgs = vec![make_text_msg(Role::User, "hello")];
        let config = CompactionConfig { enabled: false, ..Default::default() };
        let result = compact_history(&msgs, &config, None).unwrap();
        assert_eq!(result.compacted_history.len(), 1);
        assert_eq!(result.savings_pct, 0.0);
    }

    #[test]
    fn test_compact_empty_history() {
        let msgs: Vec<Message> = vec![];
        let config = CompactionConfig { enabled: true, ..Default::default() };
        let result = compact_history(&msgs, &config, None);
        assert!(result.is_ok());
        let r = result.unwrap();
        assert!(r.compacted_history.is_empty());
        assert_eq!(r.tokens_before, 0);
    }

    #[test]
    fn test_savings_pct_calculation() {
        let long_text = "x".repeat(5000);
        let msgs = vec![
            make_tool_call("bash", r#"{"command":"cat huge.log"}"#),
            make_tool_result(&long_text),
            make_text_msg(Role::User, "recent message"),
        ];
        let config = CompactionConfig {
            enabled: true,
            max_tool_output_chars: 2000,
            keep_recent_messages: 1, // only last message is recent
            ..Default::default()
        };
        let result = compact_history(&msgs, &config, None).unwrap();
        assert!(result.savings_pct > 0.0, "should have savings from truncation");
        assert_eq!(result.compacted_history.len(), 3); // tool call + truncated tool result + recent
    }

    #[test]
    fn test_recent_messages_preserved() {
        let msgs = vec![
            make_text_msg(Role::User, "old message to compact"),
            make_text_msg(Role::Assistant, "old response"),
            make_text_msg(Role::User, "recent message preserved"),
        ];
        let config = CompactionConfig {
            enabled: true,
            keep_recent_messages: 1,
            max_tool_output_chars: 2000,
            ..Default::default()
        };
        let result = compact_history(&msgs, &config, None).unwrap();
        // Recent message must be the last message in compacted history
        if let ContentPart::Text { text } = &result.compacted_history.last().unwrap().content[0] {
            assert!(text.contains("recent message preserved"));
        }
    }
}

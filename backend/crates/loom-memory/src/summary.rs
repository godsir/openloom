//! Conversation summarization engine.
//!
//! Compresses conversation history into a concise paragraph using the local LLM,
//! replacing hard truncation with intelligent compression. Supports incremental
//! updates (existing summary + new messages → updated summary) to avoid
//! re-processing the entire history on every trigger.

use loom_types::{CompletionRequest, Message};

pub struct SummaryEngine;

impl SummaryEngine {
    /// Prompt template for initial summarization.
    pub const PROMPT: &str = "\
请将以下对话总结为结构化摘要，必须覆盖以下六个维度（用 markdown 小节标题）：
## 目标
用户的目标与当前任务。
## 关键决策
已确定的关键决策与约定。
## 涉及文件
涉及的文件/路径及其关键内容。
## 已做变更
已完成的变更与操作结果。
## 重要错误
遇到的重要错误与修复方式。
## 待办
待办事项与下一步。
省略寒暄与冗余信息。保持事实准确，不要编造。语言与对话保持一致。";

    /// Prompt template for incremental (delta) summarization.
    pub const DELTA_PROMPT: &str = "\
已有摘要：

{previous}

新增对话内容：

{new_messages}

请在已有摘要基础上，整合新增内容，更新为完整摘要。保持同样六个维度的小节结构（目标/关键决策/涉及文件/已做变更/重要错误/待办）。保持事实准确，不要编造。语言与对话保持一致。";

    /// Check whether summarization should trigger.
    ///
    /// Returns true when history exceeds `threshold` messages AND at least
    /// `min_new` messages have been added since the last summary.
    pub fn should_summarize(
        history_len: usize,
        last_summary_at_len: usize,
        threshold: usize,
        min_new: usize,
    ) -> bool {
        history_len >= threshold && history_len.saturating_sub(last_summary_at_len) >= min_new
    }

    /// Token-based trigger: summarize when current window occupancy reaches
    /// `threshold_pct` of `context_window`.
    ///
    /// When context_window is 0 (unknown / not configured), use a reasonable
    /// fallback of 100 K so summarization still triggers rather than being
    /// silently disabled.
    pub fn should_summarize_by_tokens(
        current_tokens: usize,
        context_window: usize,
        threshold_pct: f32,
    ) -> bool {
        let effective_cw = if context_window > 0 {
            context_window
        } else {
            // Fallback: most current models have ≥100 K context windows.
            // Using 100 K ensures the 80 % trigger fires at 80 K tokens
            // when the model's context size is unknown.
            100_000
        };
        (current_tokens as f32) >= (effective_cw as f32 * threshold_pct)
    }

    /// Build the prompt string for summarization (initial or incremental).
    pub fn build_prompt(history: &[Message], existing_summary: Option<&str>) -> String {
        let history_text = history
            .iter()
            .map(|m| format!("[{}]: {}", m.role.as_str(), m.text_content()))
            .collect::<Vec<_>>()
            .join("\n\n");

        match existing_summary {
            Some(prev) if !prev.is_empty() => Self::DELTA_PROMPT
                .replace("{previous}", prev)
                .replace("{new_messages}", &history_text),
            _ => format!("{}\n\n{}", Self::PROMPT, history_text),
        }
    }

    /// Build the prompt from a [from, to) slice of history, using full content
    /// (text + tool calls + tool results), not just text_content().
    pub fn build_prompt_segmented(
        history: &[Message],
        from: usize,
        to: usize,
        existing_summary: Option<&str>,
    ) -> String {
        let to = to.min(history.len());
        let from = from.min(to);
        let history_text = history[from..to]
            .iter()
            .map(|m| {
                let body = m
                    .content
                    .iter()
                    .map(|c| match c {
                        loom_types::ContentPart::Text { text } => text.clone(),
                        loom_types::ContentPart::Thinking { text } => {
                            format!("[thinking] {}", text)
                        }
                        loom_types::ContentPart::ToolCall {
                            name, arguments, ..
                        } => {
                            format!("[tool_call {}] {}", name, arguments)
                        }
                        loom_types::ContentPart::ToolResult { name, result, .. } => {
                            format!("[tool_result {}] {}", name, result)
                        }
                        loom_types::ContentPart::Image { .. }
                        | loom_types::ContentPart::ImageRef { .. } => "[image]".to_string(),
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                format!("[{}]: {}", m.role.as_str(), body)
            })
            .collect::<Vec<_>>()
            .join("\n\n");
        match existing_summary {
            Some(prev) if !prev.is_empty() => Self::DELTA_PROMPT
                .replace("{previous}", prev)
                .replace("{new_messages}", &history_text),
            _ => format!("{}\n\n{}", Self::PROMPT, history_text),
        }
    }

    /// Build a CompletionRequest suitable for the local LLM.
    pub fn build_request(prompt: &str) -> CompletionRequest {
        CompletionRequest {
            messages: vec![Message::user(prompt)],
            max_tokens: 1024,
            temperature: 0.0,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use loom_types::{ContentPart, Role};

    fn msg(role: Role, text: &str) -> Message {
        Message {
            role,
            content: vec![ContentPart::Text { text: text.into() }],
            timestamp: chrono::Utc::now(),
            usage: None,
        }
    }

    #[test]
    fn test_should_summarize_below_threshold() {
        assert!(!SummaryEngine::should_summarize(10, 0, 12, 6));
    }

    #[test]
    fn test_should_summarize_at_threshold_with_enough_new() {
        assert!(SummaryEngine::should_summarize(14, 0, 12, 6));
    }

    #[test]
    fn test_should_summarize_not_enough_new() {
        assert!(!SummaryEngine::should_summarize(14, 12, 12, 6));
    }

    #[test]
    fn test_should_summarize_threshold_edge() {
        assert!(SummaryEngine::should_summarize(12, 0, 12, 6));
    }

    #[test]
    fn test_build_prompt_initial() {
        let history = vec![
            msg(Role::User, "Hello"),
            msg(Role::Assistant, "Hi! How can I help?"),
        ];
        let prompt = SummaryEngine::build_prompt(&history, None);
        assert!(prompt.contains("目标"));
        assert!(prompt.contains("[user]: Hello"));
        assert!(prompt.contains("[assistant]: Hi!"));
        assert!(!prompt.contains("Previous summary"));
    }

    #[test]
    fn test_build_prompt_incremental() {
        let history = vec![msg(Role::User, "What is Rust?")];
        let existing = "User is learning about programming.";
        let prompt = SummaryEngine::build_prompt(&history, Some(existing));
        assert!(prompt.contains("已有摘要"));
        assert!(prompt.contains(existing));
        assert!(prompt.contains("[user]: What is Rust?"));
    }

    #[test]
    fn test_build_prompt_ignores_empty_summary() {
        let history = vec![msg(Role::User, "Hello")];
        let prompt = SummaryEngine::build_prompt(&history, Some(""));
        assert!(!prompt.contains("已有摘要"));
        assert!(prompt.contains("目标"));
    }

    #[test]
    fn test_build_request_has_correct_params() {
        let req = SummaryEngine::build_request("test prompt");
        assert_eq!(req.max_tokens, 1024);
        assert_eq!(req.temperature, 0.0);
        assert_eq!(req.messages.len(), 1);
    }

    #[test]
    fn test_should_summarize_by_tokens() {
        assert!(SummaryEngine::should_summarize_by_tokens(
            81_000, 100_000, 0.8
        ));
        assert!(!SummaryEngine::should_summarize_by_tokens(
            79_000, 100_000, 0.8
        ));
        // context_window=0 now falls back to 100_000 — 81 K ≥ 80 K → true.
        assert!(SummaryEngine::should_summarize_by_tokens(81_000, 0, 0.8));
        // Below the fallback threshold: 70 K < 80 K → false.
        assert!(!SummaryEngine::should_summarize_by_tokens(70_000, 0, 0.8));
    }

    #[test]
    fn test_prompt_covers_six_dimensions() {
        let prompt = SummaryEngine::build_prompt(&[Message::user("hi")], None);
        for kw in ["目标", "决策", "文件", "变更", "错误", "待办"] {
            assert!(prompt.contains(kw), "prompt 必须覆盖六维度之一: {}", kw);
        }
    }

    #[test]
    fn test_build_prompt_segmented_range() {
        let history: Vec<Message> = (0..10)
            .map(|i| Message::user(format!("msg {}", i)))
            .collect();
        let prompt = SummaryEngine::build_prompt_segmented(&history, 2, 8, None);
        assert!(prompt.contains("msg 2"));
        assert!(prompt.contains("msg 7"));
        assert!(!prompt.contains("msg 1"));
        assert!(!prompt.contains("msg 8"));
    }

    #[test]
    fn test_build_prompt_segmented_keeps_tool_result() {
        let history = vec![
            Message::user("q"),
            Message::tool("c1", "shell", "BIG_RESULT_12345"),
        ];
        let prompt = SummaryEngine::build_prompt_segmented(&history, 0, 2, None);
        assert!(
            prompt.contains("BIG_RESULT_12345"),
            "分段 prompt 必须包含工具结果, 不能只用 text_content"
        );
    }
}

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
Summarize this conversation into one concise paragraph. Keep:
- Key facts, decisions, and technical details
- User preferences and context (what they're working on)
- Important outcomes and action items

Omit small talk, greetings, and redundant information. Match the language of the conversation.";

    /// Prompt template for incremental (delta) summarization.
    pub const DELTA_PROMPT: &str = "\
Previous summary:

{previous}

New conversation messages:

{new_messages}

Update the summary above to incorporate the new messages. Keep the same format: one concise paragraph covering key facts, decisions, user context, and outcomes. Match the language of the conversation.";

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
        history_len >= threshold
            && history_len.saturating_sub(last_summary_at_len) >= min_new
    }

    /// Build the prompt string for summarization (initial or incremental).
    pub fn build_prompt(
        history: &[Message],
        existing_summary: Option<&str>,
    ) -> String {
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

    /// Build a CompletionRequest suitable for the local LLM.
    pub fn build_request(prompt: &str) -> CompletionRequest {
        CompletionRequest {
            messages: vec![Message::user(prompt)],
            max_tokens: 512,
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
        assert!(prompt.contains("Summarize this conversation"));
        assert!(prompt.contains("[user]: Hello"));
        assert!(prompt.contains("[assistant]: Hi!"));
        assert!(!prompt.contains("Previous summary"));
    }

    #[test]
    fn test_build_prompt_incremental() {
        let history = vec![msg(Role::User, "What is Rust?")];
        let existing = "User is learning about programming.";
        let prompt = SummaryEngine::build_prompt(&history, Some(existing));
        assert!(prompt.contains("Previous summary"));
        assert!(prompt.contains(existing));
        assert!(prompt.contains("[user]: What is Rust?"));
    }

    #[test]
    fn test_build_prompt_ignores_empty_summary() {
        let history = vec![msg(Role::User, "Hello")];
        let prompt = SummaryEngine::build_prompt(&history, Some(""));
        assert!(!prompt.contains("Previous summary"));
        assert!(prompt.contains("Summarize this conversation"));
    }

    #[test]
    fn test_build_request_has_correct_params() {
        let req = SummaryEngine::build_request("test prompt");
        assert_eq!(req.max_tokens, 512);
        assert_eq!(req.temperature, 0.0);
        assert_eq!(req.messages.len(), 1);
    }
}

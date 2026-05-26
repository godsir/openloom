// SPDX-License-Identifier: Apache-2.0
//! Context window assembly for openLoom v2.
//!
//! Assembles system prompt, persona, conversation history, and tool definitions
//! into the context window sent to the LLM.

use anyhow::Result;
use loom_types::Message;

/// Assembles the full context window for an agent turn.
pub struct ContextAssembler {
    system_prompt: String,
    max_history_tokens: usize,
}

impl ContextAssembler {
    pub fn new(system_prompt: impl Into<String>, max_history_tokens: usize) -> Self {
        Self { system_prompt: system_prompt.into(), max_history_tokens }
    }

    /// Build the messages array for an LLM request.
    pub fn build(
        &self,
        persona: Option<&str>,
        history: &[Message],
        _tools: &[loom_types::ToolDefinition],
    ) -> Result<Vec<Message>> {
        let mut messages = Vec::new();

        // System message with persona injection
        let system_content = match persona {
            Some(p) if !p.is_empty() => format!("{}\n\n[User Profile]\n{}", self.system_prompt, p),
            _ => self.system_prompt.clone(),
        };
        messages.push(Message {
            role: loom_types::Role::System,
            content: vec![loom_types::ContentPart::Text { text: system_content }],
            timestamp: chrono::Utc::now(),
        });

        // Conversation history (truncated to max tokens, newest first scan)
        let mut token_count = 0usize;
        let mut included: Vec<usize> = Vec::new();
        for (i, msg) in history.iter().enumerate().rev() {
            let msg_tokens = msg.text_content().chars().count() / 4;
            if token_count + msg_tokens > self.max_history_tokens {
                break;
            }
            token_count += msg_tokens;
            included.push(i);
        }
        included.reverse(); // chronological order
        for i in included {
            messages.push(history[i].clone());
        }

        Ok(messages)
    }

    /// Compact conversation history by summarizing old messages.
    pub async fn compact(&self, _history: &[Message]) -> Result<Vec<Message>> {
        // Future: summarize old messages using a local model
        Ok(Vec::new())
    }
}

impl Default for ContextAssembler {
    fn default() -> Self {
        Self::new(
            "You are a helpful AI assistant with access to tools and long-term memory.",
            8192,
        )
    }
}

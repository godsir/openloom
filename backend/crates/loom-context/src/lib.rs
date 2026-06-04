// SPDX-License-Identifier: Apache-2.0
//! Context window assembly for openLoom v2.
//!
//! Assembles the context window with a stable-prefix / dynamic-suffix split to
//! maximize KV cache reuse: system prompt, persona, conversation summary, and
//! KG context form the stable prefix; recent history forms the dynamic suffix.

//! Uses `tiktoken-rs` (cl100k_base) for accurate token counting instead of
//! heuristic character-based estimates.

use anyhow::Result;
use loom_types::Message;
use std::sync::OnceLock;

/// Shared tiktoken BPE instance — initialised once, reused for all assemblies.
fn bpe() -> &'static tiktoken_rs::CoreBPE {
    static BPE: OnceLock<tiktoken_rs::CoreBPE> = OnceLock::new();
    BPE.get_or_init(|| {
        tiktoken_rs::cl100k_base().expect("tiktoken cl100k_base model should always load")
    })
}

/// Options for context assembly.
#[derive(Default)]
pub struct AssembleOptions {
    /// User persona text (from memory store).
    pub persona: Option<String>,
    /// Conversation summary (from SummaryEngine).
    pub summary: Option<String>,
    /// Knowledge graph context text (from query_kg_context).
    pub kg_context: Option<String>,
    /// Available tool names catalog (for lazy_tools mode).
    pub tool_catalog: Option<String>,
    /// Full conversation history (will be truncated to recent messages).
    pub history: Vec<Message>,
}

/// Assembles the full context window for an agent turn.
///
/// Output order (stable prefix → dynamic suffix):
///   1. System message: [base instructions] [persona] [summary] [KG] [tools]
///   2. Recent history messages (truncated)
///
/// The caller appends the current user message after the returned Vec.
pub struct ContextAssembler {
    system_prompt: String,
    max_history_tokens: usize,
}

impl ContextAssembler {
    pub fn new(system_prompt: impl Into<String>, max_history_tokens: usize) -> Self {
        Self {
            system_prompt: system_prompt.into(),
            max_history_tokens,
        }
    }

    /// Build the messages array for an LLM request.
    ///
    /// The system message is a single message containing all stable-prefix
    /// sections concatenated in a fixed order. Recent history follows as
    /// individual messages, truncated to fit within max_history_tokens/2.
    pub fn build(&self, opts: AssembleOptions) -> Result<Vec<Message>> {
        let mut messages = Vec::new();

        // ── Stable prefix (single system message, fixed order) ──
        let mut prefix = self.system_prompt.clone();

        if let Some(ref p) = opts.persona {
            if !p.is_empty() {
                prefix.push_str(&format!("\n\n## User Profile\n{}", p));
            }
        }
        if let Some(ref s) = opts.summary {
            if !s.is_empty() {
                prefix.push_str(&format!("\n\n## Conversation Summary\n{}", s));
            }
        }
        if let Some(ref kg) = opts.kg_context {
            if !kg.is_empty() {
                prefix.push_str(&format!("\n\n{}", kg));
            }
        }
        if let Some(ref tc) = opts.tool_catalog {
            if !tc.is_empty() {
                prefix.push_str(&format!("\n\n## Available Tools\n{}", tc));
            }
        }

        messages.push(Message {
            role: loom_types::Role::System,
            content: vec![loom_types::ContentPart::Text { text: prefix }],
            timestamp: chrono::Utc::now(),
            usage: None,
        });

        // ── Dynamic suffix: recent history (capped at half the budget) ──
        let recent_limit = (self.max_history_tokens / 2).max(1024);
        let recent = self.truncate_history(&opts.history, recent_limit);
        messages.extend(recent);

        Ok(messages)
    }

    /// Keep the most recent messages that fit within `max_tokens`, scanning
    /// from newest to oldest. Uses tiktoken for precise token counting.
    fn truncate_history(&self, history: &[Message], max_tokens: usize) -> Vec<Message> {
        let bpe = bpe();
        let mut token_count = 0usize;
        let mut included: Vec<usize> = Vec::new();
        for (i, msg) in history.iter().enumerate().rev() {
            let text = msg.text_content();
            let msg_tokens = bpe.encode_with_special_tokens(&text).len();
            if token_count + msg_tokens > max_tokens {
                break;
            }
            token_count += msg_tokens;
            included.push(i);
        }
        included.reverse();
        included.into_iter().map(|i| {
            let mut msg = history[i].clone();
            msg.compact_for_llm();
            msg
        }).collect()
    }

    /// Compact conversation history by summarizing old messages.
    /// Delegates to SummaryEngine — this is the entry point called from
    /// the agent loop when history grows too large.
    pub async fn compact(&self, _history: &[Message]) -> Result<Vec<Message>> {
        // Now wired through SummaryEngine in orchestrator.
        // This method is kept for API compatibility; actual summarization
        // happens in process_message_streaming via SummaryEngine::summarize().
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

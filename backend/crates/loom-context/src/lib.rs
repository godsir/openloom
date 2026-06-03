// SPDX-License-Identifier: Apache-2.0
//! Context window assembly for openLoom v2.
//!
//! Assembles the context window with a stable-prefix / dynamic-suffix split to
//! maximize KV cache reuse: system prompt, persona, conversation summary, and
//! KG context form the stable prefix; recent history forms the dynamic suffix.
//!
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
    /// Max tokens allocated to conversation history.
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
    /// individual messages, truncated to fit within max_history_tokens.
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

        // ── Dynamic suffix: recent history ──
        let recent = self.truncate_history(&opts.history, self.max_history_tokens);
        messages.extend(recent);

        Ok(messages)
    }

    /// Keep the most recent messages that fit within `max_tokens`, scanning
    /// from newest to oldest. Huge tool-result messages are truncated in-place
    /// so they don't consume the entire budget.
    fn truncate_history(&self, history: &[Message], max_tokens: usize) -> Vec<Message> {
        /// Maximum tokens a single message may contribute before being truncated.
        const MAX_PER_MESSAGE: usize = 4_000;

        let bpe = bpe();
        let mut token_count = 0usize;
        let mut included: Vec<usize> = Vec::new();

        for (i, msg) in history.iter().enumerate().rev() {
            let full_text = msg.text_content();
            let full_tokens = bpe.encode_with_special_tokens(&full_text).len();

            let effective_tokens = full_tokens.min(MAX_PER_MESSAGE);
            if token_count + effective_tokens > max_tokens {
                break;
            }
            token_count += effective_tokens;
            included.push(i);

            // Log when a message was too large and would have been truncated
            if full_tokens > MAX_PER_MESSAGE {
                tracing::debug!(
                    index = i,
                    role = %msg.role.as_str(),
                    full = full_tokens,
                    capped = MAX_PER_MESSAGE,
                    "truncating large history message"
                );
            }
        }
        included.reverse();

        included
            .into_iter()
            .map(|i| {
                let msg = &history[i];
                let text = msg.text_content();
                let token_count = bpe.encode_with_special_tokens(&text).len();
                if token_count <= MAX_PER_MESSAGE {
                    return msg.clone();
                }
                // Truncate the message content to fit within MAX_PER_MESSAGE tokens
                truncate_message_to_tokens(msg, MAX_PER_MESSAGE, bpe)
            })
            .collect()
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

/// Truncate a message's text content so it fits within `max_tokens` tokens.
///
/// Preserves the first ~30% and last ~70% of the content to keep both the
/// start (context) and end (result/findings) of tool output visible.
fn truncate_message_to_tokens(
    msg: &Message,
    max_tokens: usize,
    bpe: &tiktoken_rs::CoreBPE,
) -> Message {
    let text = msg.text_content();
    let tokens = bpe.encode_with_special_tokens(&text);
    if tokens.len() <= max_tokens {
        return msg.clone();
    }

    let head = max_tokens * 30 / 100;
    let tail = max_tokens - head;

    let head_tokens: Vec<_> = tokens.iter().take(head).copied().collect();
    let tail_tokens: Vec<_> = tokens.iter().rev().take(tail).copied().collect::<Vec<_>>()
        .into_iter().rev().collect();

    // Build new content parts, replacing text with truncated version
    let mut parts = msg.content.clone();
    let truncation_note = format!(
        "\n\n[...{} tokens truncated, showing start and end of content...]\n\n",
        tokens.len().saturating_sub(max_tokens)
    );
    // Prepend head marker and append tail
    let marker_tokens = bpe.encode_with_special_tokens(&truncation_note);
    let marker_text = bpe.decode(marker_tokens).unwrap_or_else(|_| truncation_note);

    let combined = format!(
        "{}{}{}",
        bpe.decode(head_tokens).unwrap_or_default(),
        marker_text,
        bpe.decode(tail_tokens).unwrap_or_default(),
    );

    // Replace all Text parts with a single truncated one
    parts.retain(|p| !matches!(p, loom_types::ContentPart::Text { .. }));
    parts.push(loom_types::ContentPart::Text { text: combined });

    Message {
        role: msg.role.clone(),
        content: parts,
        timestamp: msg.timestamp,
        usage: msg.usage.clone(),
    }
}

impl Default for ContextAssembler {
    fn default() -> Self {
        Self::new(
            "You are a helpful AI assistant with access to tools and long-term memory.",
            16_384, // doubled from 8192 now that we count real tokens
        )
    }
}

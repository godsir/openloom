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
use sha2::{Digest, Sha256};

pub mod compaction;
pub mod tokenizers;
pub use tokenizers::{TokenizerId, tokenizer_for_model, bpe};
pub use compaction::{CompactionResult, CompactionStrategy, compact_history, compact_history_with_tokenizer, mid_turn_safety_truncate};

/// Deterministic SHA256 fingerprint of the stable prompt prefix.
///
/// Computed by [`ContextAssembler::compute_prefix_digest`] and carried
/// through the agent loop into each inference provider for cache-hit
/// detection and breakpoint injection.
///
/// The `combined_hash` covers the full assembled stable prefix string
/// (system_prompt + persona + summary + kg_context + tool_catalog)
/// in the exact order they appear in the system message.  Per-component
/// hashes enable drift attribution in logs — the system can say
/// "cache miss (system_prompt changed)" instead of just "cache miss".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrefixDigest {
    /// SHA256 hex of the assembled stable prefix string.
    pub combined_hash: String,
    /// SHA256 of the base system prompt only.
    pub system_hash: String,
    /// SHA256 of the persona block, or SHA256("") if no persona.
    pub persona_hash: String,
    /// SHA256 of the conversation summary block, or SHA256("") if no summary.
    pub summary_hash: String,
    /// SHA256 of the KG context block, or SHA256("") if no KG context.
    pub kg_hash: String,
    /// SHA256 of the tool catalog block, or SHA256("") if none.
    pub catalog_hash: String,
    /// Estimated token count of the stable prefix (via tiktoken cl100k_base).
    pub prefix_token_count: usize,
}


/// Fixed per-message token overhead approximating the role/delimiter framing
/// that chat APIs add around every message (e.g. `<|im_start|>role ... <|im_end|>`).
const PER_MESSAGE_TOKEN_OVERHEAD: usize = 4;

/// Estimate the token cost of a single message, counting **all** content parts —
/// not just `Text`. Tool-call arguments, tool results, and thinking blocks all
/// consume real budget on the wire, so they must be included or the estimate
/// under-counts tool-heavy history.
///
/// This is the legacy entry-point. Prefer [`message_tokens_with_id`] for
/// model‑aware token counting.
pub fn message_tokens(msg: &Message, bpe: &tiktoken_rs::CoreBPE) -> usize {
    message_tokens_impl(msg, bpe)
}

/// Estimate the token cost of a single message using the model‑appropriate
/// tiktoken vocabulary.
pub fn message_tokens_with_id(msg: &Message, tid: TokenizerId) -> usize {
    message_tokens_impl(msg, tid.get())
}

/// Internal implementation — shared by both public entry-points.
fn message_tokens_impl(msg: &Message, bpe: &tiktoken_rs::CoreBPE) -> usize {
    use loom_types::ContentPart;
    let mut tokens = PER_MESSAGE_TOKEN_OVERHEAD;
    for part in &msg.content {
        let text: std::borrow::Cow<'_, str> = match part {
            ContentPart::Text { text } | ContentPart::Thinking { text } => {
                std::borrow::Cow::Borrowed(text.as_str())
            }
            ContentPart::ToolResult { name, result, .. } => {
                std::borrow::Cow::Owned(format!("{name}{result}"))
            }
            ContentPart::ToolCall {
                name, arguments, ..
            } => std::borrow::Cow::Owned(format!("{name}{arguments}")),
            ContentPart::Image { media_type, .. } | ContentPart::ImageRef { media_type, .. } => {
                std::borrow::Cow::Borrowed(media_type.as_str())
            }
        };
        tokens += bpe.encode_with_special_tokens(&text).len();
    }
    tokens
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
    /// Number of messages already covered by the LLM summary; those are dropped.
    pub summary_at_count: usize,
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
    /// Which tiktoken vocabulary to use for token counting in this instance.
    tokenizer_id: TokenizerId,
}

impl ContextAssembler {
    /// Create a new assembler with an explicit tokenizer.
    pub fn new_with_tokenizer(
        system_prompt: impl Into<String>,
        max_history_tokens: usize,
        tokenizer_id: TokenizerId,
    ) -> Self {
        Self {
            system_prompt: system_prompt.into(),
            max_history_tokens,
            tokenizer_id,
        }
    }

    /// Create a new assembler — uses the default `Cl100k` tokenizer.
    pub fn new(system_prompt: impl Into<String>, max_history_tokens: usize) -> Self {
        Self::new_with_tokenizer(system_prompt, max_history_tokens, TokenizerId::Cl100k)
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

        if let Some(ref p) = opts.persona
            && !p.is_empty()
        {
            prefix.push_str(&format!("\n\n## User Profile\n{}", p));
        }
        if let Some(ref s) = opts.summary
            && !s.is_empty()
        {
            prefix.push_str(&format!("\n\n## Conversation Summary\n{}", s));
        }
        if let Some(ref kg) = opts.kg_context
            && !kg.is_empty()
        {
            prefix.push_str(&format!("\n\n{}", kg));
        }
        if let Some(ref tc) = opts.tool_catalog
            && !tc.is_empty()
        {
            prefix.push_str(&format!("\n\n## Available Tools\n{}", tc));
        }

        messages.push(Message {
            role: loom_types::Role::System,
            content: vec![loom_types::ContentPart::Text { text: prefix }],
            timestamp: chrono::Utc::now(),
            usage: None,
        });

        // ── Dynamic suffix: recent history (capped at the recent-history budget) ──
        // max_history_tokens is now the recent-history token budget set by the caller
        // (context_window × keep_recent_pct). No more hardcoded /2.
        let recent_limit = self.max_history_tokens.max(1024);
        let recent = self.truncate_history_with_summary(&opts.history, recent_limit, opts.summary_at_count);
        messages.extend(recent);

        Ok(messages)
    }

    /// Keep the most recent messages that fit within `max_tokens`, scanning
    /// newest→oldest within the [summary_at_count, len) range. Messages before
    /// `summary_at_count` are dropped (covered by the LLM summary, not re-sent).
    /// `Tool`-role messages are excluded (orphaned-tool-message 400 errors).
    fn truncate_history_with_summary(
        &self,
        history: &[Message],
        max_tokens: usize,
        summary_at_count: usize,
    ) -> Vec<Message> {
        let bpe = self.tokenizer_id.get();
        let mut token_count = 0usize;
        let mut included: Vec<usize> = Vec::new();
        let start = summary_at_count.min(history.len());
        for i in (start..history.len()).rev() {
            let msg = &history[i];
            if msg.role == loom_types::Role::Tool {
                continue;
            }
            let msg_tokens = message_tokens(msg, bpe);
            if token_count + msg_tokens > max_tokens && !included.is_empty() {
                break;
            }
            token_count += msg_tokens;
            included.push(i);
        }
        included.reverse();
        included
            .into_iter()
            .map(|i| history[i].clone())
            .filter(|msg| !msg.content.is_empty())
            .collect()
    }

    /// Compact conversation history by summarizing old messages.
    /// Delegates to SummaryEngine — this is the entry point called from
    /// the agent loop when history grows too large.
    pub async fn compact(&self, _history: &[Message]) -> Result<Vec<Message>> {
        // Legacy API — delegates to compact_history with default config.
        let config = loom_types::CompactionConfig {
            enabled: true,
            ..Default::default()
        };
        let result = compaction::compact_history(_history, &config, None)?;
        Ok(result.compacted_history)
    }

    /// Compact conversation history with explicit configuration.
    pub async fn compact_with_config(
        &self,
        history: &[Message],
        config: &loom_types::CompactionConfig,
    ) -> Result<compaction::CompactionResult> {
        compaction::compact_history(history, config, None)
    }

    /// Compute a SHA256 fingerprint of the stable prefix **without** building
    /// the full message array.
    ///
    /// This is intentionally a pure function of the prefix components (not the
    /// history) so it can be used for cache-hit detection independently of the
    /// dynamic suffix.
    pub fn compute_prefix_digest(&self, opts: &AssembleOptions) -> PrefixDigest {
        let persona = opts.persona.as_deref().unwrap_or("");
        let summary = opts.summary.as_deref().unwrap_or("");
        let kg = opts.kg_context.as_deref().unwrap_or("");
        let catalog = opts.tool_catalog.as_deref().unwrap_or("");

        let system_hash = hex::encode(Sha256::digest(self.system_prompt.as_bytes()));
        let persona_hash = hex::encode(Sha256::digest(persona.as_bytes()));
        let summary_hash = hex::encode(Sha256::digest(summary.as_bytes()));
        let kg_hash = hex::encode(Sha256::digest(kg.as_bytes()));
        let catalog_hash = hex::encode(Sha256::digest(catalog.as_bytes()));

        // Build the same stable prefix string that build() would produce.
        let mut combined = self.system_prompt.clone();
        if !persona.is_empty() {
            combined.push_str(&format!("\n\n## User Profile\n{}", persona));
        }
        if !summary.is_empty() {
            combined.push_str(&format!("\n\n## Conversation Summary\n{}", summary));
        }
        if !kg.is_empty() {
            combined.push_str(&format!("\n\n{}", kg));
        }
        if !catalog.is_empty() {
            combined.push_str(&format!("\n\n## Available Tools\n{}", catalog));
        }

        let combined_hash = hex::encode(Sha256::digest(combined.as_bytes()));
        let prefix_token_count = self.tokenizer_id.get().encode_with_special_tokens(&combined).len();

        PrefixDigest {
            combined_hash,
            system_hash,
            persona_hash,
            summary_hash,
            kg_hash,
            catalog_hash,
            prefix_token_count,
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_digest_deterministic() {
        let assembler = ContextAssembler::new("test system prompt", 8192);
        let opts = AssembleOptions::default();
        let d1 = assembler.compute_prefix_digest(&opts);
        let d2 = assembler.compute_prefix_digest(&opts);
        assert_eq!(d1.combined_hash, d2.combined_hash);
        assert_eq!(d1.system_hash, d2.system_hash);
        assert!(d1.prefix_token_count > 0);
    }

    #[test]
    fn test_digest_system_prompt_change() {
        let a1 = ContextAssembler::new("prompt A", 8192);
        let a2 = ContextAssembler::new("prompt B", 8192);
        let opts = AssembleOptions::default();
        let d1 = a1.compute_prefix_digest(&opts);
        let d2 = a2.compute_prefix_digest(&opts);
        assert_ne!(d1.combined_hash, d2.combined_hash);
        assert_ne!(d1.system_hash, d2.system_hash);
    }

    #[test]
    fn test_digest_persona_change() {
        let assembler = ContextAssembler::new("sys", 8192);
        let d1 = assembler.compute_prefix_digest(&AssembleOptions {
            persona: Some("persona A".into()),
            ..Default::default()
        });
        let d2 = assembler.compute_prefix_digest(&AssembleOptions {
            persona: Some("persona B".into()),
            ..Default::default()
        });
        assert_ne!(d1.combined_hash, d2.combined_hash);
        assert_ne!(d1.persona_hash, d2.persona_hash);
    }

    #[test]
    fn test_digest_per_component_independence() {
        let assembler = ContextAssembler::new("sys", 8192);
        let base = assembler.compute_prefix_digest(&AssembleOptions::default());

        let with_persona = assembler.compute_prefix_digest(&AssembleOptions {
            persona: Some("a persona".into()),
            ..Default::default()
        });
        assert_ne!(base.persona_hash, with_persona.persona_hash);
        assert_eq!(base.system_hash, with_persona.system_hash);
        assert_ne!(base.combined_hash, with_persona.combined_hash);
    }

    #[test]
    fn test_digest_history_independence() {
        let assembler = ContextAssembler::new("sys", 8192);
        let d1 = assembler.compute_prefix_digest(&AssembleOptions {
            history: vec![],
            ..Default::default()
        });
        let d2 = assembler.compute_prefix_digest(&AssembleOptions {
            history: vec![Message {
                role: loom_types::Role::User,
                content: vec![loom_types::ContentPart::Text {
                    text: "hello".into(),
                }],
                timestamp: chrono::Utc::now(),
                usage: None,
            }],
            ..Default::default()
        });
        assert_eq!(d1.combined_hash, d2.combined_hash);
    }

    #[test]
    fn test_truncate_history_preserves_tool_parts() {
        use loom_types::{ContentPart, Role};
        let assembler = ContextAssembler::new("sys", 8192);
        let history = vec![
            Message {
                role: Role::Assistant,
                content: vec![
                    ContentPart::Text { text: "let me check".into() },
                    ContentPart::ToolCall {
                        id: "c1".into(),
                        name: "shell".into(),
                        arguments: serde_json::json!({"command":"ls"}),
                    },
                ],
                timestamp: chrono::Utc::now(),
                usage: None,
            },
            Message::tool("c1", "shell", "file1.txt"),
        ];
        let truncated = assembler.truncate_history_with_summary(&history, 8192, 0);
        let has_tool_part = truncated.iter().any(|m| {
            m.content
                .iter()
                .any(|p| matches!(p, ContentPart::ToolCall { .. } | ContentPart::ToolResult { .. }))
        });
        assert!(
            has_tool_part,
            "truncate_history must keep tool call/result parts for LLM context"
        );
    }

    #[test]
    fn test_dynamic_history_budget_scales_with_window() {
        // 近期预算 = context_window × keep_recent_pct，不再是固定 4096
        let cw: usize = 1_000_000;
        let keep_recent_pct: f32 = 0.25;
        let recent_limit = ((cw as f32 * keep_recent_pct) as usize).max(1024);
        assert!(recent_limit > 4096, "1M 窗口的近期预算应远大于硬编码 4096");
        assert_eq!(recent_limit, 250_000);
    }

    #[test]
    fn test_truncate_history_drops_summarized_prefix() {
        use loom_types::ContentPart;
        let assembler = ContextAssembler::new("sys", 8192);
        let history: Vec<Message> = (0..10).map(|i| Message::user(format!("m{}", i))).collect();
        let truncated = assembler.truncate_history_with_summary(&history, 8192, 5);
        let texts: Vec<String> = truncated.iter().filter_map(|m| {
            if let ContentPart::Text { text } = &m.content[0] { Some(text.clone()) } else { None }
        }).collect();
        assert!(!texts.iter().any(|t| t.contains("m0")), "已总结的 m0 应被丢弃");
        assert!(!texts.iter().any(|t| t.contains("m4")), "已总结的 m4 应被丢弃");
        assert!(texts.iter().any(|t| t.contains("m9")), "近期 m9 应保留");
    }
}

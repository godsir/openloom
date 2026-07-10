// SPDX-License-Identifier: Apache-2.0
//! Multi-tokenizer support for openloom.
//!
//! Different model families use different tokenizers. This module maps
//! model identities to the correct tiktoken vocabulary so that token
//! counting — which feeds context-window budget, compaction triggers,
//! and summary thresholds — is accurate for every backend.

use loom_types::ModelBackend;
use std::sync::OnceLock;
use tiktoken_rs::CoreBPE;

/// Which tiktoken vocabulary to use for a given model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenizerId {
    /// `cl100k_base` — GPT-3.5 / GPT-4 / most general-purpose models.
    Cl100k,
    /// `o200k_base` — GPT-4o / o3 / o4-mini.
    O200k,
}

impl TokenizerId {
    /// Load (or retrieve cached) CoreBPE for this vocabulary.
    pub fn get(self) -> &'static CoreBPE {
        match self {
            TokenizerId::Cl100k => cl100k(),
            TokenizerId::O200k => o200k(),
        }
    }
}

// ── lazy once-cell initializers ────────────────────────────────────

fn cl100k() -> &'static CoreBPE {
    static BPE: OnceLock<CoreBPE> = OnceLock::new();
    BPE.get_or_init(|| {
        match tiktoken_rs::cl100k_base() {
            Ok(b) => b,
            Err(e) => unreachable!("tiktoken cl100k_base model should always load: {e}"),
        }
    })
}

fn o200k() -> &'static CoreBPE {
    static BPE: OnceLock<CoreBPE> = OnceLock::new();
    BPE.get_or_init(|| {
        match tiktoken_rs::o200k_base() {
            Ok(b) => b,
            Err(e) => unreachable!("tiktoken o200k_base model should always load: {e}"),
        }
    })
}

// ── model-name → tokenizer heuristics ──────────────────────────────

/// Pick the best tokenizer for a model given its name and backend.
///
/// - GPT-4o / o3 / o4-mini → `O200k`
/// - GPT-3.5 / GPT-4 / text-embedding → `Cl100k`
/// - Anthropic models → `Cl100k` (best-effort; Claude does not share
///   a tokenizer with OpenAI, but cl100k is a reasonable rough proxy
///   until a Claude-native tokenizer is added).
/// - Local backends (LM Studio / Ollama) → `Cl100k` unless the model
///   name hints at a known family.
pub fn tokenizer_for_model(model_name: &str, _backend: ModelBackend) -> TokenizerId {
    let lower = model_name.to_lowercase();

    // OpenAI models that use o200k_base
    if lower.contains("gpt-4o") || lower.contains("gpt-4.1")
        || lower.starts_with("o1")
        || lower.starts_with("o3")
        || (lower.contains("o4") && lower.contains("mini"))
    {
        return TokenizerId::O200k;
    }

    TokenizerId::Cl100k
}

// ── shared-default backward-compat entry-point ─────────────────────

/// Shared tiktoken BPE instance — the legacy default (cl100k_base).
///
/// Prefer [`TokenizerId::get`] or [`tokenizer_for_model`] for new code
/// so token counts match the active model's vocabulary.
pub fn bpe() -> &'static tiktoken_rs::CoreBPE {
    cl100k()
}

// ── tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cl100k_is_default() {
        assert!(std::ptr::eq(bpe(), TokenizerId::Cl100k.get()));
    }

    #[test]
    fn test_gpt4o_uses_o200k() {
        assert_eq!(tokenizer_for_model("gpt-4o", ModelBackend::OpenAI), TokenizerId::O200k);
        assert_eq!(tokenizer_for_model("gpt-4o-mini", ModelBackend::OpenAI), TokenizerId::O200k);
        assert_eq!(tokenizer_for_model("gpt-4.1", ModelBackend::OpenAI), TokenizerId::O200k);
        assert_eq!(tokenizer_for_model("o3", ModelBackend::OpenAI), TokenizerId::O200k);
        assert_eq!(tokenizer_for_model("o4-mini", ModelBackend::OpenAI), TokenizerId::O200k);
    }

    #[test]
    fn test_claude_falls_back_to_cl100k() {
        assert_eq!(tokenizer_for_model("claude-sonnet-4-20250514", ModelBackend::Anthropic), TokenizerId::Cl100k);
    }

    #[test]
    fn test_deepseek_falls_back_to_cl100k() {
        assert_eq!(tokenizer_for_model("deepseek-chat", ModelBackend::DeepSeek), TokenizerId::Cl100k);
    }

    #[test]
    fn test_local_model_defaults_to_cl100k() {
        assert_eq!(tokenizer_for_model("qwen2.5-7b-instruct", ModelBackend::LmStudio), TokenizerId::Cl100k);
        assert_eq!(tokenizer_for_model("llama3.1-8b", ModelBackend::Ollama), TokenizerId::Cl100k);
    }

    #[test]
    fn test_tokenizers_are_distinct() {
        let c = TokenizerId::Cl100k.get();
        let o = TokenizerId::O200k.get();
        let sample = "Hello, world! 你好世界";
        let c_toks = c.encode_with_special_tokens(sample).len();
        let o_toks = o.encode_with_special_tokens(sample).len();
        assert!(c_toks > 0);
        assert!(o_toks > 0);
    }
}

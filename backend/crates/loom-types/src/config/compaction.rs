//! Session compaction configuration.
//!
//! Consumers: loom-context (ContextAssembler::compact), loom-core (orchestrator)

use serde::{Deserialize, Serialize};

/// Configuration for session compaction behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CompactionConfig {
    /// Master on/off switch. Default true (active).
    pub enabled: bool,
    /// Fraction of context_window at which turn-boundary LLM summarization triggers (0.0-1.0).
    pub trigger_threshold_pct: f32,
    /// Fraction of context_window at which mid-turn safety truncation triggers (0.0-1.0, > trigger).
    pub mid_turn_threshold_pct: f32,
    /// Maximum character count for a single tool output before mid-turn truncation.
    pub max_tool_output_chars: usize,
    /// Fraction of context_window kept as recent verbatim history (with full tool context).
    pub keep_recent_tokens_pct: f32,
    /// Whether to use LLM-based summarization (now implemented via SummaryEngine).
    pub use_llm_summarization: bool,
    /// Model to use for LLM summarization. None = use active model.
    pub summarization_model: Option<String>,
    /// Timeout in milliseconds for the LLM summarization call.
    pub summarization_timeout_ms: u64,
    /// Max output tokens for the summarization LLM call.
    pub summary_max_tokens: usize,
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            trigger_threshold_pct: 0.8,
            mid_turn_threshold_pct: 0.9,
            max_tool_output_chars: 2000,
            keep_recent_tokens_pct: 0.25,
            use_llm_summarization: true,
            summarization_model: None,
            summarization_timeout_ms: 60000,
            summary_max_tokens: 1024,
        }
    }
}

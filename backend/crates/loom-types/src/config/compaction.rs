//! Session compaction configuration.
//!
//! Consumers: loom-context (ContextAssembler::compact), loom-core (orchestrator)

use serde::{Deserialize, Serialize};

/// Configuration for session compaction behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionConfig {
    /// Master on/off switch. Default false during initial rollout.
    pub enabled: bool,
    /// Fraction of max_prompt_budget at which compaction triggers (0.0-1.0).
    pub trigger_threshold_pct: f32,
    /// Maximum character count for a single tool output before truncation.
    pub max_tool_output_chars: usize,
    /// Number of most recent messages to always keep uncompacted.
    pub keep_recent_messages: usize,
    /// Whether to use LLM-based summarization as a second-tier strategy.
    pub use_llm_summarization: bool,
    /// Model to use for LLM summarization. None = use active model.
    pub summarization_model: Option<String>,
    /// Timeout in milliseconds for the LLM summarization call.
    pub summarization_timeout_ms: u64,
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            trigger_threshold_pct: 0.8,
            max_tool_output_chars: 2000,
            keep_recent_messages: 6,
            use_llm_summarization: true,
            summarization_model: None,
            summarization_timeout_ms: 15000,
        }
    }
}

//! StreamDelta → AppEvent converter.
//!
//! Receives StreamDelta from the orchestrator channel and maps each
//! variant to an AppEvent for the main TUI loop.

use crate::tui::app::TokenStats;
use loom_types::StreamDelta;

/// Events the TUI main loop consumes.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum AppEvent {
    /// Append text delta to current assistant response.
    TextChunk(String),
    /// Append reasoning delta to thinking line.
    ReasoningChunk(String),
    /// A tool call started.
    ToolBegin {
        index: usize,
        id: String,
        name: String,
    },
    /// A chunk of tool call arguments (ignored by TUI display).
    ToolArgsChunk { index: usize, chunk: String },
    /// A tool call completed or failed.
    ToolResult {
        call_id: String,
        tool_name: String,
        success: bool,
    },
    /// Token usage update.
    Usage(TokenStats),
    /// Auxiliary model usage (ignored by TUI display for now).
    AuxiliaryUsage {
        model: String,
        prompt_tokens: u64,
        completion_tokens: u64,
    },
    /// Stream ended normally.
    Done,
}

/// Convert a StreamDelta into an AppEvent.
pub fn convert(delta: StreamDelta) -> AppEvent {
    match delta {
        StreamDelta::Text(t) => AppEvent::TextChunk(t),
        StreamDelta::Reasoning(r) => AppEvent::ReasoningChunk(r),
        StreamDelta::ToolCallBegin { index, id, name } => AppEvent::ToolBegin { index, id, name },
        StreamDelta::ToolCallArgsChunk { index, chunk } => {
            AppEvent::ToolArgsChunk { index, chunk }
        }
        StreamDelta::ToolResult {
            call_id,
            tool_name,
            success,
            ..
        } => AppEvent::ToolResult {
            call_id,
            tool_name,
            success,
        },
        StreamDelta::Usage {
            prompt_tokens,
            completion_tokens,
            cache_read_tokens,
            cache_write_tokens,
        } => AppEvent::Usage(TokenStats {
            prompt: prompt_tokens,
            completion: completion_tokens,
            cache_read: cache_read_tokens,
            cache_write: cache_write_tokens,
            tool_count: 0,
        }),
        StreamDelta::AuxiliaryUsage {
            model,
            prompt_tokens,
            completion_tokens,
        } => AppEvent::AuxiliaryUsage {
            model,
            prompt_tokens,
            completion_tokens,
        },
        StreamDelta::Image { .. } => AppEvent::TextChunk("[image]".into()),
    }
}

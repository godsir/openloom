//! StreamDelta → AppEvent converter.
//!
//! Single choke point for ANSI sanitization — all AI-generated text
//! passes through here before entering the TUI.

use crate::tui::app::TokenStats;
use loom_types::StreamDelta;

#[derive(Debug, Clone)]
pub enum AppEvent {
    TextChunk(String),
    ReasoningChunk(String),
    ToolBegin { index: usize, name: String },
    ToolArgsChunk { #[allow(dead_code)] index: usize, chunk: String },
    ToolResult { tool_name: String, success: bool },
    Usage(TokenStats),
    #[allow(dead_code)]
    AuxiliaryUsage { prompt_tokens: u64, completion_tokens: u64 },
}

/// Strip ANSI escape sequences and non-printable control chars.
/// Keeps \n, \t, \r.
pub fn sanitize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            if let Some(&next) = chars.peek() {
                if next == '[' || next == ']' {
                    chars.next();
                    while let Some(&inner) = chars.peek() {
                        if (0x40..=0x7E).contains(&(inner as u32)) { chars.next(); break; }
                        if inner == '\x1b' || !inner.is_ascii() { break; }
                        chars.next();
                    }
                }
            }
        } else if c == '\u{9b}' {
            while let Some(&inner) = chars.peek() {
                if (0x40..=0x7E).contains(&(inner as u32)) { chars.next(); break; }
                if inner == '\x1b' || !inner.is_ascii() { break; }
                chars.next();
            }
        } else if c == '\x07' {
            out.push('\u{2407}');
        } else if c.is_control() && c != '\n' && c != '\t' && c != '\r' {
            out.push('·');
        } else {
            out.push(c);
        }
    }
    out
}

pub fn convert(delta: StreamDelta) -> AppEvent {
    match delta {
        StreamDelta::Text(t) => AppEvent::TextChunk(sanitize(&t)),
        StreamDelta::Reasoning(r) => AppEvent::ReasoningChunk(sanitize(&r)),
        StreamDelta::ToolCallBegin { index, name, .. } => AppEvent::ToolBegin { index, name: sanitize(&name) },
        StreamDelta::ToolCallArgsChunk { index, chunk } => AppEvent::ToolArgsChunk { index, chunk: sanitize(&chunk) },
        StreamDelta::ToolResult { tool_name, success, .. } => AppEvent::ToolResult { tool_name: sanitize(&tool_name), success },
        StreamDelta::Usage { prompt_tokens, completion_tokens, cache_read_tokens, cache_write_tokens } => {
            AppEvent::Usage(TokenStats { prompt: prompt_tokens, completion: completion_tokens, cache_read: cache_read_tokens, cache_write: cache_write_tokens, tool_count: 0 })
        }
        StreamDelta::AuxiliaryUsage { prompt_tokens, completion_tokens, .. } => {
            AppEvent::AuxiliaryUsage { prompt_tokens, completion_tokens }
        }
        StreamDelta::Image { media_type, data, .. } => {
            AppEvent::TextChunk(format!("  [image: {} {}KB]", media_type, data.len() / 1024))
        }
    }
}

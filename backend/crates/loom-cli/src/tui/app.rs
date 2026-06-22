//! TUI application state — pure data, no terminal I/O.
//!
//! # Architecture
//!
//! The chat history is a flat list of typed `HistoryItem` values. This mirrors
//! how Claude Code / Gemini CLI model their conversation stream:
//!
//!   User { text }
//!   Thinking { text }
//!   ToolGroup { tools[] }
//!   Assistant { blocks[] }
//!   Info { text }
//!
//! There is no separate "tool panel" — tool calls and their results appear
//! inline in the conversation flow, exactly where they happened.

use std::cell::Cell;

// ── History items ──────────────────────────────────────────────────

/// A single entry in the scrollable conversation history.
#[derive(Debug, Clone)]
pub enum HistoryItem {
    /// User message.
    User { text: String },
    /// Assistant response — rendered markdown.
    Assistant { blocks: Vec<ContentBlock> },
    /// Extended thinking / chain-of-thought (collapsible in rendering).
    Thinking { text: String },
    /// Group of tool calls from the current turn — rendered inline.
    ToolGroup { tools: Vec<ToolCall> },
    /// Informational system message.
    Info { text: String },
}

/// Content block within an assistant message.
#[derive(Debug, Clone)]
pub enum ContentBlock {
    Markdown(String),
}

/// A single tool call within a ToolGroup.
#[derive(Debug, Clone)]
pub struct ToolCall {
    pub name: String,
    pub args: String,
    pub status: ToolStatus,
    pub result: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolStatus {
    Running,
    Done,
    Failed,
}

// ── Application state ──────────────────────────────────────────────

pub struct AppState {
    /// Conversation history — everything flows through here.
    pub history: Vec<HistoryItem>,
    /// Input buffer.
    pub input: String,
    /// Cursor position in input.
    pub cursor: usize,
    /// Chat scroll offset (0 = newest at bottom).
    pub scroll_offset: u16,
    /// Auto-follow new content.
    pub scroll_following: bool,
    /// Viewport height (updated each draw).
    pub viewport_rows: Cell<u16>,
    /// Token counters.
    pub tokens: TokenStats,
    /// Streaming flag.
    pub streaming: bool,
    /// Model name.
    pub model_name: String,
    /// Overlay popup.
    pub overlay: Option<OverlayContent>,
    /// History item count cap.
    max_history: usize,
}

#[derive(Debug, Clone)]
pub struct OverlayContent {
    pub title: String,
    pub body: String,
}

#[derive(Debug, Clone, Default)]
pub struct TokenStats {
    pub prompt: u64,
    pub completion: u64,
    #[allow(dead_code)]
    pub cache_read: u64,
    #[allow(dead_code)]
    pub cache_write: u64,
    #[allow(dead_code)]
    pub tool_count: usize,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            history: Vec::new(),
            input: String::new(),
            cursor: 0,
            scroll_offset: 0,
            scroll_following: true,
            viewport_rows: Cell::new(20),
            tokens: TokenStats::default(),
            streaming: false,
            model_name: String::new(),
            overlay: None,
            max_history: 300,
        }
    }
}

// ── State mutations ────────────────────────────────────────────────

impl AppState {
    pub fn push(&mut self, item: HistoryItem) {
        self.history.push(item);
        self.prune();
        if self.scroll_following { self.scroll_offset = 0; }
    }

    /// Find or create the current assistant message and return the
    /// last Markdown block's text buffer for appending. If no
    /// Markdown block exists, creates one.
    pub fn append_assistant_text(&mut self, text: &str) {
        let has_last = self.history.last().map_or(false, |h| {
            matches!(h, HistoryItem::Assistant { .. })
        });
        if !has_last {
            self.history.push(HistoryItem::Assistant {
                blocks: vec![ContentBlock::Markdown(text.to_string())],
            });
        } else if let Some(HistoryItem::Assistant { blocks }) = self.history.last_mut() {
            if let Some(ContentBlock::Markdown(buf)) = blocks.last_mut() {
                buf.push_str(text);
            } else {
                blocks.push(ContentBlock::Markdown(text.to_string()));
            }
        }
        self.prune();
        if self.scroll_following { self.scroll_offset = 0; }
    }

    /// Flush accumulated thinking text.
    pub fn flush_thinking(&mut self, text: String) {
        if text.is_empty() { return; }
        self.push(HistoryItem::Thinking { text });
    }

    /// Start or replace the current-turn ToolGroup.
    pub fn set_tool_group(&mut self, tools: Vec<ToolCall>) {
        // Find and remove existing incomplete ToolGroup
        self.history.retain(|h| !matches!(h, HistoryItem::ToolGroup { .. }));
        self.push(HistoryItem::ToolGroup { tools });
    }

    /// Get current tool calls for mutation.
    pub fn tool_group_mut(&mut self) -> Option<&mut Vec<ToolCall>> {
        self.history.iter_mut().rev().find_map(|h| {
            if let HistoryItem::ToolGroup { tools } = h { Some(tools) } else { None }
        })
    }

    // ── Turn lifecycle ──

    pub fn start_turn(&mut self) {
        self.tokens = TokenStats::default();
        self.streaming = true;
        self.scroll_following = true;
        self.scroll_offset = 0;
    }

    pub fn end_turn(&mut self) {
        self.streaming = false;
    }

    // ── Input helpers ──

    fn prune(&mut self) {
        if self.history.len() <= self.max_history { return; }
        let remove = self.history.len() - self.max_history;
        let drained = remove.saturating_mul(3) as u16; // rough line estimate
        self.history.drain(0..remove);
        self.scroll_offset = self.scroll_offset.saturating_sub(drained);
    }

    pub fn input_insert(&mut self, c: char) {
        let pos = self.cursor.min(self.input.len());
        self.input.insert(pos, c);
        self.cursor = pos + c.len_utf8();
    }

    pub fn input_backspace(&mut self) {
        if self.cursor == 0 { return; }
        let mut prev = self.cursor - 1;
        while prev > 0 && !self.input.is_char_boundary(prev) { prev -= 1; }
        self.input.remove(prev);
        self.cursor = prev;
    }

    pub fn input_delete(&mut self) {
        if self.cursor < self.input.len() { self.input.remove(self.cursor); }
    }

    pub fn cursor_left(&mut self) {
        if self.cursor == 0 { return; }
        let mut prev = self.cursor - 1;
        while prev > 0 && !self.input.is_char_boundary(prev) { prev -= 1; }
        self.cursor = prev;
    }

    pub fn cursor_right(&mut self) {
        if self.cursor < self.input.len() {
            let mut next = self.cursor + 1;
            while next < self.input.len() && !self.input.is_char_boundary(next) { next += 1; }
            self.cursor = next;
        }
    }

    pub fn cursor_home(&mut self) { self.cursor = 0; }
    pub fn cursor_end(&mut self)  { self.cursor = self.input.len(); }
}

pub const HELP_TEXT: &str = "\
┌──────────────────────────────────────────┐
│        openLoom TUI — Shortcuts           │
├──────────────────────────────────────────┤
│  Enter          Send message              │
│  Ctrl+Enter     Newline                   │
│  ↑ ↓            Scroll by line            │
│  PgUp / PgDn    Scroll by page            │
│  Home / End     Jump top / bottom         │
│  ← →            Move cursor               │
│  Backspace      Delete before cursor      │
│  Delete         Delete at cursor          │
│  Tab            Indent                    │
│  Esc            Close / clear input       │
│  Ctrl+C         Cancel / Quit             │
│  /help          Show this help            │
│  /tools         List available tools      │
│  /skills        List loaded skills        │
│  /exit          Quit                      │
└──────────────────────────────────────────┘";

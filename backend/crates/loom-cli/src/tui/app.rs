//! TUI application state — pure data, no terminal I/O.

/// One message in the chat area.
#[derive(Debug, Clone)]
pub struct ChatLine {
    pub role: &'static str, // "user", "assistant", "thinking", "tool"
    pub text: String,
}

/// One entry in the tool-call panel.
#[derive(Debug, Clone)]
pub struct ToolEntry {
    pub index: usize,
    pub name: String,
    pub status: ToolStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ToolStatus {
    Waiting,
    Running,
    Done,
    Failed,
}

/// Main application state.
pub struct AppState {
    /// All chat messages (scrollable).
    pub chat_lines: Vec<ChatLine>,
    /// Current-round tool calls (cleared each turn).
    pub tools: Vec<ToolEntry>,
    /// Input buffer (what the user is typing).
    pub input: String,
    /// Chat scroll offset (0 = newest at bottom).
    pub scroll_offset: u16,
    /// Token counters for status bar.
    pub tokens: TokenStats,
    /// Whether we're currently streaming a response.
    pub streaming: bool,
    /// Model name for status bar.
    pub model_name: String,
    /// Error message overlay (None = no error).
    pub error: Option<String>,
    /// Popup overlay text (for /tools / /skills).
    pub overlay: Option<OverlayContent>,
}

#[derive(Debug, Clone)]
pub struct OverlayContent {
    pub title: String,
    pub body: String,
}

#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct TokenStats {
    pub prompt: u64,
    pub completion: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub tool_count: usize,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            chat_lines: Vec::new(),
            tools: Vec::new(),
            input: String::new(),
            scroll_offset: 0,
            tokens: TokenStats::default(),
            streaming: false,
            model_name: String::new(),
            error: None,
            overlay: None,
        }
    }
}

impl AppState {
    /// Push a chat line and clamp total length.
    pub fn push_line(&mut self, line: ChatLine) {
        self.chat_lines.push(line);
        // Keep at most 500 lines to bound memory.
        if self.chat_lines.len() > 500 {
            let remove = self.chat_lines.len() - 500;
            self.chat_lines.drain(0..remove);
        }
    }

    /// Append text to the last chat line (for streaming).
    pub fn append_text(&mut self, text: &str) {
        if let Some(last) = self.chat_lines.last_mut()
            && last.role == "assistant"
        {
            last.text.push_str(text);
        } else {
            self.push_line(ChatLine {
                role: "assistant",
                text: text.to_string(),
            });
        }
    }

    /// Append reasoning to a thinking buffer (shown as a separate "thinking" line).
    #[allow(dead_code)]
    pub fn append_thinking(&mut self, text: &str) {
        if let Some(last) = self.chat_lines.last_mut()
            && last.role == "thinking"
        {
            last.text.push_str(text);
        } else {
            self.push_line(ChatLine {
                role: "thinking",
                text: text.to_string(),
            });
        }
    }

    /// Add or update a tool entry.
    pub fn upsert_tool(&mut self, index: usize, name: &str, status: ToolStatus) {
        if let Some(t) = self.tools.iter_mut().find(|t| t.index == index) {
            t.status = status;
        } else {
            self.tools.push(ToolEntry {
                index,
                name: name.to_string(),
                status,
            });
        }
    }

    /// Clear state for a new turn.
    pub fn start_turn(&mut self) {
        self.tools.clear();
        self.tokens = TokenStats::default();
        self.streaming = true;
        self.error = None;
        self.overlay = None;
    }

    pub fn end_turn(&mut self) {
        self.streaming = false;
    }

    #[allow(dead_code)]
    pub fn set_error(&mut self, msg: String) {
        self.error = Some(msg);
        self.streaming = false;
    }
}

use std::sync::Arc;

use openloom_engine::Engine;
use openloom_models::{AgentState, EngineEvent};
use tokio::sync::broadcast;
use tui_textarea::TextArea;

use crate::tui::keymap::ResolvedKeymap;
use crate::tui::overlays::Overlay;
use crate::tui::status::StatusLine;
use crate::tui::streaming::StreamState;
use crate::tui::theme::Theme;

#[derive(Debug, Clone, PartialEq)]
pub enum CtrlCAction {
    Quit,
    CancelStream,
    ShowHint,
}

#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub enum AppState {
    Idle,
    Waiting,
    Streaming,
    Overlay,
}

#[derive(Debug, Clone)]
pub struct Message {
    pub role: String,
    pub content: String,
    pub collapsed: bool,
    /// Elapsed time in milliseconds for this response (assistant messages only).
    pub elapsed_ms: Option<u64>,
}

impl Message {
    pub fn user(content: String) -> Self {
        Self {
            role: "user".into(),
            content,
            collapsed: false,
            elapsed_ms: None,
        }
    }

    pub fn assistant(content: String) -> Self {
        Self {
            role: "assistant".into(),
            content,
            collapsed: false,
            elapsed_ms: None,
        }
    }
}

/// Scroll viewport for message list. Tracks how many lines the user
/// has scrolled back from the bottom. 0 = at bottom (auto-scroll).
#[derive(Debug, Clone)]
pub struct Viewport {
    /// Lines scrolled above the visible area. 0 means showing the bottom.
    pub scroll_offset: usize,
    /// True when following new content automatically.
    pub auto_scroll: bool,
    /// Count of new messages that arrived while user was scrolled up.
    pub unseen_count: usize,
}

impl Viewport {
    pub fn new() -> Self {
        Self {
            scroll_offset: 0,
            auto_scroll: true,
            unseen_count: 0,
        }
    }

    /// Call when user manually scrolls up.
    pub fn scroll_up(&mut self, lines: usize) {
        self.auto_scroll = false;
        self.scroll_offset = self.scroll_offset.saturating_add(lines);
    }

    /// Call when user manually scrolls down.
    pub fn scroll_down(&mut self, lines: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(lines);
        if self.scroll_offset == 0 {
            self.auto_scroll = true;
            self.unseen_count = 0;
        }
    }

    /// Call when new content is added (message sent, streaming token).
    pub fn content_added(&mut self) {
        if !self.auto_scroll {
            self.unseen_count = self.unseen_count.saturating_add(1);
        }
    }

    pub fn jump_to_bottom(&mut self) {
        self.scroll_offset = 0;
        self.auto_scroll = true;
        self.unseen_count = 0;
    }
}

pub struct App {
    pub engine: Arc<Engine>,
    pub session_id: String,
    pub messages: Vec<Message>,
    pub input: TextArea<'static>,
    pub history: Vec<String>,
    pub history_idx: Option<usize>,
    pub state: AppState,
    pub viewport: Viewport,
    pub status: StatusLine,
    pub theme: Theme,
    pub event_rx: broadcast::Receiver<EngineEvent>,
    pub should_exit: bool,
    pub total_prompt_tokens: usize,
    pub total_completion_tokens: usize,
    pub total_cached_tokens: usize,
    #[allow(dead_code)]
    pub total_cost: f64,
    pub frame_count: u64,
    pub stream: StreamState,
    pub overlay: Option<Box<dyn Overlay>>,
    pub pending_command: Option<String>,
    pub keymap: ResolvedKeymap,
    pub command_palette_selected: usize,
    pub last_ctrl_c: Option<std::time::Instant>,
    pub stream_start: Option<std::time::Instant>,
    pub stream_tokens_count: usize,
    /// Index of the next message to flush into terminal scrollback.
    /// Messages before this index have already been written via insert_before.
    pub flushed_up_to: usize,
    /// External skill commands for the command palette (e.g. from plugins).
    pub external_commands: Vec<(String, String)>,
    /// Active skill context injected into the next LLM call (like Claude Code).
    pub active_skill_context: Option<String>,
    pub mode: openloom_models::Mode,
    pub model_pref: openloom_models::ModelPreference,
    pub thinking: openloom_models::ThinkingLevel,
    /// Receiver for permission requests from the engine's agent loop.
    pub perm_rx: Option<tokio::sync::mpsc::Receiver<openloom_engine::PermissionChannelItem>>,
    /// Pending oneshot sender to respond to a permission request after the approval overlay closes.
    pub pending_perm_response: Option<tokio::sync::oneshot::Sender<bool>>,
    pub history_search_active: bool,
}

pub fn build_textarea() -> TextArea<'static> {
    let mut ta = TextArea::default();
    ta.set_cursor_line_style(ratatui::style::Style::default());
    ta.set_placeholder_text("> ");
    ta.set_placeholder_style(
        ratatui::style::Style::new().fg(ratatui::style::Color::Rgb(102, 102, 102)),
    );
    ta
}

impl App {
    pub fn new(
        engine: Arc<Engine>,
        session_id: String,
        cwd: String,
        model_name: String,
        git_branch: String,
        context_max: usize,
    ) -> Self {
        let event_rx = engine.subscribe();
        let perm_rx = engine.take_permission_rx();
        Self {
            engine,
            session_id,
            messages: Vec::new(),
            input: build_textarea(),
            history: Vec::new(),
            history_idx: None,
            state: AppState::Idle,
            viewport: Viewport::new(),
            status: StatusLine {
                model: model_name,
                agent_state: AgentState::Idle,
                context_pct: 0.0,
                turn_tokens: 0,
                git_branch,
                cwd,
                context_max,
                last_model: None,
            },
            theme: Theme::dark(),
            event_rx,
            should_exit: false,
            total_prompt_tokens: 0,
            total_completion_tokens: 0,
            total_cached_tokens: 0,
            total_cost: 0.0,
            frame_count: 0,
            stream: StreamState::new(),
            overlay: None,
            pending_command: None,
            keymap: ResolvedKeymap::default(),
            command_palette_selected: 0,
            last_ctrl_c: None,
            stream_start: None,
            stream_tokens_count: 0,
            flushed_up_to: 0,
            external_commands: Vec::new(),
            active_skill_context: None,
            mode: openloom_models::Mode::default(),
            model_pref: openloom_models::ModelPreference::default(),
            thinking: openloom_models::ThinkingLevel::default(),
            perm_rx,
            pending_perm_response: None,
            history_search_active: false,
        }
    }

    pub fn current_line(&self) -> String {
        self.input.lines().join("\n")
    }

    pub fn add_assistant_message(&mut self, content: String) {
        self.messages.push(Message::assistant(content));
        self.viewport.content_added();
    }

    pub fn poll_engine_events(&mut self) {
        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                EngineEvent::AgentStateChanged { new_state, .. } => {
                    self.status.agent_state = new_state;
                }
                EngineEvent::TokenUsage {
                    prompt_tokens,
                    completion_tokens,
                    cached_tokens,
                    model,
                    ..
                } => {
                    self.total_prompt_tokens += prompt_tokens;
                    self.total_completion_tokens += completion_tokens;
                    self.total_cached_tokens += cached_tokens;
                    self.status.turn_tokens = completion_tokens;
                    self.status.last_model = Some(model);
                    self.total_cost += (prompt_tokens as f64) * 3.0 / 1_000_000.0
                        + (completion_tokens as f64) * 15.0 / 1_000_000.0;
                }
                EngineEvent::CognitionUpdated {
                    trait_name,
                    new_value,
                    confidence,
                    ..
                } => {
                    self.messages.push(Message {
                        role: "thinking".into(),
                        content: format!(
                            "[cognition] {} = {} ({:.0}%)",
                            trait_name,
                            new_value,
                            confidence * 100.0
                        ),
                        collapsed: true,
                        elapsed_ms: None,
                    });
                    self.viewport.content_added();
                }
                EngineEvent::Error { message, .. } => {
                    self.messages.push(Message {
                        role: "error".into(),
                        content: message,
                        collapsed: false,
                        elapsed_ms: None,
                    });
                    self.viewport.content_added();
                }
                EngineEvent::HeartbeatTick { .. } => {}
                EngineEvent::PermissionRequired { .. } => {}
            }
        }
    }

    pub fn poll_stream_tokens(&mut self) {
        let mut done = false;
        if let Some(ref mut rx) = self.stream.token_rx {
            loop {
                match rx.try_recv() {
                    Ok(token) => {
                        if self.state != AppState::Streaming {
                            self.state = AppState::Streaming;
                            self.stream_start = Some(std::time::Instant::now());
                            self.stream_tokens_count = 0;
                        }

                        // Parse structured markers from agent loop
                        if let Some(content) = token.strip_prefix("\x01THINK\x02") {
                            self.messages.push(Message {
                                role: "thinking".into(),
                                content: content.to_string(),
                                collapsed: true,
                                elapsed_ms: None,
                            });
                            self.viewport.content_added();
                            continue;
                        }
                        if let Some(content) = token.strip_prefix("\x01CALL\x02") {
                            self.messages.push(Message {
                                role: "tool_call".into(),
                                content: content.to_string(),
                                collapsed: true,
                                elapsed_ms: None,
                            });
                            self.viewport.content_added();
                            continue;
                        }
                        if let Some(content) = token.strip_prefix("\x01RESULT\x02") {
                            self.messages.push(Message {
                                role: "tool_result".into(),
                                content: content.to_string(),
                                collapsed: true,
                                elapsed_ms: None,
                            });
                            self.viewport.content_added();
                            continue;
                        }

                        self.stream.buffer.push_str(&token);
                        self.stream_tokens_count = self.stream.buffer.len().div_ceil(4);

                        // Update the most recent assistant message — it may not be
                        // the last message if tool_call/tool_result were inserted after it.
                        if let Some(assistant_msg) = self
                            .messages
                            .iter_mut()
                            .rev()
                            .find(|m| m.role == "assistant")
                        {
                            assistant_msg.content = self.stream.buffer.clone();
                        }
                        self.viewport.content_added();
                    }
                    Err(tokio::sync::mpsc::error::TryRecvError::Empty) => break,
                    Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                        done = true;
                        break;
                    }
                }
            }
        }
        if done {
            self.stream.token_rx = None;
            self.stream.abort_handle = None;
        }
    }

    pub fn start_streaming(&mut self, user_message: String) {
        use openloom_models::ChatMessage;

        // Inject active skill context into the user message so the LLM follows it
        let content = if let Some(ref ctx) = self.active_skill_context {
            format!(
                "[Active Skill Instructions]\n{}\n\n[User Message]\n{}",
                ctx, user_message
            )
        } else {
            user_message
        };

        let msg = ChatMessage {
            role: "user".into(),
            content,
            timestamp: chrono::Utc::now(),
        };

        let (tx, rx) = tokio::sync::mpsc::channel::<String>(64);
        self.stream.token_rx = Some(rx);
        self.stream.buffer.clear();

        self.add_assistant_message(String::new());

        let engine = self.engine.clone();
        let session_id = self.session_id.clone();
        let mode = self.mode;
        let model_pref = self.model_pref;
        let thinking = self.thinking;
        let handle = tokio::spawn(async move {
            let _ = engine.handle_message_streaming(msg, &session_id, tx, mode, model_pref, thinking).await;
        });

        self.stream.abort_handle = Some(handle);
    }

    /// Two-press Ctrl+C pattern: first press cancels streaming or shows hint,
    /// second press within 2 seconds quits. Matches Claude Code behavior.
    pub fn handle_ctrl_c(&mut self) -> CtrlCAction {
        if self.state == AppState::Streaming {
            return CtrlCAction::CancelStream;
        }

        let now = std::time::Instant::now();
        if let Some(last) = self.last_ctrl_c
            && now.duration_since(last) < std::time::Duration::from_secs(2)
        {
            return CtrlCAction::Quit;
        }
        self.last_ctrl_c = Some(now);
        CtrlCAction::ShowHint
    }
}

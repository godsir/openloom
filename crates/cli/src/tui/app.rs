use std::sync::Arc;

use openloom_engine::Engine;
use openloom_models::{AgentState, EngineEvent};
use tokio::sync::broadcast;
use tui_textarea::TextArea;

use crate::tui::keymap::ResolvedKeymap;
use crate::tui::status::StatusLine;
use crate::tui::streaming::StreamState;
use crate::tui::theme::Theme;

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
}

impl Message {
    pub fn user(content: String) -> Self {
        Self {
            role: "user".into(),
            content,
        }
    }

    pub fn assistant(content: String) -> Self {
        Self {
            role: "assistant".into(),
            content,
        }
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
    pub scroll: u16,
    pub auto_scroll: bool,
    pub status: StatusLine,
    pub theme: Theme,
    pub event_rx: broadcast::Receiver<EngineEvent>,
    pub should_exit: bool,
    pub total_prompt_tokens: usize,
    pub total_completion_tokens: usize,
    #[allow(dead_code)]
    pub total_cost: f64,
    pub frame_count: u64,
    pub stream: StreamState,
    pub pending_command: Option<String>,
    pub keymap: ResolvedKeymap,
}

pub fn build_textarea() -> TextArea<'static> {
    let mut ta = TextArea::default();
    ta.set_cursor_line_style(ratatui::style::Style::default());
    ta.set_placeholder_text("Type a message... (Enter to send, Ctrl+C to quit)");
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
        Self {
            engine,
            session_id,
            messages: Vec::new(),
            input: build_textarea(),
            history: Vec::new(),
            history_idx: None,
            state: AppState::Idle,
            scroll: 0,
            auto_scroll: true,
            status: StatusLine {
                model: model_name,
                agent_state: AgentState::Idle,
                context_pct: 0.0,
                turn_tokens: 0,
                git_branch,
                cwd,
                context_max,
            },
            theme: Theme::dark(),
            event_rx,
            should_exit: false,
            total_prompt_tokens: 0,
            total_completion_tokens: 0,
            total_cost: 0.0,
            frame_count: 0,
            stream: StreamState::new(),
            pending_command: None,
            keymap: ResolvedKeymap::default(),
        }
    }

    pub fn current_line(&self) -> String {
        self.input.lines().join("\n")
    }

    pub fn add_assistant_message(&mut self, content: String) {
        self.messages.push(Message::assistant(content));
    }

    pub fn poll_engine_events(&mut self) {
        // Milestone A: only track agent state from broadcast events.
        // Token stats come from handle_message() return value to avoid double-counting
        // (handle_message broadcasts TokenUsage AND returns it in ChatResponse).
        while let Ok(event) = self.event_rx.try_recv() {
            if let EngineEvent::AgentStateChanged { new_state, .. } = event {
                self.status.agent_state = new_state;
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
                        }
                        self.stream.buffer.push_str(&token);
                        if let Some(last) = self.messages.last_mut() {
                            if last.role == "assistant" {
                                last.content = self.stream.buffer.clone();
                            }
                        }
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
        use openloom_inference::CompletionRequest;

        let req = CompletionRequest {
            prompt: user_message,
            max_tokens: 2048,
            temperature: 0.7,
            top_p: 1.0,
            stop: Vec::new(),
            stream: true,
        };

        let (tx, rx) = tokio::sync::mpsc::channel::<String>(64);
        self.stream.token_rx = Some(rx);
        self.stream.buffer.clear();

        // Add empty assistant placeholder
        self.add_assistant_message(String::new());

        let engine = self.engine.clone();
        let handle = tokio::spawn(async move {
            let _ = engine.stream_complete(req, tx).await;
        });

        self.stream.abort_handle = Some(handle);
    }
}

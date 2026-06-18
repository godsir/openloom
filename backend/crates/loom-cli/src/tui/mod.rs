//! TUI chat entry point.
//!
//! Single event-loop architecture:
//!   1. Poll keyboard (crossterm, 50ms timeout)
//!   2. If streaming, drain the shared buffer
//!   3. Apply events → AppState
//!   4. Redraw (ratatui)
//!
//! Ctrl+C is detected through TWO paths:
//!   a. crossterm KeyEvent (primary — works on all platforms in raw mode)
//!   b. ctrlc crate signal handler (fallback — POSIX, or Windows outside raw mode)
//!
//! On Windows raw mode: Ctrl+C → keyboard input record with UnicodeChar=3 (ETX).
//! We match both `\u{3}` (Windows) and `'c'+CONTROL` (POSIX).

pub mod app;
pub mod stream;
pub mod ui;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use loom_core::Orchestrator;
use loom_types::StreamDelta;
use ratatui::prelude::*;
use tokio::sync::mpsc;

use app::{AppState, ChatLine, ToolStatus};
use stream::{convert, AppEvent};

type EventBuffer = Arc<Mutex<Vec<AppEvent>>>;

fn apply_event(ev: AppEvent, state: &mut AppState, thinking_buf: &mut String) {
    match ev {
        AppEvent::TextChunk(t) => {
            if !thinking_buf.is_empty() {
                state.push_line(ChatLine {
                    role: "thinking",
                    text: std::mem::take(thinking_buf),
                });
            }
            state.append_text(&t);
        }
        AppEvent::ReasoningChunk(r) => thinking_buf.push_str(&r),
        AppEvent::ToolBegin { index, name, .. } => {
            state.upsert_tool(index, &name, ToolStatus::Running);
        }
        AppEvent::ToolResult {
            tool_name, success, ..
        } => {
            if let Some(t) = state
                .tools
                .iter_mut()
                .find(|t| t.name == tool_name && t.status == ToolStatus::Running)
            {
                t.status = if success {
                    ToolStatus::Done
                } else {
                    ToolStatus::Failed
                };
            }
        }
        AppEvent::Usage(ts) => state.tokens = ts,
        _ => {}
    }
}

/// Detect Ctrl+C across platforms.
///
/// Windows (raw mode): `KeyCode::Char('\u{3}')` (ETX control character).
/// POSIX (raw mode):  `KeyCode::Char('c')` or `Char('C')` + `KeyModifiers::CONTROL`.
fn is_ctrl_c(key: &crossterm::event::KeyEvent) -> bool {
    if key.kind != KeyEventKind::Press {
        return false;
    }
    // Always accept \u{3} (the raw control-C character, common on Windows).
    if matches!(key.code, KeyCode::Char('\u{3}')) {
        return true;
    }
    // Also accept c/C with CONTROL modifier (POSIX, some terminals on Windows).
    if key.modifiers.contains(KeyModifiers::CONTROL)
        && matches!(key.code, KeyCode::Char('c') | KeyCode::Char('C'))
    {
        return true;
    }
    false
}

pub async fn run_tui(
    orchestrator: Arc<Orchestrator>,
    model_name: String,
    session_id: String,
) -> anyhow::Result<()> {
    // ── Terminal setup ──
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut state = AppState::default();
    state.model_name = model_name;

    let mut thinking_buf = String::new();

    // ── Ctrl+C signal flag (ctrlc crate — works on POSIX, may work on Windows) ──
    let cancel_flag = Arc::new(AtomicBool::new(false));
    {
        let f = cancel_flag.clone();
        ctrlc::set_handler(move || {
            f.store(true, Ordering::SeqCst);
        })
        .ok();
    }

    // ── Streaming state (set when a turn is in progress) ──
    // These are Option because a turn is started/ended per-message.
    let mut orch_handle: Option<tokio::task::JoinHandle<()>> = None;
    let mut bridge_handle: Option<tokio::task::JoinHandle<()>> = None;
    let mut stream_buffer: Option<EventBuffer> = None;

    // ── Single event loop (NO inner pump — everything goes through here) ──
    'outer: loop {
        // ── 1. Check ctrlc signal flag ──
        if cancel_flag.swap(false, Ordering::SeqCst) {
            if state.streaming {
                // Cancel the streaming turn
                if let Some(h) = orch_handle.take() {
                    h.abort();
                }
                if let Some(h) = bridge_handle.take() {
                    h.abort();
                }
                stream_buffer = None;
                state.streaming = false;
                state.push_line(ChatLine {
                    role: "thinking",
                    text: "[cancelled]".into(),
                });
                if !thinking_buf.is_empty() {
                    thinking_buf.clear();
                }
            } else {
                break 'outer;
            }
        }

        // ── 2. Drain stream buffer (if streaming) ──
        if let Some(ref buf) = stream_buffer {
            let events: Vec<AppEvent> = {
                let mut g = buf.lock().unwrap();
                std::mem::take(&mut *g)
            };
            for ev in events {
                apply_event(ev, &mut state, &mut thinking_buf);
            }

            // Check if bridge finished (stream ended normally)
            if let Some(ref h) = bridge_handle
                && h.is_finished()
            {
                // Drain any leftover events
                let rest: Vec<AppEvent> = {
                    let mut g = buf.lock().unwrap();
                    std::mem::take(&mut *g)
                };
                for ev in rest {
                    apply_event(ev, &mut state, &mut thinking_buf);
                }
                if !thinking_buf.is_empty() {
                    state.push_line(ChatLine {
                        role: "thinking",
                        text: std::mem::take(&mut thinking_buf),
                    });
                }
                state.end_turn();
                stream_buffer = None;
                orch_handle = None;
                bridge_handle = None;
            }
        }

        // ── 3. Draw ──
        terminal.draw(|f| ui::render(f, &state))?;

        // ── 4. Poll keyboard ──
        // Short timeout during streaming so the buffer drain loop is responsive.
        let poll_dur = if state.streaming {
            Duration::from_millis(30)
        } else {
            Duration::from_millis(200)
        };

        if !event::poll(poll_dur)? {
            continue;
        }

        let ev = event::read()?;
        if let Event::Key(key) = ev {
            // ── Ctrl+C via keyboard event ──
            if is_ctrl_c(&key) {
                if state.streaming {
                    if let Some(h) = orch_handle.take() {
                        h.abort();
                    }
                    if let Some(h) = bridge_handle.take() {
                        h.abort();
                    }
                    stream_buffer = None;
                    state.streaming = false;
                    state.push_line(ChatLine {
                        role: "thinking",
                        text: "[cancelled]".into(),
                    });
                } else {
                    break 'outer;
                }
                continue;
            }

            if key.kind != KeyEventKind::Press {
                continue;
            }

            match key.code {
                KeyCode::Enter => {
                    let line = state.input.trim().to_string();
                    if line.is_empty() {
                        continue;
                    }
                    state.input.clear();

                    // ── Slash commands ──
                    if line == "/exit" || line == "/quit" {
                        break 'outer;
                    }
                    if line == "/tools" {
                        let r = orchestrator.tool_registry().await;
                        state.overlay = Some(app::OverlayContent {
                            title: "Available Tools".into(),
                            body: r.list_names().join("\n"),
                        });
                        continue;
                    }
                    if line == "/skills" {
                        let summaries = orchestrator.get_skill_summaries().await;
                        let body = if summaries.is_empty() {
                            "none loaded".into()
                        } else {
                            summaries
                                .iter()
                                .map(|s| format!("- {}: {}", s.name, s.description))
                                .collect::<Vec<_>>()
                                .join("\n")
                        };
                        state.overlay = Some(app::OverlayContent {
                            title: "Available Skills".into(),
                            body,
                        });
                        continue;
                    }

                    // ── Start streaming turn ──
                    state.start_turn();
                    state.push_line(ChatLine {
                        role: "user",
                        text: line.clone(),
                    });
                    thinking_buf.clear();

                    let buffer: EventBuffer = Arc::new(Mutex::new(Vec::new()));
                    let (tx, tokio_rx) = mpsc::channel::<StreamDelta>(256);

                    let orch = orchestrator.clone();
                    let sess = session_id.clone();
                    let oh = tokio::spawn(async move {
                        let _ = orch
                            .process_message_streaming(
                                &line, tx, &sess, None, vec![], vec![], "operate",
                            )
                            .await;
                    });

                    let buf = buffer.clone();
                    let bh = tokio::spawn(async move {
                        let mut rx = tokio_rx;
                        while let Some(delta) = rx.recv().await {
                            buf.lock().unwrap().push(convert(delta));
                        }
                    });

                    orch_handle = Some(oh);
                    bridge_handle = Some(bh);
                    stream_buffer = Some(buffer);
                }

                KeyCode::Char(c) => {
                    if state.overlay.is_some() {
                        state.overlay = None;
                    }
                    state.input.push(c);
                }

                KeyCode::Backspace => {
                    state.input.pop();
                }

                KeyCode::Esc => {
                    state.overlay = None;
                }

                KeyCode::PageUp => {
                    state.scroll_offset = state.scroll_offset.saturating_add(1);
                }

                KeyCode::PageDown => {
                    state.scroll_offset = state.scroll_offset.saturating_sub(1);
                }

                _ => {}
            }
        }
    }

    // ── Cleanup ──
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

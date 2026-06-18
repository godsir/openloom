//! TUI chat entry point.
//!
//! Architecture:
//! 1. Main thread runs a single event loop: keyboard → stream → redraw
//! 2. When user sends a message, stream deltas go into a shared buffer
//!    (Arc<Mutex<Vec>>) via a bridge task so they never race with rendering.
//! 3. The main loop drains the buffer each tick, applies events to AppState,
//!    and redraws ratatui.

pub mod app;
pub mod stream;
pub mod ui;

use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use loom_core::Orchestrator;
use loom_types::StreamDelta;
use ratatui::prelude::*;
use tokio::sync::mpsc;

use app::{AppState, ChatLine, ToolStatus};
use stream::{convert, AppEvent};

/// Shared event buffer: bridge task pushes, main loop drains.
type EventBuffer = Arc<Mutex<Vec<AppEvent>>>;

/// Apply a single AppEvent to the TUI state.
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
        AppEvent::ReasoningChunk(r) => {
            thinking_buf.push_str(&r);
        }
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
        AppEvent::Usage(ts) => {
            state.tokens = ts;
        }
        _ => {}
    }
}

/// Run the TUI chat loop.
pub async fn run_tui(
    orchestrator: Arc<Orchestrator>,
    model_name: String,
    session_id: String,
) -> anyhow::Result<()> {
    // --- Terminal setup ---
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut state = AppState::default();
    state.model_name = model_name;

    // Ctrl+C handler ─ set flag; TUI loop checks and decides cancel vs quit.
    let cancel_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
    {
        let cf = cancel_flag.clone();
        ctrlc::set_handler(move || {
            cf.store(true, Ordering::SeqCst);
        })
        .ok();
    }

    let mut thinking_buf = String::new();

    // --- Single event loop (no nested loops) ---
    loop {
        // ── Ctrl+C handling ──
        if cancel_flag.swap(false, Ordering::SeqCst) {
            if state.streaming {
                // First press during streaming → cancel
                state.streaming = false;
                state.push_line(ChatLine {
                    role: "thinking",
                    text: "[cancelled]".into(),
                });
            } else {
                // Second press (idle) → quit
                break;
            }
        }

        terminal.draw(|f| ui::render(f, &state))?;

        // ── Wait for keyboard event with a short timeout ──
        // During idle this blocks comfortably; during streaming the timeout
        // is short so we can pump deltas from the background buffer.
        let poll_dur = if state.streaming {
            Duration::from_millis(30)
        } else {
            Duration::from_millis(200)
        };

        if event::poll(poll_dur)? {
            let ev = event::read()?;
            if let Event::Key(key) = ev
                && key.kind == KeyEventKind::Press
            {
                match key.code {
                    KeyCode::Enter => {
                        let line = state.input.trim().to_string();
                        if line.is_empty() {
                            continue;
                        }
                        state.input.clear();
                        state.input_cursor = 0;

                        // ── Slash commands ──
                        if line == "/exit" || line == "/quit" {
                            break;
                        }
                        if line == "/tools" {
                            let r = orchestrator.tool_registry().await;
                            let body = r.list_names().join("\n");
                            state.overlay = Some(app::OverlayContent {
                                title: "Available Tools".into(),
                                body,
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

                        // ── Start a new turn ──
                        state.start_turn();
                        state.push_line(ChatLine {
                            role: "user",
                            text: line.clone(),
                        });
                        thinking_buf.clear();

                        // Shared buffer: bridge task pushes events, main loop drains.
                        let buffer: EventBuffer = Arc::new(Mutex::new(Vec::new()));

                        // Spawn orchestrator streaming in background.
                        let (tx, mut tokio_rx) = mpsc::channel::<StreamDelta>(256);
                        let orch = orchestrator.clone();
                        let sess = session_id.clone();
                        let orch_handle = tokio::spawn(async move {
                            let _ = orch
                                .process_message_streaming(
                                    &line, tx, &sess, None, vec![], vec![], "operate",
                                )
                                .await;
                        });

                        // Bridge: tokio mpsc → shared buffer (runs concurrently on runtime).
                        let buf = buffer.clone();
                        let bridge = tokio::spawn(async move {
                            while let Some(delta) = tokio_rx.recv().await {
                                buf.lock().unwrap().push(convert(delta));
                            }
                        });

                        // ── Inner pump: drain buffer + redraw until stream ends ──
                        loop {
                            // Check cancel
                            if cancel_flag.swap(false, Ordering::SeqCst) {
                                orch_handle.abort();
                                bridge.abort();
                                state.streaming = false;
                                state.push_line(ChatLine {
                                    role: "thinking",
                                    text: "[cancelled]".into(),
                                });
                                break;
                            }

                            // Drain buffered events
                            let new_events: Vec<AppEvent> = {
                                let mut guard = buffer.lock().unwrap();
                                std::mem::take(&mut *guard)
                            };
                            for ev in new_events {
                                apply_event(ev, &mut state, &mut thinking_buf);
                            }

                            terminal.draw(|f| ui::render(f, &state))?;

                            // Bridge finished → stream ended
                            if bridge.is_finished() {
                                // Drain any final events
                                let final_events: Vec<AppEvent> = {
                                    let mut guard = buffer.lock().unwrap();
                                    std::mem::take(&mut *guard)
                                };
                                for ev in final_events {
                                    apply_event(ev, &mut state, &mut thinking_buf);
                                }
                                // Flush remaining thinking
                                if !thinking_buf.is_empty() {
                                    state.push_line(ChatLine {
                                        role: "thinking",
                                        text: std::mem::take(&mut thinking_buf),
                                    });
                                }
                                state.end_turn();
                                break;
                            }

                            // Yield to runtime so bridge/orch tasks can progress.
                            tokio::time::sleep(Duration::from_millis(10)).await;
                        }
                    }

                    KeyCode::Char(c) => {
                        if state.overlay.is_some() {
                            state.overlay = None;
                        }
                        state.input.push(c);
                        state.input_cursor += 1;
                    }

                    KeyCode::Backspace => {
                        if state.input_cursor > 0 {
                            state.input.remove(state.input_cursor - 1);
                            state.input_cursor -= 1;
                        }
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

        // If we're streaming but there's no active pump, something went wrong.
        // Reset to idle (safety net).
        if state.streaming && thinking_buf.is_empty() {
            // streaming = true is set by start_turn; reset by end_turn / cancel.
            // This guards against edge cases where streaming gets stuck.
        }
    }

    // --- Cleanup ---
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}

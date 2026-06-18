//! TUI chat entry point.
//!
//! Spawns the orchestrator's process_message_streaming as a background
//! tokio task, pumps StreamDelta → AppState via a channel, and runs
//! the ratatui+crossterm event loop on the main thread.

pub mod app;
pub mod stream;
pub mod ui;

use std::io;
use std::sync::atomic::Ordering;
use std::sync::Arc;

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

/// Run the TUI chat loop.
///
/// `model_name` is displayed in the status bar.
/// `session_id` is the session ID passed to the orchestrator.
/// All setup (orchestrator, skills, memory, MCP) must be done before calling this.
pub async fn run_tui(
    orchestrator: Arc<Orchestrator>,
    model_name: String,
    session_id: String,
) -> anyhow::Result<()> {
    // --- Terminal setup ---
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut state = AppState::default();
    state.model_name = model_name;

    // Ctrl+C handler: set a flag instead of exiting, so the TUI loop
    // can use the first Ctrl+C to cancel streaming.
    let cancel_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let cf = cancel_flag.clone();
    ctrlc::set_handler(move || {
        cf.store(true, Ordering::SeqCst);
    })
    .ok();

    let tick_rate = std::time::Duration::from_millis(50);

    // --- Main event loop ---
    'outer: loop {
        // Check for Ctrl+C
        if cancel_flag.load(Ordering::SeqCst) {
            if state.streaming {
                // First press during streaming: cancel the stream
                state.streaming = false;
                state.push_line(ChatLine {
                    role: "thinking",
                    text: "[cancelled]".into(),
                });
                cancel_flag.store(false, Ordering::SeqCst);
            } else {
                // Second press: quit
                break;
            }
        }

        terminal.draw(|f| ui::render(f, &state))?;

        // Poll for a terminal event with timeout
        if event::poll(tick_rate)? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                    KeyCode::Enter => {
                        let line = state.input.trim().to_string();
                        if line.is_empty() {
                            continue;
                        }
                        state.input.clear();
                        state.input_cursor = 0;

                        if line == "/exit" || line == "/quit" {
                            break 'outer;
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
                                    .map(|s| {
                                        format!("- {}: {}", s.name, s.description)
                                    })
                                    .collect::<Vec<_>>()
                                    .join("\n")
                            };
                            state.overlay = Some(app::OverlayContent {
                                title: "Available Skills".into(),
                                body,
                            });
                            continue;
                        }

                        // Start a new turn
                        state.start_turn();
                        state.push_line(ChatLine {
                            role: "user",
                            text: line.clone(),
                        });

                        // Spawn streaming in background
                        let (tx, mut rx) = mpsc::channel::<StreamDelta>(256);
                        let orch = orchestrator.clone();
                        let sess = session_id.clone();
                        let cancel = cancel_flag.clone();
                        let handle = tokio::spawn(async move {
                            let _ = orch
                                .process_message_streaming(
                                    &line, tx, &sess, None, vec![], vec![], "operate",
                                )
                                .await;
                        });

                        // Pump stream deltas on the main thread while TUI keeps rendering
                        let mut thinking_buf = String::new();
                        loop {
                            // Check cancel
                            if cancel.load(Ordering::SeqCst) {
                                handle.abort();
                                break;
                            }

                            // Non-blocking recv
                            match rx.try_recv() {
                                Ok(delta) => {
                                    let ev = convert(delta);
                                    match ev {
                                        AppEvent::TextChunk(t) => {
                                            if !thinking_buf.is_empty() {
                                                state.push_line(ChatLine {
                                                    role: "thinking",
                                                    text: std::mem::take(&mut thinking_buf),
                                                });
                                            }
                                            state.append_text(&t);
                                        }
                                        AppEvent::ReasoningChunk(r) => {
                                            thinking_buf.push_str(&r);
                                        }
                                        AppEvent::ToolBegin { index, name, .. } => {
                                            state.upsert_tool(
                                                index,
                                                &name,
                                                ToolStatus::Running,
                                            );
                                        }
                                        AppEvent::ToolResult {
                                            tool_name,
                                            success,
                                            ..
                                        } => {
                                            // Find by name and update status
                                            if let Some(t) = state.tools.iter_mut().find(|t| {
                                                t.name == tool_name
                                                    && t.status == ToolStatus::Running
                                            }) {
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
                                        AppEvent::Done => break,
                                        _ => {}
                                    }
                                    // Redraw after each event
                                    terminal.draw(|f| ui::render(f, &state))?;
                                }
                                Err(mpsc::error::TryRecvError::Empty) => {
                                    // No delta available; check if the task finished
                                    if handle.is_finished() {
                                        break;
                                    }
                                    // Yield briefly so the task can progress
                                    tokio::task::yield_now().await;
                                    terminal.draw(|f| ui::render(f, &state))?;
                                }
                                Err(mpsc::error::TryRecvError::Disconnected) => {
                                    break;
                                }
                            }
                        }

                        // Flush remaining thinking
                        if !thinking_buf.is_empty() {
                            state.push_line(ChatLine {
                                role: "thinking",
                                text: std::mem::take(&mut thinking_buf),
                            });
                        }

                        // Drain remaining deltas
                        while let Ok(delta) = rx.try_recv() {
                            if let AppEvent::TextChunk(t) = convert(delta) {
                                state.append_text(&t);
                            }
                        }

                        // End turn
                        state.end_turn();
                        cancel_flag.store(false, Ordering::SeqCst);
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
                        // Dismiss overlay
                        state.overlay = None;
                    }

                    KeyCode::PageUp => {
                        state.scroll_offset = state.scroll_offset.saturating_add(1);
                    }

                    KeyCode::PageDown => {
                        state.scroll_offset = state.scroll_offset.saturating_sub(1);
                    }

                    _ => {}
                },
                _ => {}
            }
        }
    }

    // --- Cleanup ---
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}

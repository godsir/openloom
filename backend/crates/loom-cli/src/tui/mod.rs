//! TUI chat entry point — single-threaded event loop.
//!
//! Flow: poll input → drain stream buffer → update AppState → redraw.
//! Terminal restore: TuiGuard (RAII) + panic hook covers all exit paths.

pub mod app;
pub mod stream;
pub mod ui;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers, MouseEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use loom_core::Orchestrator;
use loom_types::StreamDelta;
use ratatui::prelude::*;
use tokio::sync::mpsc;

use app::{AppState, HistoryItem, OverlayContent, ToolCall, ToolStatus, HELP_TEXT};
use stream::{AppEvent, convert};

type EventBuffer = Arc<Mutex<Vec<AppEvent>>>;

// ── Terminal guard ─────────────────────────────────────────────────

struct TuiGuard;

impl TuiGuard {
    fn enter() -> anyhow::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = std::io::stdout();
        execute!(stdout, EnterAlternateScreen, crossterm::event::EnableMouseCapture)?;
        Ok(Self)
    }
}

impl Drop for TuiGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let mut stdout = std::io::stdout();
        let _ = execute!(stdout, LeaveAlternateScreen, crossterm::event::DisableMouseCapture, crossterm::cursor::Show);
    }
}

// ── Event application ──────────────────────────────────────────────

fn apply_event(ev: AppEvent, state: &mut AppState, thinking_buf: &mut String, tool_index: &mut usize) {
    match ev {
        AppEvent::TextChunk(t) => {
            if !thinking_buf.is_empty() {
                state.flush_thinking(std::mem::take(thinking_buf));
            }
            state.append_assistant_text(&t);
        }
        AppEvent::ReasoningChunk(r) => thinking_buf.push_str(&r),
        AppEvent::ToolBegin { index, name } => {
            if !thinking_buf.is_empty() {
                state.flush_thinking(std::mem::take(thinking_buf));
            }
            *tool_index = index;
            let mut tools = if let Some(inner) = state.tool_group_mut() {
                std::mem::take(inner)
            } else { Vec::new() };
            tools.push(ToolCall { name, args: String::new(), status: ToolStatus::Running, result: None });
            state.set_tool_group(tools);
        }
        AppEvent::ToolArgsChunk { chunk, .. } => {
            if let Some(tools) = state.tool_group_mut() {
                if let Some(tc) = tools.last_mut() { tc.args.push_str(&chunk); }
            }
        }
        AppEvent::ToolResult { tool_name, success } => {
            if let Some(tools) = state.tool_group_mut() {
                if let Some(tc) = tools.iter_mut().rev().find(|t| t.name == tool_name) {
                    tc.status = if success { ToolStatus::Done } else { ToolStatus::Failed };
                }
            }
        }
        AppEvent::Usage(ts) => {
            state.tokens = ts;
            if !thinking_buf.is_empty() {
                state.flush_thinking(std::mem::take(thinking_buf));
            }
        }
        AppEvent::AuxiliaryUsage { .. } => {}
    }
}

// ── Helpers ────────────────────────────────────────────────────────

fn is_ctrl_c(key: &crossterm::event::KeyEvent) -> bool {
    key.kind == KeyEventKind::Press && (
        matches!(key.code, KeyCode::Char('\u{3}')) ||
        (key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c') | KeyCode::Char('C')))
    )
}

fn abort_stream(
    orch: &mut Option<tokio::task::JoinHandle<()>>,
    bridge: &mut Option<tokio::task::JoinHandle<()>>,
    buf: &mut Option<EventBuffer>,
    state: &mut AppState,
) {
    if let Some(h) = orch.take() { h.abort(); }
    if let Some(h) = bridge.take() { h.abort(); }
    *buf = None;
    state.streaming = false;
}

fn spawn_turn(
    orch: &mut Option<tokio::task::JoinHandle<()>>,
    bridge: &mut Option<tokio::task::JoinHandle<()>>,
    buf: &mut Option<EventBuffer>,
    line: &str,
    orchestrator: &Arc<Orchestrator>,
    session_id: &str,
) {
    if let Some(h) = orch.take() { h.abort(); }
    if let Some(h) = bridge.take() { h.abort(); }
    *buf = None;

    let buffer: EventBuffer = Arc::new(Mutex::new(Vec::new()));
    let (tx, tokio_rx) = mpsc::channel::<StreamDelta>(256);

    let line = line.to_string();
    let orch_ref = orchestrator.clone();
    let sess = session_id.to_string();
    let oh = tokio::spawn(async move {
        let _ = orch_ref.process_message_streaming(&line, tx, &sess, None, vec![], vec![], "operate").await;
    });

    let buf_clone = buffer.clone();
    let bh = tokio::spawn(async move {
        let mut rx = tokio_rx;
        while let Some(delta) = rx.recv().await {
            buf_clone.lock().unwrap().push(convert(delta));
        }
    });

    *orch = Some(oh);
    *bridge = Some(bh);
    *buf = Some(buffer);
}

// ── Main entry ────────────────────────────────────────────────────

pub async fn run_tui(
    orchestrator: Arc<Orchestrator>,
    model_name: String,
    session_id: String,
) -> anyhow::Result<()> {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let mut stdout = std::io::stdout();
        let _ = execute!(stdout, LeaveAlternateScreen, crossterm::event::DisableMouseCapture, crossterm::cursor::Show);
        prev(info);
    }));

    let _guard = TuiGuard::enter()?;
    let backend = CrosstermBackend::new(std::io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let mut state = AppState::default();
    state.model_name = model_name;
    let mut thinking_buf = String::new();
    let mut tool_index = 0usize;

    let cancel_flag = Arc::new(AtomicBool::new(false));
    { let f = cancel_flag.clone(); ctrlc::set_handler(move || { f.store(true, Ordering::SeqCst); }).ok(); }

    let mut orch_handle: Option<tokio::task::JoinHandle<()>> = None;
    let mut bridge_handle: Option<tokio::task::JoinHandle<()>> = None;
    let mut stream_buffer: Option<EventBuffer> = None;

    'outer: loop {
        // signal flag
        if cancel_flag.swap(false, Ordering::SeqCst) {
            if state.streaming {
                abort_stream(&mut orch_handle, &mut bridge_handle, &mut stream_buffer, &mut state);
                state.push(HistoryItem::Info { text: "[cancelled]".into() });
                thinking_buf.clear();
            } else { break 'outer; }
        }

        // drain stream buffer
        if let Some(ref buf) = stream_buffer {
            let events: Vec<AppEvent> = { let mut g = buf.lock().unwrap(); std::mem::take(&mut *g) };
            for ev in events { apply_event(ev, &mut state, &mut thinking_buf, &mut tool_index); }

            if let Some(ref h) = bridge_handle && h.is_finished() {
                let rest: Vec<AppEvent> = { let mut g = buf.lock().unwrap(); std::mem::take(&mut *g) };
                for ev in rest { apply_event(ev, &mut state, &mut thinking_buf, &mut tool_index); }
                if !thinking_buf.is_empty() { state.flush_thinking(std::mem::take(&mut thinking_buf)); }
                state.end_turn();
                stream_buffer = None; orch_handle = None; bridge_handle = None;
            }
        }

        // draw
        let vp = state.viewport_rows.get();
        terminal.draw(|f| ui::render(f, &state))?;

        // poll
        if !event::poll(if state.streaming { Duration::from_millis(30) } else { Duration::from_millis(100) })? { continue; }

        let ev = event::read()?;
        match ev {
            Event::Key(key) => {
                if is_ctrl_c(&key) {
                    if state.streaming {
                        abort_stream(&mut orch_handle, &mut bridge_handle, &mut stream_buffer, &mut state);
                        state.push(HistoryItem::Info { text: "[cancelled]".into() });
                    } else { break 'outer; }
                    continue;
                }
                if key.kind != KeyEventKind::Press && key.kind != KeyEventKind::Repeat { continue; }

                match key.code {
                    KeyCode::Enter => {
                        if key.modifiers.contains(KeyModifiers::CONTROL) { state.input_insert('\n'); continue; }
                        if state.streaming { continue; }

                        let line = state.input.trim().to_string();
                        state.input.clear(); state.cursor = 0;
                        if line.is_empty() { continue; }

                        // slash commands
                        match line.as_str() {
                            "/exit" | "/quit" => { abort_stream(&mut orch_handle, &mut bridge_handle, &mut stream_buffer, &mut state); break 'outer; }
                            "/help" | "/?" => { state.overlay = Some(OverlayContent { title: "Help".into(), body: HELP_TEXT.to_string() }); continue; }
                            "/tools" => {
                                let r = orchestrator.tool_registry().await;
                                state.overlay = Some(OverlayContent { title: "Tools".into(), body: r.list_names().join("\n") }); continue;
                            }
                            "/skills" => {
                                let s = orchestrator.get_skill_summaries().await;
                                let body = if s.is_empty() { "none loaded".into() } else { s.iter().map(|x| format!("- {}: {}", x.name, x.description)).collect::<Vec<_>>().join("\n") };
                                state.overlay = Some(OverlayContent { title: "Skills".into(), body }); continue;
                            }
                            _ => {}
                        }

                        // Start streaming
                        state.start_turn();
                        state.push(HistoryItem::User { text: line.clone() });
                        thinking_buf.clear();
                        spawn_turn(&mut orch_handle, &mut bridge_handle, &mut stream_buffer, &line, &orchestrator, &session_id);
                    }
                    KeyCode::Char(c) => { state.overlay = None; state.input_insert(c); }
                    KeyCode::Backspace => state.input_backspace(),
                    KeyCode::Delete    => state.input_delete(),
                    KeyCode::Left      => state.cursor_left(),
                    KeyCode::Right     => state.cursor_right(),
                    KeyCode::Tab       => { state.input_insert(' '); state.input_insert(' '); }
                    KeyCode::Esc => { if state.overlay.is_some() { state.overlay = None; } else { state.input.clear(); state.cursor = 0; } }
                    KeyCode::Up   => { state.scroll_offset = state.scroll_offset.saturating_add(1); state.scroll_following = state.scroll_offset == 0; }
                    KeyCode::Down => { state.scroll_offset = state.scroll_offset.saturating_sub(1); state.scroll_following = state.scroll_offset == 0; }
                    KeyCode::PageUp   => { let n = vp.max(5).saturating_sub(2) as u16; state.scroll_offset = state.scroll_offset.saturating_add(n); state.scroll_following = false; }
                    KeyCode::PageDown => { let n = vp.max(5).saturating_sub(2) as u16; state.scroll_offset = state.scroll_offset.saturating_sub(n); state.scroll_following = state.scroll_offset == 0; }
                    KeyCode::Home => { if state.overlay.is_some() { state.cursor_home(); } else { state.scroll_offset = u16::MAX; state.scroll_following = false; } }
                    KeyCode::End  => { if state.overlay.is_some() { state.cursor_end(); } else { state.scroll_offset = 0; state.scroll_following = true; } }
                    _ => {}
                }
            }
            Event::Mouse(m) => match m.kind {
                MouseEventKind::ScrollUp   => { state.scroll_offset = state.scroll_offset.saturating_add(3); state.scroll_following = false; }
                MouseEventKind::ScrollDown => { state.scroll_offset = state.scroll_offset.saturating_sub(3); state.scroll_following = state.scroll_offset == 0; }
                _ => {}
            },
            Event::Resize(_, _) => { if state.scroll_offset > 5000 { state.scroll_offset = 5000; } }
            Event::Paste(data) => { for c in data.chars() { if c != '\n' && c != '\r' { state.input_insert(c); } } }
            _ => {}
        }
    }

    if let Some(h) = orch_handle { h.abort(); }
    if let Some(h) = bridge_handle { h.abort(); }
    Ok(())
}

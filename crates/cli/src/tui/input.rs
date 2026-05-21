use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

use crate::tui::app::{App, AppState, CtrlCAction};
use crate::tui::keymap::{Action, KeyContext};

pub fn handle_key(app: &mut App, key: KeyEvent) -> bool {
    let context = match app.state {
        AppState::Streaming => KeyContext::Streaming,
        AppState::Overlay => KeyContext::Overlay,
        _ => KeyContext::Input,
    };

    let action = app.keymap.resolve(&key, context);

    match action {
        // ── Ctrl+C: two-press pattern ─────────────────────
        Action::Quit | Action::CancelStream => {
            match app.handle_ctrl_c() {
                CtrlCAction::CancelStream => cancel_stream(app),
                CtrlCAction::Quit => {
                    app.should_exit = true;
                    return true;
                }
                CtrlCAction::ShowHint => {
                    app.add_assistant_message("Press Ctrl+C again to exit.".into());
                }
            }
            false
        }

        // ── send message ──────────────────────────────────
        Action::Send => {
            let text = app.current_line().trim().to_string();
            if text.is_empty() {
                return false;
            }
            app.last_ctrl_c = None; // reset exit timer on activity

            if text.starts_with('/') && !text.starts_with("//") {
                if app.history.last() != Some(&text) {
                    app.history.push(text.clone());
                }
                app.history_idx = None;
                app.input = crate::tui::app::build_textarea();
                app.messages.push(crate::tui::app::Message::user(text.clone()));
                app.pending_command = Some(text);
                app.state = AppState::Waiting;
                app.viewport.jump_to_bottom();
                return false;
            }
            if app.history.last() != Some(&text) {
                app.history.push(text.clone());
            }
            app.history_idx = None;
            app.input = crate::tui::app::build_textarea();
            app.messages.push(crate::tui::app::Message::user(text));
            app.state = AppState::Waiting;
            app.viewport.jump_to_bottom();
            false
        }

        // ── history (or palette nav when visible) ────────
        Action::HistoryUp => {
            if palette_visible(app) {
                palette_cycle(app, -1);
            } else {
                navigate_history(app, Direction::Prev);
            }
            false
        }
        Action::HistoryDown => {
            if palette_visible(app) {
                palette_cycle(app, 1);
            } else {
                navigate_history(app, Direction::Next);
            }
            false
        }

        // ── scroll ────────────────────────────────────────
        Action::ScrollUp => {
            app.viewport.scroll_up(10);
            false
        }
        Action::ScrollDown => {
            app.viewport.scroll_down(10);
            false
        }

        Action::Redraw => false,

        // ── newline ───────────────────────────────────────
        Action::Newline => {
            use crossterm::event::{KeyEventKind, KeyEventState};
            app.input.input(Event::Key(KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::NONE,
                kind: KeyEventKind::Press,
                state: KeyEventState::NONE,
            }));
            false
        }

        // ── typing ────────────────────────────────────────
        Action::Noop => {
            app.input.input(Event::Key(key));
            // Reset command palette selection when typing
            if key.code != KeyCode::Up && key.code != KeyCode::Down {
                app.command_palette_selected = 0;
            }
            false
        }

        Action::ExternalEditor => {
            launch_editor(app);
            false
        }

        Action::HistorySearch => {
            app.input.input(Event::Key(key));
            false
        }

        Action::Autocomplete => {
            handle_autocomplete(app);
            false
        }

        Action::DismissOverlay
        | Action::ConfirmOverlay
        | Action::NavigateLeft
        | Action::NavigateRight => false,
    }
}

// ── helpers ──────────────────────────────────────────────────

fn palette_visible(app: &App) -> bool {
    let first = app.input.lines().first().map(|s| s.as_str()).unwrap_or("");
    first.starts_with('/') && !first.starts_with("//")
}

fn palette_cycle(app: &mut App, delta: i32) {
    let current = app.current_line();
    let prefix = &current[1..];
    let commands = SLASH_COMMANDS;
    let matches: Vec<&str> = commands
        .iter()
        .filter(|cmd| cmd[1..].starts_with(prefix))
        .copied()
        .collect();

    if matches.is_empty() {
        return;
    }

    let len = matches.len() as i32;
    let new_idx = ((app.command_palette_selected as i32 + delta) % len + len) % len;
    app.command_palette_selected = new_idx as usize;

    // Live-preview: fill input with highlighted command
    let selected = matches[app.command_palette_selected];
    app.input = crate::tui::app::build_textarea();
    app.input.insert_str(format!("{} ", selected));
}

fn cancel_stream(app: &mut App) {
    app.stream.cancel();
    if let Some(last) = app.messages.last()
        && last.role == "assistant"
    {
        app.messages.pop();
    }
    if !app.stream.buffer.is_empty() {
        let partial = std::mem::take(&mut app.stream.buffer);
        app.add_assistant_message(format!("{} [cancelled]", partial));
    } else {
        app.add_assistant_message("[cancelled]".into());
    }
    app.stream.buffer.clear();
    app.state = AppState::Idle;
}

fn launch_editor(app: &mut App) {
    let current_text = app.input.lines().join("\n");
    let temp_dir = std::env::temp_dir();
    let temp_file = temp_dir.join("openloom_edit.txt");
    let _ = std::fs::write(&temp_file, &current_text);

    let _ = crossterm::terminal::disable_raw_mode();

    let editor = std::env::var("EDITOR")
        .or_else(|_| std::env::var("VISUAL"))
        .unwrap_or_else(|_| {
            if cfg!(target_os = "windows") { "notepad".into() } else { "vi".into() }
        });

    let _ = std::process::Command::new(&editor).arg(&temp_file).status();

    if let Ok(content) = std::fs::read_to_string(&temp_file) {
        let trimmed = content.trim();
        if !trimmed.is_empty() && trimmed != current_text {
            app.input = crate::tui::app::build_textarea();
            app.input.insert_str(trimmed);
        }
    }

    let _ = std::fs::remove_file(&temp_file);
    let _ = crossterm::terminal::enable_raw_mode();
}

enum Direction { Prev, Next }

fn navigate_history(app: &mut App, dir: Direction) {
    if app.history.is_empty() {
        return;
    }
    let idx = match dir {
        Direction::Prev => match app.history_idx {
            None => Some(app.history.len().saturating_sub(1)),
            Some(0) => Some(0),
            Some(i) => Some(i.saturating_sub(1)),
        },
        Direction::Next => match app.history_idx {
            None => None,
            Some(i) if i + 1 >= app.history.len() => None,
            Some(i) => Some(i + 1),
        },
    };
    app.history_idx = idx;
    let text = match idx {
        Some(i) => app.history.get(i).cloned().unwrap_or_default(),
        None => String::new(),
    };
    app.input = crate::tui::app::build_textarea();
    app.input.insert_str(&text);
}

const SLASH_COMMANDS: [&str; 15] = [
    "/help", "/model", "/cost", "/clear",
    "/theme dark", "/theme light",
    "/session new", "/session list",
    "/memory persona", "/memory events", "/memory cognitions", "/memory search",
    "/skills list", "/config get", "/config set",
];

fn handle_autocomplete(app: &mut App) {
    let current = app.current_line();
    if !current.starts_with('/') || current.starts_with("//") {
        return;
    }
    let prefix = &current[1..];
    let matches: Vec<&str> = SLASH_COMMANDS
        .iter()
        .filter(|cmd| cmd[1..].starts_with(prefix))
        .copied()
        .collect();
    if matches.is_empty() {
        return;
    }
    let idx = app.command_palette_selected.min(matches.len().saturating_sub(1));
    let selected = matches[idx];
    app.input = crate::tui::app::build_textarea();
    app.input.insert_str(format!("{} ", selected));
    app.command_palette_selected = (idx + 1) % matches.len();
}

/// Forward mouse events to the textarea for text selection.
pub fn handle_mouse(app: &mut App, mouse: crossterm::event::MouseEvent) {
    app.input.input(Event::Mouse(mouse));
}

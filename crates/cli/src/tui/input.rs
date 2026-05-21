use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

use crate::tui::app::{App, AppState};
use crate::tui::keymap::{Action, KeyContext};

pub fn handle_key(app: &mut App, key: KeyEvent) -> bool {
    let context = match app.state {
        AppState::Streaming => KeyContext::Streaming,
        AppState::Overlay => KeyContext::Overlay,
        _ => KeyContext::Input,
    };

    let action = app.keymap.resolve(&key, context);

    match action {
        Action::Quit => {
            app.should_exit = true;
            true
        }
        Action::CancelStream => {
            app.stream.cancel();
            // Always remove the streaming placeholder (may have partial content from poll_stream_tokens)
            if let Some(last) = app.messages.last()
                && last.role == "assistant"
            {
                app.messages.pop();
            }
            // Add a single clean cancelled message
            if !app.stream.buffer.is_empty() {
                let partial = std::mem::take(&mut app.stream.buffer);
                app.add_assistant_message(format!("{} [cancelled]", partial));
            } else {
                app.add_assistant_message("[cancelled]".into());
            }
            app.stream.buffer.clear();
            app.state = AppState::Idle;
            false
        }
        Action::Send => {
            let text = app.current_line().trim().to_string();
            if text.is_empty() {
                return false;
            }
            // Slash command interception (not // which is literal)
            if text.starts_with('/') && !text.starts_with("//") {
                if app.history.last() != Some(&text) {
                    app.history.push(text.clone());
                }
                app.history_idx = None;
                app.input = crate::tui::app::build_textarea();
                app.messages
                    .push(crate::tui::app::Message::user(text.clone()));
                app.pending_command = Some(text);
                app.state = AppState::Waiting;
                app.viewport.jump_to_bottom();
                return false;
            }
            // Regular message
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
        Action::HistoryUp => {
            navigate_history(app, Direction::Prev);
            false
        }
        Action::HistoryDown => {
            navigate_history(app, Direction::Next);
            false
        }
        Action::ScrollUp => {
            app.viewport.scroll_up(10);
            false
        }
        Action::ScrollDown => {
            app.viewport.scroll_down(10);
            false
        }
        Action::Redraw => false,
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
        Action::Noop => {
            // Delegate to textarea for typing
            app.input.input(Event::Key(key));
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
        | Action::NavigateRight => {
            false // Overlay handles these itself
        }
    }
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
            if cfg!(target_os = "windows") {
                "notepad".into()
            } else {
                "vi".into()
            }
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

enum Direction {
    Prev,
    Next,
}

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

fn handle_autocomplete(app: &mut App) {
    let current = app.current_line();
    if !current.starts_with('/') || current.starts_with("//") {
        return;
    }

    let prefix = &current[1..];

    let commands = [
        "/help", "/model", "/cost", "/clear",
        "/theme dark", "/theme light",
        "/session new", "/session list",
        "/memory persona", "/memory events", "/memory cognitions", "/memory search",
        "/skills list", "/config get", "/config set",
    ];

    let matches: Vec<&str> = commands
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


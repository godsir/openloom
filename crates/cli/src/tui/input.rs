use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

use crate::tui::app::{App, AppState};

pub fn handle_key(app: &mut App, key: KeyEvent) -> bool {
    // Ctrl+C: quit
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        app.should_exit = true;
        return true;
    }

    // Ctrl+L: terminal clear (redraw)
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('l') {
        return false;
    }

    match key.code {
        KeyCode::Enter => {
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
                app.messages.push(crate::tui::app::Message::user(text.clone()));
                app.pending_command = Some(text);
                app.state = AppState::Waiting;
                app.auto_scroll = true;
                return false;
            }

            if app.history.last() != Some(&text) {
                app.history.push(text.clone());
            }
            app.history_idx = None;
            app.input = crate::tui::app::build_textarea();
            app.messages.push(crate::tui::app::Message::user(text));
            app.state = AppState::Waiting;
            app.auto_scroll = true;
            false
        }
        KeyCode::Up => {
            navigate_history(app, Direction::Prev);
            false
        }
        KeyCode::Down => {
            navigate_history(app, Direction::Next);
            false
        }
        KeyCode::PageUp => {
            app.auto_scroll = false;
            app.scroll = app.scroll.saturating_sub(10);
            false
        }
        KeyCode::PageDown => {
            app.scroll = app.scroll.saturating_add(10);
            false
        }
        _ => {
            app.input.input(Event::Key(key));
            false
        }
    }
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

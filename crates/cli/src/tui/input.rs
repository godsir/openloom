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
            // Remove the empty assistant placeholder if present
            if let Some(last) = app.messages.last()
                && last.role == "assistant"
                && last.content.is_empty()
            {
                app.messages.pop();
            }
            // If we received partial content, mark it as cancelled
            if !app.stream.buffer.is_empty() {
                let partial = std::mem::take(&mut app.stream.buffer);
                app.add_assistant_message(format!("{} [cancelled]", partial));
            } else if app.messages.last().map(|m| m.role.as_str()) != Some("assistant") {
                // No assistant message at all — add a cancelled note
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
                app.auto_scroll = true;
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
            app.auto_scroll = true;
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
            app.auto_scroll = false;
            app.scroll = app.scroll.saturating_sub(10);
            false
        }
        Action::ScrollDown => {
            app.scroll = app.scroll.saturating_add(10);
            false
        }
        Action::Redraw => false,
        Action::Newline => {
            app.input.input(Event::Key(KeyEvent::new(
                KeyCode::Enter,
                KeyModifiers::NONE,
            )));
            false
        }
        Action::Noop => {
            // Delegate to textarea for typing
            app.input.input(Event::Key(key));
            false
        }
        // Unused in Milestone B, handle gracefully
        Action::HistorySearch | Action::ExternalEditor => {
            app.input.input(Event::Key(key));
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

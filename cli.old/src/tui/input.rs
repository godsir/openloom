use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

use crate::tui::app::{App, AppState, CtrlCAction};
use crate::tui::keymap::{Action, KeyContext};
use crate::tui::render::palette_matches_dynamic;

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
            // If in history search, select the highlighted match
            if app.history_search_active {
                let query = app.current_line().to_lowercase();
                let matches: Vec<&String> = app
                    .history
                    .iter()
                    .rev()
                    .filter(|h| query.is_empty() || h.to_lowercase().contains(&query))
                    .collect();
                let idx = app
                    .command_palette_selected
                    .min(matches.len().saturating_sub(1));
                if let Some(selected) = matches.get(idx) {
                    app.input = crate::tui::app::build_textarea();
                    app.input.insert_str(selected);
                }
                app.history_search_active = false;
                app.command_palette_selected = 0;
                return false;
            }

            // If palette is visible, Enter selects the highlighted command
            if palette_visible(app) {
                select_palette_item(app);
                return false;
            }

            let text = app.current_line().trim().to_string();
            if text.is_empty() {
                return false;
            }
            app.last_ctrl_c = None;

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
                app.command_palette_selected = 0;
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
            app.command_palette_selected = 0;
            false
        }

        // ── history (or palette nav when visible) ────────
        Action::HistoryUp => {
            if app.history_search_active {
                history_search_cycle(app, -1);
            } else if palette_visible(app) {
                palette_cycle(app, -1);
            } else {
                navigate_history(app, Direction::Prev);
            }
            false
        }
        Action::HistoryDown => {
            if app.history_search_active {
                history_search_cycle(app, 1);
            } else if palette_visible(app) {
                palette_cycle(app, 1);
            } else {
                navigate_history(app, Direction::Next);
            }
            false
        }

        // ── scroll ────────────────────────────────────────
        Action::ScrollUp => {
            app.viewport.scroll_up(25);
            false
        }
        Action::ScrollDown => {
            app.viewport.scroll_down(25);
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
            // Esc dismisses history search or palette
            if key.code == KeyCode::Esc {
                if app.history_search_active {
                    app.history_search_active = false;
                    app.command_palette_selected = 0;
                    app.input = crate::tui::app::build_textarea();
                    return false;
                }
                if palette_visible(app) {
                    app.command_palette_selected = 0;
                    app.input = crate::tui::app::build_textarea();
                    return false;
                }
            }
            app.input.input(Event::Key(key));
            // Reset palette selection when typing changes input
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
            app.history_search_active = !app.history_search_active;
            if app.history_search_active {
                app.input = crate::tui::app::build_textarea();
                app.command_palette_selected = 0;
            }
            false
        }

        Action::Autocomplete => {
            if app.history_search_active {
                // Tab in history search cycles through matches
                history_search_cycle(app, 1);
            } else if palette_visible(app) {
                palette_cycle(app, 1);
                fill_from_palette(app);
            } else {
                // Try file path completion
                try_path_completion(app);
            }
            false
        }

        Action::ToggleThinking => {
            // Toggle only the last turn's thinking/tool messages (after last user message)
            let last_user_idx = app
                .messages
                .iter()
                .rposition(|m| m.role == "user")
                .unwrap_or(0);
            for msg in app.messages[last_user_idx..].iter_mut() {
                if msg.role == "thinking" || msg.role == "tool_call" || msg.role == "tool_result" {
                    msg.collapsed = !msg.collapsed;
                }
            }
            false
        }

        Action::CycleMode => {
            let old_label = app.mode.config().status_label;
            app.mode = app.mode.next();
            let cfg = app.mode.config();
            app.messages.push(crate::tui::app::Message {
                role: "mode".into(),
                content: format!(
                    "Switched from {} to {} mode ({})",
                    old_label,
                    cfg.status_label,
                    app.mode.description()
                ),
                collapsed: false,
                elapsed_ms: None,
            });
            app.viewport.content_added();
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
    !palette_matches_dynamic(first, &app.external_commands).is_empty()
}

fn palette_cycle(app: &mut App, delta: i32) {
    let current = app
        .input
        .lines()
        .first()
        .map(|s| s.to_string())
        .unwrap_or_default();
    let matches = palette_matches_dynamic(&current, &app.external_commands);
    if matches.is_empty() {
        return;
    }

    let len = matches.len() as i32;
    let new_idx = ((app.command_palette_selected as i32 + delta) % len + len) % len;
    app.command_palette_selected = new_idx as usize;
}

fn history_search_cycle(app: &mut App, delta: i32) {
    let query = app.current_line().to_lowercase();
    let count = app
        .history
        .iter()
        .filter(|h| query.is_empty() || h.to_lowercase().contains(&query))
        .count();
    if count == 0 {
        return;
    }
    let len = count as i32;
    let new_idx = ((app.command_palette_selected as i32 + delta) % len + len) % len;
    app.command_palette_selected = new_idx as usize;
}

fn select_palette_item(app: &mut App) {
    let current = app
        .input
        .lines()
        .first()
        .map(|s| s.to_string())
        .unwrap_or_default();
    let matches = palette_matches_dynamic(&current, &app.external_commands);
    if matches.is_empty() {
        return;
    }

    let idx = app
        .command_palette_selected
        .min(matches.len().saturating_sub(1));
    let selected_cmd = &matches[idx].0;

    // Fill input with the selected command
    app.input = crate::tui::app::build_textarea();
    app.input.insert_str(selected_cmd);
    app.command_palette_selected = 0;
}

fn fill_from_palette(app: &mut App) {
    let current = app
        .input
        .lines()
        .first()
        .map(|s| s.to_string())
        .unwrap_or_default();
    let matches = palette_matches_dynamic(&current, &app.external_commands);
    if matches.is_empty() {
        return;
    }

    let idx = app
        .command_palette_selected
        .min(matches.len().saturating_sub(1));
    let selected_cmd = &matches[idx].0;

    app.input = crate::tui::app::build_textarea();
    app.input.insert_str(selected_cmd);
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

fn try_path_completion(app: &mut App) {
    let text = app.current_line();
    let last_word = text.split_whitespace().next_back().unwrap_or("");
    if last_word.is_empty() {
        return;
    }

    let (dir, prefix) = if let Some(sep) = last_word.rfind(['/', '\\']) {
        let dir_part = &last_word[..=sep];
        let file_part = &last_word[sep + 1..];
        (std::path::PathBuf::from(dir_part), file_part.to_string())
    } else {
        (std::path::PathBuf::from("."), last_word.to_string())
    };

    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    let mut matches: Vec<String> = entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            if name.to_lowercase().starts_with(&prefix.to_lowercase()) {
                let full = if dir.as_os_str() == "." {
                    name.clone()
                } else {
                    format!("{}{}", dir.display(), name)
                };
                if e.path().is_dir() {
                    Some(format!("{}/", full))
                } else {
                    Some(full)
                }
            } else {
                None
            }
        })
        .collect();

    matches.sort();

    if matches.len() == 1 {
        let before = &text[..text.len() - last_word.len()];
        let completed = format!("{}{}", before, matches[0]);
        app.input = crate::tui::app::build_textarea();
        app.input.insert_str(&completed);
    }
}

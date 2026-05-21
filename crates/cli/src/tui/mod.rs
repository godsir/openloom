pub mod app;
pub mod commands;
pub mod input;
pub mod keymap;
pub mod render;
pub mod status;
pub mod streaming;
pub mod theme;

pub mod overlays;

use std::sync::Arc;
use std::time::Duration;

use crossterm::event;
use openloom_engine::Engine;

use crate::tui::app::{App, AppState};

pub async fn run(engine: Arc<Engine>) -> anyhow::Result<()> {
    let mut terminal = ratatui::init();
    crossterm::execute!(std::io::stdout(), crossterm::event::EnableMouseCapture)?;

    let session_id = engine.create_session().await?.id;
    let cwd = std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "?".into());
    let model_name = engine.model_display_name();
    let git_branch = detect_git_branch();
    let context_size = engine.model_context_size().await;

    let mut app = App::new(
        engine,
        session_id,
        cwd,
        model_name,
        git_branch,
        context_size,
    );

    let res = app_run(&mut terminal, &mut app).await;

    let _ = app.engine.shutdown().await;
    crossterm::execute!(std::io::stdout(), crossterm::event::DisableMouseCapture)?;
    ratatui::restore();
    res
}

async fn app_run(
    terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    app: &mut App,
) -> anyhow::Result<()> {
    loop {
        if app.should_exit {
            break;
        }

        app.poll_engine_events();
        app.poll_stream_tokens();
        app.frame_count = app.frame_count.wrapping_add(1);

        // Process pending slash command
        if let Some(cmd_text) = app.pending_command.take() {
            if let Some(cmd) = crate::tui::commands::parse_slash_command(&cmd_text) {
                let response = crate::tui::commands::execute_command(app, cmd).await;
                app.add_assistant_message(response);
            } else {
                app.add_assistant_message(format!(
                    "Unknown command: {}. Type /help for available commands.",
                    cmd_text
                ));
            }
            app.state = AppState::Idle;
        }

        // Process pending user message via streaming
        if app.state == AppState::Waiting {
            let pending_content = app
                .messages
                .last()
                .filter(|m| m.role == "user")
                .map(|m| m.content.clone());

            if let Some(content) = pending_content {
                app.start_streaming(content);
                app.state = AppState::Streaming;
            } else {
                app.state = AppState::Idle;
            }
        }

        // Check if streaming has completed (rx closed, task finished)
        if app.state == AppState::Streaming && !app.stream.is_active() {
            // Estimate token usage from prompt + response lengths (chars / 4 ≈ tokens)
            if let Some(user_msg) = app.messages.iter().rev().find(|m| m.role == "user") {
                app.total_prompt_tokens += user_msg.content.len() / 4;
            }
            let response_len = app.stream.buffer.len();
            app.total_completion_tokens += response_len / 4;
            app.status.turn_tokens = response_len / 4;
            // Rough cost estimate: $3/M input, $15/M output (Claude Sonnet pricing)
            app.total_cost += (app.total_prompt_tokens as f64) * 3.0 / 1_000_000.0
                + (app.total_completion_tokens as f64) * 15.0 / 1_000_000.0;

            if app.stream.buffer.is_empty()
                && let Some(last) = app.messages.last()
                && last.content.is_empty()
            {
                app.messages.pop();
                app.add_assistant_message("[no response]".into());
            }
            app.stream.buffer.clear();
            app.state = AppState::Idle;
        }

        terminal.draw(|f| {
            render::draw(f, app);
            if let Some(ref overlay) = app.overlay {
                let area = centered_rect(60, 70, f.area());
                overlay.draw(f, area);
            }
        })?;

        let poll_timeout = match app.state {
            AppState::Waiting | AppState::Streaming => Duration::from_millis(50),
            _ => Duration::from_millis(200),
        };

        if !event::poll(poll_timeout)? {
            continue;
        }

        match event::read()? {
            event::Event::Key(key) => {
                if key.kind == event::KeyEventKind::Release {
                    continue;
                }
                // Active overlay intercepts keys
                if let Some(ref mut overlay) = app.overlay {
                    match overlay.handle_key(key.code) {
                        crate::tui::overlays::OverlayResult::Dismiss => {
                            app.overlay = None;
                            app.state = AppState::Idle;
                        }
                        crate::tui::overlays::OverlayResult::Consumed => {}
                    }
                    continue;
                }
                input::handle_key(app, key);
            }
            event::Event::Mouse(mouse) => match mouse.kind {
                event::MouseEventKind::ScrollUp => {
                    app.auto_scroll = false;
                    app.scroll = app.scroll.saturating_sub(3);
                }
                event::MouseEventKind::ScrollDown => {
                    app.scroll = app.scroll.saturating_add(3);
                }
                _ => {}
            },
            event::Event::Resize(_, _) => {}
            _ => {}
        }
    }

    Ok(())
}

fn detect_git_branch() -> String {
    std::process::Command::new("git")
        .args(["branch", "--show-current"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
            } else {
                None
            }
        })
        .unwrap_or_default()
}

fn centered_rect(percent_x: u16, percent_y: u16, r: ratatui::layout::Rect) -> ratatui::layout::Rect {
    let popup_layout = ratatui::layout::Layout::default()
        .direction(ratatui::layout::Direction::Vertical)
        .constraints([
            ratatui::layout::Constraint::Percentage((100 - percent_y) / 2),
            ratatui::layout::Constraint::Percentage(percent_y),
            ratatui::layout::Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    ratatui::layout::Layout::default()
        .direction(ratatui::layout::Direction::Horizontal)
        .constraints([
            ratatui::layout::Constraint::Percentage((100 - percent_x) / 2),
            ratatui::layout::Constraint::Percentage(percent_x),
            ratatui::layout::Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

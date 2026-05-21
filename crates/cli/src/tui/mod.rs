pub mod app;
pub mod commands;
pub mod input;
pub mod keymap;
pub mod render;
pub mod status;
pub mod streaming;
pub mod theme;

mod overlays;

use std::sync::Arc;
use std::time::Duration;

use crossterm::event;
use openloom_engine::Engine;
use openloom_models::ChatMessage;

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
        app.frame_count = app.frame_count.wrapping_add(1);

        // Process pending user message (Waiting state means Enter was pressed)
        if app.state == AppState::Waiting {
            let pending_content = app
                .messages
                .last()
                .filter(|m| m.role == "user")
                .map(|m| m.content.clone());

            if let Some(content) = pending_content {
                let sid = app.session_id.clone();
                let engine = app.engine.clone();

                let msg = ChatMessage {
                    role: "user".into(),
                    content,
                    timestamp: chrono::Utc::now(),
                };
                match engine.handle_message(msg, &sid).await {
                    Ok(resp) => {
                        app.add_assistant_message(resp.response);
                        app.total_prompt_tokens += resp.token_usage.prompt_tokens;
                        app.total_completion_tokens += resp.token_usage.completion_tokens;
                        app.status.turn_tokens = resp.token_usage.completion_tokens;
                    }
                    Err(e) => {
                        app.add_assistant_message(format!("Error: {}", e));
                    }
                }
                app.state = AppState::Idle;
            } else {
                app.state = AppState::Idle;
            }
        }

        terminal.draw(|f| render::draw(f, app))?;

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

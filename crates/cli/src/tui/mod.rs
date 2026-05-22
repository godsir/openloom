pub mod app;
pub mod commands;
pub mod input;
pub mod keymap;
pub mod render;
pub mod status;
pub mod streaming;
pub mod theme;

pub mod overlays;

use std::io::stdout;
use std::sync::Arc;
use std::time::Duration;

use crossterm::event;
use crossterm::terminal::{self, ClearType};
use crossterm::{cursor, execute};
use openloom_engine::Engine;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::buffer::Buffer;
use ratatui::Viewport;

use crate::tui::app::{App, AppState};

const INLINE_HEIGHT: u16 = 6;

pub async fn run(engine: Arc<Engine>, resume_session: Option<String>) -> anyhow::Result<()> {
    // Print welcome banner BEFORE raw mode so it renders normally in terminal scrollback
    print_welcome(&engine);

    terminal::enable_raw_mode()?;
    execute!(stdout(), cursor::Hide,)?;

    let backend = CrosstermBackend::new(stdout());
    let mut terminal = Terminal::with_options(
        backend,
        ratatui::TerminalOptions {
            viewport: Viewport::Inline(INLINE_HEIGHT),
        },
    )?;

    let session_id = if let Some(sid) = resume_session {
        sid
    } else {
        engine.create_session().await?.id
    };

    let cwd = std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "?".into());
    let model_name = engine.model_display_name();
    let git_branch = detect_git_branch();
    let context_size = engine.model_context_size().await;

    let mut app = App::new(
        engine,
        session_id.clone(),
        cwd,
        model_name,
        git_branch,
        context_size,
    );

    // Populate external skill commands for palette
    let all_skills = app.engine.list_skills();
    for skill in &all_skills {
        if skill.name.contains(':') {
            let short = skill.name.split(':').next_back().unwrap_or(&skill.name);
            app.external_commands.push((
                format!("/{}", short),
                skill.description.clone(),
            ));
        }
    }

    // Load previous messages if resuming a session
    if let Ok(history) = app.engine.get_working_memory(&session_id)
        && !history.is_empty()
    {
        for msg in &history {
            app.messages.push(crate::tui::app::Message {
                role: msg.role.clone(),
                content: msg.content.clone(),
                collapsed: msg.role == "thinking"
                    || msg.role == "tool_call"
                    || msg.role == "tool_result",
                elapsed_ms: None,
            });
        }
        app.viewport.jump_to_bottom();
    }

    let res = app_run(&mut terminal, &mut app).await;

    let _ = app.engine.shutdown().await;
    execute!(
        stdout(),
        cursor::Show,
        terminal::Clear(ClearType::CurrentLine),
    )?;
    terminal::disable_raw_mode()?;
    // Print a final newline so the shell prompt starts on a clean line
    println!();
    res
}

fn print_welcome(engine: &Engine) {
    const AR: u8 = 110;
    const AG: u8 = 160;
    const AB: u8 = 255;

    let logo_lines = [
        r"            __                  ",
        r"  ___  ____/ /__  ___  ___  ___ ",
        r" / _ \/ __/ / _ \/ _ \/ _ \/ _ \",
        r"/ .__/ /_/_/\___/\___/\___/ .__/",
        r"\_/  \__/  openLoom      /_/    ",
    ];

    println!();
    for line in &logo_lines {
        println!("  \x1b[38;2;{};{};{}m{}\x1b[0m", AR, AG, AB, line);
    }
    println!();
    println!("  \x1b[90mLocal-first AI assistant with cognitive memory.\x1b[0m");
    println!();
    println!(
        "  \x1b[1mEnter\x1b[0m\x1b[90m send  \x1b[0m\x1b[1mShift+Enter\x1b[0m\x1b[90m newline  \x1b[0m\x1b[1mCtrl+C \u{00d7}2\x1b[0m\x1b[90m exit\x1b[0m"
    );
    println!(
        "  \x1b[1mCtrl+G\x1b[0m\x1b[90m editor  \x1b[0m\x1b[1m\u{2191}/\u{2193}\x1b[0m\x1b[90m history  \x1b[0m\x1b[1mTab\x1b[0m\x1b[90m autocomplete  \x1b[0m\x1b[38;2;{};{};{}m\x1b[1m/help\x1b[0m\x1b[90m commands\x1b[0m",
        AR, AG, AB,
    );
    let data_dir = engine.data_dir().display();
    println!(
        "  \x1b[90mData: {}\x1b[0m",
        data_dir,
    );
    println!(
        "\x1b[90m{}\x1b[0m",
        terminal::size().map(|(w, _)| "\u{2500}".repeat(w as usize)).unwrap_or_else(|_| "\u{2500}".repeat(80))
    );
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

        // Check for permission requests from engine
        if app.overlay.is_none()
            && let Some(ref mut rx) = app.perm_rx
            && let Ok((req, resp_tx)) = rx.try_recv()
        {
            app.pending_perm_response = Some(resp_tx);
            app.overlay = Some(Box::new(
                crate::tui::overlays::approval::ApprovalOverlay::new(
                    format!("Allow {}?", req.tool_name),
                    format!("{}\n\nRisk: {}", req.description, req.risk_level),
                )
            ));
            app.state = AppState::Overlay;
        }

        // Process pending slash command
        if let Some(cmd_text) = app.pending_command.take() {
            if let Some(cmd) = crate::tui::commands::parse_slash_command(&cmd_text) {
                let response = crate::tui::commands::execute_command(app, cmd).await;
                app.add_assistant_message(response);
            } else {
                // Try matching as a skill invocation before giving up
                let skill_name = cmd_text.trim_start_matches('/').trim();
                let mut found = false;
                let mut activated: Option<(String, String)> = None;

                if let Some(context) = app.engine.find_skill_by_name(skill_name) {
                    activated = Some((skill_name.to_string(), context));
                } else if let Some(context) = app.engine.find_skill_by_name(&format!("project:{}", skill_name)) {
                    activated = Some((format!("project:{}", skill_name), context));
                } else {
                    let all_skills = app.engine.external_skill_names();
                    for (full_name, _) in &all_skills {
                        let suffix = full_name.split(':').next_back().unwrap_or("");
                        if suffix == skill_name
                            && let Some(context) = app.engine.find_skill_by_name(full_name)
                        {
                            activated = Some((full_name.clone(), context));
                            break;
                        }
                    }
                }

                if let Some((qname, context)) = activated {
                    // Store full context for LLM injection (like Claude Code)
                    app.active_skill_context = Some(context.clone());
                    // Show collapsible preview to user
                    app.messages.push(crate::tui::app::Message {
                        role: "skill".into(),
                        content: format!("[{}] {}", qname, context),
                        collapsed: true,
                        elapsed_ms: None,
                    });
                    app.viewport.content_added();
                    found = true;
                }

                if !found {
                    app.add_assistant_message(format!(
                        "Unknown command: {}. Type /help for commands, /skills list for skills.",
                        cmd_text
                    ));
                }
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
            // Stamp elapsed time on the assistant message
            let elapsed_ms = app.stream_start.map(|s| s.elapsed().as_millis() as u64);
            if app.stream.buffer.is_empty()
                && let Some(last) = app.messages.last()
                && last.content.is_empty()
            {
                app.messages.pop();
                let mut msg = crate::tui::app::Message::assistant("[no response]".into());
                msg.elapsed_ms = elapsed_ms;
                app.messages.push(msg);
            } else if let Some(last) = app.messages.iter_mut().rev().find(|m| m.role == "assistant")
            {
                last.elapsed_ms = elapsed_ms;
            }
            app.stream.buffer.clear();
            app.stream_start = None;
            app.state = AppState::Idle;
        }

        // Flush completed messages into terminal scrollback via insert_before.
        // During streaming, keep the last assistant message in the inline area
        // so it updates live. Once streaming ends, flush everything.
        flush_messages_to_scrollback(terminal, app)?;

        // Draw the inline viewport: status bar + separator + input
        terminal.draw(|f| {
            render::draw_inline(f, app);
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
                            // Send permission response if this was an approval overlay
                            if let Some(ref overlay) = app.overlay
                                && overlay.context() == "approval"
                            {
                                let approved = overlay.approval_result().unwrap_or(false);
                                if let Some(resp_tx) = app.pending_perm_response.take() {
                                    let _ = resp_tx.send(approved);
                                }
                            }
                            app.overlay = None;
                            app.state = AppState::Idle;
                        }
                        crate::tui::overlays::OverlayResult::Consumed => {}
                    }
                    continue;
                }
                input::handle_key(app, key);
            }
            event::Event::Mouse(_) => {}
            event::Event::Resize(_, _) => {}
            _ => {}
        }
    }

    Ok(())
}

fn flush_messages_to_scrollback(
    terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    app: &mut App,
) -> anyhow::Result<()> {
    let flush_end = if app.state == AppState::Streaming {
        app.messages
            .iter()
            .rposition(|m| m.role == "assistant")
            .unwrap_or(app.messages.len())
    } else {
        app.messages.len()
    };

    if app.flushed_up_to >= flush_end {
        return Ok(());
    }

    let p = &app.theme.palette;
    let width = terminal::size().map(|(w, _)| w as usize).unwrap_or(80);

    let messages_to_flush = &app.messages[app.flushed_up_to..flush_end];
    let lines = render::build_lines_for_messages(messages_to_flush, p, width, app.flushed_up_to > 0);

    if !lines.is_empty() {
        // Convert Lines to ANSI strings and print directly to avoid
        // ratatui Buffer rendering issues with CJK full-width characters
        let ansi_lines: Vec<String> = lines.iter().map(line_to_ansi).collect();
        let height = ansi_lines.len() as u16;
        terminal.insert_before(height, |buf: &mut Buffer| {
            // Leave buffer empty — we print ANSI directly below
            let _ = buf;
        })?;
        // Move cursor up to the inserted area and print ANSI lines
        execute!(
            stdout(),
            cursor::MoveUp(height + INLINE_HEIGHT),
        )?;
        for ansi in &ansi_lines {
            execute!(
                stdout(),
                crossterm::style::Print(ansi),
                crossterm::style::Print("\r\n"),
            )?;
        }
    }

    app.flushed_up_to = flush_end;
    Ok(())
}

fn line_to_ansi(line: &ratatui::text::Line) -> String {
    let mut out = String::new();
    for span in &line.spans {
        let mut has_style = false;
        if let Some(ratatui::style::Color::Rgb(r, g, b)) = span.style.fg {
            out.push_str(&format!("\x1b[38;2;{};{};{}m", r, g, b));
            has_style = true;
        }
        if span.style.add_modifier.contains(ratatui::style::Modifier::BOLD) {
            out.push_str("\x1b[1m");
            has_style = true;
        }
        out.push_str(&span.content);
        if has_style {
            out.push_str("\x1b[0m");
        }
    }
    out
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

fn centered_rect(
    percent_x: u16,
    percent_y: u16,
    r: ratatui::layout::Rect,
) -> ratatui::layout::Rect {
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

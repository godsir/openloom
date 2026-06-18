//! TUI layout and rendering.
//!
//! Three panels:
//! 1. Chat area (scrollable)
//! 2. Tool call panel (only visible when tools are active)
//! 3. Input line + status bar

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

use crate::tui::app::{AppState, ToolStatus};

pub fn render(f: &mut Frame, state: &AppState) {
    // Main vertical split
    let has_tools = !state.tools.is_empty();
    let main_chunks = if has_tools {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(3),                          // chat
                Constraint::Length(tool_panel_height(&state.tools)),
                Constraint::Length(1),                       // input + status
            ])
            .split(f.area())
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(3),     // chat
                Constraint::Length(1),  // input + status
            ])
            .split(f.area())
    };

    if has_tools {
        render_chat(f, main_chunks[0], state);
        render_tool_panel(f, main_chunks[1], state);
        render_input_bar(f, main_chunks[2], state);
    } else {
        render_chat(f, main_chunks[0], state);
        render_input_bar(f, main_chunks[1], state);
    }

    // Render overlay if present
    if let Some(ref overlay) = state.overlay {
        render_overlay(f, f.area(), overlay);
    }
}

fn tool_panel_height(tools: &[crate::tui::app::ToolEntry]) -> u16 {
    // 1 border top + 1 border bottom + up to 6 tool rows
    (tools.len().min(6) as u16) + 2
}

fn render_chat(f: &mut Frame, area: Rect, state: &AppState) {
    let block = Block::default()
        .title(" openLoom chat ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    if state.chat_lines.is_empty() {
        render_welcome(f, block.inner(area));
        f.render_widget(block, area);
    } else {
        let lines: Vec<Line> = state
            .chat_lines
            .iter()
            .map(|cl| {
                let role_span = match cl.role {
                    "user" => Span::styled("> ", Style::default().fg(Color::Cyan)),
                    "assistant" => Span::raw(""),
                    "thinking" => {
                        Span::styled("  [think] ", Style::default().fg(Color::Yellow))
                    }
                    "tool" => Span::styled("  [tool] ", Style::default().fg(Color::Magenta)),
                    _ => Span::raw(""),
                };
                let text_span = Span::raw(&cl.text);
                Line::from(vec![role_span, text_span])
            })
            .collect();

        let paragraph = Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false })
            .scroll((state.scroll_offset, 0));

        f.render_widget(paragraph, area);
    }
}

fn render_welcome(f: &mut Frame, area: Rect) {
    let version = env!("CARGO_PKG_VERSION");

    let lines: Vec<Line> = vec![
        Line::raw(""),
        Line::from(Span::styled(
            "  ╔══════════════════════════════════════╗",
            Style::default().fg(Color::Rgb(106, 106, 247)),
        )),
        Line::from(vec![
            Span::styled(
                "  ║      ",
                Style::default().fg(Color::Rgb(106, 106, 247)),
            ),
            Span::styled(
                "openLoom",
                Style::default()
                    .fg(Color::Rgb(106, 106, 247))
                    .add_modifier(ratatui::style::Modifier::BOLD),
            ),
            Span::styled(
                " — local-first AI assistant",
                Style::default().fg(Color::Gray),
            ),
            Span::styled(
                "      ║",
                Style::default().fg(Color::Rgb(106, 106, 247)),
            ),
        ]),
        Line::from(Span::styled(
            format!(
                "  ║              v{:<22}║",
                version
            ),
            Style::default().fg(Color::Rgb(106, 106, 247)),
        )),
        Line::from(Span::styled(
            "  ╚══════════════════════════════════════╝",
            Style::default().fg(Color::Rgb(106, 106, 247)),
        )),
        Line::raw(""),
        Line::from(Span::styled(
            "  Commands:",
            Style::default()
                .fg(Color::White)
                .add_modifier(ratatui::style::Modifier::BOLD),
        )),
        Line::from(vec![
            Span::styled("    /tools ", Style::default().fg(Color::Cyan)),
            Span::raw("  — list available tools"),
        ]),
        Line::from(vec![
            Span::styled("    /skills", Style::default().fg(Color::Cyan)),
            Span::raw("  — list loaded skills"),
        ]),
        Line::from(vec![
            Span::styled("    /exit  ", Style::default().fg(Color::Cyan)),
            Span::raw("  — quit"),
        ]),
        Line::raw(""),
        Line::from(Span::styled(
            "  Type a message and press Enter to start.",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    // Center vertically
    let total_height = lines.len() as u16;
    let vpad = area.height.saturating_sub(total_height) / 2;
    let centered_area = Rect::new(
        area.x,
        area.y + vpad,
        area.width,
        total_height,
    );

    let paragraph = Paragraph::new(lines);
    f.render_widget(paragraph, centered_area);
}

fn render_tool_panel(f: &mut Frame, area: Rect, state: &AppState) {
    let block = Block::default()
        .title(" tools ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    let lines: Vec<Line> = state
        .tools
        .iter()
        .map(|t| {
            let icon = match t.status {
                ToolStatus::Waiting => "⏳",
                ToolStatus::Running => "🔄",
                ToolStatus::Done => "✅",
                ToolStatus::Failed => "❌",
            };
            let style = match t.status {
                ToolStatus::Waiting => Style::default().fg(Color::Yellow),
                ToolStatus::Running => Style::default().fg(Color::Cyan),
                ToolStatus::Done => Style::default().fg(Color::Green),
                ToolStatus::Failed => Style::default().fg(Color::Red),
            };
            Line::from(Span::styled(
                format!(
                    " [{}.{}] {} {}",
                    t.index,
                    icon,
                    t.name,
                    status_label(&t.status)
                ),
                style,
            ))
        })
        .collect();

    let paragraph = Paragraph::new(lines).block(block);
    f.render_widget(paragraph, area);
}

fn status_label(s: &ToolStatus) -> &'static str {
    match s {
        ToolStatus::Waiting => "pending",
        ToolStatus::Running => "running…",
        ToolStatus::Done => "done",
        ToolStatus::Failed => "failed",
    }
}

fn render_input_bar(f: &mut Frame, area: Rect, state: &AppState) {
    // Split into input (left) and status (right)
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(area);

    // Input: "> text" with spinner if streaming
    let input_text = if state.streaming {
        format!("> {} ⏳", state.input)
    } else {
        format!("> {}", state.input)
    };
    let input_span = Span::styled(&input_text, Style::default().fg(Color::White));
    f.render_widget(Paragraph::new(Line::from(input_span)), chunks[0]);

    // Status bar
    let status = format!(
        "{} | in {} out {}",
        state.model_name, state.tokens.prompt, state.tokens.completion,
    );
    let status_span = Span::styled(&status, Style::default().fg(Color::DarkGray));
    f.render_widget(
        Paragraph::new(Line::from(status_span))
            .alignment(ratatui::layout::Alignment::Right),
        chunks[1],
    );
}

fn render_overlay(f: &mut Frame, area: Rect, overlay: &crate::tui::app::OverlayContent) {
    // Center a popup taking ~60% width, max 60% height
    let popup_width = (area.width as f32 * 0.6) as u16;
    let popup_height = (area.height as f32 * 0.6) as u16;
    let popup_x = (area.width - popup_width) / 2;
    let popup_y = (area.height - popup_height) / 2;
    let popup_area = Rect::new(area.x + popup_x, area.y + popup_y, popup_width, popup_height);

    let block = Block::default()
        .title(format!(" {} ", overlay.title))
        .borders(Borders::ALL)
        .style(Style::default().bg(Color::Rgb(30, 30, 40)));

    let paragraph = Paragraph::new(overlay.body.as_str())
        .block(block)
        .wrap(Wrap { trim: false });

    f.render_widget(ratatui::widgets::Clear, popup_area);
    f.render_widget(paragraph, popup_area);
}

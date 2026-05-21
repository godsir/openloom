use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style, Stylize},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::tui::app::{App, AppState};
use crate::tui::theme::Palette;

// ── layout ──────────────────────────────────────────────────────

pub fn draw(f: &mut Frame, app: &App) {
    let p = &app.theme.palette;

    let [main_area, status_area, input_area] = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(input_height(app)),
        ])
        .areas(f.area());

    draw_messages(f, main_area, app, p);
    draw_status_line(f, status_area, app, p);
    draw_input(f, input_area, app, p);
}

// ── message area ─────────────────────────────────────────────────

fn draw_messages(f: &mut Frame, area: Rect, app: &App, p: &Palette) {
    let block = Block::default().borders(Borders::NONE);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let visible_height = inner.height as usize;

    if app.messages.is_empty() {
        draw_welcome(f, inner, p);
        return;
    }

    // Build lines for ALL messages, then slice to visible range.
    let all_lines = build_message_lines(app, p, inner.width as usize);
    let total_lines = all_lines.len();

    // Compute visible window
    let max_offset = total_lines.saturating_sub(visible_height);
    let effective_offset = if app.viewport.auto_scroll {
        max_offset
    } else {
        app.viewport.scroll_offset.min(max_offset)
    };

    let end = (effective_offset + visible_height).min(total_lines);
    let start = effective_offset.min(end);

    let visible: Vec<Line> = if start < all_lines.len() {
        all_lines[start..end].to_vec()
    } else {
        Vec::new()
    };

    let para = Paragraph::new(Text::from(visible));
    f.render_widget(para, inner);

    // "↓ N new messages" pill when user scrolled up during streaming
    if !app.viewport.auto_scroll && app.viewport.unseen_count > 0 {
        let indicator = format!(" ↓ {} new messages ", app.viewport.unseen_count);
        let width = indicator.len() as u16;
        let x = inner.width.saturating_sub(width) / 2;
        let y = inner.y + inner.height.saturating_sub(1);
        if width < inner.width {
            let pill = Paragraph::new(Span::styled(
                indicator,
                Style::new().fg(p.bg).bg(p.accent).bold(),
            ));
            f.render_widget(pill, Rect::new(x, y, width.min(inner.width), 1));
        }
    }
}

fn build_message_lines(app: &App, p: &Palette, wrap_width: usize) -> Vec<Line<'static>> {
    let mut lines: Vec<Line> = Vec::new();

    for msg in &app.messages {
        let (role_label, role_color) = role_attrs(&msg.role, p);
        let header = format!("  {} {}", role_label.0, role_label.1);
        lines.push(Line::from(Span::styled(
            header,
            Style::new().fg(role_color).bold(),
        )));

        for raw_line in msg.content.lines() {
            if raw_line.is_empty() {
                lines.push(Line::from(""));
                continue;
            }
            let wrapped = wrap_line(raw_line, wrap_width.saturating_sub(4));
            for w in &wrapped {
                lines.push(Line::from(Span::styled(
                    format!("  {}", w),
                    Style::new().fg(p.text),
                )));
            }
        }

        // Blinking streaming cursor on last assistant message
        let is_last = app.messages.last().map(|m| std::ptr::eq(m, msg)).unwrap_or(false);
        if app.state == AppState::Streaming
            && msg.role == "assistant"
            && is_last
            && app.frame_count % 16 < 8
        {
            lines.push(Line::from(Span::styled("  ▊", Style::new().fg(p.accent))));
        }

        lines.push(Line::from(""));
    }

    lines
}

fn role_attrs(role: &str, p: &Palette) -> ((&'static str, String), Color) {
    match role {
        "user" => (("▸", "You".into()), p.user_bubble),
        "assistant" => (("●", "openLoom".into()), p.accent),
        "thinking" => (("◉", "Thinking".into()), p.warning),
        "tool_call" => (("◆", "Tool".into()), p.warning),
        "tool_result" => (("◇", "Result".into()), p.success),
        "error" => (("✖", "Error".into()), p.error),
        _ => (("●", role.into()), p.accent),
    }
}

fn wrap_line(text: &str, max_width: usize) -> Vec<String> {
    let w = max_width.max(20);
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= w {
        return vec![text.to_string()];
    }
    chars
        .chunks(w)
        .map(|c| c.iter().collect::<String>())
        .collect()
}

fn draw_welcome(f: &mut Frame, area: Rect, p: &Palette) {
    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            "  Welcome to openLoom.",
            Style::new().fg(p.accent).bold(),
        )),
        Line::from(Span::styled(
            "  Type a message and press Enter to start.",
            Style::new().fg(p.text_dim),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("  ", Style::new()),
            Span::styled("Enter", Style::new().fg(p.accent).bold()),
            Span::styled(" send  ", Style::new().fg(p.text_dim)),
            Span::styled("Ctrl+C", Style::new().fg(p.warning).bold()),
            Span::styled(" quit  ", Style::new().fg(p.text_dim)),
            Span::styled("/help", Style::new().fg(p.accent).bold()),
            Span::styled(" commands", Style::new().fg(p.text_dim)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "  Shift+Enter for newline  |  Ctrl+G for external editor  |  Up/Down for history",
            Style::new().fg(p.text_dim),
        )),
    ];
    let para = Paragraph::new(Text::from(lines));
    f.render_widget(para, area);
}

// ── status bar ───────────────────────────────────────────────────

fn draw_status_line(f: &mut Frame, area: Rect, app: &App, p: &Palette) {
    let state_indicator = match app.state {
        AppState::Idle => format!(" {} ", app.status.state_icon()),
        AppState::Waiting => " ○ ".to_string(),
        AppState::Streaming => " ● ".to_string(),
        AppState::Overlay => " ◉ ".to_string(),
    };

    let model = &app.status.model;
    let cwd = &app.status.cwd;
    let tokens = format_tokens(app.status.turn_tokens);

    let left = format!(
        "{}{} │ {} │ {} tokens",
        state_indicator, model, cwd, tokens,
    );

    let right = if app.status.git_branch.is_empty() {
        format!(
            "prompt {}k  comp {}k  ${:.4}",
            app.total_prompt_tokens / 1000,
            app.total_completion_tokens / 1000,
            app.total_cost,
        )
    } else {
        format!(
            "{} │ prompt {}k  comp {}k  ${:.4}",
            app.status.git_branch,
            app.total_prompt_tokens / 1000,
            app.total_completion_tokens / 1000,
            app.total_cost,
        )
    };

    let total_width = area.width as usize;
    let left_w = left.chars().count();
    let right_w = right.chars().count();
    let padding = total_width.saturating_sub(left_w + right_w);
    let full = format!("{}{}{}", left, " ".repeat(padding), right);

    let bar = Paragraph::new(Span::styled(
        full,
        Style::new().fg(p.text_dim).bg(p.surface),
    ))
    .block(Block::default().borders(Borders::NONE));

    f.render_widget(bar, area);
}

fn format_tokens(n: usize) -> String {
    if n >= 1000 {
        format!("{:.1}k", n as f64 / 1000.0)
    } else if n > 0 {
        format!("{}", n)
    } else {
        "-".into()
    }
}

// ── input area ───────────────────────────────────────────────────

fn draw_input(f: &mut Frame, area: Rect, app: &App, p: &Palette) {
    let border_style = match app.state {
        AppState::Waiting => Style::new().fg(p.warning),
        AppState::Streaming => Style::new().fg(p.accent),
        _ => Style::new().fg(p.surface),
    };

    let title = match app.state {
        AppState::Waiting => " Thinking... ",
        AppState::Streaming => " Streaming... ",
        _ => "",
    };

    let mut block = Block::default()
        .borders(Borders::TOP)
        .border_style(border_style);

    if !title.is_empty() {
        block = block.title_top(Span::styled(title, border_style));
    }

    let inner = block.inner(area);
    f.render_widget(block, area);
    f.render_widget(&app.input, inner);

    // Command palette popup when typing /
    draw_command_palette(f, app, area, p);
}

fn input_height(app: &App) -> u16 {
    let lines = app.input.lines().len().clamp(1, 8) as u16;
    lines + 2
}

// ── command palette ──────────────────────────────────────────────

const SLASH_COMMANDS: &[(&str, &str)] = &[
    ("/help", "Show help and keybindings"),
    ("/model", "Show current model"),
    ("/cost", "Show token usage and cost"),
    ("/clear", "Clear all messages"),
    ("/theme dark", "Switch to dark theme"),
    ("/theme light", "Switch to light theme"),
    ("/session new", "Create a new session"),
    ("/session list", "List all sessions"),
    ("/memory persona", "Show persona summary"),
    ("/memory events", "List recent events"),
    ("/memory cognitions", "List cognitions for USER"),
    ("/memory search", "Search events with FTS5"),
    ("/skills list", "List registered skills"),
    ("/config get", "Get config values"),
    ("/config set", "Set config values"),
];

pub fn draw_command_palette(f: &mut Frame, app: &App, input_area: Rect, p: &Palette) {
    let current_input = app.input.lines().first().map(|s| s.as_str()).unwrap_or("");
    if !current_input.starts_with('/') || current_input.starts_with("//") {
        return;
    }

    let prefix = &current_input[1..]; // strip the '/'
    let matches: Vec<&(&str, &str)> = SLASH_COMMANDS
        .iter()
        .filter(|(cmd, _)| {
            let cmd_name = cmd[1..].split(' ').next().unwrap_or("");
            cmd_name.starts_with(prefix) || prefix.is_empty()
        })
        .collect();

    if matches.is_empty() && !prefix.is_empty() {
        return; // no matches, don't show palette
    }

    let max_height = (matches.len() as u16).min(8);
    if max_height == 0 {
        return;
    }

    // Palette just above the input area
    let palette_width = 40u16;
    let x = 2u16;
    let y = input_area.y.saturating_sub(max_height + 2);

    if y < 2 {
        return; // not enough space
    }

    let palette_area = Rect::new(x, y, palette_width, max_height + 2);

    // Background
    f.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::new().fg(p.accent))
            .style(Style::new().bg(p.surface)),
        palette_area,
    );

    let inner = Rect::new(
        palette_area.x + 1,
        palette_area.y + 1,
        palette_area.width - 2,
        palette_area.height - 2,
    );

    let lines: Vec<Line> = matches
        .iter()
        .enumerate()
        .map(|(i, (cmd, desc))| {
            let is_selected = i == app.command_palette_selected.min(matches.len().saturating_sub(1));
            let style = if is_selected {
                Style::new().fg(p.bg).bg(p.accent).bold()
            } else {
                Style::new().fg(p.text)
            };
            Line::from(vec![
                Span::styled(format!(" {} ", cmd), style),
                Span::styled(
                    format!(" {}", desc),
                    if is_selected {
                        Style::new().fg(p.bg).bg(p.accent)
                    } else {
                        Style::new().fg(p.text_dim)
                    },
                ),
            ])
        })
        .collect();

    let para = Paragraph::new(Text::from(lines));
    f.render_widget(para, inner);
}


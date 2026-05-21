use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span, Text},
    widgets::{Block, Borders, Gauge, Paragraph, Wrap},
    Frame,
};

use crate::tui::app::{App, AppState};
use crate::tui::theme::Palette;

pub fn draw(f: &mut Frame, app: &App) {
    let palette = &app.theme.palette;

    let [main_area, status_area, input_area] = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(input_height(app)),
        ])
        .areas(f.area());

    draw_messages(f, main_area, app, palette);
    draw_status(f, status_area, app, palette);
    draw_input(f, input_area, app, palette);
}

fn draw_messages(f: &mut Frame, area: Rect, app: &App, p: &Palette) {
    let mut lines: Vec<Line> = Vec::new();

    if app.messages.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled(
                "  Welcome to openLoom.",
                Style::new()
                    .fg(p.accent)
                    .add_modifier(ratatui::style::Modifier::BOLD),
            ),
        ]));
        lines.push(Line::from(vec![Span::styled(
            "  Type a message and press Enter to start.",
            Style::new().fg(p.text_dim),
        )]));
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("  Ctrl+C ", Style::new().fg(p.warning)),
            Span::styled("quit  ", Style::new().fg(p.text_dim)),
            Span::styled("Enter ", Style::new().fg(p.accent)),
            Span::styled("send  ", Style::new().fg(p.text_dim)),
            Span::styled("PageUp/Down ", Style::new().fg(p.accent)),
            Span::styled("scroll", Style::new().fg(p.text_dim)),
        ]));
    }

    for msg in &app.messages {
        let (label, color) = match msg.role.as_str() {
            "user" => ("  You ", p.user_bubble),
            _ => ("  openLoom ", p.accent),
        };

        lines.push(Line::from(vec![Span::styled(
            label,
            Style::new()
                .fg(color)
                .add_modifier(ratatui::style::Modifier::BOLD),
        )]));

        for line in msg.content.lines() {
            lines.push(Line::from(vec![Span::styled(
                format!("  {}", line),
                Style::new().fg(p.text),
            )]));
        }
        lines.push(Line::from(""));
    }

    // Scroll indicator
    if app.scroll > 0 {
        lines.push(Line::from(vec![Span::styled(
            format!("  [scrolled {} lines]", app.scroll),
            Style::new().fg(p.text_dim),
        )]));
    }

    let scroll_offset = if app.auto_scroll {
        // Estimate total visual lines (base lines + wrap overhead from long content lines)
        let base_lines = lines.len();
        let extra: usize = app
            .messages
            .iter()
            .flat_map(|m| m.content.lines())
            .map(|l| l.chars().count().saturating_sub(2) / area.width.max(1) as usize)
            .sum::<usize>();
        (base_lines + extra).saturating_sub(area.height as usize) as u16
    } else {
        app.scroll
    };

    let paragraph = Paragraph::new(Text::from(lines))
        .block(Block::default().borders(Borders::NONE))
        .wrap(Wrap { trim: false })
        .scroll((scroll_offset, 0));

    f.render_widget(&paragraph, area);
}

fn draw_status(f: &mut Frame, area: Rect, app: &App, p: &Palette) {
    let left = format!(
        " {} {} | {} | {} ",
        app.status.state_icon(),
        app.status.model,
        app.status.cwd,
        format_tokens(app.status.turn_tokens),
    );

    let right = if app.status.git_branch.is_empty() {
        String::new()
    } else {
        format!(" git:{} ", app.status.git_branch)
    };

    let bar = Gauge::default()
        .gauge_style(Style::new().fg(p.surface).bg(p.surface))
        .ratio(1.0)
        .label(Span::styled(
            format!("{}{}", left, right),
            Style::new().fg(p.text_dim),
        ));

    f.render_widget(&bar, area);
}

fn draw_input(f: &mut Frame, area: Rect, app: &App, p: &Palette) {
    let block = match app.state {
        AppState::Waiting => Block::default()
            .borders(Borders::TOP)
            .border_style(Style::new().fg(p.warning))
            .title_top(Span::styled(
                " Thinking... ",
                Style::new().fg(p.warning),
            )),
        AppState::Streaming => Block::default()
            .borders(Borders::TOP)
            .border_style(Style::new().fg(p.accent)),
        _ => Block::default()
            .borders(Borders::TOP)
            .border_style(Style::new().fg(p.surface)),
    };

    let inner_area = block.inner(area);
    f.render_widget(block, area);
    f.render_widget(&app.input, inner_area);
}

fn format_tokens(n: usize) -> String {
    if n >= 1000 {
        format!("{:.1}kt", n as f64 / 1000.0)
    } else if n > 0 {
        format!("{}t", n)
    } else {
        "-".into()
    }
}

fn input_height(app: &App) -> u16 {
    let lines = app.input.lines().len().max(1).min(8) as u16;
    lines + 2 // +2 for borders
}

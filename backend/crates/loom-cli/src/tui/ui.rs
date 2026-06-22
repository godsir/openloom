//! TUI rendering — Claude Code-style inline history flow.
//!
//! No panel borders. No separate tool panel. Everything flows inline.

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};
use pulldown_cmark::{Event as MdEvent, Parser, Tag, TagEnd, CodeBlockKind, HeadingLevel};

use crate::tui::app::{AppState, ContentBlock, HistoryItem, ToolStatus};

// ── Colors ─────────────────────────────────────────────────────────

mod c {
    use ratatui::style::Color;
    pub const MUTED:     Color = Color::DarkGray;
    pub const ACCENT:    Color = Color::Rgb(110, 110, 250);
    pub const USER:      Color = Color::Cyan;
    pub const THINK:     Color = Color::Rgb(180, 160, 80);
    pub const TOOL_DONE: Color = Color::Green;
    pub const TOOL_ERR:  Color = Color::Red;
    pub const TOOL_RUN:  Color = Color::Rgb(110, 110, 250);
    pub const CODE_BG:   Color = Color::Rgb(28, 30, 38);
    pub const CODE_FG:   Color = Color::Rgb(200, 210, 220);
    pub const WARN:      Color = Color::Yellow;
    pub const CURSOR:    Color = Color::Rgb(100, 100, 150);
}

// ── Entry ──────────────────────────────────────────────────────────

pub fn render(f: &mut Frame, state: &AppState) {
    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(2)])
        .split(area);

    state.viewport_rows.set(chunks[0].height);
    render_history(f, chunks[0], state);
    render_input(f, chunks[1], state);
    if let Some(ref o) = state.overlay { render_overlay(f, area, o); }
}

// ── History ────────────────────────────────────────────────────────

fn render_history(f: &mut Frame, area: Rect, state: &AppState) {
    if state.history.is_empty() {
        render_welcome(f, area);
        return;
    }

    let mut lines: Vec<Line> = Vec::new();
    for item in &state.history {
        match item {
            HistoryItem::User { text } => {
                if !lines.is_empty() { lines.push(Line::raw("")); }
                lines.push(Line::from(Span::styled(format!("▌ {}", text), Style::default().fg(c::USER).add_modifier(Modifier::BOLD))));
            }
            HistoryItem::Assistant { blocks } => {
                for block in blocks {
                    match block {
                        ContentBlock::Markdown(md) => {
                            render_markdown_lines(&mut lines, md, area.width.saturating_sub(2));
                        }
                    }
                }
            }
            HistoryItem::Thinking { text } => {
                lines.push(Line::raw(""));
                lines.push(Line::from(Span::styled("  ▶ thinking…", Style::default().fg(c::THINK).add_modifier(Modifier::ITALIC))));
                for ln in text.lines() {
                    let t = ln.trim();
                    if t.is_empty() { continue; }
                    lines.push(Line::from(Span::styled(format!("    {}", truncate(t, area.width.saturating_sub(4) as usize)), Style::default().fg(c::THINK))));
                }
            }
            HistoryItem::ToolGroup { tools } => {
                lines.push(Line::raw(""));
                for tc in tools {
                    let (icon, color) = match tc.status {
                        ToolStatus::Running => ("◉", c::TOOL_RUN),
                        ToolStatus::Done    => ("●", c::TOOL_DONE),
                        ToolStatus::Failed  => ("✕", c::TOOL_ERR),
                    };
                    lines.push(Line::from(Span::styled(
                        format!("  {} {} {}", icon, tc.name, truncate(&tc.args, 60)),
                        Style::default().fg(color),
                    )));
                    if let Some(ref res) = tc.result {
                        for ln in res.lines().take(10) {
                            lines.push(Line::from(Span::styled(format!("    │ {}", truncate(ln, area.width.saturating_sub(6) as usize)), Style::default().fg(c::MUTED))));
                        }
                    }
                }
            }
            HistoryItem::Info { text } => {
                lines.push(Line::raw(""));
                lines.push(Line::from(Span::styled(format!("  ✦ {}", text), Style::default().fg(c::MUTED).add_modifier(Modifier::ITALIC))));
            }
        }
    }

    if state.streaming {
        if let Some(HistoryItem::Assistant { .. }) = state.history.last() {} else {
            lines.push(Line::from(Span::styled(" ●", Style::default().fg(c::ACCENT))));
        }
    }

    let p = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .scroll((state.scroll_offset, 0));
    f.render_widget(p, area);

    if !state.scroll_following && state.scroll_offset > 0 {
        let ha = Rect::new(area.x, area.y + area.height.saturating_sub(1), area.width, 1);
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(" ↑ scrolled · End to follow ↑ ", Style::default().fg(c::WARN).bg(Color::Rgb(40, 38, 20))))),
            ha,
        );
    }
}

// ── Markdown → lines ───────────────────────────────────────────────

fn render_markdown_lines(lines: &mut Vec<Line>, md: &str, max_w: u16) {
    let max_chars = max_w.saturating_sub(2) as usize;
    let parser = Parser::new_ext(md, pulldown_cmark::Options::ENABLE_STRIKETHROUGH);

    let mut code_buf = String::new();
    let mut code_lang = String::new();
    let mut in_code = false;

    for event in parser {
        match event {
            MdEvent::Start(Tag::CodeBlock(kind)) => {
                in_code = true;
                code_lang = match kind { CodeBlockKind::Fenced(l) => l.to_string(), _ => String::new() };
                code_buf.clear();
            }
            MdEvent::End(TagEnd::CodeBlock) => {
                if in_code {
                    if !code_lang.is_empty() {
                        lines.push(Line::from(Span::styled(format!("  ── {} ──", code_lang), Style::default().fg(c::MUTED))));
                    }
                    for cl in code_buf.lines() {
                        lines.push(Line::from(Span::styled(
                            format!("  {}", truncate(cl, max_chars.saturating_sub(2))),
                            Style::default().fg(c::CODE_FG).bg(c::CODE_BG),
                        )));
                    }
                    lines.push(Line::raw(""));
                }
                in_code = false; code_lang.clear(); code_buf.clear();
            }
            MdEvent::Text(t) | MdEvent::Code(t) => {
                if in_code { code_buf.push_str(&t); }
                else {
                    let indented = indent_if_list(lines);
                    let inline_spans = render_inline(&t);
                    let mut combined = vec![Span::raw(indented)];
                    combined.extend(inline_spans);
                    lines.push(Line::from(combined));
                }
            }
            MdEvent::Start(Tag::Heading { level, .. }) => {
                let sz = match level {
                    HeadingLevel::H1 => ("\n══ ", Modifier::BOLD),
                    HeadingLevel::H2 => ("\n── ", Modifier::BOLD),
                    _                => ("\n·· ", Modifier::empty()),
                };
                lines.push(Line::from(Span::styled(sz.0, Style::default().fg(c::ACCENT).add_modifier(sz.1))));
            }
            MdEvent::Start(Tag::Item) => {
                lines.push(Line::from(Span::raw("  • ")));
            }
            MdEvent::HardBreak | MdEvent::SoftBreak => {
                if !in_code { lines.push(Line::raw("")); } else { code_buf.push('\n'); }
            }
            MdEvent::Rule => {
                lines.push(Line::raw(""));
                lines.push(Line::from(Span::styled("─".repeat(max_chars.min(40)), Style::default().fg(c::MUTED))));
                lines.push(Line::raw(""));
            }
            MdEvent::Start(Tag::BlockQuote(_)) => {
                lines.push(Line::from(Span::styled("▌ ", Style::default().fg(c::MUTED).add_modifier(Modifier::BOLD))));
            }
            _ => {}
        }
    }
}

fn indent_if_list(lines: &[Line]) -> String {
    // Cheap heuristic: if previous line starts with "  • ", indent.
    if lines.last().map_or(false, |l| {
        l.spans.first().map_or(false, |s| s.content.starts_with("  • "))
    }) { "  ".to_string() } else { String::new() }
}

// ── Inline formatting ──────────────────────────────────────────────

fn render_inline(text: &str) -> Vec<Span<'static>> {
    let mut spans: Vec<Span> = Vec::new();
    let mut pos = 0;
    let bytes = text.as_bytes();
    let len = text.len();

    while pos < len {
        if bytes[pos..].starts_with(b"**") {
            if let Some(end) = text[pos + 2..].find("**") {
                let c = &text[pos + 2..pos + 2 + end];
                spans.push(Span::styled(c.to_string(), Style::default().add_modifier(Modifier::BOLD)));
                pos += 2 + end + 2; continue;
            }
        }
        if bytes[pos] == b'*' && pos + 1 < len && bytes[pos + 1] != b'*' {
            if let Some(end) = text[pos + 1..].find('*') {
                let c = &text[pos + 1..pos + 1 + end];
                spans.push(Span::styled(c.to_string(), Style::default().add_modifier(Modifier::ITALIC)));
                pos += 1 + end + 1; continue;
            }
        }
        if bytes[pos] == b'`' {
            if let Some(end) = text[pos + 1..].find('`') {
                let c = &text[pos + 1..pos + 1 + end];
                spans.push(Span::styled(c.to_string(), Style::default().fg(c::WARN).bg(c::CODE_BG)));
                pos += 1 + end + 1; continue;
            }
        }
        if bytes[pos..].starts_with(b"~~") {
            if let Some(end) = text[pos + 2..].find("~~") {
                let c = &text[pos + 2..pos + 2 + end];
                spans.push(Span::styled(c.to_string(), Style::default().add_modifier(Modifier::CROSSED_OUT)));
                pos += 2 + end + 2; continue;
            }
        }
        // [link](url)
        if bytes[pos] == b'[' {
            if let Some(bracket_end) = text[pos..].find("](") {
                let link_text = &text[pos + 1..pos + bracket_end];
                let after = &text[pos + bracket_end + 2..];
                if let Some(paren_end) = after.find(')') {
                    spans.push(Span::styled(link_text.to_string(), Style::default().fg(c::ACCENT).add_modifier(Modifier::UNDERLINED)));
                    pos += 1 + bracket_end + 2 + paren_end + 1; continue;
                }
            }
        }

        // Literal run
        let next = find_next_marker(text, pos);
        if next > pos { spans.push(Span::raw(text[pos..next].to_string())); pos = next; }
        else { spans.push(Span::raw(text[pos..].to_string())); break; }
    }
    spans
}

fn find_next_marker(text: &str, pos: usize) -> usize {
    let from = safe_next(text, pos);
    let mut next = text.len();
    for m in &["**", "*", "`", "~~", "["] {
        if let Some(i) = text[from..].find(m) {
            next = next.min(from + i);
        }
    }
    next
}

fn safe_next(text: &str, pos: usize) -> usize {
    text[pos..].chars().next().map(|c| pos + c.len_utf8()).unwrap_or(text.len())
}

fn truncate(s: &str, n: usize) -> &str {
    if s.len() <= n { return s; }
    let mut count = 0;
    for (i, _) in s.char_indices() { if count >= n { return &s[..i]; } count += 1; }
    s
}

// ── Welcome ────────────────────────────────────────────────────────

fn render_welcome(f: &mut Frame, area: Rect) {
    let version = env!("CARGO_PKG_VERSION");
    let lines = vec![
        Line::raw(""),
        Line::from(Span::styled(format!("  openLoom v{}", version), Style::default().fg(c::ACCENT).add_modifier(Modifier::BOLD))),
        Line::from(Span::styled("  local-first AI assistant", Style::default().fg(c::MUTED))),
        Line::raw(""),
        Line::from(Span::styled("  Commands:", Style::default().add_modifier(Modifier::BOLD))),
        Line::from(vec![Span::styled("    /help  ", Style::default().fg(c::USER)), Span::raw("shortcuts & commands")]),
        Line::from(vec![Span::styled("    /tools ", Style::default().fg(c::USER)), Span::raw("list tools")]),
        Line::from(vec![Span::styled("    /skills", Style::default().fg(c::USER)), Span::raw("list skills")]),
        Line::from(vec![Span::styled("    /exit  ", Style::default().fg(c::USER)), Span::raw("quit")]),
        Line::raw(""),
        Line::from(Span::styled("  ↑↓ scroll · PgUp/Dn page · Esc clear · ^C quit", Style::default().fg(c::MUTED))),
    ];
    let h = lines.len() as u16;
    f.render_widget(Paragraph::new(lines), Rect::new(area.x, area.y + area.height.saturating_sub(h) / 2, area.width, h));
}

// ── Input bar ──────────────────────────────────────────────────────

fn render_input(f: &mut Frame, area: Rect, state: &AppState) {
    let chunks = Layout::default().direction(Direction::Vertical).constraints([Constraint::Length(1), Constraint::Length(1)]).split(area);

    let full = format!("> {}", state.input);
    let cursor_col = 2 + state.cursor;
    let mut spans: Vec<Span> = Vec::new();

    if state.input.is_empty() {
        spans.push(Span::styled("> ", Style::default().fg(c::MUTED)));
        spans.push(Span::styled("Type a message…", Style::default().fg(c::MUTED).add_modifier(Modifier::ITALIC)));
    } else if cursor_col < full.len() && !state.streaming {
        let before = &full[..cursor_col];
        let at = full[cursor_col..].chars().next().unwrap_or(' ');
        let after = &full[cursor_col + at.len_utf8()..];
        spans.push(Span::raw(before.to_string()));
        spans.push(Span::styled(at.to_string(), Style::default().fg(Color::Black).bg(c::CURSOR)));
        if !after.is_empty() { spans.push(Span::raw(after.to_string())); }
    } else if state.streaming {
        spans.push(Span::styled(&full, Style::default().fg(c::MUTED)));
    } else {
        spans.push(Span::raw(&full));
        spans.push(Span::styled(" ", Style::default().fg(Color::Black).bg(c::CURSOR)));
    }

    f.render_widget(Paragraph::new(Line::from(spans)), chunks[0]);

    let lhs = format!("{} │ in {} out {}", state.model_name, state.tokens.prompt, state.tokens.completion);
    let rhs = if state.streaming { "^C cancel" } else { "^C quit │ ↑↓ scroll │ Esc clear │ /help" };

    let bar = Layout::default().direction(Direction::Horizontal).constraints([Constraint::Percentage(50), Constraint::Percentage(50)]).split(chunks[1]);
    f.render_widget(Paragraph::new(Line::styled(&lhs, Style::default().fg(c::MUTED))), bar[0]);
    f.render_widget(Paragraph::new(Line::styled(rhs, Style::default().fg(c::MUTED))).alignment(Alignment::Right), bar[1]);
}

// ── Overlay ────────────────────────────────────────────────────────

fn render_overlay(f: &mut Frame, area: Rect, overlay: &crate::tui::app::OverlayContent) {
    let lines = overlay.body.lines().count();
    let h = (lines + 2).min((area.height as f32 * 0.85) as usize) as u16;
    let w = (area.width as f32 * 0.65) as u16;
    let x = (area.width - w) / 2;
    let y = (area.height - h) / 2;

    let block = Block::default()
        .title(format!(" {} (Esc) ", overlay.title))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(c::ACCENT))
        .style(Style::default().bg(Color::Rgb(20, 22, 30)));

    f.render_widget(ratatui::widgets::Clear, Rect::new(area.x + x, area.y + y, w, h));
    f.render_widget(Paragraph::new(overlay.body.as_str()).block(block).wrap(Wrap { trim: false }), Rect::new(area.x + x, area.y + y, w, h));
}

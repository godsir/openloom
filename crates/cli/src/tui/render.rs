use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style, Stylize},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph},
};

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::tui::app::{App, AppState, Message};
use crate::tui::theme::Palette;

/// Draw the inline viewport (bottom of terminal): streaming preview + status + input.
pub fn draw_inline(f: &mut Frame, app: &App) {
    let p = &app.theme.palette;

    let bg_block = Block::default().style(Style::new().bg(p.bg));
    f.render_widget(bg_block, f.area());

    let streaming_h = if app.state == AppState::Streaming {
        streaming_preview_height(app, f.area().width as usize)
    } else {
        0
    };

    let [_spacer, stream_area, status_area, input_area] = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(streaming_h),
            Constraint::Length(1),
            Constraint::Length(input_height(app)),
        ])
        .areas(f.area());

    if streaming_h > 0 {
        draw_streaming_preview(f, stream_area, app, p);
    }
    draw_status_line(f, status_area, app, p);
    draw_input(f, input_area, app, p);
}

/// How many rows the streaming preview needs in the inline viewport.
fn streaming_preview_height(app: &App, _width: usize) -> u16 {
    if app.state != AppState::Streaming {
        return 0;
    }
    // Show a compact 1-line streaming indicator
    1
}

fn draw_streaming_preview(f: &mut Frame, area: Rect, app: &App, p: &Palette) {
    let tail: String = app
        .stream
        .buffer
        .chars()
        .rev()
        .take(area.width as usize - 8)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();

    let cursor_char = if app.frame_count % 20 < 10 {
        "\u{258d}"
    } else {
        " "
    };

    let line = Line::from(vec![
        Span::styled("    ", Style::new().bg(p.bg)),
        Span::styled(tail, Style::new().fg(p.text).bg(p.bg)),
        Span::styled(cursor_char, Style::new().fg(p.accent).bg(p.bg)),
    ]);

    let para = Paragraph::new(line);
    f.render_widget(para, area);
}

// ── message rendering (for scrollback flush) ───────────────────

/// Build styled lines for a slice of messages.
/// Called from mod.rs to render into terminal scrollback via insert_before.
pub fn build_lines_for_messages(
    messages: &[Message],
    p: &Palette,
    area_width: usize,
    add_leading_blank: bool,
) -> Vec<Line<'static>> {
    let mut lines: Vec<Line> = Vec::new();
    let content_width = area_width.saturating_sub(6);

    for (idx, msg) in messages.iter().enumerate() {
        if idx > 0 || add_leading_blank {
            lines.push(Line::from(""));
        }

        let (marker, label, color) = role_style(&msg.role, p);

        if msg.collapsed {
            let preview = collapsed_preview(&msg.role, &msg.content);
            lines.push(Line::from(vec![
                Span::styled(format!("  {} ", marker), Style::new().fg(color)),
                Span::styled(label.to_string(), Style::new().fg(color).bold()),
                Span::styled(format!("  {}", preview), Style::new().fg(p.text_dim)),
            ]));
            continue;
        }

        // Header line with optional elapsed time
        let mut header_spans = vec![
            Span::styled(format!("  {} ", marker), Style::new().fg(color)),
            Span::styled(label.to_string(), Style::new().fg(color).bold()),
        ];
        if let Some(ms) = msg.elapsed_ms {
            header_spans.push(Span::styled(
                format!(" ({})", format_elapsed(ms)),
                Style::new().fg(p.text_dim),
            ));
        }
        lines.push(Line::from(header_spans));

        if msg.content.is_empty() {
            continue;
        }

        let is_diff_content = msg.role == "tool_result"
            && (msg.content.contains("\n+") || msg.content.contains("\n-"))
            && msg.content.contains("---");

        let use_markdown = msg.role == "assistant";
        let mut in_code_block = false;

        for raw_line in msg.content.lines() {
            if raw_line.is_empty() {
                // In code blocks, preserve blank lines for readability
                if in_code_block {
                    lines.push(Line::from(""));
                }
                // Outside code blocks, skip blank lines entirely for compact output
                continue;
            }

            if is_diff_content {
                let line_color = if raw_line.starts_with('+') && !raw_line.starts_with("+++") {
                    p.success
                } else if raw_line.starts_with('-') && !raw_line.starts_with("---") {
                    p.error
                } else if raw_line.starts_with("@@") {
                    p.accent
                } else if raw_line.starts_with("---") || raw_line.starts_with("+++") {
                    p.text_dim
                } else {
                    p.text
                };
                lines.push(Line::from(Span::styled(
                    format!("    {}", raw_line),
                    Style::new().fg(line_color),
                )));
            } else if use_markdown {
                // Code block fence toggle
                if raw_line.trim_start().starts_with("```") {
                    in_code_block = !in_code_block;
                    // Don't render the fence line itself
                    continue;
                }

                if in_code_block {
                    lines.push(Line::from(vec![
                        Span::styled("    ", Style::new()),
                        Span::styled(
                            raw_line.to_string(),
                            Style::new().fg(p.text_dim),
                        ),
                    ]));
                } else {
                    for w in wrap_text(raw_line, content_width) {
                        lines.push(render_markdown_line(&w, p));
                    }
                }
            } else {
                for w in wrap_text(raw_line, content_width) {
                    lines.push(render_line_with_commands(&w, p));
                }
            }
        }
    }

    lines
}

fn role_style<'a>(role: &'a str, p: &Palette) -> (&'a str, &'a str, Color) {
    match role {
        "user" => ("\u{276f}", "you", p.user_bubble),
        "assistant" => ("\u{25c6}", "openLoom", p.accent),
        "thinking" => ("\u{25cb}", "thinking", p.text_dim),
        "tool_call" => ("\u{25cf}", "tool", p.warning),
        "tool_result" => ("\u{2514}", "result", p.success),
        "skill" => ("\u{2726}", "skill", p.accent),
        "mode" => ("\u{2726}", "mode", p.accent),
        "error" => ("\u{2716}", "error", p.error),
        _ => ("\u{25cf}", role, p.text_dim),
    }
}

fn collapsed_preview(role: &str, content: &str) -> String {
    match role {
        "tool_call" => tool_call_summary(content),
        "tool_result" => tool_result_summary(content),
        "thinking" => {
            let preview: String = content.chars().take(60).collect();
            if content.len() > 60 {
                format!("{} ...", preview)
            } else {
                preview
            }
        }
        _ => {
            let preview: String = content.chars().take(40).collect();
            if content.len() > 40 {
                format!("{} ...", preview)
            } else {
                preview
            }
        }
    }
}

fn tool_call_summary(content: &str) -> String {
    if let Ok(obj) = serde_json::from_str::<serde_json::Value>(content) {
        let tool_name = obj
            .get("tool")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let params = obj.get("params").and_then(|v| v.as_object());

        match tool_name {
            "file_read" => {
                let path = params
                    .and_then(|p| p.get("path"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("?");
                let short = short_path(path);
                format!("Read({})", short)
            }
            "file_write" => {
                let path = params
                    .and_then(|p| p.get("path"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("?");
                let short = short_path(path);
                format!("Write({})", short)
            }
            "file_edit" => {
                let path = params
                    .and_then(|p| p.get("path"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("?");
                let short = short_path(path);
                format!("Update({})", short)
            }
            "shell" => {
                let cmd = params
                    .and_then(|p| p.get("command"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("?");
                let truncated: String = cmd.chars().take(50).collect();
                if cmd.len() > 50 {
                    format!("Bash({}...)", truncated)
                } else {
                    format!("Bash({})", truncated)
                }
            }
            "file_search" | "content_search" => {
                let query = params
                    .and_then(|p| p.get("query").or(p.get("pattern")))
                    .and_then(|v| v.as_str())
                    .unwrap_or("?");
                format!("Search({})", query)
            }
            _ => {
                format!("{}()", tool_name)
            }
        }
    } else {
        let preview: String = content.chars().take(60).collect();
        if content.len() > 60 {
            format!("{} ...", preview)
        } else {
            preview
        }
    }
}

fn tool_result_summary(content: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let added = lines.iter().filter(|l| l.starts_with('+') && !l.starts_with("+++")).count();
    let removed = lines.iter().filter(|l| l.starts_with('-') && !l.starts_with("---")).count();

    if added > 0 || removed > 0 {
        return format!("+{} -{} lines", added, removed);
    }

    if lines.len() == 1 {
        let preview: String = content.chars().take(60).collect();
        return preview;
    }

    format!("{} lines", lines.len())
}

fn short_path(path: &str) -> String {
    let parts: Vec<&str> = path.split(['/', '\\']).collect();
    if parts.len() <= 3 {
        return path.to_string();
    }
    parts[parts.len() - 3..].join("/")
}

fn render_markdown_line(text: &str, p: &Palette) -> Line<'static> {
    let mut spans: Vec<Span> = Vec::new();
    spans.push(Span::styled("    ", Style::new()));

    let trimmed = text.trim_start();

    // Headings (check longest prefix first)
    if let Some(rest) = trimmed.strip_prefix("### ") {
        spans.push(Span::styled(
            rest.to_string(),
            Style::new().fg(p.accent).bold(),
        ));
        return Line::from(spans);
    }
    if let Some(rest) = trimmed.strip_prefix("## ") {
        spans.push(Span::styled(
            rest.to_string(),
            Style::new().fg(p.accent).bold(),
        ));
        return Line::from(spans);
    }
    if let Some(rest) = trimmed.strip_prefix("# ") {
        spans.push(Span::styled(
            rest.to_string(),
            Style::new().fg(p.accent).bold(),
        ));
        return Line::from(spans);
    }

    // Table lines
    if trimmed.starts_with('|') && trimmed.ends_with('|') {
        // Table separator line (|---|---|)
        if trimmed
            .chars()
            .all(|c| c == '|' || c == '-' || c == ':' || c == ' ')
        {
            spans.push(Span::styled(
                trimmed.to_string(),
                Style::new().fg(p.text_dim),
            ));
            return Line::from(spans);
        }
        // Data row — color the pipes dim, content normal
        for segment in trimmed.split('|') {
            if segment.is_empty() {
                spans.push(Span::styled("|", Style::new().fg(p.text_dim)));
            } else {
                spans.push(Span::styled("|", Style::new().fg(p.text_dim)));
                parse_inline_markdown(segment, p, &mut spans);
            }
        }
        return Line::from(spans);
    }

    // Bullet lists
    if let Some(rest) = trimmed.strip_prefix("- ").or_else(|| trimmed.strip_prefix("* ")) {
        spans.push(Span::styled("\u{2022} ", Style::new().fg(p.accent)));
        parse_inline_markdown(rest, p, &mut spans);
        return Line::from(spans);
    }

    // Numbered lists (e.g. "1. ", "10. ", "123. ")
    {
        let bytes = trimmed.as_bytes();
        let mut digit_end = 0;
        while digit_end < bytes.len() && bytes[digit_end].is_ascii_digit() {
            digit_end += 1;
        }
        if digit_end > 0 && trimmed[digit_end..].starts_with(". ") {
            let prefix_end = digit_end + 2;
            spans.push(Span::styled(
                trimmed[..prefix_end].to_string(),
                Style::new().fg(p.accent),
            ));
            parse_inline_markdown(&trimmed[prefix_end..], p, &mut spans);
            return Line::from(spans);
        }
    }

    // Regular line with inline markdown
    parse_inline_markdown(text, p, &mut spans);
    Line::from(spans)
}

fn parse_inline_markdown(text: &str, p: &Palette, spans: &mut Vec<Span<'static>>) {
    let mut remaining = text;

    while !remaining.is_empty() {
        // Find the earliest marker
        let bold_pos = remaining.find("**");
        let code_pos = remaining.find('`');

        // Determine which comes first
        let next = match (bold_pos, code_pos) {
            (Some(b), Some(c)) => {
                if b <= c {
                    Some(('B', b))
                } else {
                    Some(('C', c))
                }
            }
            (Some(b), None) => Some(('B', b)),
            (None, Some(c)) => Some(('C', c)),
            (None, None) => None,
        };

        match next {
            Some(('B', start)) => {
                // Push text before the marker
                if start > 0 {
                    spans.push(Span::styled(
                        remaining[..start].to_string(),
                        Style::new().fg(p.text),
                    ));
                }
                let after = &remaining[start + 2..];
                if let Some(end) = after.find("**") {
                    spans.push(Span::styled(
                        after[..end].to_string(),
                        Style::new().fg(p.text).bold(),
                    ));
                    remaining = &after[end + 2..];
                } else {
                    // Unclosed **, render as-is
                    spans.push(Span::styled(
                        remaining[start..].to_string(),
                        Style::new().fg(p.text),
                    ));
                    return;
                }
            }
            Some(('C', start)) => {
                if start > 0 {
                    spans.push(Span::styled(
                        remaining[..start].to_string(),
                        Style::new().fg(p.text),
                    ));
                }
                let after = &remaining[start + 1..];
                if let Some(end) = after.find('`') {
                    spans.push(Span::styled(
                        after[..end].to_string(),
                        Style::new().fg(p.accent),
                    ));
                    remaining = &after[end + 1..];
                } else {
                    spans.push(Span::styled(
                        remaining[start..].to_string(),
                        Style::new().fg(p.text),
                    ));
                    return;
                }
            }
            _ => {
                // No more markers — push rest as plain text
                spans.push(Span::styled(
                    remaining.to_string(),
                    Style::new().fg(p.text),
                ));
                break;
            }
        }
    }
}

#[allow(dead_code)]
fn render_line_with_commands(text: &str, p: &Palette) -> Line<'static> {
    let mut spans: Vec<Span> = Vec::new();
    spans.push(Span::styled("    ", Style::new()));

    let mut remaining = text;
    while !remaining.is_empty() {
        if let Some(slash_pos) = remaining.find('/') {
            if slash_pos == 0
                || remaining.as_bytes().get(slash_pos.saturating_sub(1)) == Some(&b' ')
            {
                let after_slash = &remaining[slash_pos + 1..];
                let cmd_end = after_slash
                    .find([' ', '\n', ','])
                    .unwrap_or(after_slash.len());
                let cmd_text = &remaining[slash_pos..slash_pos + 1 + cmd_end];

                if is_known_command(cmd_text) {
                    if slash_pos > 0 {
                        spans.push(Span::styled(
                            remaining[..slash_pos].to_string(),
                            Style::new().fg(p.text),
                        ));
                    }
                    spans.push(Span::styled(
                        cmd_text.to_string(),
                        Style::new().fg(p.accent).bold(),
                    ));
                    remaining = &remaining[slash_pos + 1 + cmd_end..];
                    continue;
                }
            }
            let end = slash_pos + 1;
            spans.push(Span::styled(
                remaining[..end].to_string(),
                Style::new().fg(p.text),
            ));
            remaining = &remaining[end..];
        } else {
            spans.push(Span::styled(remaining.to_string(), Style::new().fg(p.text)));
            break;
        }
    }

    Line::from(spans)
}

fn is_known_command(text: &str) -> bool {
    let cmd = text.split(' ').next().unwrap_or(text);
    SLASH_COMMANDS
        .iter()
        .any(|(c, _)| c == &cmd || c.starts_with(&format!("{} ", cmd)))
}

fn wrap_text(text: &str, max_width: usize) -> Vec<String> {
    let w = max_width.max(20);
    if UnicodeWidthStr::width(text) <= w {
        return vec![text.to_string()];
    }

    let mut lines = Vec::new();
    let mut current = String::new();
    let mut current_width = 0usize;

    for word in text.split_inclusive(' ') {
        let word_width = UnicodeWidthStr::width(word);
        if current_width + word_width > w && !current.is_empty() {
            lines.push(current.trim_end().to_string());
            current = String::new();
            current_width = 0;
        }
        current.push_str(word);
        current_width += word_width;
    }
    if !current.is_empty() {
        lines.push(current.trim_end().to_string());
    }

    if lines.is_empty() {
        let mut line = String::new();
        let mut line_w = 0usize;
        for ch in text.chars() {
            let ch_w = UnicodeWidthChar::width(ch).unwrap_or(0);
            if line_w + ch_w > w && !line.is_empty() {
                lines.push(line);
                line = String::new();
                line_w = 0;
            }
            line.push(ch);
            line_w += ch_w;
        }
        if !line.is_empty() {
            lines.push(line);
        }
    }

    lines
}

// ── status bar ───────────────────────────────────────────────────

fn draw_status_line(f: &mut Frame, area: Rect, app: &App, p: &Palette) {
    const SPINNER: &[&str] = &[
        "\u{280b}", "\u{2819}", "\u{2839}", "\u{2838}", "\u{283c}", "\u{2834}", "\u{2826}",
        "\u{2827}", "\u{2807}", "\u{280f}",
    ];

    let state_indicator = match app.state {
        AppState::Idle => ("\u{25cf}".to_string(), p.success),
        AppState::Waiting => {
            let idx = (app.frame_count / 4) as usize % SPINNER.len();
            (SPINNER[idx].to_string(), p.warning)
        }
        AppState::Streaming => {
            let idx = (app.frame_count / 3) as usize % SPINNER.len();
            (SPINNER[idx].to_string(), p.accent)
        }
        AppState::Overlay => ("\u{25cf}".to_string(), p.warning),
    };

    let mut left_parts: Vec<String> = Vec::new();

    match app.state {
        AppState::Waiting => {
            let elapsed = app.stream_start.map(|s| s.elapsed().as_secs()).unwrap_or(0);
            left_parts.push(format!("Thinking\u{2026} ({}s)", elapsed));
        }
        AppState::Streaming => {
            let elapsed = app.stream_start.map(|s| s.elapsed().as_secs()).unwrap_or(0);
            let elapsed_str = if elapsed >= 60 {
                format!("{}m {}s", elapsed / 60, elapsed % 60)
            } else {
                format!("{}s", elapsed)
            };
            let tokens = app.stream.buffer.len().div_ceil(4);
            left_parts.push(format!(
                "Streaming\u{2026} ({} \u{00b7} \u{2193} ~{} tokens)",
                elapsed_str, tokens
            ));
        }
        _ => {
            if let Some(ref last) = app.status.last_model {
                left_parts.push(format!("{} \u{2190} {}", app.status.model, last));
            } else {
                left_parts.push(app.status.model.clone());
            }
            left_parts.push(format!("[{}]", app.mode.config().status_label));
            if app.thinking != openloom_models::ThinkingLevel::None {
                left_parts.push(format!("think:{}", app.thinking.label()));
            }
        }
    }

    if matches!(app.state, AppState::Idle | AppState::Overlay) {
        left_parts.push(short_cwd(&app.status.cwd));
        if app.status.context_max > 0 {
            let total_tokens = app.total_prompt_tokens + app.total_completion_tokens;
            if total_tokens > 0 {
                let pct = (total_tokens as f64 / app.status.context_max as f64 * 100.0).min(100.0);
                left_parts.push(format!(
                    "{:.0}% of {}",
                    pct,
                    format_context_size(app.status.context_max)
                ));
            } else {
                left_parts.push(format_context_size(app.status.context_max));
            }
        }
    }

    let left_text = left_parts.join(" \u{2502} ");

    let right = if app.total_prompt_tokens > 0 || app.total_completion_tokens > 0 {
        let cache_info = if app.total_cached_tokens > 0 && app.total_prompt_tokens > 0 {
            let hit_rate =
                (app.total_cached_tokens as f64 / app.total_prompt_tokens as f64) * 100.0;
            format!("  cache {:.0}%", hit_rate)
        } else {
            String::new()
        };
        format!(
            "{} / {} used{}  ${:.4} ",
            format_tokens(app.total_prompt_tokens),
            format_tokens(app.total_completion_tokens),
            cache_info,
            app.total_cost,
        )
    } else {
        format!("{} ", short_cwd(&app.status.cwd))
    };

    let total_width = area.width as usize;
    let dot_and_space = 3;
    let left_w = dot_and_space + UnicodeWidthStr::width(left_text.as_str());
    let right_w = UnicodeWidthStr::width(right.as_str());
    let padding = total_width.saturating_sub(left_w + right_w);

    let bar_line = Line::from(vec![
        Span::styled(" ", Style::new().bg(p.surface)),
        Span::styled(
            &state_indicator.0,
            Style::new().fg(state_indicator.1).bg(p.surface),
        ),
        Span::styled(
            format!(" {}", left_text),
            Style::new().fg(p.text).bg(p.surface),
        ),
        Span::styled(
            " ".repeat(padding),
            Style::new().fg(p.text_dim).bg(p.surface),
        ),
        Span::styled(right, Style::new().fg(p.text_dim).bg(p.surface)),
    ]);

    let bar = Paragraph::new(bar_line);
    f.render_widget(bar, area);
}

fn format_tokens(n: usize) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1000 {
        format!("{:.1}k", n as f64 / 1000.0)
    } else if n > 0 {
        format!("{}", n)
    } else {
        "0".into()
    }
}

fn format_elapsed(ms: u64) -> String {
    let secs = ms / 1000;
    if secs >= 60 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else if secs > 0 {
        let frac = (ms % 1000) / 100;
        format!("{}.{}s", secs, frac)
    } else {
        format!("{}ms", ms)
    }
}

fn format_context_size(n: usize) -> String {
    if n >= 1_000_000 {
        format!("{}M ctx", n / 1_000_000)
    } else if n >= 1000 {
        format!("{}k ctx", n / 1000)
    } else {
        format!("{} ctx", n)
    }
}

fn short_cwd(cwd: &str) -> String {
    let parts: Vec<&str> = cwd.split(['/', '\\']).collect();
    if parts.len() <= 2 {
        return cwd.to_string();
    }
    parts[parts.len() - 2..].join("/")
}

// ── input area ───────────────────────────────────────────────────

fn draw_input(f: &mut Frame, area: Rect, app: &App, p: &Palette) {
    let separator_style = match app.state {
        AppState::Streaming => Style::new().fg(p.accent).bg(p.bg),
        AppState::Waiting => Style::new().fg(p.warning).bg(p.bg),
        _ => Style::new().fg(p.surface).bg(p.bg),
    };

    let title = match app.state {
        AppState::Waiting => " thinking ",
        AppState::Streaming => " streaming ",
        _ => "",
    };

    let sep_width = area.width as usize;

    if !title.is_empty() {
        let title_color = separator_style.fg.unwrap_or(p.text_dim);
        let spans = vec![
            Span::styled("\u{2500}", separator_style),
            Span::styled(title, Style::new().fg(title_color).bg(p.bg).bold()),
            Span::styled(
                "\u{2500}".repeat(sep_width.saturating_sub(1 + title.len())),
                separator_style,
            ),
        ];
        let sep = Paragraph::new(Line::from(spans));
        f.render_widget(sep, Rect::new(area.x, area.y, area.width, 1));
    } else {
        let sep_line = "\u{2500}".repeat(sep_width);
        let sep = Paragraph::new(Span::styled(sep_line, separator_style));
        f.render_widget(sep, Rect::new(area.x, area.y, area.width, 1));
    }

    let input_rect = Rect::new(
        area.x,
        area.y + 1,
        area.width,
        area.height.saturating_sub(1),
    );
    let input_block = Block::default().style(Style::new().bg(p.bg));
    let inner = input_block.inner(input_rect);
    f.render_widget(input_block, input_rect);
    f.render_widget(&app.input, inner);

    draw_command_palette(f, app, area, p);
}

fn input_height(app: &App) -> u16 {
    let lines = app.input.lines().len().clamp(1, 8) as u16;
    lines + 1
}

// ── command palette ──────────────────────────────────────────────

pub const SLASH_COMMANDS: &[(&str, &str)] = &[
    ("/help", "Show help and keybindings"),
    ("/model", "Show model info"),
    ("/model set", "Configure cloud model"),
    ("/model use", "Switch local/cloud/auto"),
    ("/think", "Show/set thinking level"),
    ("/think none|low|mid|high|max", "Set extended thinking"),
    ("/token", "Session token usage and cost"),
    ("/token summary", "Usage by model"),
    ("/token today", "Today's usage"),
    ("/token history", "Recent request history"),
    ("/cost", "Alias for /token"),
    ("/health", "Engine status and diagnostics"),
    ("/local status", "Show local model info"),
    ("/local set", "Configure local model"),
    ("/local test", "Test local model connectivity"),
    ("/clear", "Clear all messages"),
    ("/theme dark", "Switch to dark theme"),
    ("/theme light", "Switch to light theme"),
    ("/session new", "Create a new session"),
    ("/session list", "List all sessions"),
    ("/session", "Resume a session by ID"),
    ("/memory persona", "Show persona summary"),
    ("/memory events", "List recent events"),
    ("/memory cognitions", "List cognitions"),
    ("/memory search", "Search events"),
    ("/skills list", "List registered skills"),
    ("/skills invoke", "Invoke a skill by name"),
    ("/mode", "Show/switch agent mode"),
    ("/mode chat", "Chat mode — pure conversation"),
    ("/mode plan", "Plan mode — read-only exploration"),
    ("/mode code", "Code mode — full agent + tools"),
    ("/mode assistant", "Assistant mode — general helper"),
    ("/config get", "Get config values"),
    ("/config set", "Set config values"),
];

#[allow(dead_code)]
pub fn palette_matches(input: &str) -> Vec<(&'static str, &'static str)> {
    if !input.starts_with('/') || input.starts_with("//") {
        return Vec::new();
    }

    let typed = &input[1..];
    let typed_lower = typed.to_lowercase();

    let has_exact = SLASH_COMMANDS
        .iter()
        .any(|(cmd, _)| cmd[1..].eq_ignore_ascii_case(typed_lower.trim()));
    if has_exact && !typed_lower.trim().is_empty() {
        return Vec::new();
    }

    SLASH_COMMANDS
        .iter()
        .filter(|(cmd, _)| {
            let full = &cmd[1..];
            full.starts_with(&typed_lower) || typed_lower.is_empty()
        })
        .map(|(cmd, desc)| (*cmd, *desc))
        .collect()
}

/// Like `palette_matches` but also includes dynamic external commands (e.g. plugin skills).
/// Returns owned strings so it can combine static built-in commands with runtime data.
pub fn palette_matches_dynamic(
    input: &str,
    external_commands: &[(String, String)],
) -> Vec<(String, String)> {
    if !input.starts_with('/') || input.starts_with("//") {
        return Vec::new();
    }

    let typed = &input[1..];
    let typed_lower = typed.to_lowercase();

    // Check for exact match in builtins
    let has_exact_builtin = SLASH_COMMANDS
        .iter()
        .any(|(cmd, _)| cmd[1..].eq_ignore_ascii_case(typed_lower.trim()));
    // Check for exact match in externals
    let has_exact_external = external_commands
        .iter()
        .any(|(cmd, _)| cmd[1..].eq_ignore_ascii_case(typed_lower.trim()));

    if (has_exact_builtin || has_exact_external) && !typed_lower.trim().is_empty() {
        return Vec::new();
    }

    let mut results: Vec<(String, String)> = SLASH_COMMANDS
        .iter()
        .filter(|(cmd, _)| {
            let full = &cmd[1..];
            full.starts_with(&typed_lower) || typed_lower.is_empty()
        })
        .map(|(cmd, desc)| (cmd.to_string(), desc.to_string()))
        .collect();

    for (cmd, desc) in external_commands {
        let full = &cmd[1..];
        if full.to_lowercase().starts_with(&typed_lower) || typed_lower.is_empty() {
            results.push((cmd.clone(), desc.clone()));
        }
    }

    results
}

pub fn draw_command_palette(f: &mut Frame, app: &App, input_area: Rect, p: &Palette) {
    let current_input = app.input.lines().first().map(|s| s.as_str()).unwrap_or("");

    // History search mode: show matching history entries
    if app.history_search_active {
        let query = current_input.to_lowercase();
        let history_matches: Vec<(String, String)> = app.history.iter().rev()
            .filter(|h| query.is_empty() || h.to_lowercase().contains(&query))
            .take(10)
            .map(|h| {
                let preview: String = h.chars().take(50).collect();
                (preview, String::new())
            })
            .collect();

        if history_matches.is_empty() {
            return;
        }

        draw_palette_items(f, app, input_area, p, &history_matches, "search: ");
        return;
    }

    let matches = palette_matches_dynamic(current_input, &app.external_commands);

    if matches.is_empty() {
        return;
    }

    draw_palette_items(f, app, input_area, p, &matches, "");
}

fn draw_palette_items(
    f: &mut Frame,
    app: &App,
    input_area: Rect,
    p: &Palette,
    matches: &[(String, String)],
    _prefix: &str,
) {
    let total_items = matches.len();
    let frame_area = f.area();
    let available_above = input_area.y.saturating_sub(frame_area.y) as usize;
    if available_above == 0 {
        return;
    }
    let visible_count = total_items.min(10).min(available_above);
    let max_height = visible_count as u16;

    let palette_width = (input_area.width).min(52);
    let x = input_area.x + 1;
    let y = input_area.y.saturating_sub(max_height).max(frame_area.y);

    let palette_area = Rect::new(x, y, palette_width, max_height);

    f.render_widget(Clear, palette_area);
    f.render_widget(
        Block::default()
            .borders(Borders::NONE)
            .style(Style::new().bg(p.surface)),
        palette_area,
    );

    let selected = app
        .command_palette_selected
        .min(total_items.saturating_sub(1));
    let scroll_start = if selected >= visible_count {
        selected - visible_count + 1
    } else {
        0
    };
    let scroll_end = (scroll_start + visible_count).min(total_items);

    let lines: Vec<Line> = matches[scroll_start..scroll_end]
        .iter()
        .enumerate()
        .map(|(i, (cmd, desc))| {
            let actual_idx = scroll_start + i;
            let is_selected = actual_idx == selected;
            let cmd_w = 20;
            let padded_cmd = format!(" {:width$}", cmd, width = cmd_w);
            if is_selected {
                Line::from(vec![
                    Span::styled(padded_cmd, Style::new().fg(p.bg).bg(p.accent).bold()),
                    Span::styled(format!(" {}", desc), Style::new().fg(p.bg).bg(p.accent)),
                ])
            } else {
                Line::from(vec![
                    Span::styled(padded_cmd, Style::new().fg(p.accent).bg(p.surface)),
                    Span::styled(
                        format!(" {}", desc),
                        Style::new().fg(p.text_dim).bg(p.surface),
                    ),
                ])
            }
        })
        .collect();

    let para = Paragraph::new(Text::from(lines));
    f.render_widget(para, palette_area);

    if total_items > visible_count {
        let indicator = format!(" {}/{} ", selected + 1, total_items);
        let ind_width = indicator.len() as u16;
        let ind_x = palette_area.x + palette_area.width.saturating_sub(ind_width);
        let ind_y = palette_area.y;
        let ind = Paragraph::new(Span::styled(
            indicator,
            Style::new().fg(p.text_dim).bg(p.surface),
        ));
        f.render_widget(ind, Rect::new(ind_x, ind_y, ind_width, 1));
    }
}

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style, Stylize},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph},
};

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::tui::app::{App, AppState};
use crate::tui::theme::Palette;

pub fn draw(f: &mut Frame, app: &App) {
    let p = &app.theme.palette;

    let bg_block = Block::default().style(Style::new().bg(p.bg));
    f.render_widget(bg_block, f.area());

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
    let visible_height = area.height as usize;

    let all_lines = build_message_lines(app, p, area.width as usize);
    let total_lines = all_lines.len();

    let max_offset = total_lines.saturating_sub(visible_height);
    // scroll_offset is "lines from bottom": 0 = at bottom, N = scrolled N lines up
    let effective_offset = if app.viewport.auto_scroll {
        max_offset
    } else {
        max_offset.saturating_sub(app.viewport.scroll_offset)
    };

    let end = (effective_offset + visible_height).min(total_lines);
    let start = effective_offset.min(end);

    let visible: Vec<Line> = if start < all_lines.len() {
        all_lines[start..end].to_vec()
    } else {
        Vec::new()
    };

    let para = Paragraph::new(Text::from(visible)).style(Style::new().bg(p.bg));
    f.render_widget(para, area);

    if !app.viewport.auto_scroll && app.viewport.unseen_count > 0 {
        let indicator = format!(" \u{2193} {} new ", app.viewport.unseen_count);
        let width = indicator.len() as u16;
        let x = area.x + area.width.saturating_sub(width + 1);
        let y = area.y + area.height.saturating_sub(1);
        if width < area.width {
            let pill = Paragraph::new(Span::styled(indicator, Style::new().fg(p.bg).bg(p.accent)));
            f.render_widget(pill, Rect::new(x, y, width.min(area.width), 1));
        }
    }
}

fn build_message_lines(app: &App, p: &Palette, area_width: usize) -> Vec<Line<'static>> {
    let mut lines: Vec<Line> = Vec::new();
    let content_width = area_width.saturating_sub(6);

    // Always render welcome banner at the top
    lines.extend(welcome_lines(p));
    lines.push(Line::from(""));

    for (idx, msg) in app.messages.iter().enumerate() {
        if idx > 0 {
            lines.push(Line::from(""));
        }

        let (marker, label, color) = role_style(&msg.role, p);

        // Collapsed messages show only header with expand hint
        if msg.collapsed {
            let preview: String = msg.content.chars().take(40).collect();
            lines.push(Line::from(vec![
                Span::styled(format!("  {} ", marker), Style::new().fg(color)),
                Span::styled(label.to_string(), Style::new().fg(color).bold()),
                Span::styled(format!("  {} ...", preview), Style::new().fg(p.text_dim)),
                Span::styled(" [Ctrl+O]", Style::new().fg(p.text_dim)),
            ]));
            continue;
        }

        lines.push(Line::from(vec![
            Span::styled(format!("  {} ", marker), Style::new().fg(color)),
            Span::styled(label.to_string(), Style::new().fg(color).bold()),
        ]));

        if msg.content.is_empty() {
            continue;
        }

        // Diff-aware rendering for tool_result messages
        let is_diff_content = msg.role == "tool_result"
            && (msg.content.contains("\n+") || msg.content.contains("\n-"))
            && msg.content.contains("---");

        for raw_line in msg.content.lines() {
            if raw_line.is_empty() {
                lines.push(Line::from(""));
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
            } else {
                for w in wrap_text(raw_line, content_width) {
                    lines.push(render_line_with_commands(&w, p));
                }
            }
        }

        let is_last = idx == app.messages.len() - 1;
        if app.state == AppState::Streaming && msg.role == "assistant" && is_last {
            if app.frame_count % 20 < 10 {
                lines.push(Line::from(Span::styled(
                    "    \u{258d}",
                    Style::new().fg(p.accent),
                )));
            } else {
                lines.push(Line::from(Span::styled("     ", Style::new().fg(p.accent))));
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
        "tool_call" => ("\u{25b8}", "tool", p.warning),
        "tool_result" => ("\u{25c7}", "result", p.success),
        "error" => ("\u{2716}", "error", p.error),
        _ => ("\u{25cf}", role, p.text_dim),
    }
}

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

fn welcome_lines(p: &Palette) -> Vec<Line<'static>> {
    let logo_lines = [
        r"            __                  ",
        r"  ___  ____/ /__  ___  ___  ___ ",
        r" / _ \/ __/ / _ \/ _ \/ _ \/ _ \",
        r"/ .__/ /_/_/\___/\___/\___/ .__/",
        r"\_/  \__/  openLoom      /_/    ",
    ];

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(""));

    for logo in &logo_lines {
        lines.push(Line::from(Span::styled(
            format!("  {}", logo),
            Style::new().fg(p.accent),
        )));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  Local-first AI assistant with cognitive memory.",
        Style::new().fg(p.text_dim),
    )));
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("  Enter", Style::new().fg(p.text).bold()),
        Span::styled(" send  ", Style::new().fg(p.text_dim)),
        Span::styled("Shift+Enter", Style::new().fg(p.text).bold()),
        Span::styled(" newline  ", Style::new().fg(p.text_dim)),
        Span::styled("Ctrl+C \u{00d7}2", Style::new().fg(p.text).bold()),
        Span::styled(" exit", Style::new().fg(p.text_dim)),
    ]));
    lines.push(Line::from(vec![
        Span::styled("  Ctrl+G", Style::new().fg(p.text).bold()),
        Span::styled(" editor  ", Style::new().fg(p.text_dim)),
        Span::styled("\u{2191}/\u{2193}", Style::new().fg(p.text).bold()),
        Span::styled(" history  ", Style::new().fg(p.text_dim)),
        Span::styled("Tab", Style::new().fg(p.text).bold()),
        Span::styled(" autocomplete  ", Style::new().fg(p.text_dim)),
        Span::styled("/help", Style::new().fg(p.accent).bold()),
        Span::styled(" commands", Style::new().fg(p.text_dim)),
    ]));

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

    // During streaming/waiting, show activity status instead of model name
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
            let tokens = app.stream_tokens_count;
            left_parts.push(format!(
                "Streaming\u{2026} ({} \u{00b7} \u{2193} {} tokens)",
                elapsed_str, tokens
            ));
        }
        _ => {
            left_parts.push(app.status.model.clone());
        }
    }

    if matches!(app.state, AppState::Idle | AppState::Overlay) {
        if !app.status.git_branch.is_empty() {
            left_parts.push(app.status.git_branch.clone());
        }
        if app.status.context_max > 0 {
            left_parts.push(format_context_size(app.status.context_max));
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
    let dot_and_space = 3; // " X "
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
    ("/config get", "Get config values"),
    ("/config set", "Set config values"),
];

/// Returns the filtered list of matching commands for the current input.
/// Used by both render and input for consistent palette behavior.
pub fn palette_matches(input: &str) -> Vec<(&'static str, &'static str)> {
    if !input.starts_with('/') || input.starts_with("//") {
        return Vec::new();
    }

    let typed = &input[1..]; // strip leading /
    let typed_lower = typed.to_lowercase();

    // If typed exactly matches a command, don't show palette (user already selected)
    let has_exact = SLASH_COMMANDS
        .iter()
        .any(|(cmd, _)| cmd[1..].eq_ignore_ascii_case(typed_lower.trim()));
    if has_exact && !typed_lower.trim().is_empty() {
        return Vec::new();
    }

    SLASH_COMMANDS
        .iter()
        .filter(|(cmd, _)| {
            let full = &cmd[1..]; // e.g. "help", "theme dark"
            full.starts_with(&typed_lower) || typed_lower.is_empty()
        })
        .map(|(cmd, desc)| (*cmd, *desc))
        .collect()
}

pub fn draw_command_palette(f: &mut Frame, app: &App, input_area: Rect, p: &Palette) {
    let current_input = app.input.lines().first().map(|s| s.as_str()).unwrap_or("");
    let matches = palette_matches(current_input);

    if matches.is_empty() {
        return;
    }

    let total_items = matches.len();
    let visible_count = total_items.min(10);
    let max_height = visible_count as u16;

    let palette_width = (input_area.width).min(52);
    let x = input_area.x + 1;
    let y = input_area.y.saturating_sub(max_height);

    if y < 1 {
        return;
    }

    let palette_area = Rect::new(x, y, palette_width, max_height);

    f.render_widget(Clear, palette_area);
    f.render_widget(
        Block::default()
            .borders(Borders::NONE)
            .style(Style::new().bg(p.surface)),
        palette_area,
    );

    // Scroll the palette view so the selected item is always visible
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

    // Show scroll indicator if there are more items than visible
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

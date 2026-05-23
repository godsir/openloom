use crossterm::event::KeyCode;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style, Stylize},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph},
};

use super::{Overlay, OverlayResult};

const ACCENT: Color = Color::Rgb(110, 160, 255);
const DIM: Color = Color::Rgb(102, 102, 102);
const WARN: Color = Color::Rgb(214, 174, 60);
const GREEN: Color = Color::Rgb(72, 199, 142);
const SURFACE: Color = Color::Rgb(24, 24, 24);

pub struct HelpOverlay {
    scroll: u16,
}

impl HelpOverlay {
    pub fn new() -> Self {
        Self { scroll: 0 }
    }

    fn content(&self) -> Vec<Line<'static>> {
        vec![
            Line::from(Span::styled(
                " Slash Commands",
                Style::new().fg(ACCENT).bold(),
            )),
            Line::from(""),
            cmd_line("/help, /h", "Show this help"),
            cmd_line("/model, /m", "Show current model"),
            cmd_line("/cost", "Show token usage and estimated cost"),
            cmd_line("/clear, /cls", "Clear all messages"),
            cmd_line("/theme dark|light", "Switch color theme"),
            cmd_line("/session new|list", "Create or list sessions"),
            cmd_line("/memory persona|events|cognitions|search", "Query memory"),
            cmd_line("/skills list", "List registered skills"),
            cmd_line("/skills invoke <name>", "Invoke a skill by name"),
            cmd_line("/<skill-name>", "Invoke external skill directly"),
            cmd_line("/config get|set", "View or modify config"),
            cmd_line("/mode", "Show/switch agent mode"),
            cmd_line("/mode chat|plan|code|asst", "Switch mode"),
            cmd_line("/think", "Show/set thinking level"),
            cmd_line("/think none|low|mid|high|max", "Set extended thinking"),
            Line::from(""),
            Line::from(Span::styled(" Modes", Style::new().fg(GREEN).bold())),
            Line::from(""),
            key_line("Ctrl+M", "Cycle to next mode"),
            Line::from(Span::styled(
                "  chat         Pure conversation, no tools",
                Style::new().fg(DIM),
            )),
            Line::from(Span::styled(
                "  plan         Read-only exploration",
                Style::new().fg(DIM),
            )),
            Line::from(Span::styled(
                "  code         Full agent loop + tools (default)",
                Style::new().fg(DIM),
            )),
            Line::from(Span::styled(
                "  assistant    General helper + memory + skills",
                Style::new().fg(DIM),
            )),
            Line::from(""),
            Line::from(Span::styled(
                " Skills & Plugins",
                Style::new().fg(GREEN).bold(),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  Skills are loaded from:",
                Style::new().fg(DIM),
            )),
            Line::from(Span::styled(
                "    <data_dir>/plugins/*/skills/*/SKILL.md",
                Style::new().fg(DIM),
            )),
            Line::from(Span::styled(
                "    <cwd>/.loom/skills/*/SKILL.md",
                Style::new().fg(DIM),
            )),
            Line::from(""),
            Line::from(Span::styled(
                " Project Instructions (loom.md)",
                Style::new().fg(GREEN).bold(),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  Place loom.md in project root for per-project context.",
                Style::new().fg(DIM),
            )),
            Line::from(Span::styled(
                "  Global: <data_dir>/loom.md",
                Style::new().fg(DIM),
            )),
            Line::from(""),
            Line::from(Span::styled(" Keybindings", Style::new().fg(WARN).bold())),
            Line::from(""),
            key_line("Enter", "Send message (or select palette item)"),
            key_line("Shift+Enter", "Insert newline"),
            key_line("Tab", "Autocomplete / cycle palette"),
            key_line("Ctrl+C", "Cancel stream, or quit (press twice)"),
            key_line("Ctrl+G", "Open $EDITOR for long input"),
            key_line("Ctrl+L", "Redraw screen"),
            key_line("Ctrl+J", "Insert newline (alternative)"),
            key_line("Ctrl+R", "Search input history"),
            key_line("\u{2191}/\u{2193}", "History nav (or palette nav)"),
            key_line("PageUp/Down", "Scroll message history"),
            key_line("Esc", "Cancel stream / dismiss overlay"),
            Line::from(""),
            Line::from(Span::styled(" Overlay Keys", Style::new().fg(GREEN).bold())),
            Line::from(""),
            key_line("Esc / q", "Close overlay"),
            key_line("j/k", "Scroll up/down"),
            key_line("A/D/S/C", "Approve / Deny / Session / Cancel (approval)"),
            Line::from(""),
            Line::from(Span::styled(
                " Tip: Start with // to send a literal /message",
                Style::new().fg(DIM),
            )),
        ]
    }
}

fn cmd_line(cmd: &str, desc: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("  {:32}", cmd), Style::new().fg(ACCENT)),
        Span::styled(desc.to_string(), Style::new().fg(DIM)),
    ])
}

fn key_line(key: &str, desc: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("  {:16}", key), Style::new().fg(ACCENT)),
        Span::styled(desc.to_string(), Style::new().fg(DIM)),
    ])
}

impl Overlay for HelpOverlay {
    fn draw(&self, f: &mut Frame, area: Rect) {
        f.render_widget(Clear, area);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::new().fg(ACCENT).bg(SURFACE))
            .style(Style::new().bg(SURFACE))
            .title_top(Span::styled(" Help ", Style::new().fg(ACCENT).bold()))
            .title_bottom(Span::styled(
                " \u{2191}\u{2193} scroll  Esc close ",
                Style::new().fg(DIM),
            ));

        let inner = block.inner(area);
        f.render_widget(block, area);

        let content = self.content();
        let visible_count = inner.height as usize;
        let max_scroll = content.len().saturating_sub(visible_count) as u16;
        let visible_start = (self.scroll.min(max_scroll)) as usize;
        let visible_end = (visible_start + visible_count).min(content.len());
        let visible: Vec<Line> = content[visible_start..visible_end].to_vec();

        let para = Paragraph::new(Text::from(visible));
        f.render_widget(para, inner);
    }

    fn handle_key(&mut self, key: KeyCode) -> OverlayResult {
        match key {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?') => OverlayResult::Dismiss,
            KeyCode::Up | KeyCode::Char('k') => {
                self.scroll = self.scroll.saturating_sub(1);
                OverlayResult::Consumed
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.scroll = self.scroll.saturating_add(1);
                OverlayResult::Consumed
            }
            KeyCode::PageUp => {
                self.scroll = self.scroll.saturating_sub(15);
                OverlayResult::Consumed
            }
            KeyCode::PageDown => {
                self.scroll = self.scroll.saturating_add(15);
                OverlayResult::Consumed
            }
            KeyCode::Home => {
                self.scroll = 0;
                OverlayResult::Consumed
            }
            KeyCode::End => {
                self.scroll = u16::MAX;
                OverlayResult::Consumed
            }
            _ => OverlayResult::Consumed,
        }
    }

    fn context(&self) -> &str {
        "help"
    }
}

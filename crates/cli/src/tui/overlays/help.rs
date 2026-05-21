use crossterm::event::KeyCode;
use ratatui::{
    layout::Rect,
    style::{Style, Stylize},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use super::{Overlay, OverlayResult};

pub struct HelpOverlay {
    scroll: u16,
}

impl HelpOverlay {
    pub fn new() -> Self {
        Self { scroll: 0 }
    }

    fn content(&self) -> Vec<Line<'static>> {
        let accent = ratatui::style::Color::Rgb(99, 150, 240);
        let dim = ratatui::style::Color::Rgb(140, 140, 155);
        let warn = ratatui::style::Color::Rgb(220, 180, 60);
        let green = ratatui::style::Color::Rgb(80, 200, 120);

        vec![
            Line::from(Span::styled(" Slash Commands", Style::new().fg(accent).bold())),
            Line::from(""),
            cmd_line("/help, /h", "Show this help", accent, dim),
            cmd_line("/model, /m", "Show current model", accent, dim),
            cmd_line("/cost", "Show token usage and estimated cost", accent, dim),
            cmd_line("/clear, /cls", "Clear all messages", accent, dim),
            cmd_line("/theme dark|light", "Switch color theme", accent, dim),
            cmd_line("/session new|list", "Create or list sessions", accent, dim),
            cmd_line("/memory persona|events|cognitions|search", "Query memory store", accent, dim),
            cmd_line("/skills list", "List registered skills", accent, dim),
            cmd_line("/config get|set", "View or modify config", accent, dim),
            Line::from(""),
            Line::from(Span::styled(" Global Keys", Style::new().fg(warn).bold())),
            Line::from(""),
            key_line("Ctrl+C", "Quit (or cancel streaming, then quit)", accent, dim),
            key_line("Ctrl+L", "Redraw screen", accent, dim),
            key_line("PageUp/Down", "Scroll message history", accent, dim),
            Line::from(""),
            Line::from(Span::styled(" Input Keys", Style::new().fg(green).bold())),
            Line::from(""),
            key_line("Enter", "Send message", accent, dim),
            key_line("Shift+Enter", "Insert newline", accent, dim),
            key_line("Ctrl+J", "Insert newline (alternative)", accent, dim),
            key_line("Up/Down", "Navigate input history", accent, dim),
            key_line("Ctrl+R", "Search input history (soon)", accent, dim),
            key_line("Ctrl+G", "Open $EDITOR for long input (soon)", accent, dim),
            Line::from(""),
            Line::from(Span::styled(" Streaming Keys", Style::new().fg(green).bold())),
            Line::from(""),
            key_line("Ctrl+C", "Cancel streaming (first press)", accent, dim),
            key_line("Esc", "Cancel streaming", accent, dim),
            Line::from(""),
            Line::from(Span::styled(" Overlay Keys", Style::new().fg(green).bold())),
            Line::from(""),
            key_line("Esc", "Dismiss overlay", accent, dim),
            key_line("Enter", "Confirm selection", accent, dim),
            key_line("A/D/S/C", "Approve / Deny / Approve-Session / Cancel", accent, dim),
            key_line("Up/Down/j/k", "Scroll in diff/help", accent, dim),
            Line::from(""),
            Line::from(Span::styled(" // Tip: Start message with // to send a literal /message", Style::new().fg(dim))),
        ]
    }
}

fn cmd_line(cmd: &str, desc: &str, accent: ratatui::style::Color, dim: ratatui::style::Color) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("  {:28}", cmd), Style::new().fg(accent)),
        Span::styled(desc.to_string(), Style::new().fg(dim)),
    ])
}

fn key_line(key: &str, desc: &str, accent: ratatui::style::Color, dim: ratatui::style::Color) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("  {:16}", key), Style::new().fg(accent)),
        Span::styled(desc.to_string(), Style::new().fg(dim)),
    ])
}

impl Overlay for HelpOverlay {
    fn draw(&self, f: &mut Frame, area: Rect) {
        f.render_widget(Clear, area);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::new().fg(ratatui::style::Color::Rgb(99, 150, 240)))
            .title_top(Span::styled(
                " Help ",
                Style::new().fg(ratatui::style::Color::Rgb(99, 150, 240)).bold(),
            ))
            .title_bottom(Span::styled(
                " ↑↓ scroll  Esc close ",
                Style::new().fg(ratatui::style::Color::Rgb(120, 120, 140)),
            ));

        let inner = block.inner(area);
        f.render_widget(block, area);

        let content = self.content();
        let visible_count = inner.height.saturating_sub(1) as usize;
        let max_scroll = content.len().saturating_sub(visible_count).max(0) as u16;
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
            _ => OverlayResult::Consumed,
        }
    }

    fn context(&self) -> &str {
        "help"
    }
}

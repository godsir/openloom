use crossterm::event::KeyCode;
use ratatui::{
    layout::Rect,
    style::{Style, Stylize},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use super::{Overlay, OverlayResult};

pub struct DiffViewer {
    pub filename: String,
    lines: Vec<DiffLine>,
    scroll: u16,
}

enum DiffLine {
    Header(String),
    Add(String),
    Del(String),
    Context(String),
    Hunk(String),
}

impl DiffViewer {
    pub fn new(filename: String, diff_text: &str) -> Self {
        let lines = Self::parse_diff(diff_text);
        Self {
            filename,
            lines,
            scroll: 0,
        }
    }

    fn parse_diff(text: &str) -> Vec<DiffLine> {
        text.lines()
            .map(|line| {
                if line.starts_with("+++") || line.starts_with("---") {
                    DiffLine::Header(line.to_string())
                } else if line.starts_with("@@") {
                    DiffLine::Hunk(line.to_string())
                } else if line.starts_with('+') {
                    DiffLine::Add(line.to_string())
                } else if line.starts_with('-') {
                    DiffLine::Del(line.to_string())
                } else {
                    DiffLine::Context(line.to_string())
                }
            })
            .collect()
    }
}

impl Overlay for DiffViewer {
    fn draw(&self, f: &mut Frame, area: Rect) {
        f.render_widget(Clear, area);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::new().fg(ratatui::style::Color::Rgb(99, 150, 240)))
            .title_top(Span::styled(
                format!(" Diff: {} ", self.filename),
                Style::new().fg(ratatui::style::Color::Rgb(99, 150, 240)).bold(),
            ))
            .title_bottom(Span::styled(
                " up/down scroll  Esc close ",
                Style::new().fg(ratatui::style::Color::Rgb(120, 120, 140)),
            ));

        let inner = block.inner(area);
        f.render_widget(block, area);

        let visible_count = inner.height.saturating_sub(2) as usize;
        let max_scroll = self.lines.len().saturating_sub(visible_count).max(0) as u16;
        let visible_start = (self.scroll.min(max_scroll)) as usize;
        let visible_end = (visible_start + visible_count).min(self.lines.len());

        let visible_lines: Vec<Line> = self.lines[visible_start..visible_end]
            .iter()
            .enumerate()
            .map(|(i, dl)| {
                let line_num = visible_start + i + 1;
                let num = Span::styled(
                    format!("{:4} ", line_num),
                    Style::new().fg(ratatui::style::Color::Rgb(100, 100, 110)),
                );
                match dl {
                    DiffLine::Header(text) => Line::from(vec![
                        num,
                        Span::styled(text.clone(), Style::new().fg(ratatui::style::Color::Rgb(200, 200, 100)).bold()),
                    ]),
                    DiffLine::Hunk(text) => Line::from(vec![
                        num,
                        Span::styled(text.clone(), Style::new().fg(ratatui::style::Color::Rgb(120, 180, 220))),
                    ]),
                    DiffLine::Add(text) => Line::from(vec![
                        num,
                        Span::styled(text.clone(), Style::new().fg(ratatui::style::Color::Rgb(80, 200, 120)).bg(ratatui::style::Color::Rgb(20, 50, 25))),
                    ]),
                    DiffLine::Del(text) => Line::from(vec![
                        num,
                        Span::styled(text.clone(), Style::new().fg(ratatui::style::Color::Rgb(220, 80, 80)).bg(ratatui::style::Color::Rgb(50, 20, 20))),
                    ]),
                    DiffLine::Context(text) => Line::from(vec![
                        num,
                        Span::styled(text.clone(), Style::new().fg(ratatui::style::Color::Rgb(200, 200, 210))),
                    ]),
                }
            })
            .collect();

        let para = Paragraph::new(Text::from(visible_lines));
        f.render_widget(para, inner);
    }

    fn handle_key(&mut self, key: KeyCode) -> OverlayResult {
        match key {
            KeyCode::Esc | KeyCode::Char('q') => OverlayResult::Dismiss,
            KeyCode::Up | KeyCode::Char('k') => {
                self.scroll = self.scroll.saturating_sub(1);
                OverlayResult::Consumed
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.scroll = self.scroll.saturating_add(1);
                OverlayResult::Consumed
            }
            KeyCode::PageUp => {
                self.scroll = self.scroll.saturating_sub(20);
                OverlayResult::Consumed
            }
            KeyCode::PageDown => {
                self.scroll = self.scroll.saturating_add(20);
                OverlayResult::Consumed
            }
            KeyCode::Home => {
                self.scroll = 0;
                OverlayResult::Consumed
            }
            KeyCode::End => {
                self.scroll = u16::MAX; // clamped in draw
                OverlayResult::Consumed
            }
            _ => OverlayResult::Consumed,
        }
    }

    fn context(&self) -> &str {
        "diff"
    }
}

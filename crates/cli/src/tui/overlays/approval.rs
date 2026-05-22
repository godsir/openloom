use crossterm::event::KeyCode;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Style, Stylize},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use super::{Overlay, OverlayResult};

#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(dead_code)]
pub enum ApprovalChoice {
    Approve,
    Deny,
    ApproveSession,
    Cancel,
}

#[allow(dead_code)]
pub struct ApprovalOverlay {
    pub title: String,
    pub message: String,
    pub selected: usize,
    options: Vec<(char, &'static str, ApprovalChoice)>,
    confirmed: Option<ApprovalChoice>,
}

#[allow(dead_code)]
impl ApprovalOverlay {
    pub fn new(title: String, message: String) -> Self {
        Self {
            title,
            message,
            selected: 0,
            options: vec![
                ('A', "Approve", ApprovalChoice::Approve),
                ('D', "Deny", ApprovalChoice::Deny),
                ('S', "Approve Session", ApprovalChoice::ApproveSession),
                ('C', "Cancel", ApprovalChoice::Cancel),
            ],
            confirmed: None,
        }
    }

    pub fn confirmed_choice(&self) -> Option<ApprovalChoice> {
        self.confirmed
    }
}

impl Overlay for ApprovalOverlay {
    fn draw(&self, f: &mut Frame, area: Rect) {
        // Clear the background area first
        f.render_widget(Clear, area);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::new().fg(ratatui::style::Color::Rgb(220, 180, 60)))
            .title_top(Span::styled(
                format!(" ⚠ {} ", self.title),
                Style::new()
                    .fg(ratatui::style::Color::Rgb(220, 180, 60))
                    .bold(),
            ));

        let inner = block.inner(area);
        f.render_widget(block, area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(3),
                Constraint::Length(self.options.len() as u16 + 2),
            ])
            .split(inner);

        // Message
        let msg = Paragraph::new(Text::from(self.message.as_str())).wrap(Wrap { trim: true });
        f.render_widget(msg, chunks[0]);

        // Options
        let option_lines: Vec<Line> = self
            .options
            .iter()
            .enumerate()
            .map(|(i, (key, label, _))| {
                let prefix = if i == self.selected { "▶ " } else { "  " };
                let style = if i == self.selected {
                    Style::new()
                        .fg(ratatui::style::Color::Rgb(99, 150, 240))
                        .bold()
                } else {
                    Style::new().fg(ratatui::style::Color::Rgb(180, 180, 190))
                };
                Line::from(vec![Span::styled(
                    format!("{}[{}] {}", prefix, key, label),
                    style,
                )])
            })
            .collect();

        let options_para = Paragraph::new(Text::from(option_lines));
        f.render_widget(options_para, chunks[1]);
    }

    fn handle_key(&mut self, key: KeyCode) -> OverlayResult {
        match key {
            KeyCode::Left | KeyCode::Up => {
                self.selected = self.selected.saturating_sub(1);
                OverlayResult::Consumed
            }
            KeyCode::Right | KeyCode::Down | KeyCode::Tab => {
                self.selected = (self.selected + 1).min(self.options.len() - 1);
                OverlayResult::Consumed
            }
            KeyCode::Enter => {
                self.confirmed = Some(self.options[self.selected].2);
                OverlayResult::Dismiss
            }
            KeyCode::Char('a') | KeyCode::Char('A') => {
                self.confirmed = Some(ApprovalChoice::Approve);
                OverlayResult::Dismiss
            }
            KeyCode::Char('d') | KeyCode::Char('D') => {
                self.confirmed = Some(ApprovalChoice::Deny);
                OverlayResult::Dismiss
            }
            KeyCode::Char('s') | KeyCode::Char('S') => {
                self.confirmed = Some(ApprovalChoice::ApproveSession);
                OverlayResult::Dismiss
            }
            KeyCode::Char('c') | KeyCode::Char('C') => {
                self.confirmed = Some(ApprovalChoice::Cancel);
                OverlayResult::Dismiss
            }
            KeyCode::Esc => {
                self.confirmed = Some(ApprovalChoice::Cancel);
                OverlayResult::Dismiss
            }
            _ => OverlayResult::Consumed,
        }
    }

    fn context(&self) -> &str {
        "approval"
    }

    fn approval_result(&self) -> Option<bool> {
        self.confirmed.map(|c| matches!(c, ApprovalChoice::Approve | ApprovalChoice::ApproveSession))
    }
}

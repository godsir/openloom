pub mod approval;
pub mod diff;

use crossterm::event::KeyCode;
use ratatui::{layout::Rect, Frame};

/// Result of an overlay handling a key event.
pub enum OverlayResult {
    Consumed,
    Dismiss,
}

/// Trait for modal overlay UI elements.
pub trait Overlay {
    /// Draw the overlay within the given area.
    fn draw(&self, f: &mut Frame, area: Rect);
    /// Handle a key event. Return OverlayResult indicating what to do next.
    fn handle_key(&mut self, key: KeyCode) -> OverlayResult;
    /// Context string for keybinding resolution ("approval", "diff", "help").
    fn context(&self) -> &str;
}

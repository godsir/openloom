pub mod approval;
pub mod diff;
pub mod help;

use crossterm::event::KeyCode;
use ratatui::{Frame, layout::Rect};

/// Result of an overlay handling a key event.
pub enum OverlayResult {
    Consumed,
    Dismiss,
}

/// Trait for modal overlay UI elements.
#[allow(dead_code)]
pub trait Overlay {
    /// Draw the overlay within the given area.
    fn draw(&self, f: &mut Frame, area: Rect);
    /// Handle a key event. Return OverlayResult indicating what to do next.
    fn handle_key(&mut self, key: KeyCode) -> OverlayResult;
    /// Context string for keybinding resolution ("approval", "diff", "help").
    fn context(&self) -> &str;
    /// For approval overlays, return the user's decision (true = approved).
    /// Returns None for non-approval overlays or when no decision has been made.
    fn approval_result(&self) -> Option<bool> {
        None
    }
}

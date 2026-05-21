use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Action names used throughout the TUI
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Quit,
    Send,
    Newline,
    HistoryUp,
    HistoryDown,
    ScrollUp,
    ScrollDown,
    Redraw,
    CancelStream,
    HistorySearch,
    ExternalEditor,
    Autocomplete,
    DismissOverlay,
    ConfirmOverlay,
    NavigateLeft,
    NavigateRight,
    ToggleThinking,
    Noop,
}

/// Context determines which bindings are active
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum KeyContext {
    Global,
    Input,
    Streaming,
    Overlay,
}

#[derive(Debug, Clone)]
pub struct KeyBinding {
    pub key: KeyCode,
    pub modifiers: KeyModifiers,
    pub action: Action,
    pub context: KeyContext,
}

#[derive(Debug, Clone)]
pub struct ResolvedKeymap {
    bindings: Vec<KeyBinding>,
}

impl Default for ResolvedKeymap {
    fn default() -> Self {
        Self {
            bindings: default_bindings(),
        }
    }
}

impl ResolvedKeymap {
    pub fn resolve(&self, key: &KeyEvent, context: KeyContext) -> Action {
        // Check context-specific bindings first
        for binding in &self.bindings {
            if binding.context == context
                && binding.key == key.code
                && binding.modifiers == key.modifiers
            {
                return binding.action.clone();
            }
        }
        // Fall back to Global context
        for binding in &self.bindings {
            if binding.context == KeyContext::Global
                && binding.key == key.code
                && binding.modifiers == key.modifiers
            {
                return binding.action.clone();
            }
        }
        Action::Noop
    }
}

fn default_bindings() -> Vec<KeyBinding> {
    vec![
        // Global
        KeyBinding {
            key: KeyCode::Char('c'),
            modifiers: KeyModifiers::CONTROL,
            action: Action::Quit,
            context: KeyContext::Global,
        },
        KeyBinding {
            key: KeyCode::Char('l'),
            modifiers: KeyModifiers::CONTROL,
            action: Action::Redraw,
            context: KeyContext::Global,
        },
        KeyBinding {
            key: KeyCode::PageUp,
            modifiers: KeyModifiers::NONE,
            action: Action::ScrollUp,
            context: KeyContext::Global,
        },
        KeyBinding {
            key: KeyCode::PageDown,
            modifiers: KeyModifiers::NONE,
            action: Action::ScrollDown,
            context: KeyContext::Global,
        },
        // Input
        KeyBinding {
            key: KeyCode::Enter,
            modifiers: KeyModifiers::NONE,
            action: Action::Send,
            context: KeyContext::Input,
        },
        KeyBinding {
            key: KeyCode::Enter,
            modifiers: KeyModifiers::SHIFT,
            action: Action::Newline,
            context: KeyContext::Input,
        },
        KeyBinding {
            key: KeyCode::Tab,
            modifiers: KeyModifiers::NONE,
            action: Action::Autocomplete,
            context: KeyContext::Input,
        },
        KeyBinding {
            key: KeyCode::Char('j'),
            modifiers: KeyModifiers::CONTROL,
            action: Action::Newline,
            context: KeyContext::Input,
        },
        KeyBinding {
            key: KeyCode::Up,
            modifiers: KeyModifiers::NONE,
            action: Action::HistoryUp,
            context: KeyContext::Input,
        },
        KeyBinding {
            key: KeyCode::Down,
            modifiers: KeyModifiers::NONE,
            action: Action::HistoryDown,
            context: KeyContext::Input,
        },
        KeyBinding {
            key: KeyCode::Char('r'),
            modifiers: KeyModifiers::CONTROL,
            action: Action::HistorySearch,
            context: KeyContext::Input,
        },
        KeyBinding {
            key: KeyCode::Char('g'),
            modifiers: KeyModifiers::CONTROL,
            action: Action::ExternalEditor,
            context: KeyContext::Input,
        },
        KeyBinding {
            key: KeyCode::Char('o'),
            modifiers: KeyModifiers::CONTROL,
            action: Action::ToggleThinking,
            context: KeyContext::Global,
        },
        // Streaming
        KeyBinding {
            key: KeyCode::Char('c'),
            modifiers: KeyModifiers::CONTROL,
            action: Action::CancelStream,
            context: KeyContext::Streaming,
        },
        KeyBinding {
            key: KeyCode::Esc,
            modifiers: KeyModifiers::NONE,
            action: Action::CancelStream,
            context: KeyContext::Streaming,
        },
        // Overlay
        KeyBinding {
            key: KeyCode::Esc,
            modifiers: KeyModifiers::NONE,
            action: Action::DismissOverlay,
            context: KeyContext::Overlay,
        },
        KeyBinding {
            key: KeyCode::Enter,
            modifiers: KeyModifiers::NONE,
            action: Action::ConfirmOverlay,
            context: KeyContext::Overlay,
        },
        KeyBinding {
            key: KeyCode::Left,
            modifiers: KeyModifiers::NONE,
            action: Action::NavigateLeft,
            context: KeyContext::Overlay,
        },
        KeyBinding {
            key: KeyCode::Right,
            modifiers: KeyModifiers::NONE,
            action: Action::NavigateRight,
            context: KeyContext::Overlay,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyEvent;

    fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    #[test]
    fn test_input_enter_is_send() {
        let km = ResolvedKeymap::default();
        let ev = key(KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(km.resolve(&ev, KeyContext::Input), Action::Send);
    }

    #[test]
    fn test_global_ctrl_c_is_quit() {
        let km = ResolvedKeymap::default();
        let ev = key(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(km.resolve(&ev, KeyContext::Input), Action::Quit);
    }

    #[test]
    fn test_streaming_ctrl_c_is_cancel() {
        let km = ResolvedKeymap::default();
        let ev = key(KeyCode::Char('c'), KeyModifiers::CONTROL);
        // Streaming context has its own Ctrl+C -> CancelStream
        assert_eq!(km.resolve(&ev, KeyContext::Streaming), Action::CancelStream);
    }

    #[test]
    fn test_streaming_esc_is_cancel() {
        let km = ResolvedKeymap::default();
        let ev = key(KeyCode::Esc, KeyModifiers::NONE);
        assert_eq!(km.resolve(&ev, KeyContext::Streaming), Action::CancelStream);
    }

    #[test]
    fn test_unknown_key_is_noop() {
        let km = ResolvedKeymap::default();
        let ev = key(KeyCode::Char('x'), KeyModifiers::NONE);
        assert_eq!(km.resolve(&ev, KeyContext::Input), Action::Noop);
    }

    #[test]
    fn test_up_is_history_up() {
        let km = ResolvedKeymap::default();
        let ev = key(KeyCode::Up, KeyModifiers::NONE);
        assert_eq!(km.resolve(&ev, KeyContext::Input), Action::HistoryUp);
    }

    #[test]
    fn test_down_is_history_down() {
        let km = ResolvedKeymap::default();
        let ev = key(KeyCode::Down, KeyModifiers::NONE);
        assert_eq!(km.resolve(&ev, KeyContext::Input), Action::HistoryDown);
    }

    #[test]
    fn test_pageup_is_scroll_up() {
        let km = ResolvedKeymap::default();
        let ev = key(KeyCode::PageUp, KeyModifiers::NONE);
        assert_eq!(km.resolve(&ev, KeyContext::Global), Action::ScrollUp);
    }

    #[test]
    fn test_overlay_esc_is_dismiss() {
        let km = ResolvedKeymap::default();
        let ev = key(KeyCode::Esc, KeyModifiers::NONE);
        assert_eq!(km.resolve(&ev, KeyContext::Overlay), Action::DismissOverlay);
    }

    #[test]
    fn test_overlay_enter_is_confirm() {
        let km = ResolvedKeymap::default();
        let ev = key(KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(km.resolve(&ev, KeyContext::Overlay), Action::ConfirmOverlay);
    }

    #[test]
    fn test_shift_enter_is_newline() {
        let km = ResolvedKeymap::default();
        let ev = key(KeyCode::Enter, KeyModifiers::SHIFT);
        assert_eq!(km.resolve(&ev, KeyContext::Input), Action::Newline);
    }
}

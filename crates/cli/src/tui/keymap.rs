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
    DismissOverlay,
    ConfirmOverlay,
    NavigateLeft,
    NavigateRight,
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

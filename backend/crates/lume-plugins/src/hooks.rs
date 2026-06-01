//! Hook type definitions for openLoom v2 — Claude Code compatible.
//!
//! Defines the 14 hook events, 3 handler types, and the hooks.json config format.
//! The execution engine will be added in a future phase.

use anyhow::Result;
use serde::{Deserialize, Serialize};

// ============================================================================
// Hook Events
// ============================================================================

/// All hook events in the Claude Code lifecycle.
///
/// Plugins register handlers for specific events in `hooks/hooks.json`.
/// Each event fires at a well-defined point in the agent loop or session lifecycle.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum HookEvent {
    /// Fires before a tool executes. The handler can block or modify the call.
    PreToolUse,
    /// Fires after a tool completes successfully.
    PostToolUse,
    /// Fires after a tool fails (non-zero exit, exception, etc.).
    PostToolUseFailure,
    /// Fires when Claude requests user permission for an operation.
    PermissionRequest,
    /// Fires when the user submits a prompt (before processing).
    UserPromptSubmit,
    /// Fires when a system notification is generated.
    Notification,
    /// Fires when Claude is asked to stop (user interrupt).
    Stop,
    /// Fires when a sub-agent is spawned (before execution).
    SubagentStart,
    /// Fires when a sub-agent completes (success or failure).
    SubagentStop,
    /// Fires when a Claude Code session begins.
    SessionStart,
    /// Fires when a Claude Code session ends.
    SessionEnd,
    /// Fires when a teammate (parallel agent) becomes idle.
    TeammateIdle,
    /// Fires when a task completes.
    TaskCompleted,
    /// Fires before context compaction (summary generation).
    PreCompact,
}

// ============================================================================
// Hook Handler Types
// ============================================================================

/// The three types of hook handlers supported by Claude Code.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum HookHandlerType {
    /// Execute a shell command/script. The command string supports
    /// `${CLAUDE_PLUGIN_ROOT}` and `${PLUGIN_ROOT}` variables.
    Command,
    /// Inject a prompt into the model context at this lifecycle point.
    Prompt,
    /// Invoke a custom agent to handle the event.
    Agent,
}

// ============================================================================
// Hook Configuration
// ============================================================================

/// A single hook handler entry within a hook configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookHandler {
    /// The handler type.
    #[serde(rename = "type")]
    pub handler_type: HookHandlerType,

    /// Shell command to execute (for `Command` type).
    /// Supports `${CLAUDE_PLUGIN_ROOT}` and `${PLUGIN_ROOT}` expansion.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,

    /// Prompt text to inject (for `Prompt` type).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,

    /// Timeout in seconds for command execution (default: 30).
    #[serde(default = "default_timeout")]
    pub timeout: u64,
}

fn default_timeout() -> u64 {
    30
}

/// A hook entry in hooks.json — binds events to handlers with optional matchers.
///
/// Example hooks.json entry:
/// ```json
/// {
///   "event": "preToolUse",
///   "matcher": "Edit|Write",
///   "hooks": [{
///     "type": "command",
///     "command": "bash ${CLAUDE_PLUGIN_ROOT}/hooks/scripts/check.sh"
///   }]
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookEntry {
    /// The lifecycle event that triggers this hook.
    pub event: HookEvent,

    /// Optional regex matcher for filtering tool/event names.
    /// When absent, the hook fires for all instances of the event.
    /// When present (e.g. "Edit|Write"), only matching tool names trigger the hook.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matcher: Option<String>,

    /// One or more handlers to invoke when this event fires.
    #[serde(default)]
    pub hooks: Vec<HookHandler>,
}

/// Top-level hook configuration, typically loaded from `hooks/hooks.json`
/// in a plugin directory.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HookConfig {
    /// All registered hook entries.
    #[serde(default)]
    pub hooks: Vec<HookEntry>,
}

// ============================================================================
// HookConfig methods
// ============================================================================

impl HookConfig {
    /// Load hook configuration from a plugin directory's `hooks/hooks.json`.
    ///
    /// Returns `Ok(HookConfig::default())` if the file doesn't exist (not an error —
    /// hooks are optional). Returns `Err` only on parse failures.
    pub fn from_plugin_dir(plugin_dir: &std::path::Path) -> Result<Self> {
        let path = plugin_dir.join("hooks").join("hooks.json");
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(&path)?;
        let config: HookConfig = serde_json::from_str(&content)?;
        Ok(config)
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// Expand `${CLAUDE_PLUGIN_ROOT}` and `${PLUGIN_ROOT}` variables in a string.
///
/// Both variables are replaced with the provided plugin root directory path.
/// This allows portable paths in hook command strings and prompt templates.
///
/// # Example
/// ```
/// # use lume_plugins::hooks::expand_plugin_root;
/// let result = expand_plugin_root(
///     "bash ${CLAUDE_PLUGIN_ROOT}/scripts/check.sh",
///     "/home/user/.claude/plugins/my-plugin",
/// );
/// assert_eq!(result, "bash /home/user/.claude/plugins/my-plugin/scripts/check.sh");
/// ```
pub fn expand_plugin_root(s: &str, plugin_dir: &str) -> String {
    s.replace("${CLAUDE_PLUGIN_ROOT}", plugin_dir)
        .replace("${PLUGIN_ROOT}", plugin_dir)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_hook_config() {
        let json = r#"{
            "hooks": [{
                "event": "preToolUse",
                "matcher": "Edit|Write",
                "hooks": [{
                    "type": "command",
                    "command": "bash ${CLAUDE_PLUGIN_ROOT}/hooks/check.sh",
                    "timeout": 30
                }]
            }]
        }"#;
        let config: HookConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.hooks.len(), 1);
        let entry = &config.hooks[0];
        assert_eq!(entry.event, HookEvent::PreToolUse);
        assert_eq!(entry.matcher.as_deref(), Some("Edit|Write"));
        assert_eq!(entry.hooks.len(), 1);
        assert_eq!(entry.hooks[0].handler_type, HookHandlerType::Command);
        assert_eq!(
            entry.hooks[0].command.as_deref(),
            Some("bash ${CLAUDE_PLUGIN_ROOT}/hooks/check.sh")
        );
        assert_eq!(entry.hooks[0].timeout, 30);
    }

    #[test]
    fn test_hook_config_default_on_missing_file() {
        let config = HookConfig::from_plugin_dir(
            std::path::Path::new("/nonexistent_plugin_dir_12345"),
        )
        .unwrap();
        assert!(config.hooks.is_empty());
    }

    #[test]
    fn test_expand_plugin_root() {
        let result = expand_plugin_root(
            "bash ${CLAUDE_PLUGIN_ROOT}/scripts/test.sh arg1",
            "/home/plugins/my-plugin",
        );
        assert_eq!(result, "bash /home/plugins/my-plugin/scripts/test.sh arg1");
    }

    #[test]
    fn test_expand_plugin_root_both_vars() {
        let result = expand_plugin_root(
            "${CLAUDE_PLUGIN_ROOT}/a && ${PLUGIN_ROOT}/b",
            "/p",
        );
        assert_eq!(result, "/p/a && /p/b");
    }

    #[test]
    fn test_expand_plugin_root_no_var() {
        let result = expand_plugin_root("echo hello", "/p");
        assert_eq!(result, "echo hello");
    }

    #[test]
    fn test_hook_event_all_variants_deserialize() {
        let events = [
            "preToolUse",
            "postToolUse",
            "postToolUseFailure",
            "permissionRequest",
            "userPromptSubmit",
            "notification",
            "stop",
            "subagentStart",
            "subagentStop",
            "sessionStart",
            "sessionEnd",
            "teammateIdle",
            "taskCompleted",
            "preCompact",
        ];
        for name in &events {
            let json = format!(r#"{{"event":"{}","hooks":[]}}"#, name);
            let entry: HookEntry = serde_json::from_str(&json).unwrap();
            let event_str = format!("{:?}", entry.event);
            // Verify it parsed without panicking
            assert!(!event_str.is_empty());
        }
    }
}

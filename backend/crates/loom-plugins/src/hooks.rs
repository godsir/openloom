//! Hook type definitions for openLoom — agent lifecycle events and handlers.
//!
//! Defines 14 hook events, 3 handler types, and the hooks.json config format.
//! Compatible with Claude Code hook plugins (both array and map JSON formats).

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ============================================================================
// Hook Events
// ============================================================================

/// All hook events in the agent lifecycle.
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
    /// Fires when the agent requests user permission for an operation.
    PermissionRequest,
    /// Fires when the user submits a prompt (before processing).
    UserPromptSubmit,
    /// Fires when a system notification is generated.
    Notification,
    /// Fires when the agent is asked to stop (user interrupt).
    Stop,
    /// Fires when a sub-agent is spawned (before execution).
    SubagentStart,
    /// Fires when a sub-agent completes (success or failure).
    SubagentStop,
    /// Fires when an agent session begins.
    SessionStart,
    /// Fires when an agent session ends.
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

/// The three types of hook handlers supported by the hook system.
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
///
/// Accepts two JSON formats:
/// 1. **Array format** (our canonical format):
///    `{ "hooks": [ { "event": "PreToolUse", "matcher": "...", "hooks": [...] } ] }`
/// 2. **Claude Code map format** (events as object keys):
///    `{ "hooks": { "PreToolUse": [ { "matcher": "...", "hooks": [...] } ] } }`
#[derive(Debug, Clone, Serialize, Default)]
pub struct HookConfig {
    /// Optional human-readable description of this hook configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// All registered hook entries.
    #[serde(default)]
    pub hooks: Vec<HookEntry>,
}

// ============================================================================
// Deserialization helpers — accept both Claude Code map format and array format
// ============================================================================

/// Raw hook entry without the `event` field.
/// Used when parsing the map format where event names are the object keys.
#[derive(Debug, Clone, Deserialize)]
struct HookEntryRaw {
    #[serde(default)]
    pub matcher: Option<String>,
    #[serde(default)]
    pub hooks: Vec<HookHandler>,
}

/// Intermediate representation that accepts either the Claude Code map
/// format or the array format from a hooks.json file.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum HookConfigRaw {
    /// Claude Code format: hooks keyed by event name.
    /// `{ "description": "...", "hooks": { "PreToolUse": [ ... ] } }`
    Map {
        #[serde(default)]
        description: Option<String>,
        #[serde(default)]
        hooks: HashMap<String, Vec<HookEntryRaw>>,
    },
    /// Array format: each entry has an explicit "event" field.
    /// `{ "description": "...", "hooks": [ { "event": "PreToolUse", ... } ] }`
    Array {
        #[serde(default)]
        description: Option<String>,
        #[serde(default)]
        hooks: Vec<HookEntry>,
    },
}

/// Parse a hook event from a JSON object key. Handles both camelCase and PascalCase.
fn event_from_key(key: &str) -> Option<HookEvent> {
    match key.to_lowercase().as_str() {
        "pretooluse" => Some(HookEvent::PreToolUse),
        "posttooluse" => Some(HookEvent::PostToolUse),
        "posttoolusefailure" => Some(HookEvent::PostToolUseFailure),
        "permissionrequest" => Some(HookEvent::PermissionRequest),
        "userpromptsubmit" => Some(HookEvent::UserPromptSubmit),
        "notification" => Some(HookEvent::Notification),
        "stop" => Some(HookEvent::Stop),
        "subagentstart" => Some(HookEvent::SubagentStart),
        "subagentstop" => Some(HookEvent::SubagentStop),
        "sessionstart" => Some(HookEvent::SessionStart),
        "sessionend" => Some(HookEvent::SessionEnd),
        "teammateidle" => Some(HookEvent::TeammateIdle),
        "taskcompleted" => Some(HookEvent::TaskCompleted),
        "precompact" => Some(HookEvent::PreCompact),
        _ => None,
    }
}

impl From<HookConfigRaw> for HookConfig {
    fn from(raw: HookConfigRaw) -> Self {
        match raw {
            HookConfigRaw::Map { description, hooks } => {
                let mut entries = Vec::new();
                for (event_name, raw_entries) in hooks {
                    let Some(event) = event_from_key(&event_name) else {
                        tracing::warn!(
                            event_name = %event_name,
                            "unknown hook event in hooks.json, skipping entries"
                        );
                        continue;
                    };
                    for raw_entry in raw_entries {
                        entries.push(HookEntry {
                            event: event.clone(),
                            matcher: raw_entry.matcher,
                            hooks: raw_entry.hooks,
                        });
                    }
                }
                HookConfig {
                    description,
                    hooks: entries,
                }
            }
            HookConfigRaw::Array { description, hooks } => HookConfig { description, hooks },
        }
    }
}

impl<'de> Deserialize<'de> for HookConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = HookConfigRaw::deserialize(deserializer)?;
        Ok(HookConfig::from(raw))
    }
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
/// # Security
///
/// Hook command strings come from an *installed* plugin's `hooks.json` and are
/// **trusted-by-install**: the user opted into running this plugin's code, and
/// the hook model intentionally executes shell commands. The marketplace
/// validates entry/dir names upstream, but the plugin root path is still
/// interpolated into a string later handed to `sh -c` / `cmd /C`.
///
/// As bounded hardening, the substituted path is POSIX-shell single-quoted so a
/// directory name containing spaces or shell metacharacters cannot alter the
/// command's structure (e.g. a `foo; rm -rf ~` segment stays a literal path
/// component). Single-quoting concatenates correctly with following path
/// segments under `sh` (`'<root>'/scripts/x.sh`). This is not a full sandbox —
/// arbitrary command execution by a trusted plugin is inherent to hooks — it
/// only prevents the *path* value from being misinterpreted by the shell.
///
/// # Example
/// ```
/// # use loom_plugins::hooks::expand_plugin_root;
/// let result = expand_plugin_root(
///     "bash ${CLAUDE_PLUGIN_ROOT}/scripts/check.sh",
///     "/home/user/.claude/plugins/my-plugin",
/// );
/// assert_eq!(result, "bash '/home/user/.claude/plugins/my-plugin'/scripts/check.sh");
/// ```
///
/// Also supports the loom-native `${LOOM_PLUGIN_ROOT}` alias.
pub fn expand_plugin_root(s: &str, plugin_dir: &str) -> String {
    let quoted = shell_single_quote(plugin_dir);
    s.replace("${CLAUDE_PLUGIN_ROOT}", &quoted)
        .replace("${PLUGIN_ROOT}", &quoted)
        .replace("${LOOM_PLUGIN_ROOT}", &quoted)
}

/// POSIX-shell single-quote a value so it is treated as a single literal token.
///
/// Wraps the value in `'...'` and rewrites each embedded `'` as `'\''` (close
/// quote, escaped quote, reopen quote) — the standard way to embed a literal
/// single quote inside a single-quoted shell word.
fn shell_single_quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('\'');
    for ch in value.chars() {
        if ch == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(ch);
        }
    }
    out.push('\'');
    out
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
        let config =
            HookConfig::from_plugin_dir(std::path::Path::new("/nonexistent_plugin_dir_12345"))
                .unwrap();
        assert!(config.hooks.is_empty());
    }

    #[test]
    fn test_expand_plugin_root() {
        let result = expand_plugin_root(
            "bash ${CLAUDE_PLUGIN_ROOT}/scripts/test.sh arg1",
            "/home/plugins/my-plugin",
        );
        // The substituted path is single-quoted; it concatenates with the
        // following path segment under POSIX sh.
        assert_eq!(
            result,
            "bash '/home/plugins/my-plugin'/scripts/test.sh arg1"
        );
    }

    #[test]
    fn test_expand_plugin_root_both_vars() {
        let result = expand_plugin_root("${CLAUDE_PLUGIN_ROOT}/a && ${PLUGIN_ROOT}/b", "/p");
        assert_eq!(result, "'/p'/a && '/p'/b");
    }

    #[test]
    fn test_expand_plugin_root_no_var() {
        let result = expand_plugin_root("echo hello", "/p");
        assert_eq!(result, "echo hello");
    }

    #[test]
    fn test_expand_plugin_root_quotes_metacharacters() {
        // A plugin root containing shell metacharacters must stay a literal
        // path component and not break out of the command.
        let result = expand_plugin_root(
            "bash ${PLUGIN_ROOT}/run.sh",
            "/tmp/evil; rm -rf ~",
        );
        assert_eq!(result, "bash '/tmp/evil; rm -rf ~'/run.sh");
    }

    #[test]
    fn test_expand_plugin_root_escapes_embedded_quote() {
        // An embedded single quote is escaped via the '\'' idiom.
        let result = expand_plugin_root("${PLUGIN_ROOT}/x", "/a'b");
        assert_eq!(result, "'/a'\\''b'/x");
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

    #[test]
    fn test_deserialize_hook_config_map_format() {
        // Claude Code native format — events as object keys
        let json = r#"{
            "description": "Security guidance hooks",
            "hooks": {
                "PreToolUse": [{
                    "matcher": "Edit|Write|MultiEdit",
                    "hooks": [{
                        "type": "command",
                        "command": "python3 ${CLAUDE_PLUGIN_ROOT}/hooks/security_reminder_hook.py"
                    }]
                }]
            }
        }"#;
        let config: HookConfig = serde_json::from_str(json).unwrap();
        assert_eq!(
            config.description.as_deref(),
            Some("Security guidance hooks")
        );
        assert_eq!(config.hooks.len(), 1);
        let entry = &config.hooks[0];
        assert_eq!(entry.event, HookEvent::PreToolUse);
        assert_eq!(entry.matcher.as_deref(), Some("Edit|Write|MultiEdit"));
        assert_eq!(entry.hooks.len(), 1);
        assert_eq!(entry.hooks[0].handler_type, HookHandlerType::Command);
        assert_eq!(
            entry.hooks[0].command.as_deref(),
            Some("python3 ${CLAUDE_PLUGIN_ROOT}/hooks/security_reminder_hook.py")
        );
    }

    #[test]
    fn test_deserialize_hook_config_map_format_camelcase() {
        // Map format with camelCase event keys (as per HookEvent serialization)
        let json = r#"{
            "hooks": {
                "preToolUse": [{
                    "matcher": "Edit",
                    "hooks": [{ "type": "command", "command": "echo hi" }]
                }],
                "sessionStart": [{
                    "hooks": [{ "type": "prompt", "prompt": "Ready." }]
                }]
            }
        }"#;
        let config: HookConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.hooks.len(), 2);
        // Map keys are unordered, so check events are present in any order
        let events: Vec<&HookEvent> = config.hooks.iter().map(|h| &h.event).collect();
        assert!(
            events.contains(&&HookEvent::PreToolUse),
            "should contain PreToolUse"
        );
        assert!(
            events.contains(&&HookEvent::SessionStart),
            "should contain SessionStart"
        );
    }

    #[test]
    fn test_deserialize_hook_config_map_format_mixed_case() {
        // PascalCase keys — should be normalized to lowercase for matching
        let json = r#"{
            "hooks": {
                "postToolUse": [{ "hooks": [{ "type": "command", "command": "echo done" }] }],
                "PreToolUse": [{ "hooks": [{ "type": "command", "command": "echo check" }] }],
                "SESSIONSTART": [{ "hooks": [{ "type": "command", "command": "echo init" }] }]
            }
        }"#;
        let config: HookConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.hooks.len(), 3);
    }

    #[test]
    fn test_deserialize_hook_config_map_format_unknown_event() {
        let json = r#"{
            "hooks": {
                "preToolUse": [{ "hooks": [{ "type": "command", "command": "echo ok" }] }],
                "UnknownEventXYZ": [{ "hooks": [{ "type": "command", "command": "echo bad" }] }]
            }
        }"#;
        // Unknown events are skipped with a warning, not a hard error
        let config: HookConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.hooks.len(), 1);
        assert_eq!(config.hooks[0].event, HookEvent::PreToolUse);
    }
}

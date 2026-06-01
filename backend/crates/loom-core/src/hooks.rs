//! Hook execution engine — compiles HookConfig into a runtime registry and fires
//! events at trigger points in the orchestrator and agent loop.

use std::collections::HashMap;
use std::sync::Arc;

use lume_plugins::hooks::{expand_plugin_root, HookEntry, HookEvent, HookHandlerType};
use lume_plugins::PluginManager;
use regex::Regex;
use tokio::sync::RwLock;
use tracing;

// ── Compiled entry ──────────────────────────────────────────────────────────

/// A pre-compiled hook entry ready for fast matching at runtime.
#[derive(Clone)]
struct CompiledHook {
    /// The original hook entry (handlers, event type).
    entry: HookEntry,
    /// Pre-compiled matcher regex, or None (matches everything).
    /// Wrapped in Arc so CompiledHook is Clone.
    matcher: Option<Arc<Regex>>,
    /// Plugin directory for `${CLAUDE_PLUGIN_ROOT}` expansion.
    plugin_dir: String,
}

impl CompiledHook {
    /// Returns true if this hook matches the given subject (tool name, subagent name, etc.).
    fn matches_subject(&self, subject: Option<&str>) -> bool {
        match (&self.matcher, subject) {
            (None, _) => true,
            (Some(_), None) => false,
            (Some(re), Some(s)) => re.is_match(s),
        }
    }
}

// ── Hook context (input) ────────────────────────────────────────────────────

/// Context passed to hook handlers at fire time. All fields are optional;
/// each event populates the relevant subset.
#[derive(Debug, Clone, Default)]
pub struct HookContext {
    pub session_id: String,
    pub agent_id: String,
    pub tool_name: Option<String>,
    pub tool_args: Option<serde_json::Value>,
    pub tool_result: Option<String>,
    pub tool_success: Option<bool>,
    pub user_message: Option<String>,
    pub subagent_name: Option<String>,
    pub subagent_result: Option<String>,
    /// Buffer that Prompt-type handlers append to. Caller reads this after fire().
    pub prompt_injections: Vec<String>,
}

// ── Hook result (output) ────────────────────────────────────────────────────

/// Summary of what happened when hooks fired. Used for tracing and debugging.
#[derive(Debug, Clone, Default)]
pub struct HookFireResult {
    pub commands_run: usize,
    pub commands_failed: usize,
    pub prompts_injected: usize,
    pub agents_scheduled: usize,
}

// ── Registry ────────────────────────────────────────────────────────────────

/// Runtime hook registry compiled from plugin hook configs.
///
/// Owned by the Orchestrator. Compiled once at startup and rebuilt when plugins
/// are installed/removed. Thread-safe via Arc<RwLock<>>.
#[derive(Clone)]
pub struct HookRegistry {
    inner: Arc<RwLock<HookRegistryInner>>,
}

#[derive(Clone)]
struct HookRegistryInner {
    /// event -> Vec<CompiledHook> (ordered by registration order)
    hooks: HashMap<HookEvent, Vec<CompiledHook>>,
}

impl HookRegistry {
    /// Create an empty hook registry.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(HookRegistryInner {
                hooks: HashMap::new(),
            })),
        }
    }

    /// Load all hook configs from discovered plugins.
    ///
    /// Walks each plugin directory, loads `hooks/hooks.json` if present,
    /// and compiles the hook entries into the runtime registry.
    /// Invalid matchers are logged and treated as match-all.
    pub async fn load_from_plugins(plugin_manager: &PluginManager) -> Self {
        let mut hooks: HashMap<HookEvent, Vec<CompiledHook>> = HashMap::new();

        for (plugin_dir, _name) in plugin_manager.plugin_dirs() {
            let config = match lume_plugins::hooks::HookConfig::from_plugin_dir(plugin_dir) {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!(
                        dir = %plugin_dir.display(),
                        error = %e,
                        "failed to load hook config from plugin dir"
                    );
                    continue;
                }
            };

            let dir_str = plugin_dir.to_string_lossy().to_string();
            let loaded_count = config.hooks.len();

            for entry in config.hooks {
                let matcher = entry.matcher.as_deref().and_then(|m| match Regex::new(m) {
                    Ok(re) => Some(Arc::new(re)),
                    Err(e) => {
                        tracing::warn!(
                            matcher = %m,
                            error = %e,
                            "invalid hook matcher regex, treating as match-all"
                        );
                        None
                    }
                });

                let event = entry.event.clone();
                let compiled = CompiledHook {
                    entry,
                    matcher,
                    plugin_dir: dir_str.clone(),
                };
                hooks.entry(event).or_default().push(compiled);
            }

            if loaded_count > 0 {
                tracing::info!(
                    dir = %plugin_dir.display(),
                    count = loaded_count,
                    "loaded hook entries from plugin"
                );
            }
        }

        let total: usize = hooks.values().map(|v| v.len()).sum();
        let events: usize = hooks.len();
        tracing::info!(total_hooks = total, event_types = events, "hook registry compiled");

        Self {
            inner: Arc::new(RwLock::new(HookRegistryInner { hooks })),
        }
    }

    /// Replace the entire registry with a new set of hooks.
    /// Called when plugins are installed/removed to rebuild the runtime.
    pub async fn reload(&self, plugin_manager: &PluginManager) {
        let new_registry = Self::load_from_plugins(plugin_manager).await;
        *self.inner.write().await = new_registry.inner.read().await.clone();
    }

    /// Fire all hooks registered for the given event.
    ///
    /// - `event`: the lifecycle event that occurred
    /// - `subject`: optional subject name (tool name, subagent name, etc.) for
    ///   matcher filtering
    /// - `ctx`: mutable context — Prompt hook output is appended to
    ///   `ctx.prompt_injections`
    ///
    /// Returns a summary of what happened. Never panics — all handler errors
    /// are caught and logged.
    pub async fn fire(
        &self,
        event: &HookEvent,
        subject: Option<&str>,
        ctx: &mut HookContext,
    ) -> HookFireResult {
        let inner = self.inner.read().await;
        let entries = match inner.hooks.get(event) {
            Some(e) => e,
            None => return HookFireResult::default(),
        };

        let mut result = HookFireResult::default();

        for compiled in entries {
            if !compiled.matches_subject(subject) {
                continue;
            }

            for handler in &compiled.entry.hooks {
                match handler.handler_type {
                    HookHandlerType::Command => {
                        let Some(cmd) = &handler.command else {
                            continue;
                        };
                        result.commands_run += 1;
                        let expanded = expand_plugin_root(cmd, &compiled.plugin_dir);
                        let timeout = handler.timeout;
                        match Self::run_command(&expanded, timeout).await {
                            Ok((stdout, _stderr, exit_code)) => {
                                if exit_code == 0 {
                                    tracing::trace!(
                                        event = ?event,
                                        command = %cmd,
                                        stdout = %stdout,
                                        "hook command succeeded"
                                    );
                                } else {
                                    result.commands_failed += 1;
                                    tracing::warn!(
                                        event = ?event,
                                        command = %cmd,
                                        exit_code,
                                        "hook command failed"
                                    );
                                }
                            }
                            Err(e) => {
                                result.commands_failed += 1;
                                tracing::warn!(
                                    event = ?event,
                                    command = %cmd,
                                    error = %e,
                                    "hook command execution error"
                                );
                            }
                        }
                    }
                    HookHandlerType::Prompt => {
                        let Some(prompt) = &handler.prompt else {
                            continue;
                        };
                        let expanded = expand_plugin_root(prompt, &compiled.plugin_dir);
                        ctx.prompt_injections.push(expanded);
                        result.prompts_injected += 1;
                    }
                    HookHandlerType::Agent => {
                        result.agents_scheduled += 1;
                        tracing::info!(
                            event = ?event,
                            plugin_dir = %compiled.plugin_dir,
                            "agent hook triggered — Orchestrator handles execution"
                        );
                    }
                }
            }
        }

        result
    }

    /// Return the number of registered hooks (all events combined).
    pub async fn hook_count(&self) -> usize {
        self.inner.read().await.hooks.values().map(|v| v.len()).sum()
    }

    /// Return the number of registered hook event types.
    pub async fn event_count(&self) -> usize {
        self.inner.read().await.hooks.len()
    }

    // ── Internal helpers ──────────────────────────────────────────────────

    /// Execute a shell command synchronously (on a blocking thread) with a
    /// timeout. Returns (stdout, stderr, exit_code).
    async fn run_command(cmd: &str, timeout_secs: u64) -> anyhow::Result<(String, String, i32)> {
        let cmd_owned = cmd.to_string();
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(timeout_secs),
            tokio::task::spawn_blocking(move || {
                let output = std::process::Command::new(if cfg!(windows) { "cmd" } else { "sh" })
                    .arg(if cfg!(windows) { "/C" } else { "-c" })
                    .arg(&cmd_owned)
                    .output();

                match output {
                    Ok(o) => {
                        let stdout = String::from_utf8_lossy(&o.stdout).to_string();
                        let stderr = String::from_utf8_lossy(&o.stderr).to_string();
                        Ok((stdout, stderr, o.status.code().unwrap_or(-1)))
                    }
                    Err(e) => Err(anyhow::anyhow!("command execution failed: {}", e)),
                }
            }),
        )
        .await;

        match result {
            Ok(Ok(inner)) => inner,
            Ok(Err(e)) => Err(anyhow::anyhow!("spawn_blocking failed: {}", e)),
            Err(_elapsed) => Err(anyhow::anyhow!(
                "hook command timed out after {}s",
                timeout_secs
            )),
        }
    }
}

impl Default for HookRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use lume_plugins::hooks::{HookConfig, HookHandler};

    #[tokio::test]
    async fn test_empty_registry_fire_returns_default() {
        let registry = HookRegistry::new();
        let mut ctx = HookContext::default();
        let result = registry
            .fire(&HookEvent::PreToolUse, Some("test_tool"), &mut ctx)
            .await;
        assert_eq!(result.commands_run, 0);
        assert_eq!(result.commands_failed, 0);
        assert_eq!(result.prompts_injected, 0);
        assert_eq!(result.agents_scheduled, 0);
        assert!(ctx.prompt_injections.is_empty());
    }

    #[tokio::test]
    async fn test_hook_registry_new_empty_counts() {
        let registry = HookRegistry::new();
        assert_eq!(registry.hook_count().await, 0);
        assert_eq!(registry.event_count().await, 0);
    }

    #[test]
    fn test_hook_context_default_all_none() {
        let ctx = HookContext::default();
        assert!(ctx.session_id.is_empty());
        assert!(ctx.agent_id.is_empty());
        assert!(ctx.tool_name.is_none());
        assert!(ctx.tool_args.is_none());
        assert!(ctx.tool_result.is_none());
        assert!(ctx.tool_success.is_none());
        assert!(ctx.user_message.is_none());
        assert!(ctx.subagent_name.is_none());
        assert!(ctx.subagent_result.is_none());
        assert!(ctx.prompt_injections.is_empty());
    }

    #[test]
    fn test_compiled_hook_matches_all_when_no_matcher() {
        let hook = CompiledHook {
            entry: HookEntry {
                event: HookEvent::PreToolUse,
                matcher: None,
                hooks: vec![],
            },
            matcher: None,
            plugin_dir: "/test".into(),
        };
        assert!(hook.matches_subject(Some("any_tool")));
        assert!(hook.matches_subject(None));
        assert!(hook.matches_subject(Some("")));
    }

    #[test]
    fn test_compiled_hook_matches_with_regex() {
        let hook = CompiledHook {
            entry: HookEntry {
                event: HookEvent::PreToolUse,
                matcher: Some("Edit|Write".into()),
                hooks: vec![],
            },
            matcher: Some(Arc::new(Regex::new("Edit|Write").unwrap())),
            plugin_dir: "/test".into(),
        };
        assert!(hook.matches_subject(Some("Edit")));
        assert!(hook.matches_subject(Some("Write")));
        assert!(!hook.matches_subject(Some("Read")));
        assert!(!hook.matches_subject(Some("shell")));
        assert!(!hook.matches_subject(None));
    }

    #[test]
    fn test_hook_fire_result_default() {
        let result = HookFireResult::default();
        assert_eq!(result.commands_run, 0);
        assert_eq!(result.commands_failed, 0);
        assert_eq!(result.prompts_injected, 0);
        assert_eq!(result.agents_scheduled, 0);
    }

    #[test]
    fn test_hook_config_no_hooks_returns_default() {
        let config = HookConfig { hooks: vec![] };
        assert!(config.hooks.is_empty());
    }

    #[test]
    fn test_hook_config_single_entry() {
        let entry = HookEntry {
            event: HookEvent::PreToolUse,
            matcher: None,
            hooks: vec![HookHandler {
                handler_type: HookHandlerType::Command,
                command: Some("echo hello".into()),
                prompt: None,
                timeout: 30,
            }],
        };
        let config = HookConfig {
            hooks: vec![entry],
        };
        assert_eq!(config.hooks.len(), 1);
        assert_eq!(config.hooks[0].event, HookEvent::PreToolUse);
        assert_eq!(config.hooks[0].hooks.len(), 1);
        assert_eq!(
            config.hooks[0].hooks[0].handler_type,
            HookHandlerType::Command
        );
    }

    #[tokio::test]
    async fn test_fire_with_no_matching_event() {
        // Register hooks for PreToolUse, fire PostToolUse — should get default result
        let entry = HookEntry {
            event: HookEvent::PreToolUse,
            matcher: None,
            hooks: vec![HookHandler {
                handler_type: HookHandlerType::Prompt,
                command: None,
                prompt: Some("injected".into()),
                timeout: 30,
            }],
        };

        let inner = HookRegistryInner {
            hooks: HashMap::from([(HookEvent::PreToolUse, vec![CompiledHook {
                entry,
                matcher: None,
                plugin_dir: "/test".into(),
            }])]),
        };

        let registry = HookRegistry {
            inner: Arc::new(RwLock::new(inner)),
        };

        let mut ctx = HookContext::default();
        let result = registry
            .fire(&HookEvent::PostToolUse, None, &mut ctx)
            .await;
        assert_eq!(result.prompts_injected, 0);
        assert!(ctx.prompt_injections.is_empty());
    }

    #[tokio::test]
    async fn test_fire_prompt_hook_injects_into_context() {
        let entry = HookEntry {
            event: HookEvent::PreToolUse,
            matcher: Some("Write".into()),
            hooks: vec![HookHandler {
                handler_type: HookHandlerType::Prompt,
                command: None,
                prompt: Some("Check before writing".into()),
                timeout: 30,
            }],
        };

        let inner = HookRegistryInner {
            hooks: HashMap::from([(HookEvent::PreToolUse, vec![CompiledHook {
                entry,
                matcher: Some(Arc::new(Regex::new("Write").unwrap())),
                plugin_dir: "/test".into(),
            }])]),
        };

        let registry = HookRegistry {
            inner: Arc::new(RwLock::new(inner)),
        };

        let mut ctx = HookContext::default();
        let result = registry
            .fire(&HookEvent::PreToolUse, Some("Write"), &mut ctx)
            .await;
        assert_eq!(result.prompts_injected, 1);
        assert_eq!(ctx.prompt_injections.len(), 1);
        assert_eq!(ctx.prompt_injections[0], "Check before writing");
    }

    #[tokio::test]
    async fn test_fire_prompt_hook_no_match_skipped() {
        let entry = HookEntry {
            event: HookEvent::PreToolUse,
            matcher: Some("Write".into()),
            hooks: vec![HookHandler {
                handler_type: HookHandlerType::Prompt,
                command: None,
                prompt: Some("should not inject".into()),
                timeout: 30,
            }],
        };

        let inner = HookRegistryInner {
            hooks: HashMap::from([(HookEvent::PreToolUse, vec![CompiledHook {
                entry,
                matcher: Some(Arc::new(Regex::new("Write").unwrap())),
                plugin_dir: "/test".into(),
            }])]),
        };

        let registry = HookRegistry {
            inner: Arc::new(RwLock::new(inner)),
        };

        let mut ctx = HookContext::default();
        let result = registry
            .fire(&HookEvent::PreToolUse, Some("Read"), &mut ctx)
            .await;
        assert_eq!(result.prompts_injected, 0);
        assert!(ctx.prompt_injections.is_empty());
    }

    #[tokio::test]
    async fn test_reload_replaces_hooks() {
        // First create a registry with a hook for PreToolUse
        let entry = HookEntry {
            event: HookEvent::PreToolUse,
            matcher: None,
            hooks: vec![HookHandler {
                handler_type: HookHandlerType::Prompt,
                command: None,
                prompt: Some("original".into()),
                timeout: 30,
            }],
        };

        let registry = HookRegistry {
            inner: Arc::new(RwLock::new(HookRegistryInner {
                hooks: HashMap::from([(HookEvent::PreToolUse, vec![CompiledHook {
                    entry,
                    matcher: None,
                    plugin_dir: "/test".into(),
                }])]),
            })),
        };

        assert_eq!(registry.hook_count().await, 1);

        // Simulate a reload by replacing inner
        {
            let mut inner = registry.inner.write().await;
            inner.hooks = HashMap::new();
        }

        assert_eq!(registry.hook_count().await, 0);
    }
}

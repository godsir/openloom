//! Slash-command pre-processor — harness-level interception of /skillname
//! before the model sees the user message, matching Claude Code's pattern.
//!
//! When `cc_dispatch` is enabled, the SlashRouter intercepts messages starting
//! with "/" and performs a direct hashmap lookup against installed skills.
//! The matched skill body is injected as a system message, and the slash
//! prefix is stripped from the user message — the model never sees it.

use std::collections::HashMap;

/// Result of intercepting a slash command.
#[derive(Debug, Clone)]
pub struct SlashIntercept {
    /// Full SKILL.md body to inject into context.
    pub skill_body: String,
    /// Skill name (for logging and system prompt injection).
    pub skill_name: String,
    /// User message with the /skillname prefix stripped.
    pub stripped_message: String,
}

/// Pre-processor that intercepts /skillname + /builtin commands before the
/// model sees them.
///
/// Builtin commands (like `/loom`) are registered at startup. Intercepted
/// builtin commands inject guard instructions so the model knows to use the
/// right management tool.
pub struct SlashRouter {
    /// Map of skill name → skill body for direct lookup.
    skill_bodies: HashMap<String, String>,
    /// Map of builtin command name → guard prompt body.
    builtins: HashMap<String, String>,
}

impl SlashRouter {
    /// Create an empty router. Populate via `rebuild()`.
    pub fn new() -> Self {
        Self {
            skill_bodies: HashMap::new(),
            builtins: HashMap::new(),
        }
    }

    /// Rebuild the router from skill names and bodies.
    pub fn rebuild(&mut self, bodies: HashMap<String, String>) {
        self.skill_bodies = bodies;
    }

    /// Register a builtin slash command with its guard prompt.
    /// The guard prompt is injected as a system message telling the model
    /// which tools to use for the user's intent.
    pub fn register_builtin(&mut self, name: &str, guard_prompt: &str) {
        self.builtins
            .insert(name.to_string(), guard_prompt.to_string());
    }

    /// Try to intercept a slash command.
    ///
    /// Checks builtins first, then installed skills. Returns
    /// `Some(SlashIntercept)` if the message starts with "/<cmd>" matching
    /// either a builtin or a skill. Returns `None` for unmatched messages.
    pub fn intercept(&self, user_message: &str) -> Option<SlashIntercept> {
        let trimmed = user_message.trim();
        let slash_name = trimmed.strip_prefix('/')?;

        let cmd = slash_name.split_whitespace().next().unwrap_or("");
        if cmd.is_empty() {
            return None;
        }

        // Builtins take priority over skills
        if let Some(body) = self.builtins.get(cmd) {
            let rest = slash_name[cmd.len()..].trim();
            return Some(SlashIntercept {
                skill_body: body.clone(),
                skill_name: cmd.to_string(),
                stripped_message: rest.to_string(),
            });
        }

        self.skill_bodies.get(cmd).map(|body| {
            let rest = slash_name[cmd.len()..].trim();
            SlashIntercept {
                skill_body: body.clone(),
                skill_name: cmd.to_string(),
                stripped_message: rest.to_string(),
            }
        })
    }

    /// Check if a skill name is registered in the router.
    pub fn has_skill(&self, name: &str) -> bool {
        self.skill_bodies.contains_key(name)
    }

    /// Check if a name is a registered builtin command.
    pub fn is_builtin(&self, name: &str) -> bool {
        self.builtins.contains_key(name)
    }

    /// Number of registered slash commands.
    pub fn len(&self) -> usize {
        self.skill_bodies.len()
    }

    /// Whether the router is empty.
    pub fn is_empty(&self) -> bool {
        self.skill_bodies.is_empty()
    }
}

impl Default for SlashRouter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_router() -> SlashRouter {
        let mut router = SlashRouter::new();
        let mut bodies = HashMap::new();
        bodies.insert(
            "brainstorming".to_string(),
            "BRAINSTORMING_BODY".to_string(),
        );
        bodies.insert("debug".to_string(), "DEBUG_BODY".to_string());
        router.rebuild(bodies);
        router
    }

    #[test]
    fn test_intercept_exact_skill_name() {
        let router = make_router();
        let result = router.intercept("/brainstorming");
        assert!(result.is_some());
        let intercept = result.unwrap();
        assert_eq!(intercept.skill_name, "brainstorming");
        assert_eq!(intercept.skill_body, "BRAINSTORMING_BODY");
        assert_eq!(intercept.stripped_message, "");
    }

    #[test]
    fn test_intercept_with_args() {
        let router = make_router();
        let result = router.intercept("/brainstorming design a login page");
        assert!(result.is_some());
        let intercept = result.unwrap();
        assert_eq!(intercept.skill_name, "brainstorming");
        assert_eq!(intercept.stripped_message, "design a login page");
    }

    #[test]
    fn test_intercept_no_match() {
        let router = make_router();
        let result = router.intercept("/unknown_skill");
        assert!(result.is_none());
    }

    #[test]
    fn test_intercept_non_slash() {
        let router = make_router();
        let result = router.intercept("help me brainstorm");
        assert!(result.is_none());
    }

    #[test]
    fn test_intercept_empty() {
        let router = make_router();
        let result = router.intercept("");
        assert!(result.is_none());
    }

    #[test]
    fn test_intercept_slash_only() {
        let router = make_router();
        let result = router.intercept("/");
        assert!(result.is_none());
    }

    #[test]
    fn test_intercept_with_leading_whitespace() {
        let router = make_router();
        let result = router.intercept("  /debug investigate crash");
        assert!(result.is_some());
        let intercept = result.unwrap();
        assert_eq!(intercept.skill_name, "debug");
        assert_eq!(intercept.stripped_message, "investigate crash");
    }

    #[test]
    fn test_has_skill() {
        let router = make_router();
        assert!(router.has_skill("brainstorming"));
        assert!(!router.has_skill("nonexistent"));
    }

    #[test]
    fn test_len() {
        let router = make_router();
        assert_eq!(router.len(), 2);
    }
}

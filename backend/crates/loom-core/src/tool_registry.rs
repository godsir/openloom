//! Tool dispatch system — unified trait for builtin skills, MCP tools, and CLI bridge.
//!
//! The `AgentTool` trait is the single dispatch point for all tool-like things
//! the agent can call. ToolRegistry is the canonical tool index.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use loom_types::{ToolDefinition, ToolProgress};
use tokio::sync::RwLock;
use tokio::sync::mpsc::UnboundedSender;

use crate::tool_context::ToolContext;

/// Result of executing a tool.
#[derive(Debug, Clone)]
pub struct ToolResult {
    pub content: String,
    pub is_error: bool,
    pub structured_content: Option<serde_json::Value>,
}

/// Provenance of a tool for telemetry and routing.
#[derive(Debug, Clone)]
pub enum ToolProvenance {
    Builtin,
    Mcp { server: String },
}

/// Unified trait for all tool-like things the agent can call.
///
/// Implementations: builtin skills, MCP tools from external servers,
/// CLI bridge tools discovered from PATH.
#[async_trait]
pub trait AgentTool: Send + Sync {
    /// Model-visible name (already namespaced: "file_read", "mcp__github__create_issue").
    fn tool_name(&self) -> &str;

    /// Tool definition sent to the LLM.
    fn tool_definition(&self) -> ToolDefinition;

    /// Execute the tool and return a structured result.
    async fn execute(
        &self,
        arguments: serde_json::Value,
        progress: UnboundedSender<ToolProgress>,
        context: &ToolContext,
    ) -> Result<ToolResult>;

    /// Whether this tool supports parallel execution with others.
    fn supports_parallel(&self) -> bool {
        false
    }

    /// Source provenance for telemetry.
    fn provenance(&self) -> ToolProvenance;
}

/// Canonical registry of all tools available to an agent.
///
/// Replaces the old SkillRegistry. Tools are indexed by their model-visible name.
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn AgentTool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Register a tool. Returns error if name collision.
    pub fn register(&mut self, tool: Arc<dyn AgentTool>) -> Result<()> {
        let name = tool.tool_name().to_string();
        if self.tools.contains_key(&name) {
            anyhow::bail!("tool name collision: '{}'", name);
        }
        tracing::debug!(name = %name, "tool registered");
        self.tools.insert(name, tool);
        Ok(())
    }

    /// Remove a tool by its model-visible name. Returns the removed tool or None.
    pub fn remove(&mut self, name: &str) -> Option<Arc<dyn AgentTool>> {
        let removed = self.tools.remove(name);
        if removed.is_some() {
            tracing::debug!(name = %name, "tool unregistered");
        }
        removed
    }

    /// Remove all tools whose name starts with the given prefix.
    /// Useful for cleaning up MCP server tools (prefixed "mcp__<server>__").
    pub fn remove_by_prefix(&mut self, prefix: &str) -> Vec<Arc<dyn AgentTool>> {
        let keys: Vec<String> = self
            .tools
            .keys()
            .filter(|k| k.starts_with(prefix))
            .cloned()
            .collect();
        let mut removed = Vec::new();
        for key in keys {
            if let Some(tool) = self.tools.remove(&key) {
                tracing::debug!(name = %key, "tool unregistered by prefix");
                removed.push(tool);
            }
        }
        removed
    }

    /// Build a model-visible tool name for an MCP server tool.
    /// Format: `mcp__<server>__<tool>`.
    pub fn mcp_tool_name(server: &str, tool: &str) -> String {
        format!("mcp__{}__{}", server, tool)
    }

    /// Build the prefix that all tools from a given MCP server share.
    /// Format: `mcp__<server>__`.
    pub fn mcp_tool_prefix(server: &str) -> String {
        format!("mcp__{}__", server)
    }

    /// Look up a tool by its model-visible name.
    pub fn find(&self, name: &str) -> Option<Arc<dyn AgentTool>> {
        self.tools.get(name).cloned()
    }

    /// Build all tool definitions for an LLM request.
    pub fn all_definitions(&self) -> Vec<ToolDefinition> {
        self.tools.values().map(|t| t.tool_definition()).collect()
    }

    /// Build tool definitions filtered by allow/deny lists.
    /// If allowed is Some, only tools in the list are returned.
    /// If disallowed is Some, tools in the list are excluded (applied after allow).
    /// If both are None, returns all definitions.
    pub fn filtered_definitions(
        &self,
        allowed: &Option<Vec<String>>,
        disallowed: &Option<Vec<String>>,
    ) -> Vec<ToolDefinition> {
        let mut defs: Vec<ToolDefinition> = self.all_definitions();
        if let Some(allow) = allowed {
            defs.retain(|d| allow.contains(&d.name));
        }
        if let Some(deny) = disallowed {
            defs.retain(|d| !deny.contains(&d.name));
        }
        defs
    }

    /// Execute a tool by name, dispatching to the correct handler.
    pub async fn execute(
        &self,
        name: &str,
        arguments: serde_json::Value,
        progress: UnboundedSender<ToolProgress>,
        context: &ToolContext,
    ) -> Result<ToolResult> {
        let tool = self
            .find(name)
            .ok_or_else(|| anyhow::anyhow!("unknown tool: {}", name))?;
        tool.execute(arguments, progress, context).await
    }

    /// List all registered tool names.
    pub fn list_names(&self) -> Vec<String> {
        self.tools.keys().cloned().collect()
    }

    /// Number of registered tools.
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Builtin spawn_agent tool
// ============================================================================

/// Context needed by spawn_agent to create and run child agents.
pub struct SpawnContext {
    pub cloud_client: Arc<RwLock<Option<Arc<dyn loom_inference::engine::CloudClient>>>>,
    pub tool_registry: Arc<RwLock<ToolRegistry>>,
    pub agent_pool: Arc<crate::agent_pool::AgentPool>,
    pub loop_config: Arc<RwLock<crate::agent_loop::AgentLoopConfig>>,
    pub event_bus: crate::event_bus::EventBus,
}

/// The spawn_agent tool allows an agent to delegate a subtask to a child agent.
pub struct SpawnAgentTool {
    pub max_depth: usize,
    pub default_timeout_secs: u64,
    pub context: Arc<SpawnContext>,
}

#[async_trait]
impl AgentTool for SpawnAgentTool {
    fn tool_name(&self) -> &str {
        "spawn_agent"
    }

    fn tool_definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "spawn_agent".into(),
            description: "Spawn a sub-agent to handle a delegated task. The sub-agent runs in an isolated context and returns its result. Use for complex multi-step work.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "description": {
                        "type": "string",
                        "description": "One-line description of the task"
                    },
                    "prompt": {
                        "type": "string",
                        "description": "Detailed instructions for the sub-agent"
                    }
                },
                "required": ["description", "prompt"]
            }),
            tags: vec![],
        }
    }

    async fn execute(
        &self,
        arguments: serde_json::Value,
        _progress: UnboundedSender<ToolProgress>,
        _context: &ToolContext,
    ) -> Result<ToolResult> {
        let description = arguments["description"].as_str().unwrap_or("subtask");
        let prompt = arguments["prompt"].as_str().unwrap_or("");
        if prompt.is_empty() {
            return Ok(ToolResult {
                content: "No prompt provided.".into(),
                is_error: true,
                structured_content: None,
            });
        }

        let config = self.context.loop_config.read().await;
        // Build a sub-agent agent loop config
        let sub_config = crate::agent_loop::AgentLoopConfig {
            system_prompt: format!(
                "You are a sub-agent. Task: {}\n\nInstructions:\n{}",
                description, prompt
            ),
            max_iterations: config.max_iterations.min(5),
            max_tokens: config.max_tokens,
            temperature: config.temperature,
            lazy_tools: config.lazy_tools,
            persona: None,
            summary: None,
            kg_context: None,
            thinking_budget: config.thinking_budget,
            model_configs: Vec::new(),
            active_model_name: None,
            workspace_path: config.workspace_path.clone(),
            max_prompt_budget: 0, // sub-agents: no budget limit
            default_permissions: config.default_permissions.clone(),
            hook_registry: None,
            session_id: String::new(),
            agent_id: String::new(),
            key_store: None,
            loom_dir: config.loom_dir.clone(),
            permission_mode: "operate".to_string(), // sub-agents always operate
            event_bus: None,
            pending_permissions: None,
        };
        drop(config);

        // Spawn child agent in the pool
        let session_id = loom_types::SessionId::new();
        let child_id = self
            .context
            .agent_pool
            .spawn(loom_types::AgentConfig::default(), None, session_id.clone())
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        let _ = self
            .context
            .agent_pool
            .transition(
                &child_id,
                crate::agent::AgentStatus::Thinking,
                Some("sub-agent processing".into()),
            )
            .await;

        // Run the agent loop — clone Arc to release RwLock before long-running turn
        let client: std::sync::Arc<dyn loom_inference::engine::CloudClient> = {
            let guard = self.context.cloud_client.read().await;
            match guard.as_ref() {
                Some(c) => c.clone(),
                None => {
                    return Ok(ToolResult {
                        content: "No model configured.".into(),
                        is_error: true,
                        structured_content: None,
                    });
                }
            }
        };
        let registry = self.context.tool_registry.read().await;

        let result = crate::agent_loop::run_agent_turn(
            client.as_ref(),
            &registry,
            &[],
            prompt,
            &sub_config,
            &None,
            &None,
        )
        .await;

        drop(registry);
        drop(client);

        match result {
            Ok(turn) => {
                let _ = self
                    .context
                    .agent_pool
                    .transition(&child_id, crate::agent::AgentStatus::Completed, None)
                    .await;
                let _ = self.context.agent_pool.remove(&child_id).await;
                tracing::info!(%description, tokens = turn.prompt_tokens + turn.completion_tokens, "sub-agent done");
                Ok(ToolResult {
                    content: turn.response,
                    is_error: false,
                    structured_content: None,
                })
            }
            Err(e) => {
                let _ = self
                    .context
                    .agent_pool
                    .transition(
                        &child_id,
                        crate::agent::AgentStatus::Errored {
                            message: e.to_string(),
                        },
                        None,
                    )
                    .await;
                let _ = self.context.agent_pool.remove(&child_id).await;
                Ok(ToolResult {
                    content: format!("Sub-agent error: {}", e),
                    is_error: true,
                    structured_content: None,
                })
            }
        }
    }

    fn provenance(&self) -> ToolProvenance {
        ToolProvenance::Builtin
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use serde_json::json;

    /// Minimal AgentTool stub for unit-testing ToolRegistry.
    struct TestTool {
        name: String,
    }

    impl TestTool {
        fn new(name: &str) -> Self {
            Self {
                name: name.to_string(),
            }
        }
    }

    #[async_trait]
    impl AgentTool for TestTool {
        fn tool_name(&self) -> &str {
            &self.name
        }

        fn tool_definition(&self) -> ToolDefinition {
            ToolDefinition {
                name: self.name.clone(),
                description: format!("Test tool: {}", self.name),
                input_schema: json!({"type": "object", "properties": {}}),
                tags: vec![],
            }
        }

        async fn execute(
            &self,
            _arguments: serde_json::Value,
            _progress: UnboundedSender<ToolProgress>,
            _context: &ToolContext,
        ) -> Result<ToolResult> {
            Ok(ToolResult {
                content: format!("{} executed", self.name),
                is_error: false,
                structured_content: None,
            })
        }

        fn provenance(&self) -> ToolProvenance {
            ToolProvenance::Builtin
        }
    }

    #[test]
    fn test_mcp_tool_name_format() {
        let name = ToolRegistry::mcp_tool_name("github", "create_issue");
        assert_eq!(name, "mcp__github__create_issue");
    }

    #[test]
    fn test_mcp_tool_prefix() {
        let prefix = ToolRegistry::mcp_tool_prefix("github");
        assert_eq!(prefix, "mcp__github__");
    }

    #[test]
    fn test_remove_existing() {
        let mut registry = ToolRegistry::new();
        let tool = Arc::new(TestTool::new("my_tool"));
        registry.register(tool).unwrap();

        let removed = registry.remove("my_tool");
        assert!(removed.is_some());
        assert!(registry.find("my_tool").is_none());
    }

    #[test]
    fn test_remove_nonexistent() {
        let mut registry = ToolRegistry::new();
        let removed = registry.remove("nonexistent");
        assert!(removed.is_none());
    }

    #[test]
    fn test_remove_by_prefix() {
        let mut registry = ToolRegistry::new();

        // Register 3 tools with prefix "mcp__foo__" and 1 with "mcp__bar__"
        for name in &["mcp__foo__a", "mcp__foo__b", "mcp__foo__c"] {
            registry
                .register(Arc::new(TestTool::new(name)))
                .unwrap();
        }
        registry
            .register(Arc::new(TestTool::new("mcp__bar__x")))
            .unwrap();

        assert_eq!(registry.len(), 4);

        let removed = registry.remove_by_prefix("mcp__foo__");
        assert_eq!(removed.len(), 3, "should remove 3 foo tools");
        assert_eq!(registry.len(), 1, "1 bar tool should remain");
        assert!(registry.find("mcp__bar__x").is_some());
    }

    #[test]
    fn test_remove_by_prefix_no_match() {
        let mut registry = ToolRegistry::new();
        let removed = registry.remove_by_prefix("nonexistent_prefix");
        assert!(removed.is_empty());
    }
}

//! Top-level orchestrator — wires AgentPool, ToolRegistry, McpClient,
//! inference, and the agent loop into a single entry point.

use std::sync::Arc;

use anyhow::Result;
use loom_inference::engine::CloudClient;
use loom_types::{AgentConfig, SessionId, CompletionRequest, Message};
use lume_mcp::McpClient;
use loom_memory::{ExtractedEntity, ExtractedRelationship, LLM_EXTRACTION_PROMPT, parse_llm_extraction};
use tokio::sync::RwLock;

use crate::agent_loop::{run_agent_turn, AgentLoopConfig, TurnResult};
use crate::agent_pool::{AgentPool, AgentSummary};
use crate::agent::AgentStatus;
use crate::event_bus::EventBus;
use crate::tool_registry::{AgentTool, SpawnAgentTool, SpawnContext, ToolRegistry};

/// The central orchestrator for openLoom v2.
pub struct Orchestrator {
    pool: AgentPool,
    tool_registry: Arc<RwLock<ToolRegistry>>,
    mcp_client: Arc<McpClient>,
    cloud_client: Arc<RwLock<Option<Box<dyn CloudClient>>>>,
    loop_config: Arc<RwLock<AgentLoopConfig>>,
    history: Arc<RwLock<Vec<Message>>>,
    skill_context: Arc<RwLock<String>>,
    persona_context: Arc<RwLock<String>>,
    memory_store: Arc<RwLock<Option<Box<dyn crate::MemoryStore>>>>,
}

/// Trait for memory backends (SqliteEventStore, etc.)
#[async_trait::async_trait]
pub trait MemoryStore: Send + Sync {
    async fn save_turn(&self, session_id: &str, user_msg: &str, assistant_msg: &str, tools: usize, tokens: usize) -> Result<()>;
    async fn load_history(&self, session_id: &str, limit: usize) -> Result<Vec<Message>>;
    async fn extract_cognitions(&self, session_id: &str, text: &str) -> Result<Vec<String>>;
    async fn get_persona(&self) -> Result<String>;
    async fn feed_knowledge_graph(&self, entities: &[loom_memory::ExtractedEntity], relationships: &[loom_memory::ExtractedRelationship]) -> Result<(usize, usize)>;
    async fn save_extracted_entities(&self, entities: &[loom_memory::ExtractedEntity], relationships: &[loom_memory::ExtractedRelationship]) -> Result<()>;
}

impl Orchestrator {
    pub fn new(max_depth: usize, default_max_iterations: usize, default_timeout_secs: u64) -> Self {
        let mut registry = ToolRegistry::new();
        let _ = registry.register(Arc::new(crate::builtin_tools::ShellTool));
        let _ = registry.register(Arc::new(crate::builtin_tools::FileListTool));
        let _ = registry.register(Arc::new(crate::builtin_tools::FileReadTool));
        let _ = registry.register(Arc::new(crate::builtin_tools::FileWriteTool));
        let _ = registry.register(Arc::new(crate::builtin_tools::ContentSearchTool));
        let _ = registry.register(Arc::new(crate::builtin_tools::FileDeleteTool));

        Self {
            pool: AgentPool::new(max_depth, default_max_iterations, default_timeout_secs),
            tool_registry: Arc::new(RwLock::new(registry)),
            mcp_client: Arc::new(McpClient::new()),
            cloud_client: Arc::new(RwLock::new(None)),
            loop_config: Arc::new(RwLock::new(AgentLoopConfig::default())),
            history: Arc::new(RwLock::new(Vec::new())),
            skill_context: Arc::new(RwLock::new(String::new())),
            persona_context: Arc::new(RwLock::new(String::new())),
            memory_store: Arc::new(RwLock::new(None)),
        }
    }

    /// Must be called after construction to wire spawn_agent (needs self references).
    pub async fn init_spawn_agent(self: &Arc<Self>, max_depth: usize, default_timeout_secs: u64) {
        let ctx = Arc::new(SpawnContext {
            cloud_client: self.cloud_client.clone(),
            tool_registry: self.tool_registry.clone(),
            agent_pool: Arc::new(AgentPool::new(max_depth, 20, default_timeout_secs)),
            loop_config: self.loop_config.clone(),
        });
        let _ = self.tool_registry.write().await.register(Arc::new(SpawnAgentTool {
            max_depth, default_timeout_secs, context: ctx,
        }));
    }

    // === Inference ===

    /// Set the cloud client (call after configuring a model).
    pub async fn set_cloud_client(&self, client: Box<dyn CloudClient>) {
        *self.cloud_client.write().await = Some(client);
    }

    /// Get a reference to the cloud client for direct access.
    pub async fn with_cloud_client<F, R>(&self, f: F) -> Result<R>
    where F: FnOnce(&dyn CloudClient) -> Result<R>,
    {
        let guard = self.cloud_client.read().await;
        match &*guard {
            Some(client) => f(client.as_ref()),
            None => Err(anyhow::anyhow!("No cloud client configured. Set up a model first.")),
        }
    }

    // === MCP ===

    /// Connect to an MCP server and register its tools in the registry.
    pub async fn connect_mcp_server(&self, config: lume_mcp::McpServerConfig) -> Result<String> {
        let name = self.mcp_client.connect(config).await?;
        // Register MCP tools into the tool registry
        let tools = self.mcp_client.server_tools(&name).await?;
        let mut registry = self.tool_registry.write().await;
        for tool in tools {
            let server = name.clone();
            let tool_name = format!("mcp__{}__{}", server, tool.name);
            let definition = loom_types::ToolDefinition {
                name: tool_name.clone(),
                description: format!("[MCP:{}] {}", server, tool.description),
                input_schema: tool.input_schema.clone(),
            };
            let mcp_client = self.mcp_client.clone();
            let mcp_tool = McpAgentTool {
                server_name: server,
                tool_name: tool.name,
                tool_definition: definition,
                mcp_client,
            };
            registry.register(Arc::new(mcp_tool))?;
        }
        Ok(name)
    }

    pub fn mcp_client(&self) -> &Arc<McpClient> {
        &self.mcp_client
    }

    // === Tool Registry ===

    pub async fn tool_registry(&self) -> tokio::sync::RwLockReadGuard<'_, ToolRegistry> {
        self.tool_registry.read().await
    }

    /// Register a custom tool.
    pub async fn register_tool(&self, tool: Arc<dyn AgentTool>) -> Result<()> {
        self.tool_registry.write().await.register(tool)
    }

    // === Skills / Persona / Memory ===

    /// Set loaded skill context (injected into system prompt).
    pub async fn set_skill_context(&self, ctx: String) {
        *self.skill_context.write().await = ctx;
    }

    /// Set persona summary (injected into system prompt).
    pub async fn set_persona(&self, persona: String) {
        *self.persona_context.write().await = persona;
    }

    /// Set the memory store for persistence and cognition extraction.
    pub async fn set_memory_store(&self, store: Box<dyn MemoryStore>) {
        *self.memory_store.write().await = Some(store);
    }

    /// Rebuild the system prompt with skills and persona injected.
    pub async fn build_system_prompt(&self) -> String {
        let cfg = self.loop_config.read().await;
        let mut prompt = cfg.system_prompt.clone();

        let persona = self.persona_context.read().await;
        if !persona.is_empty() {
            prompt.push_str(&format!("\n\n## User Profile\n{}", persona));
        }

        let skills = self.skill_context.read().await;
        if !skills.is_empty() {
            prompt.push_str(&format!("\n\n## Available Skills\n{}", skills));
        }

        prompt
    }

    /// Load conversation history from memory (restore on startup).
    pub async fn load_history(&self, session_id: &str) -> Result<()> {
        let store = self.memory_store.read().await;
        if let Some(ref s) = *store {
            let msgs = s.load_history(session_id, 50).await?;
            if !msgs.is_empty() {
                *self.history.write().await = msgs;
            }
        }
        Ok(())
    }

    // === Agent Loop ===

    /// Process a user message through the agent loop and return the response.
    /// Uses the Agent state machine: Idle → Thinking → Completed (or Errored).
    pub async fn process_message(&self, user_message: &str) -> Result<TurnResult> {
        // Register agent in pool
        let agent_id = self.pool.spawn(AgentConfig::default(), None, SessionId::new()).await?;

        // Transition: Idle → Thinking
        let _ = self.pool.transition(&agent_id, AgentStatus::Thinking, Some("processing".into())).await;

        let guard = self.cloud_client.read().await;
        let client = guard.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No cloud client configured"))?;
        let registry = self.tool_registry.read().await;
        let history = self.history.read().await;

        let system_prompt = self.build_system_prompt().await;
        let mut config = AgentLoopConfig::default();
        config.system_prompt = system_prompt;

        let result = run_agent_turn(
            client.as_ref(),
            &registry,
            &history,
            user_message,
            &config,
        ).await;

        drop(history); drop(registry);

        if let Ok(ref turn) = result {
            let _ = self.pool.transition(&agent_id, AgentStatus::Completed, None).await;
            let mut hist = self.history.write().await;
            hist.push(Message::user(user_message));
            hist.push(Message::assistant(&turn.response));

            // Persist to memory store
            let mem = self.memory_store.read().await;
            if let Some(ref store) = *mem {
                let session_id = "default";
                let _ = store.save_turn(session_id, user_message, &turn.response, turn.tool_calls_made, turn.prompt_tokens + turn.completion_tokens).await;

                // LLM-based entity extraction (synchronous, runs after response)
                let msg = user_message.to_string();
                let client_opt = self.cloud_client.read().await;
                if let Some(ref client) = *client_opt {
                    match llm_extract_entities(client.as_ref(), &msg).await {
                        Ok((entities, relationships)) => {
                            if let Ok((n, e)) = store.feed_knowledge_graph(&entities, &relationships).await {
                                let _ = store.save_extracted_entities(&entities, &relationships).await;
                                if n > 0 || e > 0 {
                                    tracing::info!(n, e, "KG updated via LLM");
                                }
                            }
                            if let Ok(persona) = store.get_persona().await {
                                if !persona.is_empty() {
                                    *self.persona_context.write().await = persona;
                                }
                            }
                        }
                        Err(e) => tracing::debug!("LLM extraction: {}", e),
                    }
                }
                drop(client_opt);
            }
        } else {
            let _ = self.pool.transition(&agent_id, AgentStatus::Errored { message: "LLM error".into() }, None).await;
        }

        // Clean up agent from pool
        let _ = self.pool.remove(&agent_id).await;

        result
    }

    // === Agent Pool (delegated) ===

    pub fn event_bus(&self) -> &EventBus { self.pool.event_bus() }
    pub async fn spawn_agent(&self, config: AgentConfig, parent_id: Option<loom_types::AgentId>, session_id: SessionId) -> Result<loom_types::AgentId> {
        self.pool.spawn(config, parent_id, session_id).await
    }
    pub async fn kill_agent(&self, agent_id: &loom_types::AgentId) -> Result<()> { self.pool.kill(agent_id).await }
    pub async fn list_agents(&self) -> Vec<AgentSummary> { self.pool.list().await }
    pub async fn agent_status(&self, agent_id: &loom_types::AgentId) -> Result<AgentSummary> { self.pool.summary(agent_id).await }
    pub async fn set_agent_status(&self, agent_id: &loom_types::AgentId, status: AgentStatus, message: Option<String>) -> Result<()> {
        self.pool.transition(agent_id, status, message).await
    }
}

// ============================================================================
// McpAgentTool — wraps an MCP tool as an AgentTool
// ============================================================================

struct McpAgentTool {
    server_name: String,
    tool_name: String,
    tool_definition: loom_types::ToolDefinition,
    mcp_client: Arc<McpClient>,
}

#[async_trait::async_trait]
impl AgentTool for McpAgentTool {
    fn tool_name(&self) -> &str {
        &self.tool_definition.name
    }

    fn tool_definition(&self) -> loom_types::ToolDefinition {
        self.tool_definition.clone()
    }

    async fn execute(
        &self, arguments: serde_json::Value,
        _progress: tokio::sync::mpsc::UnboundedSender<loom_types::ToolProgress>,
    ) -> Result<crate::tool_registry::ToolResult> {
        match self.mcp_client.call_tool(&self.server_name, &self.tool_name, arguments).await {
            Ok(result) => Ok(crate::tool_registry::ToolResult {
                content: serde_json::to_string_pretty(&result).unwrap_or_else(|_| "{}".into()),
                is_error: false,
                structured_content: Some(result),
            }),
            Err(e) => Ok(crate::tool_registry::ToolResult {
                content: format!("MCP tool error: {}", e),
                is_error: true,
                structured_content: None,
            }),
        }
    }

    fn provenance(&self) -> crate::tool_registry::ToolProvenance {
        crate::tool_registry::ToolProvenance::Mcp { server: self.server_name.clone() }
    }
}

// ============================================================================
// LLM-based entity extraction
// ============================================================================

async fn llm_extract_entities(
    client: &dyn CloudClient,
    text: &str,
) -> Result<(Vec<ExtractedEntity>, Vec<ExtractedRelationship>)> {
    let prompt = format!("{}\n\nUser message: {}", LLM_EXTRACTION_PROMPT, text);

    let request = CompletionRequest {
        messages: vec![Message::user(&prompt)],
        tools: vec![],
        tool_choice: None,
        prompt: prompt,
        max_tokens: 1024,
        temperature: 0.0,
        top_p: 1.0,
        stop: vec![],
        stream: false,
        thinking_budget: None,
    };

    let response = client.complete(request).await?;
    let (mut entities, mut relationships) = parse_llm_extraction(&response.text)?;

    // Normalize: USER entity should always be "USER"
    for e in &mut entities {
        if e.name.to_lowercase() == "user" { e.name = "USER".into(); e.entity_type = "Person".into(); }
    }
    for r in &mut relationships {
        if r.source_name.to_lowercase() == "user" { r.source_name = "USER".into(); }
        if r.target_name.to_lowercase() == "user" { r.target_name = "USER".into(); }
    }

    Ok((entities, relationships))
}


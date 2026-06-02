//! Top-level orchestrator — wires AgentPool, ToolRegistry, McpClient,
//! inference, and the agent loop into a single entry point.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use base64::Engine;
use loom_inference::engine::CloudClient;
use loom_memory::{
    ExtractedEntity, ExtractedRelationship, LLM_EXTRACTION_PROMPT, parse_llm_extraction,
};
use loom_security::merge_multi_permissions;
use loom_types::{
    AgentConfig, CompletionRequest, ContentPart, Message, ModelBackend, Role, SessionId, SkillPermissions,
    StreamDelta,
};
use loom_lsp::LspClient;
use loom_mcp::McpClient;
use loom_skills::SkillPermissionConfig;
use tokio::sync::{RwLock, mpsc};

use crate::agent::AgentStatus;
use crate::agent_loop::{
    AgentLoopConfig, TurnResult, build_user_message, run_agent_turn_streaming_with_images,
};
use crate::agent_pool::{AgentPool, AgentSummary};
use crate::event_bus::{AgentEvent, EventBus};
use crate::hooks::{HookContext, HookRegistry};
use crate::tool_registry::{AgentTool, SpawnAgentTool, SpawnContext, ToolRegistry};

/// The central orchestrator for openLoom v2.
pub struct Orchestrator {
    pool: AgentPool,
    tool_registry: Arc<RwLock<ToolRegistry>>,
    mcp_client: Arc<McpClient>,
    lsp_client: Arc<LspClient>,
    cloud_client: Arc<RwLock<Option<Arc<dyn CloudClient>>>>,
    loop_config: Arc<RwLock<AgentLoopConfig>>,
    session_histories: Arc<RwLock<std::collections::HashMap<String, Vec<Message>>>>,
    skill_state: Arc<RwLock<loom_skills::SkillState>>,
    persona_context: Arc<RwLock<String>>,
    memory_store: Arc<RwLock<Option<Box<dyn crate::MemoryStore>>>>,
    agent_configs: Arc<RwLock<std::collections::HashMap<String, loom_types::AgentConfig>>>,
    model_configs: Arc<RwLock<std::collections::HashMap<String, loom_types::ModelConfig>>>,
    active_model_name: Arc<RwLock<Option<String>>>,
    /// Serialises concurrent model-switch calls so `model.switch` and
    /// the per-message model override in `chat.send` never race.
    model_switch_lock: tokio::sync::Mutex<()>,
    /// Compiled hook registry from plugin hook configs.
    hook_registry: Arc<RwLock<HookRegistry>>,
    /// In-memory API key store shared with the server's AppState.
    /// Maps env-var names (e.g. "OPENAI_API_KEY") to their values.
    /// Set via set_key_store() after construction, before any cloud client is built.
    key_store: Arc<RwLock<HashMap<String, String>>>,
    /// Pending permission approvals for "ask" mode (call_id → oneshot sender).
    pending_permissions: Arc<RwLock<HashMap<String, tokio::sync::oneshot::Sender<bool>>>>,
    data_dir: PathBuf,
}

/// Trait for memory backends (SqliteEventStore, etc.)
#[async_trait::async_trait]
pub trait MemoryStore: Send + Sync {
    async fn save_turn(
        &self,
        session_id: &str,
        user_msg: &str,
        assistant_msg: &str,
        tools: usize,
        prompt_tokens: usize,
        completion_tokens: usize,
    ) -> Result<i64>;
    async fn save_interrupted_turn(
        &self,
        session_id: &str,
        user_msg: &str,
    ) -> Result<()>;
    async fn load_history(&self, session_id: &str, limit: usize) -> Result<Vec<Message>>;
    async fn delete_message(&self, session_id: &str, index: usize) -> Result<()>;
    async fn extract_cognitions(&self, session_id: &str, text: &str) -> Result<Vec<String>>;
    async fn get_persona(&self) -> Result<String>;
    async fn feed_knowledge_graph(
        &self,
        entities: &[loom_memory::ExtractedEntity],
        relationships: &[loom_memory::ExtractedRelationship],
        source_event_id: i64,
        scope: &str,
    ) -> Result<(usize, usize)>;
    async fn save_extracted_entities(
        &self,
        entities: &[loom_memory::ExtractedEntity],
        relationships: &[loom_memory::ExtractedRelationship],
        scope: &str,
    ) -> Result<()>;
    // Agent config CRUD
    async fn save_agent_config(&self, config: &loom_types::AgentConfig) -> Result<()>;
    async fn get_agent_config(&self, name: &str) -> Result<Option<loom_types::AgentConfig>>;
    async fn list_agent_configs(&self) -> Result<Vec<loom_types::AgentConfig>>;
    async fn delete_agent_config(&self, name: &str) -> Result<()>;
    // Session-agent binding
    async fn save_session_agent_name(
        &self,
        session_id: &str,
        agent_config_name: &str,
    ) -> Result<()>;
    async fn get_session_agent_name(&self, session_id: &str) -> Result<Option<String>>;
    // Session workspace
    async fn save_session_workspace(&self, session_id: &str, path: &str) -> Result<()>;
    async fn get_session_workspace(&self, session_id: &str) -> Result<Option<String>>;
    async fn get_default_workspace(&self) -> Result<Option<String>>;
    async fn set_default_workspace(&self, path: &str) -> Result<()>;
    // Model config CRUD
    async fn save_model_config(&self, config: &loom_types::ModelConfig) -> Result<()>;
    async fn get_model_config(&self, name: &str) -> Result<Option<loom_types::ModelConfig>>;
    async fn list_model_configs(&self) -> Result<Vec<loom_types::ModelConfig>>;
    async fn delete_model_config(&self, name: &str) -> Result<()>;
    async fn set_active_model(&self, name: &str) -> Result<()>;
    async fn get_active_model(&self) -> Result<Option<loom_types::ModelConfig>>;
    // MCP server config CRUD — persisted across restarts so users don't have
    // to re-enter command/URL/headers/etc. every time the backend restarts.
    async fn save_mcp_server(
        &self,
        config: &loom_mcp::McpServerConfig,
        autostart: bool,
    ) -> Result<()>;
    async fn list_mcp_servers(&self) -> Result<Vec<(loom_mcp::McpServerConfig, bool)>>;
    async fn delete_mcp_server(&self, name: &str) -> Result<()>;
    // Knowledge graph read
    async fn query_kg_context(&self, entity_names: &[&str], limit: usize, scope: &str) -> Result<String>;
    // Conversation summary (P0 memory optimization)
    async fn get_summary(&self, session_id: &str) -> Result<Option<String>>;
    async fn save_summary(&self, session_id: &str, summary: &str) -> Result<()>;
    async fn get_summary_at_count(&self, session_id: &str) -> Result<usize>;
    async fn get_message_count(&self, session_id: &str) -> Result<usize>;
    // Memory maintenance (P2)
    async fn prune_memory(&self) -> Result<usize>;
    // Cross-session knowledge search (P2)
    async fn search_knowledge(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<(String, String, String, f64)>>;
    async fn kg_node_count(&self) -> Result<usize>;
    async fn kg_edge_count(&self) -> Result<usize>;
    async fn kg_neighbors(&self, node_name: &str, limit: usize) -> Result<loom_types::KgGraph>;
    async fn kg_walk(
        &self,
        start_name: &str,
        max_depth: u8,
        scope: Option<&str>,
        limit: usize,
    ) -> Result<loom_types::KgGraph>;
    async fn kg_list_nodes(
        &self,
        limit: usize,
        offset: usize,
        scope: Option<&str>,
    ) -> Result<Vec<loom_types::KgNode>>;
    async fn kg_edges_between(&self, node_names: &[String]) -> Result<Vec<loom_types::KgEdge>>;
    async fn kg_delete_node(&self, name: &str) -> Result<bool>;
    async fn kg_delete_edge(&self, source: &str, target: &str, relation: &str) -> Result<bool>;
    // Cognition records
    async fn cognition_list(
        &self,
        subject: &str,
        scope: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<loom_types::Cognition>>;
    async fn cognition_list_subjects(&self) -> Result<Vec<String>>;
    async fn cognition_snapshots(
        &self,
        cognition_id: i64,
    ) -> Result<Vec<loom_types::CognitionHistory>>;
    // Knowledge graph maintenance
    async fn kg_prune(&self, older_than_days: i64) -> Result<usize>;
    // Session persistence
    async fn list_sessions(&self) -> Result<Vec<(String, String, usize, Option<String>)>>;
    async fn ensure_session(&self, id: &str) -> Result<()>;
    async fn delete_session(&self, id: &str) -> Result<()>;
    async fn rename_session(&self, id: &str, title: &str) -> Result<()>;

    // Token usage tracking
    async fn record_token_usage(
        &self,
        session_id: &str,
        model: &str,
        prompt_tokens: usize,
        completion_tokens: usize,
        cached_read_tokens: usize,
        cached_write_tokens: usize,
        latency_ms: u64,
        context_window: usize,
    ) -> Result<()>;
    async fn get_token_summary(&self, from: &str, to: &str) -> Result<serde_json::Value>;
    async fn get_token_history(
        &self,
        from: &str,
        to: &str,
        granularity: &str,
    ) -> Result<serde_json::Value>;
    async fn reset_token_usage(&self) -> Result<()>;
}

// ── Entity extraction helper (English + Chinese) ─────────────────────────

/// Extract candidate entity names from text for KG context injection.
/// English: whitespace-delimited capitalized words > 3 chars.
/// Chinese: CJK character n-grams (2-5 chars) via sliding window.
fn extract_text(content: &[ContentPart]) -> String {
    content
        .iter()
        .filter_map(|p| match p {
            ContentPart::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn truncate_str(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max).collect::<String>() + "..."
    }
}

fn extract_entity_candidates(text: &str) -> Vec<String> {
    let mut candidates: Vec<String> = Vec::new();

    // English: split by whitespace, keep capitalized words
    for w in text.split_whitespace() {
        let trimmed = w.trim_matches(|c: char| !c.is_alphabetic());
        if trimmed.len() >= 3 && trimmed.chars().next().is_some_and(|c| c.is_uppercase()) {
            candidates.push(trimmed.to_string());
        }
    }

    // Chinese: sliding window of 2-5 CJK characters on consecutive runs
    let chars: Vec<char> = text.chars().collect();
    let cjk_indices: Vec<usize> = chars
        .iter()
        .enumerate()
        .filter(|(_, c)| is_cjk_char(**c))
        .map(|(i, _)| i)
        .collect();

    let mut run_start = 0usize;
    while run_start < cjk_indices.len() {
        let mut run_end = run_start;
        while run_end + 1 < cjk_indices.len()
            && cjk_indices[run_end + 1] == cjk_indices[run_end] + 1
        {
            run_end += 1;
        }
        let run_len = run_end - run_start + 1;
        if run_len >= 2 {
            // Whole run
            if run_len <= 5 {
                candidates.push(
                    chars[cjk_indices[run_start]..=cjk_indices[run_end]]
                        .iter()
                        .collect(),
                );
            }
            // Sub-ngrams (2-4 chars)
            for n in 2..=5.min(run_len) {
                for i in 0..=(run_len - n) {
                    let s: String = chars
                        [cjk_indices[run_start + i]..=cjk_indices[run_start + i + n - 1]]
                        .iter()
                        .collect();
                    candidates.push(s);
                }
            }
        }
        run_start = run_end + 1;
    }

    candidates.sort();
    candidates.dedup();
    candidates.truncate(10);
    candidates
}

fn is_cjk_char(c: char) -> bool {
    matches!(c,
        '\u{4E00}'..='\u{9FFF}'    // CJK Unified
        | '\u{3400}'..='\u{4DBF}'   // CJK Extended A
        | '\u{F900}'..='\u{FAFF}'   // CJK Compatibility
        | '\u{2F800}'..='\u{2FA1F}' // CJK Compatibility Supplement
    )
}

impl Orchestrator {
    pub fn new(
        max_depth: usize,
        default_max_iterations: usize,
        default_timeout_secs: u64,
        data_dir: PathBuf,
    ) -> Self {
        let mut registry = ToolRegistry::new();
        let _ = registry.register(Arc::new(crate::builtin_tools::ShellTool));
        let _ = registry.register(Arc::new(crate::builtin_tools::FileListTool));
        let _ = registry.register(Arc::new(crate::builtin_tools::FileReadTool));
        let _ = registry.register(Arc::new(crate::builtin_tools::FileWriteTool));
        let _ = registry.register(Arc::new(crate::builtin_tools::ContentSearchTool));
        let _ = registry.register(Arc::new(crate::builtin_tools::FileDeleteTool));

        let skill_state = Arc::new(RwLock::new(loom_skills::SkillState::default()));
        let _ = registry.register(Arc::new(crate::builtin_tools::UseSkillTool {
            skill_state: skill_state.clone(),
        }));
        let _ = registry.register(Arc::new(crate::builtin_tools::WebSearchTool));
        let _ = registry.register(Arc::new(crate::builtin_tools::WebFetchTool));

        // LSP tools — registered for on-demand loading via request_tools
        let lsp_client = Arc::new(LspClient::new());
        register_lsp_tools(&mut registry, &lsp_client);

        let hook_registry = Arc::new(RwLock::new(HookRegistry::new()));
        let mut pool = AgentPool::new(max_depth, default_max_iterations, default_timeout_secs);
        pool.set_hook_registry(Some(hook_registry.clone()));

        Self {
            pool,
            tool_registry: Arc::new(RwLock::new(registry)),
            mcp_client: Arc::new(McpClient::new()),
            lsp_client,
            cloud_client: Arc::new(RwLock::new(None)),
            loop_config: Arc::new(RwLock::new(AgentLoopConfig::default())),
            session_histories: Arc::new(RwLock::new(std::collections::HashMap::new())),
            skill_state,
            persona_context: Arc::new(RwLock::new(String::new())),
            memory_store: Arc::new(RwLock::new(None)),
            agent_configs: Arc::new(RwLock::new(std::collections::HashMap::from([(
                "default".to_string(),
                loom_types::AgentConfig::default(),
            )]))),
            model_configs: Arc::new(RwLock::new(std::collections::HashMap::new())),
            active_model_name: Arc::new(RwLock::new(None)),
            model_switch_lock: tokio::sync::Mutex::new(()),
            hook_registry,
            key_store: Arc::new(RwLock::new(HashMap::new())),
            pending_permissions: Arc::new(RwLock::new(HashMap::new())),
            data_dir,
        }
    }

    /// Must be called after construction to wire spawn_agent (needs self references).
    pub async fn init_spawn_agent(self: &Arc<Self>, max_depth: usize, default_timeout_secs: u64) {
        let mut spawn_pool = AgentPool::new(max_depth, 20, default_timeout_secs);
        spawn_pool.set_hook_registry(Some(self.hook_registry.clone()));
        let ctx = Arc::new(SpawnContext {
            cloud_client: self.cloud_client.clone(),
            tool_registry: self.tool_registry.clone(),
            agent_pool: Arc::new(spawn_pool),
            loop_config: self.loop_config.clone(),
            event_bus: self.pool.event_bus().clone(),
        });
        let _ = self
            .tool_registry
            .write()
            .await
            .register(Arc::new(SpawnAgentTool {
                max_depth,
                default_timeout_secs,
                context: ctx,
            }));
    }

    // === Inference ===

    /// Set the cloud client (call after configuring a model).
    pub async fn set_cloud_client(&self, client: Arc<dyn CloudClient>) {
        *self.cloud_client.write().await = Some(client);
    }

    /// Set the API key store shared with the server layer.
    /// Must be called before any cloud client is built (i.e., before
    /// load_model_configs or model.switch).
    pub async fn set_key_store(
        &self,
        ks: Arc<tokio::sync::RwLock<std::collections::HashMap<String, String>>>,
    ) {
        let mut target = self.key_store.write().await;
        let source = ks.read().await;
        target.clear();
        for (k, v) in source.iter() {
            target.insert(k.clone(), v.clone());
        }
        tracing::info!(count = target.len(), "key_store populated");
    }

    /// Resolve an API key from the key_store given an api_key_env hint.
    /// 1. If api_key_env matches a key in the store, return its value.
    /// 2. If api_key_env doesn't look like an env var name (not ALL_CAPS_UNDERSCORE),
    ///    treat it as a literal key value.
    /// 3. Fall back to well-known backend env var names.

    /// Get a clone of the shared key_store Arc for direct access.
    pub fn key_store_arc(&self) -> Arc<RwLock<HashMap<String, String>>> {
        self.key_store.clone()
    }

    /// Returns the data directory path.
    pub fn data_dir_path(&self) -> &std::path::Path {
        &self.data_dir
    }

    pub async fn resolve_api_key(
        &self,
        api_key_env: Option<&str>,
        backend: &ModelBackend,
    ) -> Option<String> {
        let guard = self.key_store.read().await;
        // 1. Try env var name from config
        if let Some(raw) = api_key_env {
            if let Some(val) = guard.get(raw) {
                return Some(val.clone());
            }
            // Only treat as literal key if it doesn't look like an env var name
            let looks_like_env_var = !raw.is_empty()
                && raw
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_');
            if !looks_like_env_var && !raw.is_empty() {
                return Some(raw.to_string());
            }
            // Fall back to OS environment variable
            if let Ok(val) = std::env::var(raw) {
                if !val.is_empty() {
                    return Some(val);
                }
            }
        }
        // 3. Fallback to well-known backend env vars
        let auto_env = match backend {
            ModelBackend::DeepSeek => "DEEPSEEK_API_KEY",
            ModelBackend::OpenAI => "OPENAI_API_KEY",
            ModelBackend::Anthropic => "ANTHROPIC_API_KEY",
            _ => "OPENLOOM_API_KEY",
        };
        guard
            .get(auto_env)
            .cloned()
            .or_else(|| std::env::var(auto_env).ok().filter(|v| !v.is_empty()))
    }

    /// Get a reference to the cloud client for direct access.
    pub async fn with_cloud_client<F, R>(&self, f: F) -> Result<R>
    where
        F: FnOnce(&dyn CloudClient) -> Result<R>,
    {
        let client: Option<Arc<dyn CloudClient>> = self.cloud_client.read().await.clone();
        match client {
            Some(c) => f(c.as_ref()),
            None => Err(anyhow::anyhow!(
                "No cloud client configured. Set up a model first."
            )),
        }
    }

    // === MCP ===

    /// Connect to an MCP server and register its tools in the registry.
    pub async fn connect_mcp_server(&self, config: loom_mcp::McpServerConfig) -> Result<String> {
        let name = self.mcp_client.connect(config).await?;
        // Register MCP tools into the tool registry
        let tools = self.mcp_client.server_tools(&name).await?;
        let mut registry = self.tool_registry.write().await;
        for tool in tools {
            let server = name.clone();
            let tool_name = ToolRegistry::mcp_tool_name(&server, &tool.name);
            let definition = loom_types::ToolDefinition {
                name: tool_name.clone(),
                description: format!("[MCP:{}] {}", server, tool.description),
                input_schema: tool.input_schema.clone(),
                tags: vec!["mcp".into(), server.clone().into()],
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

        // Register resource/prompt tools for this server
        let mcp_client = self.mcp_client.clone();
        let server = name.clone();

        // mcp__<server>__list_resources
        registry.register(Arc::new(McpMetaTool {
            server_name: server.clone(),
            op: McpMetaOp::ListResources,
            tool_definition: loom_types::ToolDefinition {
                name: ToolRegistry::mcp_tool_name(&server, "list_resources"),
                description: format!("[MCP:{}] List available resources from this server", server),
                input_schema: serde_json::json!({"type":"object","properties":{},"required":[]}),
                tags: vec!["mcp".into(), "resource".into()],
            },
            mcp_client: mcp_client.clone(),
        }))?;

        // mcp__<server>__read_resource
        registry.register(Arc::new(McpMetaTool {
            server_name: server.clone(),
            op: McpMetaOp::ReadResource,
            tool_definition: loom_types::ToolDefinition {
                name: ToolRegistry::mcp_tool_name(&server, "read_resource"),
                description: format!("[MCP:{}] Read a resource by URI. Use list_resources first to discover URIs.", server),
                input_schema: serde_json::json!({"type":"object","properties":{"uri":{"type":"string","description":"Resource URI to read"}},"required":["uri"]}),
                tags: vec!["mcp".into(), "resource".into()],
            },
            mcp_client: mcp_client.clone(),
        }))?;

        // mcp__<server>__list_prompts
        registry.register(Arc::new(McpMetaTool {
            server_name: server.clone(),
            op: McpMetaOp::ListPrompts,
            tool_definition: loom_types::ToolDefinition {
                name: ToolRegistry::mcp_tool_name(&server, "list_prompts"),
                description: format!(
                    "[MCP:{}] List available prompt templates from this server",
                    server
                ),
                input_schema: serde_json::json!({"type":"object","properties":{},"required":[]}),
                tags: vec!["mcp".into(), "prompt".into()],
            },
            mcp_client: mcp_client.clone(),
        }))?;

        // mcp__<server>__get_prompt
        registry.register(Arc::new(McpMetaTool {
            server_name: server.clone(),
            op: McpMetaOp::GetPrompt,
            tool_definition: loom_types::ToolDefinition {
                name: ToolRegistry::mcp_tool_name(&server, "get_prompt"),
                description: format!("[MCP:{}] Get a prompt template with arguments filled in. Use list_prompts first to see available prompts and their argument schemas.", server),
                input_schema: serde_json::json!({"type":"object","properties":{"name":{"type":"string","description":"Prompt name"},"arguments":{"type":"object","description":"Prompt arguments (key-value pairs)"}},"required":["name"]}),
                tags: vec!["mcp".into(), "prompt".into()],
            },
            mcp_client: mcp_client.clone(),
        }))?;

        Ok(name)
    }

    pub fn mcp_client(&self) -> &Arc<McpClient> {
        &self.mcp_client
    }

    // === MCP saved server CRUD ===

    /// Persist (or upsert) an MCP server config. Does not touch live state.
    pub async fn save_mcp_server(
        &self,
        config: &loom_mcp::McpServerConfig,
        autostart: bool,
    ) -> Result<()> {
        if let Some(ref store) = *self.memory_store.read().await {
            store.save_mcp_server(config, autostart).await?;
        }
        Ok(())
    }

    /// List persisted MCP server configs + autostart flag.
    pub async fn list_saved_mcp_servers(&self) -> Result<Vec<(loom_mcp::McpServerConfig, bool)>> {
        if let Some(ref store) = *self.memory_store.read().await {
            return store.list_mcp_servers().await;
        }
        Ok(Vec::new())
    }

    /// Delete a persisted MCP server config (and disconnect if live).
    pub async fn delete_saved_mcp_server(&self, name: &str) -> Result<()> {
        // Best-effort disconnect — ignore "not connected" errors.
        let _ = self.mcp_client.disconnect(name).await;
        // Unregister all tools belonging to this MCP server
        let prefix = ToolRegistry::mcp_tool_prefix(name);
        let removed = self.tool_registry.write().await.remove_by_prefix(&prefix);
        tracing::info!(server = %name, count = removed.len(), "unregistered MCP tools");
        if let Some(ref store) = *self.memory_store.read().await {
            store.delete_mcp_server(name).await?;
        }
        Ok(())
    }

    /// Reconnect every saved server with `autostart=true`. Called once at
    /// engine start. Failures are logged but do not abort startup.
    pub async fn autostart_mcp_servers(&self) {
        let configs = match self.list_saved_mcp_servers().await {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(error = %e, "failed to read saved MCP servers");
                return;
            }
        };
        let live: std::collections::HashSet<String> =
            self.mcp_client.server_names().await.into_iter().collect();
        for (cfg, autostart) in configs {
            if !autostart || live.contains(&cfg.name) {
                continue;
            }
            let name = cfg.name.clone();
            if let Err(e) = self.connect_mcp_server(cfg).await {
                tracing::warn!(server = %name, error = %e, "MCP autostart failed");
            } else {
                tracing::info!(server = %name, "MCP autostart connected");
            }
        }
    }

    pub fn lsp_client(&self) -> &Arc<LspClient> {
        &self.lsp_client
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

    /// Atomically replace all skill state (context, bodies, permissions, summaries).
    pub async fn set_skills(&self, state: loom_skills::SkillState) {
        *self.skill_state.write().await = state;
    }

    /// Look up a skill's full body by name.
    pub async fn get_skill_body(&self, name: &str) -> Option<String> {
        self.skill_state.read().await.bodies.get(name).cloned()
    }

    /// Return lightweight summaries for all cached skills (for `skills.list` RPC).
    pub async fn get_skill_summaries(&self) -> Vec<loom_skills::SkillSummary> {
        self.skill_state.read().await.summaries.clone()
    }

    /// Return all cached skill names.
    pub async fn get_skill_names(&self) -> Vec<String> {
        self.skill_state.read().await.bodies.keys().cloned().collect()
    }

    // === Hook Registry ===

    /// Load hook configs from discovered plugins into the runtime registry.
    /// Called once at startup after PluginManager discovery.
    pub async fn load_hooks_from_plugins(&self, plugin_manager: &loom_plugins::PluginManager) {
        let registry = HookRegistry::load_from_plugins(plugin_manager).await;
        *self.hook_registry.write().await = registry;
    }

    /// Reload the hook registry from plugins (when plugins are installed/removed).
    pub async fn reload_hooks(&self, plugin_manager: &loom_plugins::PluginManager) {
        self.hook_registry
            .read()
            .await
            .reload(plugin_manager)
            .await;
    }

    /// Get a clone of the hook registry for external access.
    pub fn hook_registry(&self) -> Arc<RwLock<HookRegistry>> {
        self.hook_registry.clone()
    }

    /// Fire hooks for a given event with the provided context.
    /// Returns a summary of what happened. Never panics.
    pub async fn fire_hooks(
        &self,
        event: &loom_plugins::hooks::HookEvent,
        subject: Option<&str>,
        ctx: &mut HookContext,
    ) -> crate::hooks::HookFireResult {
        let registry = self.hook_registry.read().await;
        registry.fire(event, subject, ctx).await
    }

    /// Set persona summary (injected into system prompt).
    pub async fn set_persona(&self, persona: String) {
        *self.persona_context.write().await = persona;
    }

    /// Set the memory store for persistence and cognition extraction.
    pub async fn set_memory_store(&self, store: Box<dyn MemoryStore>) {
        *self.memory_store.write().await = Some(store);
    }

    /// Prune stale low-confidence KG entities on startup.
    pub async fn prune_memory(&self) -> Result<usize> {
        if let Some(ref store) = *self.memory_store.read().await {
            store.prune_memory().await
        } else {
            Ok(0)
        }
    }

    // === Knowledge Graph Queries ===

    pub async fn kg_search(&self, query: &str, limit: usize) -> Result<Vec<loom_types::KgNode>> {
        if let Some(ref store) = *self.memory_store.read().await {
            let results = store.search_knowledge(query, limit).await?;
            Ok(results
                .into_iter()
                .map(
                    |(name, entity_type, description, confidence)| loom_types::KgNode {
                        node_id: 0,
                        name,
                        entity_type,
                        description,
                        confidence,
                        scope: "global".to_string(),
                    },
                )
                .collect())
        } else {
            Ok(Vec::new())
        }
    }

    pub async fn kg_stats(&self) -> Result<loom_types::KgStats> {
        if let Some(ref store) = *self.memory_store.read().await {
            Ok(loom_types::KgStats {
                node_count: store.kg_node_count().await?,
                edge_count: store.kg_edge_count().await?,
            })
        } else {
            Ok(loom_types::KgStats {
                node_count: 0,
                edge_count: 0,
            })
        }
    }

    pub async fn kg_neighbors(&self, node_name: &str, limit: usize) -> Result<loom_types::KgGraph> {
        if let Some(ref store) = *self.memory_store.read().await {
            store.kg_neighbors(node_name, limit).await
        } else {
            Ok(loom_types::KgGraph {
                nodes: Vec::new(),
                edges: Vec::new(),
            })
        }
    }

    pub async fn kg_walk(
        &self,
        start_name: &str,
        max_depth: u8,
        scope: Option<&str>,
        limit: usize,
    ) -> Result<loom_types::KgGraph> {
        if let Some(ref store) = *self.memory_store.read().await {
            store.kg_walk(start_name, max_depth, scope, limit).await
        } else {
            Ok(loom_types::KgGraph {
                nodes: Vec::new(),
                edges: Vec::new(),
            })
        }
    }

    pub async fn kg_list_nodes(
        &self,
        limit: usize,
        offset: usize,
        scope: Option<&str>,
    ) -> Result<Vec<loom_types::KgNode>> {
        if let Some(ref store) = *self.memory_store.read().await {
            store.kg_list_nodes(limit, offset, scope).await
        } else {
            Ok(Vec::new())
        }
    }

    pub async fn kg_edges_between(&self, node_names: &[String]) -> Result<Vec<loom_types::KgEdge>> {
        if let Some(ref store) = *self.memory_store.read().await {
            store.kg_edges_between(node_names).await
        } else {
            Ok(Vec::new())
        }
    }

    pub async fn kg_delete_node(&self, name: &str) -> Result<bool> {
        if let Some(ref store) = *self.memory_store.read().await {
            store.kg_delete_node(name).await
        } else {
            Ok(false)
        }
    }

    pub async fn kg_delete_edge(&self, source: &str, target: &str, relation: &str) -> Result<bool> {
        if let Some(ref store) = *self.memory_store.read().await {
            store.kg_delete_edge(source, target, relation).await
        } else {
            Ok(false)
        }
    }

    pub async fn cognition_list(
        &self,
        subject: &str,
        scope: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<loom_types::Cognition>> {
        if let Some(ref store) = *self.memory_store.read().await {
            store.cognition_list(subject, scope, limit, offset).await
        } else {
            Ok(Vec::new())
        }
    }

    pub async fn cognition_list_subjects(&self) -> Result<Vec<String>> {
        if let Some(ref store) = *self.memory_store.read().await {
            store.cognition_list_subjects().await
        } else {
            Ok(Vec::new())
        }
    }

    pub async fn cognition_snapshots(
        &self,
        cognition_id: i64,
    ) -> Result<Vec<loom_types::CognitionHistory>> {
        if let Some(ref store) = *self.memory_store.read().await {
            store.cognition_snapshots(cognition_id).await
        } else {
            Ok(Vec::new())
        }
    }

    pub async fn kg_prune(&self, older_than_days: i64) -> Result<usize> {
        if let Some(ref store) = *self.memory_store.read().await {
            store.kg_prune(older_than_days).await
        } else {
            Ok(0)
        }
    }

    /// Return the base system prompt without persona/skills injection.
    /// Persona, skills, and agent-specific additions are injected separately
    /// by the caller (process_message_with_config / process_message_streaming).
    pub async fn build_system_prompt(&self) -> String {
        self.loop_config.read().await.system_prompt.clone()
    }

    /// Load conversation history for a session from memory (restore on startup).
    /// Migrates old base64 `Image` parts to `ImageRef` on the fly.
    pub async fn load_history(&self, session_id: &str) -> Result<()> {
        let store = self.memory_store.read().await;
        if let Some(ref s) = *store {
            let mut msgs = s.load_history(session_id, 50).await?;
            tracing::info!(
                session_id,
                db_returned = msgs.len(),
                "orchestrator.load_history: DB result"
            );
            // Lazy migration: convert old base64 Image parts to ImageRef
            let mut migrated = 0usize;
            for msg in &mut msgs {
                for part in &mut msg.content {
                    if let ContentPart::Image {
                        media_type, data, ..
                    } = part
                    {
                        match self.save_image_to_disk(session_id, media_type, data).await {
                            Ok(file_id) => {
                                *part = ContentPart::ImageRef {
                                    media_type: std::mem::take(media_type),
                                    file_id,
                                };
                                migrated += 1;
                            }
                            Err(e) => {
                                tracing::warn!(session_id, error = %e, "failed to migrate image, keeping base64");
                            }
                        }
                    }
                }
            }
            if migrated > 0 {
                tracing::info!(session_id, migrated, "migrated old base64 images to disk");
            }
            if !msgs.is_empty() {
                self.session_histories
                    .write()
                    .await
                    .insert(session_id.to_string(), msgs);
            }
        } else {
            tracing::warn!(
                session_id,
                "orchestrator.load_history: memory_store is None"
            );
        }
        Ok(())
    }

    /// Delete a message from a session's history by its index (0-based position).
    pub async fn delete_message(&self, session_id: &str, index: usize) -> Result<()> {
        let store = self.memory_store.read().await;
        if let Some(ref s) = *store {
            s.delete_message(session_id, index).await?;
        }
        // Clear in-memory cache so it reloads from DB on next access
        self.session_histories.write().await.remove(session_id);
        Ok(())
    }

    /// Get history for a session (empty if none).
    pub async fn session_history(&self, session_id: &str) -> Vec<Message> {
        self.session_histories
            .read()
            .await
            .get(session_id)
            .cloned()
            .unwrap_or_default()
    }

    /// Generate a 2-7 character title for a session using the LLM.
    /// Reads the first user message and assistant response, then asks the LLM to
    /// summarize the topic concisely.
    pub async fn auto_title(&self, session_id: &str) -> Result<String> {
        let history = self.session_history(session_id).await;
        if history.len() < 2 {
            anyhow::bail!("not enough messages for auto-title");
        }

        // Grab first exchange: user message + assistant response
        let user_text = history
            .iter()
            .find(|m| m.role == Role::User)
            .map(|m| extract_text(&m.content))
            .unwrap_or_default();
        let ai_text = history
            .iter()
            .filter(|m| m.role == Role::Assistant)
            .filter_map(|m| {
                let t = extract_text(&m.content);
                if t.is_empty() { None } else { Some(t) }
            })
            .next()
            .unwrap_or_default();

        if user_text.is_empty() && ai_text.is_empty() {
            anyhow::bail!("no text content for auto-title");
        }

        let prompt = format!(
            "你是一位标题编辑。根据以下对话内容，生成一个 2-7 个汉字的简短会话标题。\n\
             只输出标题本身，不要加引号、不要解释、不要换行。\n\n\
             用户: {}\nAI: {}",
            truncate_str(&user_text, 200),
            truncate_str(&ai_text, 300),
        );

        let request = loom_types::CompletionRequest {
            messages: vec![Message {
                role: Role::User,
                content: vec![ContentPart::Text { text: prompt }],
                timestamp: chrono::Utc::now(),
                usage: None,
            }],
            tools: vec![],
            tool_choice: None,
            prompt: String::new(),
            max_tokens: 32,
            temperature: 0.3,
            top_p: 1.0,
            stop: vec!["\n".to_string()],
            stream: false,
            thinking_budget: None,
        };

        let client = {
            self.cloud_client
                .read()
                .await
                .clone()
                .ok_or_else(|| anyhow::anyhow!("No cloud client configured"))?
        };
        let response = client.complete(request).await?;
        let title = response.text.trim().to_string();

        // Sanitize: keep only Chinese chars, letters, digits, spaces
        let title: String = title
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == ' ' || ('\u{4e00}'..='\u{9fff}').contains(c))
            .take(10)
            .collect();

        if title.is_empty() {
            anyhow::bail!("generated empty title");
        }

        Ok(title)
    }

    /// Append a message to a session's history.
    pub async fn add_to_history(&self, session_id: &str, msg: Message) {
        self.session_histories
            .write()
            .await
            .entry(session_id.to_string())
            .or_default()
            .push(msg);
    }

    /// List sessions from the memory store (or empty if no store).
    pub async fn list_persisted_sessions(&self) -> Vec<(String, String, usize, Option<String>)> {
        let store = self.memory_store.read().await;
        if let Some(ref s) = *store {
            s.list_sessions().await.unwrap_or_default()
        } else {
            Vec::new()
        }
    }

    /// Ensure a session exists in the persisted store.
    pub async fn ensure_session_persisted(&self, id: &str) {
        let store = self.memory_store.read().await;
        if let Some(ref s) = *store {
            let _ = s.ensure_session(id).await;
        }
    }

    /// Persist a session rename to the memory store.
    pub async fn rename_session_persisted(&self, id: &str, title: &str) {
        let store = self.memory_store.read().await;
        if let Some(ref s) = *store {
            let _ = s.rename_session(id, title).await;
        }
    }

    /// Delete a session from the persisted store, in-memory cache, and image files.
    pub async fn delete_session_persisted(&self, id: &str) {
        let store = self.memory_store.read().await;
        if let Some(ref s) = *store {
            let _ = s.delete_session(id).await;
        }
        self.session_histories.write().await.remove(id);
        if let Err(e) = self.delete_session_images(id).await {
            tracing::warn!(session_id = %id, error = %e, "failed to delete session images");
        }
    }

    /// Persist a session-agent binding to the memory store.
    pub async fn bind_agent_persisted(&self, session_id: &str, agent_config_name: &str) {
        let store = self.memory_store.read().await;
        if let Some(ref s) = *store {
            let _ = s.save_session_agent_name(session_id, agent_config_name).await;
        }
    }

    /// Query token usage summary for a time range.
    pub async fn token_summary(
        &self,
        from: &str,
        to: &str,
    ) -> Result<serde_json::Value, anyhow::Error> {
        let mut summary = match &*self.memory_store.read().await {
            Some(store) => store.get_token_summary(from, to).await?,
            None => serde_json::json!({
                "total_prompt_tokens": 0, "total_completion_tokens": 0,
                "total_cached_tokens": 0, "total_requests": 0,
                "avg_latency_ms": 0, "cache_hit_rate": 0, "by_model": []
            }),
        };

        // Enrich by_model entries with pricing from model configs
        let configs = self.model_configs.read().await;
        let mut total_cost = 0.0_f64;
        if let Some(by_model) = summary["by_model"].as_array_mut() {
            for entry in by_model {
                let model_name = entry["model"].as_str().unwrap_or("");
                let (input_price, output_price, cache_read_price, cache_write_price) = configs
                    .get(model_name)
                    .map(|c| {
                        (
                            c.input_price,
                            c.output_price,
                            c.cache_read_price,
                            c.cache_write_price,
                        )
                    })
                    .unwrap_or((0.0, 0.0, 0.0, 0.0));
                let prompt = entry["prompt"].as_i64().unwrap_or(0) as f64;
                let completion = entry["completion"].as_i64().unwrap_or(0) as f64;
                let cached_read = entry
                    .get("cached_read")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0) as f64;
                let cached_write = entry
                    .get("cached_write")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0) as f64;
                let cache_cost = (cached_read * cache_read_price
                    + cached_write * cache_write_price)
                    / 1_000_000.0;
                let cost =
                    (prompt * input_price + completion * output_price) / 1_000_000.0 + cache_cost;
                total_cost += cost;
                entry["input_price"] = serde_json::json!(input_price);
                entry["output_price"] = serde_json::json!(output_price);
                entry["cache_read_price"] = serde_json::json!(cache_read_price);
                entry["cache_write_price"] = serde_json::json!(cache_write_price);
                entry["cost"] = serde_json::json!((cost * 10000.0).round() / 10000.0);
            }
        }
        summary["total_cost"] = serde_json::json!((total_cost * 10000.0).round() / 10000.0);

        Ok(summary)
    }

    /// Query token usage history (time-series) for a time range.
    pub async fn token_history(
        &self,
        from: &str,
        to: &str,
        granularity: &str,
    ) -> Result<serde_json::Value, anyhow::Error> {
        match &*self.memory_store.read().await {
            Some(store) => store.get_token_history(from, to, granularity).await,
            None => Ok(serde_json::json!({ "points": [] })),
        }
    }

    pub async fn reset_token_usage(&self) -> Result<(), anyhow::Error> {
        match &*self.memory_store.read().await {
            Some(store) => store.reset_token_usage().await,
            None => Err(anyhow::anyhow!("memory store not available")),
        }
    }

    // ── Image file management ──────────────────────────────────────────

    fn session_images_dir(&self, session_id: &str) -> PathBuf {
        self.data_dir
            .join("sessions")
            .join(session_id)
            .join("images")
    }

    fn media_type_to_ext(media_type: &str) -> &str {
        match media_type {
            "image/png" => "png",
            "image/jpeg" => "jpg",
            "image/gif" => "gif",
            "image/webp" => "webp",
            "image/bmp" => "bmp",
            "image/svg+xml" => "svg",
            _ => "png",
        }
    }

    async fn save_image_to_disk(
        &self,
        session_id: &str,
        media_type: &str,
        data: &str,
    ) -> Result<String> {
        let dir = self.session_images_dir(session_id);
        tokio::fs::create_dir_all(&dir).await?;
        let ext = Self::media_type_to_ext(media_type);
        let file_id = format!("{}.{}", uuid::Uuid::now_v7().simple(), ext);
        let path = dir.join(&file_id);
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(data)
            .context("invalid base64 image data")?;
        tokio::fs::write(&path, &bytes).await?;
        tracing::info!(session_id, file_id = %file_id, path = %path.display(), size = bytes.len(), "image saved to disk");
        Ok(file_id)
    }

    async fn delete_session_images(&self, session_id: &str) -> Result<()> {
        let dir = self.session_images_dir(session_id);
        if dir.exists() {
            tokio::fs::remove_dir_all(&dir).await?;
            tracing::info!(session_id, "session images deleted");
        }
        Ok(())
    }

    /// Convert all `ContentPart::Image` to `ContentPart::ImageRef` in-place.
    async fn convert_images_to_refs(
        &self,
        session_id: &str,
        parts: &mut Vec<ContentPart>,
    ) -> Result<()> {
        for part in parts.iter_mut() {
            if let ContentPart::Image {
                media_type, data, ..
            } = part
            {
                let file_id = self
                    .save_image_to_disk(session_id, media_type, data)
                    .await?;
                *part = ContentPart::ImageRef {
                    media_type: std::mem::take(media_type),
                    file_id,
                };
            }
        }
        Ok(())
    }

    // === Agent Config ===

    /// Load all agent configs from the memory store into the in-memory cache.
    pub async fn load_agent_configs(&self) -> Result<()> {
        let store = self.memory_store.read().await;
        if let Some(ref s) = *store {
            let configs = s.list_agent_configs().await?;
            let mut cache = self.agent_configs.write().await;
            for c in configs {
                cache.insert(c.name.clone(), c);
            }
            // Ensure default always exists
            cache
                .entry("default".to_string())
                .or_insert_with(loom_types::AgentConfig::default);
            tracing::info!(count = cache.len(), "agent configs loaded");
        }
        Ok(())
    }

    pub async fn agent_config_list(&self) -> Vec<loom_types::AgentConfig> {
        self.agent_configs.read().await.values().cloned().collect()
    }

    pub async fn agent_config_get(&self, name: &str) -> Result<loom_types::AgentConfig> {
        // Check cache first
        {
            let cache = self.agent_configs.read().await;
            if let Some(cfg) = cache.get(name).cloned() {
                return Ok(cfg);
            }
        }
        // Fall back to DB — cache may be stale after a silent load failure
        let store = self.memory_store.read().await;
        if let Some(ref s) = *store {
            if let Some(cfg) = s.get_agent_config(name).await? {
                self.agent_configs
                    .write()
                    .await
                    .insert(name.to_string(), cfg.clone());
                return Ok(cfg);
            }
        }
        anyhow::bail!("agent config '{}' not found", name)
    }

    pub async fn agent_config_create(&self, config: loom_types::AgentConfig) -> Result<()> {
        let name = config.name.clone();
        // Reject duplicate against the union of cache + DB
        {
            let cache = self.agent_configs.read().await;
            if cache.contains_key(&name) {
                anyhow::bail!("agent config '{}' already exists", name);
            }
        }
        // DB first — avoid polluting cache if persistence fails
        let store = self.memory_store.read().await;
        if let Some(ref s) = *store {
            if s.get_agent_config(&name).await?.is_some() {
                anyhow::bail!("agent config '{}' already exists", name);
            }
            s.save_agent_config(&config).await?;
        }
        drop(store);
        self.agent_configs.write().await.insert(name, config);
        Ok(())
    }

    pub async fn agent_config_update(
        &self,
        config: loom_types::AgentConfig,
        prev_name: &str,
    ) -> Result<()> {
        let new_name = config.name.clone();
        // Update is upsert-with-rename semantics: if prev_name no longer exists
        // anywhere (cache + DB out of sync, prior failed save, etc.), treat as
        // create rather than failing — the user clearly sees this entry in the
        // UI and expects save to succeed. Renames still delete the old row.
        let store = self.memory_store.read().await;
        // DB first
        if let Some(ref s) = *store {
            if new_name != prev_name {
                // Best-effort delete of the old row; ignore "not found"
                let _ = s.delete_agent_config(prev_name).await;
            }
            s.save_agent_config(&config).await?;
        }
        drop(store);
        // Then sync cache
        let mut cache = self.agent_configs.write().await;
        if new_name != prev_name {
            cache.remove(prev_name);
        }
        cache.insert(new_name, config);
        Ok(())
    }

    pub async fn agent_config_delete(&self, name: &str) -> Result<()> {
        if name == "default" {
            anyhow::bail!("cannot delete the default agent config");
        }
        self.agent_configs.write().await.remove(name);
        let store = self.memory_store.read().await;
        if let Some(ref s) = *store {
            s.delete_agent_config(name).await?;
        }
        Ok(())
    }

    /// Resolve the agent config for a session (falls back to "default").
    pub async fn resolve_session_agent_config(&self, session_id: &str) -> loom_types::AgentConfig {
        let store = self.memory_store.read().await;
        let config_name = if let Some(ref s) = *store {
            s.get_session_agent_name(session_id).await.ok().flatten()
        } else {
            None
        };
        let name = config_name.unwrap_or_else(|| "default".to_string());
        self.agent_config_get(&name).await.unwrap_or_default()
    }

    // === Workspace Management ===

    /// Set the workspace path for a session.
    pub async fn set_session_workspace(&self, session_id: &str, path: &str) -> Result<()> {
        let store = self.memory_store.read().await;
        if let Some(ref s) = *store {
            s.save_session_workspace(session_id, path).await
        } else {
            Err(anyhow::anyhow!("memory store not available"))
        }
    }

    /// Get the workspace path for a session.
    pub async fn get_session_workspace(&self, session_id: &str) -> Result<Option<String>> {
        let store = self.memory_store.read().await;
        if let Some(ref s) = *store {
            s.get_session_workspace(session_id).await
        } else {
            Ok(None)
        }
    }

    /// Set the default workspace path.
    pub async fn set_default_workspace(&self, path: &str) -> Result<()> {
        let store = self.memory_store.read().await;
        if let Some(ref s) = *store {
            s.set_default_workspace(path).await
        } else {
            Err(anyhow::anyhow!("memory store not available"))
        }
    }

    /// Get the default workspace path.
    pub async fn get_default_workspace(&self) -> Result<Option<String>> {
        let store = self.memory_store.read().await;
        if let Some(ref s) = *store {
            s.get_default_workspace().await
        } else {
            Ok(None)
        }
    }

    // === Model Config Management ===

    /// Load model configs from the memory store into the in-memory cache.
    pub async fn load_model_configs(&self) -> Result<()> {
        let store = self.memory_store.read().await;
        let mut active_config: Option<loom_types::ModelConfig> = None;
        if let Some(ref s) = *store {
            let configs = s.list_model_configs().await?;
            let mut cache = self.model_configs.write().await;
            for c in configs {
                cache.insert(c.name.clone(), c);
            }
            // Track the active model
            if let Ok(Some(active)) = s.get_active_model().await {
                *self.active_model_name.write().await = Some(active.name.clone());
                active_config = Some(active);
            }
            tracing::info!(count = cache.len(), "model configs loaded");
        }
        drop(store);
        // Build cloud client for the active model on startup
        if let Some(config) = active_config {
            self.try_build_cloud_client(&config).await;
        }
        Ok(())
    }

    pub async fn model_config_list(&self) -> Vec<loom_types::ModelConfig> {
        self.model_configs.read().await.values().cloned().collect()
    }

    pub async fn model_config_get(&self, name: &str) -> Result<loom_types::ModelConfig> {
        self.model_configs
            .read()
            .await
            .get(name)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("model config '{}' not found", name))
    }

    pub async fn model_config_create(&self, config: loom_types::ModelConfig) -> Result<()> {
        let name = config.name.clone();
        {
            let mut cache = self.model_configs.write().await;
            if cache.contains_key(&name) {
                anyhow::bail!("model config '{}' already exists", name);
            }
            // Check for exact duplicate: same model + same backend + same label
            if let Some(ref model_id) = config.model {
                for (existing_name, existing) in cache.iter() {
                    if existing.model.as_deref() == Some(model_id.as_str())
                        && existing_name != &name
                        && existing.backend == config.backend
                        && existing.backend_label == config.backend_label
                    {
                        anyhow::bail!(
                            "model '{}' is already configured as '{}' for this provider",
                            model_id,
                            existing_name
                        );
                    }
                }
            }
            cache.insert(name.clone(), config.clone());
        }
        let store = self.memory_store.read().await;
        if let Some(ref s) = *store {
            s.save_model_config(&config).await?;
        }
        Ok(())
    }

    pub async fn model_config_update(
        &self,
        config: loom_types::ModelConfig,
        prev_name: &str,
    ) -> Result<()> {
        let new_name = config.name.clone();
        let store = self.memory_store.read().await;
        if let Some(ref s) = *store {
            if new_name != prev_name {
                let _ = s.delete_model_config(prev_name).await;
            }
            s.save_model_config(&config).await?;
        }
        drop(store);
        let mut cache = self.model_configs.write().await;
        if new_name != prev_name {
            cache.remove(prev_name);
        }
        cache.insert(new_name, config);
        Ok(())
    }

    pub async fn model_config_delete(&self, name: &str) -> Result<()> {
        // Prevent deleting the active model
        let active = self.active_model_name.read().await;
        if active.as_deref() == Some(name) {
            return Err(anyhow::anyhow!("cannot delete the active model '{}'", name));
        }
        drop(active);
        self.model_configs.write().await.remove(name);
        let store = self.memory_store.read().await;
        if let Some(ref s) = *store {
            s.delete_model_config(name).await?;
        }
        Ok(())
    }

    pub async fn model_config_set_active(&self, name: &str) -> Result<()> {
        // Serialise concurrent switch calls (e.g. model.switch racing with chat.send override)
        let _lock = self.model_switch_lock.lock().await;

        // Verify the model config exists
        let config = self.model_config_get(name).await?;

        // Unload the previous local model before switching (avoids piling up in LM Studio)
        {
            let old_name = self.active_model_name.read().await.clone();
            if let Some(ref old_name) = old_name {
                if old_name != name {
                    if let Ok(old_config) = self.model_config_get(old_name).await {
                        if old_config.backend.is_local_inference() {
                            if let Some(ref old_model_id) = old_config.model {
                                let base = old_config.base_url.as_deref().unwrap_or(
                                    match old_config.backend {
                                        loom_types::ModelBackend::LmStudio => {
                                            "http://localhost:1234/v1"
                                        }
                                        loom_types::ModelBackend::Ollama => {
                                            "http://localhost:11434/v1"
                                        }
                                        _ => "http://localhost:1234/v1",
                                    },
                                );
                                loom_inference::unload_local_model(base, old_model_id).await;
                            }
                        }
                    }
                }
            }
        }

        // Update DB
        let store = self.memory_store.read().await;
        if let Some(ref s) = *store {
            s.set_active_model(name).await?;
        }
        // Update in-memory
        *self.active_model_name.write().await = Some(name.to_string());
        tracing::info!(name = name, "active_model_name updated in memory");
        // Try to rebuild cloud client
        if let Some(model_id) = &config.model {
            tracing::info!(name = name, model = %model_id, "switching active model — rebuilding cloud client");
            drop(store);
            self.try_build_cloud_client(&config).await;
            tracing::info!(name = name, model = %model_id, "model switch complete");
        } else {
            tracing::warn!(
                name = name,
                "model config has no model ID — cloud client not rebuilt"
            );
        }
        Ok(())
    }

    pub async fn active_model_name(&self) -> Option<String> {
        self.active_model_name.read().await.clone()
    }

    /// Try to build a new CloudClient from a ModelConfig.
    /// Replaces the current cloud_client on success.
    async fn try_build_cloud_client(&self, config: &loom_types::ModelConfig) {
        let model = match &config.model {
            Some(m) => m.clone(),
            None => {
                tracing::warn!(name = %config.name, "try_build_cloud_client: no model ID in config, skipping");
                return;
            }
        };

        // Log if current client model matches (for debugging)
        {
            let existing: Option<Arc<dyn CloudClient>> = self.cloud_client.read().await.clone();
            if let Some(ref client) = existing {
                if client.model_name() == model {
                    tracing::info!(%model, name = %config.name, "cloud client model_name matches — force rebuild anyway to pick up config changes");
                }
            }
        }

        let is_local = config.backend.is_local_inference();
        let base_url = config
            .base_url
            .clone()
            .unwrap_or_else(|| match config.backend {
                loom_types::ModelBackend::LmStudio => "http://localhost:1234/v1".into(),
                loom_types::ModelBackend::Ollama => "http://localhost:11434/v1".into(),
                loom_types::ModelBackend::Anthropic => "https://api.anthropic.com".into(),
                loom_types::ModelBackend::OpenAI => "https://api.openai.com".into(),
                loom_types::ModelBackend::DeepSeek => "https://api.deepseek.com/v1".into(),
                loom_types::ModelBackend::Custom => "http://localhost:8080/v1".into(),
            });

        let client: Option<Arc<dyn CloudClient>> = if is_local {
            match loom_inference::InferenceEngine::connect(&base_url, &model, config.context_size)
                .await
            {
                Ok(engine) => Some(Arc::new(engine)),
                Err(e) => {
                    tracing::warn!(model = %model, error = %e, "failed to connect local inference engine");
                    None
                }
            }
        } else {
            // Cloud provider — resolve API key from the in-memory key_store.
            let api_key = self
                .resolve_api_key(config.api_key_env.as_deref(), &config.backend)
                .await;
            let model_id = model.clone();
            match api_key {
                Some(key) => {
                    let is_anthropic = config.api_format.as_deref() == Some("anthropic")
                        || matches!(config.backend, loom_types::ModelBackend::Anthropic);
                    if is_anthropic {
                        Some(Arc::new(loom_inference::AnthropicClient::new(
                            key,
                            model_id.clone(),
                            base_url,
                        )) as Arc<dyn CloudClient>)
                    } else {
                        Some(Arc::new(loom_inference::OpenAIClient::new(
                            key, model_id, base_url, false,
                        )) as Arc<dyn CloudClient>)
                    }
                }
                None => {
                    tracing::warn!(model = %model, "no API key found; model set but cloud client not created");
                    None
                }
            }
        };

        if let Some(c) = client {
            *self.cloud_client.write().await = Some(c);
            tracing::info!(model = %model, "cloud client switched");
        }
    }

    /// Read auxiliary model config for a specific task type.
    /// Returns the configured model name, or None if not configured.
    async fn read_auxiliary_model(&self, task: &str) -> Option<String> {
        let home = dirs::home_dir()?.join(".loom").join("auxiliary.json");
        let content = std::fs::read_to_string(&home).ok()?;
        let config: serde_json::Value = serde_json::from_str(&content).ok()?;
        let key = match task {
            "summary" => "summary_model",
            "entity" => "entity_model",
            _ => return None,
        };
        config.get(key)
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
    }

    /// Build a CloudClient for an auxiliary model task.
    /// Returns None if no auxiliary model is configured for the task.
    async fn build_auxiliary_client(&self, task: &str) -> Option<Arc<dyn CloudClient>> {
        let model_name = self.read_auxiliary_model(task).await?;

        // Find the model config by name
        let configs = self.model_configs.read().await;
        let config = configs.get(&model_name)?;

        let model = config.model.clone()?;
        let is_local = config.backend.is_local_inference();
        let base_url = config
            .base_url
            .clone()
            .unwrap_or_else(|| match config.backend {
                loom_types::ModelBackend::LmStudio => "http://localhost:1234/v1".into(),
                loom_types::ModelBackend::Ollama => "http://localhost:11434/v1".into(),
                loom_types::ModelBackend::Anthropic => "https://api.anthropic.com".into(),
                loom_types::ModelBackend::OpenAI => "https://api.openai.com".into(),
                loom_types::ModelBackend::DeepSeek => "https://api.deepseek.com/v1".into(),
                loom_types::ModelBackend::Custom => "http://localhost:8080/v1".into(),
            });

        if is_local {
            match loom_inference::InferenceEngine::connect(&base_url, &model, config.context_size).await {
                Ok(engine) => Some(Arc::new(engine) as Arc<dyn CloudClient>),
                Err(e) => {
                    tracing::warn!(task, %model, error = %e, "failed to connect auxiliary inference engine");
                    None
                }
            }
        } else {
            let api_key = self
                .resolve_api_key(config.api_key_env.as_deref(), &config.backend)
                .await;

            match api_key {
                Some(key) => {
                    let is_anthropic = config.api_format.as_deref() == Some("anthropic")
                        || matches!(config.backend, loom_types::ModelBackend::Anthropic);
                    if is_anthropic {
                        Some(Arc::new(loom_inference::AnthropicClient::new(
                            key, model, base_url,
                        )) as Arc<dyn CloudClient>)
                    } else {
                        Some(Arc::new(loom_inference::OpenAIClient::new(
                            key, model, base_url, false,
                        )) as Arc<dyn CloudClient>)
                    }
                }
                None => {
                    tracing::warn!(task, %model_name, "no API key found for auxiliary model");
                    None
                }
            }
        }
    }

    // === Agent Loop ===

    /// Process a user message through the agent loop and return the response.
    /// Backward-compatible: uses the default agent config and session "default".
    pub async fn process_message(&self, user_message: &str) -> Result<TurnResult> {
        self.process_message_with_config(
            user_message,
            "default",
            &loom_types::AgentConfig::default(),
            None,
            vec![],
            vec![],
            "operate",
        )
        .await
    }

    /// Build the full system prompt by assembling agent persona, system instructions,
    /// user profile, selected skills, available skills, KG context, and workspace path.
    /// Shared by both `process_message_with_config` and `process_message_streaming`
    /// to eliminate code duplication.
    async fn build_full_system_prompt(
        &self,
        agent_config: &loom_types::AgentConfig,
        user_persona: &str,
        available_skills: &str,
        selected_skills: &[String],
        user_message: &str,
        session_id: &str,
        workspace_path: &Option<String>,
    ) -> String {
        let mut system_prompt = String::new();

        // 1. Agent persona — the agent's core identity, placed first for maximum effect
        if !agent_config.persona.is_empty() {
            system_prompt.push_str(&agent_config.persona);
        }
        // 2. System instructions — base prompt or agent-specific override
        if let Some(ref override_prompt) = agent_config.system_prompt_override
            && !override_prompt.is_empty()
        {
            if !system_prompt.is_empty() {
                system_prompt.push_str("\n\n");
            }
            system_prompt.push_str(override_prompt);
        } else {
            let base = self.build_system_prompt().await;
            if !base.is_empty() {
                if !system_prompt.is_empty() {
                    system_prompt.push_str("\n\n");
                }
                system_prompt.push_str(&base);
            }
        }
        // 3. User profile — learned facts about the user (context, not identity)
        if !user_persona.is_empty() {
            system_prompt.push_str(&format!("\n\n## User Profile\n{}", user_persona));
        }
        // 4a. Selected skills — inject full SKILL.md content for user-chosen skills
        if !selected_skills.is_empty() {
            let bodies = self.skill_state.read().await.bodies.clone();
            system_prompt.push_str("\n\n## Active Skills (User Selected)\nThe following skills are activated for this conversation. Follow their instructions directly — do NOT call use_skill for these.\n");
            for name in selected_skills {
                if let Some(body) = bodies.get(name) {
                    system_prompt.push_str(&format!("\n\n### Skill: {}\n{}", name, body));
                }
            }
        }
        // 4b. Available skills — name-only list for LLM autonomous use_skill calls
        if !available_skills.is_empty() {
            system_prompt.push_str(&format!("\n\n## Available Skills\n{}", available_skills));
        }

        // Inject knowledge graph context
        {
            let mem_guard = self.memory_store.read().await;
            if let Some(ref store) = *mem_guard {
                let candidates = extract_entity_candidates(user_message);
                let mut entities: Vec<&str> = vec!["USER"];
                for c in &candidates {
                    entities.push(c.as_str());
                }
                entities.truncate(6);
                match store.query_kg_context(&entities, 5, session_id).await {
                    Ok(kg) if !kg.is_empty() => {
                        system_prompt.push_str(&format!("\n\n{}", kg));
                    }
                    _ => {}
                }
            }
        }

        // Workspace path
        if let Some(ws) = workspace_path {
            system_prompt.push_str(&format!(
                "\n\n## 工作空间\n当前工作目录：{}\n所有相对路径都基于此目录。创建、读取、修改文件时优先使用此目录。",
                ws
            ));
        }

        system_prompt
    }

    /// Process a user message with a specific session and agent config.
    /// Uses the Agent state machine: Idle → Thinking → Completed (or Errored).
    /// `attached_images` are ContentPart::Image items to send directly to the model.
    /// `selected_skills` are skill names whose full content should be injected into the system prompt.
    pub async fn process_message_with_config(
        &self,
        user_message: &str,
        session_id: &str,
        agent_config: &loom_types::AgentConfig,
        thinking_budget: Option<usize>,
        attached_images: Vec<ContentPart>,
        selected_skills: Vec<String>,
        permission_mode: &str,
    ) -> Result<TurnResult> {
        tracing::info!(
            session_id,
            msg_len = user_message.len(),
            img_count = attached_images.len(),
            "[orchestrator] process_message_with_config ENTER"
        );

        // Read shared contexts BEFORE pool.spawn to avoid any lock interaction
        // with the agent pool state (especially when another message's
        // entity-extraction completion is about to write persona_context).
        let user_persona = {
            let p = self.persona_context.read().await;
            p.clone()
        };
        let skills = {
            let state = self.skill_state.read().await;
            state.context.clone()
        };

        // Register agent in pool
        let agent_id = self
            .pool
            .spawn(
                agent_config.clone(),
                None,
                SessionId::from(session_id.to_string()),
            )
            .await?;

        // ── System prompt assembly ──
        // Collect skill permissions for merged tool-call permissions.
        let mut merged_skill_permissions: Vec<SkillPermissionConfig> = Vec::new();
        if !selected_skills.is_empty() {
            let state = self.skill_state.read().await;
            for name in &selected_skills {
                if let Some(perms) = state.permissions.get(name) {
                    merged_skill_permissions.push(perms.clone());
                }
            }
        }

        // Get workspace path for this session
        let workspace_path = if let Some(ref store) = *self.memory_store.read().await {
            let session_ws = store.get_session_workspace(session_id).await.ok().flatten();
            if session_ws.is_some() {
                session_ws
            } else {
                store.get_default_workspace().await.ok().flatten()
            }
        } else {
            None
        };

        // Build full system prompt via shared method (persona, instructions, profile,
        // selected skills, available skills, KG context, workspace path).
        let system_prompt = self
            .build_full_system_prompt(
                agent_config,
                &user_persona,
                &skills,
                &selected_skills,
                user_message,
                session_id,
                &workspace_path,
            )
            .await;

        // Build default permissions based on permission_mode:
        // - "operate": allow everything (legacy behavior)
        // - "ask": allow everything but agent loop will prompt for medium/high risk
        // - "read_only": deny all writes and shell, allow reads only
        tracing::info!(
            session_id,
            permission_mode,
            "building base_permissions from permission_mode"
        );
        let base_permissions = match permission_mode {
            "read_only" => SkillPermissions {
                shell: false,
                fs_write: None,
                ..Default::default()
            },
            _ => SkillPermissions {
                shell: true,
                fs_write: Some(vec![]),
                ..Default::default()
            },
        };
        let default_permissions = if merged_skill_permissions.is_empty() {
            base_permissions
        } else {
            merge_multi_permissions(
                merged_skill_permissions.iter().map(Some),
                &base_permissions,
            )
        };

        // Compute smart prompt budget: 80% of the active model's context window
        let max_prompt_budget = {
            let configs = self.model_configs.read().await;
            let active = self.active_model_name.read().await;
            let ctx_window = active
                .as_deref()
                .and_then(|name| configs.values().find(|c| c.name == name))
                .map(|c| c.context_size)
                .unwrap_or(128_000); // default: 128K
            (ctx_window as f64 * 0.8) as usize
        };

        let loop_config = AgentLoopConfig {
            system_prompt,
            temperature: agent_config.temperature.unwrap_or(0.0),
            max_iterations: 100, // safety net — LLM decides when done
            thinking_budget,
            model_configs: self.model_configs.read().await.values().cloned().collect(),
            active_model_name: self.active_model_name.read().await.clone(),
            workspace_path,
            default_permissions,
            max_prompt_budget,
            hook_registry: Some(self.hook_registry.clone()),
            session_id: session_id.to_string(),
            agent_id: agent_id.to_string(),
            key_store: Some(self.key_store.clone()),
            loom_dir: Some(self.data_dir_path().to_path_buf()),
            permission_mode: permission_mode.to_string(),
            event_bus: Some(self.pool.event_bus().clone()),
            pending_permissions: Some(self.pending_permissions.clone()),
            ..Default::default()
        };
        tracing::info!(session_id, "[orchestrator] step: loop_config built");

        // Ensure history is loaded from DB if not already in cache (e.g. after restart or session switch)
        if self.session_history(session_id).await.is_empty() {
            tracing::info!(session_id, "[orchestrator] step: loading history from DB");
            let _ = self.load_history(session_id).await;
            tracing::info!(session_id, "[orchestrator] step: history loaded");
        }
        let history = self.session_history(session_id).await;
        tracing::info!(
            session_id,
            hist_len = history.len(),
            "[orchestrator] step: history ready"
        );
        let user_msg = user_message.to_string();
        let sid = session_id.to_string();

        // Transition: Idle → Thinking
        tracing::info!(session_id, "[orchestrator] step: pool.transition");
        let _ = self
            .pool
            .transition(&agent_id, AgentStatus::Thinking, Some("processing".into()))
            .await;
        tracing::info!(session_id, "[orchestrator] step: pool.transition done");

        // Fire SessionStart hook
        {
            let mut hook_ctx = HookContext {
                session_id: sid.clone(),
                agent_id: agent_id.to_string(),
                ..Default::default()
            };
            tracing::debug!(session_id = %sid, agent_id = %agent_id, "firing SessionStart hook");
            let _ = self
                .hook_registry
                .read()
                .await
                .fire(
                    &loom_plugins::hooks::HookEvent::SessionStart,
                    None,
                    &mut hook_ctx,
                )
                .await;
        }

        // Fire UserPromptSubmit hook
        {
            let mut hook_ctx = HookContext {
                session_id: sid.clone(),
                agent_id: agent_id.to_string(),
                user_message: Some(user_msg.clone()),
                ..Default::default()
            };
            let _result = self
                .hook_registry
                .read()
                .await
                .fire(
                    &loom_plugins::hooks::HookEvent::UserPromptSubmit,
                    None,
                    &mut hook_ctx,
                )
                .await;
            // Inject prompt hook output into system prompt
            if !hook_ctx.prompt_injections.is_empty() {
                let injections = hook_ctx.prompt_injections.join("\n");
                tracing::info!(
                    count = hook_ctx.prompt_injections.len(),
                    "hook prompt injections added to context"
                );
                // Note: injections would be appended to system prompt here.
                // For now, log them at debug level so they are visible but not
                // injected (future phase: pass to agent loop).
                tracing::debug!(injections = %injections, "hook prompt injections");
            }
        }

        tracing::info!(
            session_id,
            "[orchestrator] step: creating stream channel + forwarder"
        );
        // Create streaming channel and forward deltas to event bus
        let (delta_tx, mut delta_rx) = tokio::sync::mpsc::channel::<StreamDelta>(256);
        let event_bus = self.pool.event_bus().clone();
        let memory_store = self.memory_store.clone();
        let forward_agent_id = agent_id.clone();
        let forward_session_id = sid.clone();

        // Resolve current active model name + context window for TokenUsage events.
        // We read it once at turn start; if the user switches models mid-turn the
        // value still reflects the model that produced this response.
        let (active_model_name, active_context_window) = {
            let name = self
                .active_model_name
                .read()
                .await
                .clone()
                .unwrap_or_default();
            let ctx = self
                .model_configs
                .read()
                .await
                .get(&name)
                .map(|c| c.context_size)
                .unwrap_or(0);
            (name, ctx)
        };
        let usage_model = active_model_name.clone();
        let usage_ctx = active_context_window;

        let forward_handle = tokio::spawn(async move {
            let mut full_text = String::new();
            let mut started_tools: Vec<(String, String)> = Vec::new();
            let mut tool_results: std::collections::HashMap<String, bool> =
                std::collections::HashMap::new();
            let mut tool_result_contents: std::collections::HashMap<String, String> =
                std::collections::HashMap::new();
            let mut tool_args_acc: std::collections::HashMap<usize, String> =
                std::collections::HashMap::new();
            let mut pending_tool_announces: std::collections::HashMap<usize, (String, String)> =
                std::collections::HashMap::new();
            let mut announced_tools: std::collections::HashSet<String> =
                std::collections::HashSet::new();
            let mut delta_seq: u64 = 0;
            // Anthropic sends Usage split across message_start (input_tokens only) and
            // message_delta (output_tokens only).  Accumulate partials until we have a
            // complete picture, otherwise the front-end ContextRing flashes between
            // prompt-only and completion-only states.
            let mut partial_prompt: u64 = 0;
            let mut partial_cache_read: u64 = 0;
            let mut usage_pending = false;

            while let Some(delta) = delta_rx.recv().await {
                match delta {
                    StreamDelta::Text(t) => {
                        delta_seq += 1;
                        full_text.push_str(&t);
                        tracing::debug!(seq = delta_seq, delta = %t, "forward_handle Text delta");
                        let _ = event_bus.publish(AgentEvent::StreamDelta {
                            agent_id: forward_agent_id.clone(),
                            session_id: forward_session_id.clone(),
                            delta: t,
                        });
                    }
                    StreamDelta::Reasoning(t) => {
                        let _ = event_bus.publish(AgentEvent::StreamDelta {
                            agent_id: forward_agent_id.clone(),
                            session_id: forward_session_id.clone(),
                            delta: format!("\x02REASONING\x02{}", t),
                        });
                    }
                    StreamDelta::ToolCallBegin { index, id, name } => {
                        if announced_tools.insert(id.clone()) {
                            started_tools.push((id.clone(), name.clone()));
                            // Publish immediately so the frontend shows the
                            // tool drawer / terminal as soon as the LLM
                            // starts calling a tool — don't wait for args.
                            let _ = event_bus.publish(AgentEvent::ToolStarted {
                                agent_id: forward_agent_id.clone(),
                                call_id: id.clone(),
                                tool_name: name.clone(),
                                args: serde_json::json!({}),
                            });
                        }
                        // Also track for later args accumulation
                        pending_tool_announces.insert(index, (id, name));
                    }
                    StreamDelta::ToolCallArgsChunk { index, chunk } => {
                        tool_args_acc.entry(index).or_default().push_str(&chunk);
                        // Remove from pending once args arrive — the tool is
                        // already announced above, but we clear pending so
                        // cleanup doesn't double-announce.
                        pending_tool_announces.remove(&index);
                    }
                    StreamDelta::ToolResult {
                        call_id,
                        tool_name,
                        success,
                        result,
                        structured_content,
                    } => {
                        tool_results.insert(call_id.clone(), success);
                        // Emit ToolCompleted immediately so the frontend updates in real-time
                        let _ = event_bus.publish(AgentEvent::ToolCompleted {
                            agent_id: forward_agent_id.clone(),
                            call_id: call_id.clone(),
                            tool_name: tool_name.clone(),
                            success,
                            result: result.clone(),
                            structured_content,
                        });
                        if let Some(r) = result {
                            tool_result_contents.insert(call_id, r);
                        }
                    }
                    StreamDelta::Image { media_type, data } => {
                        let _ = event_bus.publish(AgentEvent::StreamDelta {
                            agent_id: forward_agent_id.clone(),
                            session_id: forward_session_id.clone(),
                            delta: format!("\x02IMAGE\x02{};{}", media_type, data),
                        });
                    }
                    StreamDelta::Usage {
                        prompt_tokens,
                        completion_tokens,
                        cache_read_tokens,
                        cache_write_tokens,
                        ..
                    } => {
                        // Merge Anthropic split-usage deltas (message_start + message_delta).
                        // OpenAI / local engines send complete usage in a single delta.
                        let (p_tokens, c_tokens, cr_tokens, cw_tokens) =
                            if prompt_tokens > 0 && completion_tokens == 0 {
                                // Anthropic message_start — store partial, don't publish yet
                                partial_prompt = prompt_tokens;
                                partial_cache_read = cache_read_tokens;
                                usage_pending = true;
                                tracing::debug!(
                                    prompt = prompt_tokens,
                                    "[token-stats] partial usage (message_start), waiting for message_delta"
                                );
                                continue;
                            } else if prompt_tokens == 0 && completion_tokens > 0 && usage_pending {
                                // Anthropic message_delta — merge with stored partial
                                usage_pending = false;
                                let merged = (
                                    partial_prompt,
                                    completion_tokens,
                                    partial_cache_read,
                                    cache_write_tokens,
                                );
                                tracing::debug!(
                                    prompt = partial_prompt,
                                    completion = completion_tokens,
                                    "[token-stats] merged partial usage (message_delta)"
                                );
                                merged
                            } else {
                                // OpenAI / local engine complete delta, or no pending partial
                                usage_pending = false;
                                (
                                    prompt_tokens,
                                    completion_tokens,
                                    cache_read_tokens,
                                    cache_write_tokens,
                                )
                            };
                        let _ = event_bus.publish(AgentEvent::TokenUsage {
                            agent_id: forward_agent_id.clone(),
                            session_id: forward_session_id.clone(),
                            model: usage_model.clone(),
                            prompt_tokens: p_tokens as usize,
                            completion_tokens: c_tokens as usize,
                            cached_tokens: (cr_tokens + cw_tokens) as usize,
                            latency_ms: 0,
                            context_window: usage_ctx,
                        });
                        // Persist token usage to SQLite for historical stats
                        tracing::info!(
                            session_id = %forward_session_id,
                            model = %usage_model,
                            prompt = p_tokens,
                            completion = c_tokens,
                            cache_read = cr_tokens,
                            cache_write = cw_tokens,
                            "[token-stats] received StreamDelta::Usage for main model"
                        );
                        if let Some(store) = &*memory_store.read().await {
                            if let Err(e) = store
                                .record_token_usage(
                                    &forward_session_id,
                                    &usage_model,
                                    p_tokens as usize,
                                    c_tokens as usize,
                                    cr_tokens as usize,
                                    cw_tokens as usize,
                                    0, // latency not tracked at StreamDelta level
                                    usage_ctx,
                                )
                                .await
                            {
                                tracing::warn!(error = %e, model = %usage_model, "[token-stats] failed to record main model token usage");
                            } else {
                                tracing::info!(model = %usage_model, prompt = p_tokens, completion = c_tokens, "[token-stats] main model token usage recorded");
                            }
                        } else {
                            tracing::warn!("[token-stats] memory_store is None, cannot record main model usage");
                        }
                    }
                    StreamDelta::AuxiliaryUsage {
                        model,
                        prompt_tokens,
                        completion_tokens,
                    } => {
                        // Persist auxiliary model token usage under its own model name
                        let _ = event_bus.publish(AgentEvent::TokenUsage {
                            agent_id: forward_agent_id.clone(),
                            session_id: forward_session_id.clone(),
                            model: model.clone(),
                            prompt_tokens: prompt_tokens as usize,
                            completion_tokens: completion_tokens as usize,
                            cached_tokens: 0,
                            latency_ms: 0,
                            context_window: 0,
                        });
                        if let Some(store) = &*memory_store.read().await {
                            if let Err(e) = store
                                .record_token_usage(
                                    &forward_session_id,
                                    &model,
                                    prompt_tokens as usize,
                                    completion_tokens as usize,
                                    0,
                                    0,
                                    0,
                                    0,
                                )
                                .await
                            {
                                tracing::warn!(error = %e, model = %model, "failed to record auxiliary model token usage (stream)");
                            }
                        }
                    }
                }
            }
            // Emit ToolStarted for any tools that had no args chunks
            for (index, (id, name)) in pending_tool_announces {
                if announced_tools.insert(id.clone()) {
                    started_tools.push((id.clone(), name.clone()));
                    let args_str = tool_args_acc.get(&index).cloned().unwrap_or_default();
                    let args: serde_json::Value =
                        serde_json::from_str(&args_str).unwrap_or(serde_json::Value::Null);
                    let _ = event_bus.publish(AgentEvent::ToolStarted {
                        agent_id: forward_agent_id.clone(),
                        call_id: id,
                        tool_name: name,
                        args,
                    });
                }
            }
            // Emit ToolCompleted for any tools that were started but never
            // received a ToolResult (e.g. request_tools meta-tool)
            for (call_id, tool_name) in &started_tools {
                if tool_results.contains_key(call_id) {
                    continue; // already emitted via ToolResult handler
                }
                let _ = event_bus.publish(AgentEvent::ToolCompleted {
                    agent_id: forward_agent_id.clone(),
                    call_id: call_id.clone(),
                    tool_name: tool_name.clone(),
                    success: true,
                    result: None,
                    structured_content: None,
                });
            }
            // Send StreamEnd when channel closes
            let _ = event_bus.publish(AgentEvent::StreamEnd {
                agent_id: forward_agent_id.clone(),
                session_id: forward_session_id.clone(),
                full_response: full_text,
            });
        });

        // Run the agent turn with streaming
        // IMPORTANT: clone the Arc<dyn CloudClient> and immediately release the
        // RwLock so concurrent model switches (write lock) are never blocked by
        // a long-running turn holding a read lock. This prevents the deadlock:
        //   Session A holds cloud_client.read() for full turn
        //   → model_switch tries write() → blocked
        //   → Tokio RwLock blocks new readers while writer waits
        //   → Session B's process_message_with_config blocks on read()
        tracing::info!(session_id, "[orchestrator] step: reading cloud_client");
        let client: Arc<dyn CloudClient> = {
            let guard = self.cloud_client.read().await;
            match guard.as_ref() {
                Some(c) => c.clone(), // clone Arc, release lock immediately
                None => return Err(anyhow::anyhow!("No cloud client configured")),
            }
        }; // guard (read lock) released here
        tracing::info!(
            session_id,
            "[orchestrator] step: cloud_client acquired, running agent loop"
        );
        let registry = self.tool_registry.clone();
        let allowed = agent_config.allowed_tools.clone();
        let disallowed = agent_config.disallowed_tools.clone();
        let cancel = self.pool.cancel_token(&agent_id).await?;

        let result = {
            let reg = registry.read().await;
            run_agent_turn_streaming_with_images(
                client.as_ref(),
                &reg,
                &history,
                &user_msg,
                &attached_images,
                &loop_config,
                delta_tx,
                &allowed,
                &disallowed,
                &cancel,
            )
            .await
        };

        // Wait for forwarder to finish flushing
        drop(client);
        drop(registry);
        let _ = forward_handle.await;

        if let Ok(ref turn) = result {
            let was_interrupted = turn.response == "[已中断]";

            // Stop hook is already fired by agent_loop on cancellation; do not double-fire.

            let _ = self
                .pool
                .transition(
                    &agent_id,
                    if was_interrupted { AgentStatus::Killed } else { AgentStatus::Completed },
                    None,
                )
                .await;

            if was_interrupted {
                // Save user message so it isn't lost, skip the partial assistant response
                let user_msg_full = build_user_message(&user_msg, &attached_images);
                let mut user_parts = user_msg_full.content.clone();
                let _ = self
                    .convert_images_to_refs(session_id, &mut user_parts)
                    .await;
                self.add_to_history(
                    session_id,
                    Message {
                        role: Role::User,
                        content: user_parts.clone(),
                        timestamp: user_msg_full.timestamp,
                        usage: user_msg_full.usage,
                    },
                )
                .await;
                let mem = self.memory_store.read().await;
                if let Some(ref store) = *mem {
                    let user_json =
                        serde_json::to_string(&user_parts).unwrap_or_else(|_| user_msg.clone());
                    let _ = store.save_interrupted_turn(session_id, &user_json).await;
                }
            } else {
                // Convert images to file refs before caching / persisting
                let user_msg_full = build_user_message(&user_msg, &attached_images);
                let mut user_parts = user_msg_full.content.clone();
                if let Err(e) = self
                    .convert_images_to_refs(session_id, &mut user_parts)
                    .await
                {
                    tracing::warn!(session_id, error = %e, "failed to convert user images to file refs");
                }

                self.add_to_history(
                    session_id,
                    Message {
                        role: Role::User,
                        content: user_parts.clone(),
                        timestamp: user_msg_full.timestamp,
                        usage: user_msg_full.usage,
                    },
                )
                .await;

            let mut assistant_parts = if turn.content_parts.is_empty() {
                vec![ContentPart::Text {
                    text: turn.response.clone(),
                }]
            } else {
                turn.content_parts.clone()
            };
            if let Err(e) = self
                .convert_images_to_refs(session_id, &mut assistant_parts)
                .await
            {
                tracing::warn!(session_id, error = %e, "failed to convert assistant images to file refs");
            }
            let content_json = serde_json::to_string(&assistant_parts)
                .unwrap_or_else(|_| serde_json::json!([{"text": turn.response}]).to_string());

            self.add_to_history(
                session_id,
                Message {
                    role: Role::Assistant,
                    content: assistant_parts.clone(),
                    timestamp: chrono::Utc::now(),
                    usage: None,
                },
            )
            .await;

            // Persist intermediate tool-call and tool-result messages to history
            for tool_msg in &turn.tool_messages {
                self.add_to_history(session_id, tool_msg.clone()).await;
            }

            // Persist to memory store
            let mem = self.memory_store.read().await;
            if let Some(ref store) = *mem {
                let user_content_json =
                    serde_json::to_string(&user_parts).unwrap_or_else(|_| user_msg.clone());
                let event_id = store
                    .save_turn(
                        session_id,
                        &user_content_json,
                        &content_json,
                        turn.tool_calls_made,
                        turn.prompt_tokens,
                        turn.completion_tokens,
                    )
                    .await?;

                // LLM-based entity extraction (synchronous, runs after response)
                let msg = user_message.to_string();

                // Try auxiliary client first, fall back to main client
                let (client_opt, extract_model) = if let Some(aux_client) = self.build_auxiliary_client("entity").await {
                    let model_name = aux_client.model_name().to_string();
                    (Some(aux_client), model_name)
                } else {
                    let main_client: Option<Arc<dyn CloudClient>> = self.cloud_client.read().await.clone();
                    let model_name = match main_client.as_ref() {
                        Some(c) => c.model_name().to_string(),
                        None => self.active_model_name.read().await.clone().unwrap_or_default(),
                    };
                    (main_client, model_name)
                };

                if let Some(ref client) = client_opt {
                    match llm_extract_entities(client.as_ref(), &msg, session_id).await {
                        Ok((entities, relationships, ext_prompt, ext_completion)) => {
                            // Record entity extraction token usage
                            if ext_prompt > 0 || ext_completion > 0 {
                                if let Err(e) = store
                                    .record_token_usage(
                                        session_id,
                                        &extract_model,
                                        ext_prompt,
                                        ext_completion,
                                        0,
                                        0,
                                        0,
                                        0,
                                    )
                                    .await
                                {
                                    tracing::warn!(error = %e, model = %extract_model, "failed to record entity extraction token usage");
                                }
                            }
                            if let Ok((n, e)) = store
                                .feed_knowledge_graph(&entities, &relationships, event_id, session_id)
                                .await
                            {
                                let _ = store
                                    .save_extracted_entities(&entities, &relationships, session_id)
                                    .await;
                                if n > 0 || e > 0 {
                                    tracing::info!(n, e, "KG updated via LLM");
                                }
                            }
                            if let Ok(persona) = store.get_persona().await
                                && !persona.is_empty()
                            {
                                *self.persona_context.write().await = persona;
                            }
                        }
                        Err(e) => tracing::debug!("LLM extraction: {}", e),
                    }
                }
                drop(client_opt);
            }

            // Fire TaskCompleted hook
            {
                let mut hook_ctx = HookContext {
                    session_id: sid.clone(),
                    agent_id: agent_id.to_string(),
                    ..Default::default()
                };
                tracing::debug!(session_id = %sid, agent_id = %agent_id, "firing TaskCompleted hook");
                let _ = self
                    .hook_registry
                    .read()
                    .await
                    .fire(
                        &loom_plugins::hooks::HookEvent::TaskCompleted,
                        None,
                        &mut hook_ctx,
                    )
                    .await;
            }
            } // end else (not interrupted)
        } else {
            let _ = self
                .pool
                .transition(
                    &agent_id,
                    AgentStatus::Errored {
                        message: "LLM error".into(),
                    },
                    None,
                )
                .await;
        }

        // Fire SessionEnd hook
        {
            let mut hook_ctx = HookContext {
                session_id: sid.clone(),
                agent_id: agent_id.to_string(),
                ..Default::default()
            };
            tracing::debug!(session_id = %sid, agent_id = %agent_id, "firing SessionEnd hook");
            let _ = self
                .hook_registry
                .read()
                .await
                .fire(
                    &loom_plugins::hooks::HookEvent::SessionEnd,
                    None,
                    &mut hook_ctx,
                )
                .await;
        }

        // Clean up agent from pool
        let _ = self.pool.remove(&agent_id).await;

        tracing::info!(
            session_id,
            is_ok = result.is_ok(),
            "[orchestrator] process_message_with_config EXIT"
        );
        result
    }

    /// Process a user message with streaming deltas sent over the channel.
    pub async fn process_message_streaming(
        &self,
        user_message: &str,
        delta_tx: mpsc::Sender<StreamDelta>,
        session_id: &str,
        thinking_budget: Option<usize>,
        attached_images: Vec<ContentPart>,
        selected_skills: Vec<String>,
    ) -> Result<TurnResult> {
        // Read shared contexts BEFORE pool.spawn (same reason as
        // process_message_with_config).
        let user_persona = {
            let p = self.persona_context.read().await;
            p.clone()
        };
        let skills = {
            let state = self.skill_state.read().await;
            state.context.clone()
        };

        // Resolve agent config for this session (falls back to "default")
        let agent_config = self.resolve_session_agent_config(session_id).await;

        // Register agent in pool
        let agent_id = self
            .pool
            .spawn(
                agent_config.clone(),
                None,
                SessionId::from(session_id.to_string()),
            )
            .await?;

        // Transition: Idle → Thinking
        let _ = self
            .pool
            .transition(&agent_id, AgentStatus::Thinking, Some("processing".into()))
            .await;

        let sid = session_id.to_string();
        let user_msg = user_message.to_string();

        // Fire SessionStart hook
        {
            let mut hook_ctx = HookContext {
                session_id: sid.clone(),
                agent_id: agent_id.to_string(),
                ..Default::default()
            };
            tracing::debug!(session_id = %sid, agent_id = %agent_id, "firing SessionStart hook");
            let _ = self
                .hook_registry
                .read()
                .await
                .fire(&loom_plugins::hooks::HookEvent::SessionStart, None, &mut hook_ctx)
                .await;
        }

        // Fire UserPromptSubmit hook
        {
            let mut hook_ctx = HookContext {
                session_id: sid.clone(),
                agent_id: agent_id.to_string(),
                user_message: Some(user_msg.clone()),
                ..Default::default()
            };
            let _ = self
                .hook_registry
                .read()
                .await
                .fire(
                    &loom_plugins::hooks::HookEvent::UserPromptSubmit,
                    None,
                    &mut hook_ctx,
                )
                .await;
            if !hook_ctx.prompt_injections.is_empty() {
                tracing::debug!(
                    count = hook_ctx.prompt_injections.len(),
                    "hook prompt injections"
                );
            }
        }

        let client: Arc<dyn CloudClient> = {
            let guard = self.cloud_client.read().await;
            guard
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("No cloud client configured"))?
                .clone()
        }; // read lock released immediately
        let registry = self.tool_registry.read().await;
        let history = self.session_history(session_id).await;

        // Get workspace path for this session
        let workspace_path = if let Some(ref store) = *self.memory_store.read().await {
            let session_ws = store.get_session_workspace(session_id).await.ok().flatten();
            if session_ws.is_some() {
                session_ws
            } else {
                store.get_default_workspace().await.ok().flatten()
            }
        } else {
            None
        };

        // ── Summary check (P0 memory optimization) ──
        // Phase 1: read existing summary (lock held briefly)
        // KG context is handled by build_full_system_prompt below.
        let (existing_summary, should_summarize) = {
            let mem = self.memory_store.read().await;
            if let Some(ref store) = *mem {
                let existing = store.get_summary(session_id).await.unwrap_or(None);
                let last_at = store.get_summary_at_count(session_id).await.unwrap_or(0);
                // Use DB message_count (not truncated history.len()) for accurate summary trigger
                let total_msgs = store
                    .get_message_count(session_id)
                    .await
                    .unwrap_or(history.len());
                let do_summarize =
                    loom_memory::SummaryEngine::should_summarize(total_msgs, last_at, 12, 6);
                (existing, do_summarize)
            } else {
                (None, false)
            }
        }; // lock dropped here

        // Fire PreCompact hook before summarization
        if should_summarize {
            let mut hook_ctx = HookContext {
                session_id: sid.clone(),
                agent_id: agent_id.to_string(),
                ..Default::default()
            };
            tracing::debug!(session_id = %sid, "firing PreCompact hook");
            let _ = self
                .hook_registry
                .read()
                .await
                .fire(
                    &loom_plugins::hooks::HookEvent::PreCompact,
                    None,
                    &mut hook_ctx,
                )
                .await;
        }

        // Phase 2: call LLM for summary if needed (no lock held)
        let summary = if should_summarize {
            let prompt =
                loom_memory::SummaryEngine::build_prompt(&history, existing_summary.as_deref());
            let request = loom_memory::SummaryEngine::build_request(&prompt);

            // Try auxiliary client first, fall back to main client
            let (summary_client, summary_model) = if let Some(aux_client) = self.build_auxiliary_client("summary").await {
                let model_name = aux_client.model_name().to_string();
                (aux_client, model_name)
            } else {
                let model_name = client.model_name().to_string();
                (client.clone(), model_name)
            };

            let saved_hash = summary_client.prefix_hash_snapshot();
            let result = summary_client.complete(request).await;
            summary_client.prefix_hash_restore(saved_hash);
            match result {
                Ok(resp) if !resp.text.is_empty() => {
                    let new_summary = resp.text;
                    // Record summary token usage
                    let sum_prompt = resp.prompt_tokens;
                    let sum_completion = resp.completion_tokens;
                    if sum_prompt > 0 || sum_completion > 0 {
                        if let Some(ref store) = *self.memory_store.read().await {
                            if let Err(e) = store
                                .record_token_usage(
                                    session_id,
                                    &summary_model,
                                    sum_prompt,
                                    sum_completion,
                                    0,
                                    0,
                                    0,
                                    0,
                                )
                                .await
                            {
                                tracing::warn!(error = %e, model = %summary_model, "failed to record summary token usage");
                            }
                        }
                    }
                    // Phase 3: save summary (re-acquire lock briefly)
                    if let Some(ref store) = *self.memory_store.read().await {
                        let _ = store.save_summary(session_id, &new_summary).await;
                    }
                    tracing::info!(chars = new_summary.len(), "conversation summarized");
                    Some(new_summary)
                }
                _ => existing_summary,
            }
        } else {
            existing_summary
        };

        // ── System prompt assembly (shared method) ──
        let system_prompt = self
            .build_full_system_prompt(
                &agent_config,
                &user_persona,
                &skills,
                &selected_skills,
                user_message,
                session_id,
                &workspace_path,
            )
            .await;

        // Collect skill permissions for merged tool-call permissions
        let mut merged_skill_permissions: Vec<SkillPermissionConfig> = Vec::new();
        if !selected_skills.is_empty() {
            let state = self.skill_state.read().await;
            for name in &selected_skills {
                if let Some(perms) = state.permissions.get(name) {
                    merged_skill_permissions.push(perms.clone());
                }
            }
        }
        let base_permissions = SkillPermissions {
            shell: true,
            fs_write: Some(vec![]),
            ..Default::default()
        };
        let default_permissions = if merged_skill_permissions.is_empty() {
            base_permissions
        } else {
            merge_multi_permissions(
                merged_skill_permissions.iter().map(Some),
                &base_permissions,
            )
        };

        let allowed = agent_config.allowed_tools.clone();
        let disallowed = agent_config.disallowed_tools.clone();
        let cancel = self.pool.cancel_token(&agent_id).await?;

        // Compute smart prompt budget: 80% of active model's context window
        let max_prompt_budget = {
            let configs = self.model_configs.read().await;
            let active = self.active_model_name.read().await;
            let ctx_window = active
                .as_deref()
                .and_then(|name| configs.values().find(|c| c.name == name))
                .map(|c| c.context_size)
                .unwrap_or(128_000);
            (ctx_window as f64 * 0.8) as usize
        };

        let config = AgentLoopConfig {
            system_prompt,
            // persona already baked into system_prompt via build_full_system_prompt
            persona: None,
            summary,
            kg_context: None,
            thinking_budget,
            model_configs: self.model_configs.read().await.values().cloned().collect(),
            active_model_name: self.active_model_name.read().await.clone(),
            workspace_path,
            default_permissions,
            max_prompt_budget,
            hook_registry: Some(self.hook_registry.clone()),
            session_id: session_id.to_string(),
            agent_id: agent_id.to_string(),
            key_store: Some(self.key_store.clone()),
            loom_dir: Some(self.data_dir_path().to_path_buf()),
            permission_mode: "operate".to_string(), // CLI always uses operate mode
            event_bus: Some(self.pool.event_bus().clone()),
            pending_permissions: Some(self.pending_permissions.clone()),
            ..Default::default()
        };

        let result = run_agent_turn_streaming_with_images(
            client.as_ref(),
            &registry,
            &history,
            user_message,
            &attached_images,
            &config,
            delta_tx,
            &allowed,
            &disallowed,
            &cancel,
        )
        .await;

        drop(registry);

        if let Ok(ref turn) = result {
            let was_interrupted = turn.response == "[已中断]";

            // Stop hook is already fired by agent_loop on cancellation; do not double-fire.

            let _ = self
                .pool
                .transition(
                    &agent_id,
                    if was_interrupted { AgentStatus::Killed } else { AgentStatus::Completed },
                    None,
                )
                .await;

            if was_interrupted {
                // Save user message so it isn't lost, skip the partial assistant response
                let user_msg_full = build_user_message(user_message, &attached_images);
                let mut user_parts = user_msg_full.content.clone();
                let _ = self
                    .convert_images_to_refs(session_id, &mut user_parts)
                    .await;
                self.add_to_history(
                    session_id,
                    Message {
                        role: Role::User,
                        content: user_parts.clone(),
                        timestamp: user_msg_full.timestamp,
                        usage: user_msg_full.usage,
                    },
                )
                .await;
                let mem = self.memory_store.read().await;
                if let Some(ref store) = *mem {
                    let user_json =
                        serde_json::to_string(&user_parts).unwrap_or_else(|_| user_message.to_string());
                    let _ = store.save_interrupted_turn(session_id, &user_json).await;
                }
            } else {
                // Convert images to file refs before caching / persisting
                let user_msg_full = build_user_message(user_message, &attached_images);
                let mut user_parts = user_msg_full.content.clone();
                if let Err(e) = self
                    .convert_images_to_refs(session_id, &mut user_parts)
                    .await
                {
                    tracing::warn!(session_id, error = %e, "failed to convert user images to file refs");
                }

                self.add_to_history(
                    session_id,
                    Message {
                        role: Role::User,
                        content: user_parts.clone(),
                        timestamp: user_msg_full.timestamp,
                        usage: user_msg_full.usage,
                    },
                )
                .await;

                let mut assistant_parts = if turn.content_parts.is_empty() {
                    vec![ContentPart::Text {
                        text: turn.response.clone(),
                    }]
                } else {
                    turn.content_parts.clone()
                };
                if let Err(e) = self
                    .convert_images_to_refs(session_id, &mut assistant_parts)
                    .await
                {
                    tracing::warn!(session_id, error = %e, "failed to convert assistant images to file refs");
                }
                let content_json = serde_json::to_string(&assistant_parts)
                    .unwrap_or_else(|_| serde_json::json!([{"text": turn.response}]).to_string());

                self.add_to_history(
                    session_id,
                    Message {
                        role: Role::Assistant,
                        content: assistant_parts.clone(),
                        timestamp: chrono::Utc::now(),
                        usage: None,
                    },
                )
                .await;

                // Persist intermediate tool-call and tool-result messages to history
                for tool_msg in &turn.tool_messages {
                    self.add_to_history(session_id, tool_msg.clone()).await;
                }

                // Persist to memory store
                let mem = self.memory_store.read().await;
                if let Some(ref store) = *mem {
                    let user_content_json =
                        serde_json::to_string(&user_parts).unwrap_or_else(|_| user_message.to_string());
                    let event_id = store
                        .save_turn(
                            session_id,
                            &user_content_json,
                            &content_json,
                            turn.tool_calls_made,
                            turn.prompt_tokens,
                            turn.completion_tokens,
                        )
                        .await?;

                    // LLM-based entity extraction (synchronous, runs after response)
                    let msg = user_message.to_string();

                    // Try auxiliary client first, fall back to main client
                    let (client_opt, extract_model) = if let Some(aux_client) = self.build_auxiliary_client("entity").await {
                        let model_name = aux_client.model_name().to_string();
                        (Some(aux_client), model_name)
                    } else {
                        let main_client: Option<Arc<dyn CloudClient>> = self.cloud_client.read().await.clone();
                        let model_name = match main_client.as_ref() {
                            Some(c) => c.model_name().to_string(),
                            None => self.active_model_name.read().await.clone().unwrap_or_default(),
                        };
                        (main_client, model_name)
                    };

                    if let Some(ref client) = client_opt {
                        match llm_extract_entities(client.as_ref(), &msg, session_id).await {
                            Ok((entities, relationships, ext_prompt, ext_completion)) => {
                                // Record entity extraction token usage
                                if ext_prompt > 0 || ext_completion > 0 {
                                    let _ = store
                                        .record_token_usage(
                                            session_id,
                                            &extract_model,
                                            ext_prompt,
                                            ext_completion,
                                            0,
                                            0,
                                            0,
                                            0,
                                        )
                                        .await;
                                }
                                if let Ok((n, e)) = store
                                    .feed_knowledge_graph(&entities, &relationships, event_id, session_id)
                                    .await
                                {
                                    let _ = store
                                        .save_extracted_entities(&entities, &relationships, session_id)
                                        .await;
                                    if n > 0 || e > 0 {
                                        tracing::info!(n, e, "KG updated via LLM");
                                    }
                                }
                                if let Ok(persona) = store.get_persona().await
                                    && !persona.is_empty()
                                {
                                    *self.persona_context.write().await = persona;
                                }
                            }
                            Err(e) => tracing::debug!("LLM extraction: {}", e),
                        }
                    }
                    drop(client_opt);
                }

            // Fire TaskCompleted hook
            {
                let mut hook_ctx = HookContext {
                    session_id: sid.clone(),
                    agent_id: agent_id.to_string(),
                    ..Default::default()
                };
                tracing::debug!(session_id = %sid, agent_id = %agent_id, "firing TaskCompleted hook");
                let _ = self
                    .hook_registry
                    .read()
                    .await
                    .fire(
                        &loom_plugins::hooks::HookEvent::TaskCompleted,
                        None,
                        &mut hook_ctx,
                    )
                    .await;
            }
            } // end else (not interrupted)
        } else {
            let _ = self
                .pool
                .transition(
                    &agent_id,
                    AgentStatus::Errored {
                        message: "LLM error".into(),
                    },
                    None,
                )
                .await;
        }

        // Fire SessionEnd hook
        {
            let mut hook_ctx = HookContext {
                session_id: sid.clone(),
                agent_id: agent_id.to_string(),
                ..Default::default()
            };
            tracing::debug!(session_id = %sid, agent_id = %agent_id, "firing SessionEnd hook");
            let _ = self
                .hook_registry
                .read()
                .await
                .fire(
                    &loom_plugins::hooks::HookEvent::SessionEnd,
                    None,
                    &mut hook_ctx,
                )
                .await;
        }

        // Clean up agent from pool
        let _ = self.pool.remove(&agent_id).await;

        result
    }

    pub fn event_bus(&self) -> &EventBus {
        self.pool.event_bus()
    }

    /// Get a clone of the pending permissions map for "ask" mode tool approval.
    pub async fn pending_permissions(
        &self,
    ) -> tokio::sync::RwLockWriteGuard<'_, HashMap<String, tokio::sync::oneshot::Sender<bool>>> {
        self.pending_permissions.write().await
    }

    pub async fn spawn_agent(
        &self,
        config: AgentConfig,
        parent_id: Option<loom_types::AgentId>,
        session_id: SessionId,
    ) -> Result<loom_types::AgentId> {
        self.pool.spawn(config, parent_id, session_id).await
    }
    pub async fn kill_agent(&self, agent_id: &loom_types::AgentId) -> Result<()> {
        self.pool.kill(agent_id).await
    }
    /// Stop all running agents for a session.
    pub async fn stop_session(&self, session_id: &str) -> Result<usize> {
        let agents = self.pool.list().await;
        let mut killed = 0usize;
        for a in agents {
            if a.session_id == session_id && !a.status.is_terminal() {
                let _ = self.pool.kill(&a.id).await;
                killed += 1;
            }
        }
        Ok(killed)
    }
    /// Graceful shutdown: cancel all inflight agents, drain with 10s timeout,
    /// close SQLite memory store, then drop MCP connections.
    pub async fn shutdown(&self) {
        // 1. Cancel all non-terminal agents so their loops exit
        let agents = self.pool.list().await;
        for a in &agents {
            if !a.status.is_terminal() {
                tracing::info!(
                    agent_id = %a.id,
                    session_id = %a.session_id,
                    status = ?a.status,
                    "shutdown: cancelling inflight agent"
                );
                if let Ok(token) = self.pool.cancel_token(&a.id).await {
                    token.cancel();
                }
            }
        }

        // 2. Wait up to 10 seconds for agents to reach terminal state
        let drain_timeout = tokio::time::Duration::from_secs(10);
        let drain_start = tokio::time::Instant::now();
        loop {
            let agents = self.pool.list().await;
            let inflight: Vec<_> = agents.iter().filter(|a| !a.status.is_terminal()).collect();
            if inflight.is_empty() {
                tracing::info!("shutdown: all agents drained");
                break;
            }
            if drain_start.elapsed() >= drain_timeout {
                tracing::warn!(
                    remaining = inflight.len(),
                    "shutdown: drain timeout — {} agents still inflight, forcing shutdown",
                    inflight.len()
                );
                break;
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }

        // 3. Close SQLite by dropping the memory store
        {
            let mut store = self.memory_store.write().await;
            if store.is_some() {
                tracing::info!("shutdown: closing memory store (SQLite)");
                *store = None;
            }
        }

        tracing::info!("shutdown: complete");
    }

    pub async fn list_agents(&self) -> Vec<AgentSummary> {
        self.pool.list().await
    }
    pub async fn agent_status(&self, agent_id: &loom_types::AgentId) -> Result<AgentSummary> {
        self.pool.summary(agent_id).await
    }
    pub async fn set_agent_status(
        &self,
        agent_id: &loom_types::AgentId,
        status: AgentStatus,
        message: Option<String>,
    ) -> Result<()> {
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
        &self,
        arguments: serde_json::Value,
        _progress: tokio::sync::mpsc::UnboundedSender<loom_types::ToolProgress>,
        _context: &crate::tool_context::ToolContext,
    ) -> Result<crate::tool_registry::ToolResult> {
        match self
            .mcp_client
            .call_tool(&self.server_name, &self.tool_name, arguments)
            .await
        {
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
        crate::tool_registry::ToolProvenance::Mcp {
            server: self.server_name.clone(),
        }
    }
}

// ============================================================================
// McpMetaTool — wraps MCP resource/prompt operations as AgentTools
// ============================================================================

enum McpMetaOp {
    ListResources,
    ReadResource,
    ListPrompts,
    GetPrompt,
}

struct McpMetaTool {
    server_name: String,
    op: McpMetaOp,
    tool_definition: loom_types::ToolDefinition,
    mcp_client: Arc<McpClient>,
}

#[async_trait::async_trait]
impl AgentTool for McpMetaTool {
    fn tool_name(&self) -> &str {
        &self.tool_definition.name
    }

    fn tool_definition(&self) -> loom_types::ToolDefinition {
        self.tool_definition.clone()
    }

    async fn execute(
        &self,
        arguments: serde_json::Value,
        _progress: tokio::sync::mpsc::UnboundedSender<loom_types::ToolProgress>,
        _context: &crate::tool_context::ToolContext,
    ) -> Result<crate::tool_registry::ToolResult> {
        let result: Result<serde_json::Value> = match &self.op {
            McpMetaOp::ListResources => {
                let resources = self.mcp_client.list_resources(&self.server_name).await?;
                Ok(serde_json::to_value(resources).unwrap_or_default())
            }
            McpMetaOp::ReadResource => {
                let uri = arguments.get("uri").and_then(|v| v.as_str()).unwrap_or("");
                if uri.is_empty() {
                    return Ok(crate::tool_registry::ToolResult {
                        content: "Error: 'uri' parameter is required".into(),
                        is_error: true,
                        structured_content: None,
                    });
                }
                let contents = self
                    .mcp_client
                    .read_resource(&self.server_name, uri)
                    .await?;
                Ok(serde_json::to_value(contents).unwrap_or_default())
            }
            McpMetaOp::ListPrompts => {
                let prompts = self.mcp_client.list_prompts(&self.server_name).await?;
                Ok(serde_json::to_value(prompts).unwrap_or_default())
            }
            McpMetaOp::GetPrompt => {
                let name = arguments.get("name").and_then(|v| v.as_str()).unwrap_or("");
                if name.is_empty() {
                    return Ok(crate::tool_registry::ToolResult {
                        content: "Error: 'name' parameter is required".into(),
                        is_error: true,
                        structured_content: None,
                    });
                }
                let args = arguments.get("arguments");
                let prompt_result = self
                    .mcp_client
                    .get_prompt(&self.server_name, name, args)
                    .await?;
                Ok(serde_json::to_value(prompt_result).unwrap_or_default())
            }
        };

        match result {
            Ok(value) => Ok(crate::tool_registry::ToolResult {
                content: serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".into()),
                is_error: false,
                structured_content: Some(value),
            }),
            Err(e) => Ok(crate::tool_registry::ToolResult {
                content: format!("MCP error: {}", e),
                is_error: true,
                structured_content: None,
            }),
        }
    }

    fn provenance(&self) -> crate::tool_registry::ToolProvenance {
        crate::tool_registry::ToolProvenance::Mcp {
            server: self.server_name.clone(),
        }
    }
}

// ============================================================================
// LspTool — wraps LSP operations as AgentTools
// ============================================================================

#[allow(dead_code)]
enum LspOp {
    Diagnostics,
    Completion,
    Hover,
    Definition,
    References,
    DocumentSymbols,
}

#[allow(dead_code)]
struct LspTool {
    op: LspOp,
    tool_definition: loom_types::ToolDefinition,
    lsp_client: Arc<LspClient>,
}

#[allow(dead_code)]
#[async_trait::async_trait]
impl AgentTool for LspTool {
    fn tool_name(&self) -> &str {
        &self.tool_definition.name
    }

    fn tool_definition(&self) -> loom_types::ToolDefinition {
        self.tool_definition.clone()
    }

    async fn execute(
        &self,
        arguments: serde_json::Value,
        _progress: tokio::sync::mpsc::UnboundedSender<loom_types::ToolProgress>,
        context: &crate::tool_context::ToolContext,
    ) -> Result<crate::tool_registry::ToolResult> {
        let file_path_raw = arguments
            .get("file_path")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if file_path_raw.is_empty() {
            return Ok(crate::tool_registry::ToolResult {
                content: "Error: 'file_path' parameter is required".into(),
                is_error: true,
                structured_content: None,
            });
        }
        let file_path = context.resolve_path(file_path_raw);
        let file_path_str = file_path.to_string_lossy().to_string();

        let result: Result<serde_json::Value> = match &self.op {
            LspOp::Diagnostics => self.lsp_client.diagnostics(&file_path_str).await,
            LspOp::Completion => {
                let line = arguments.get("line").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                let character = arguments
                    .get("character")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32;
                self.lsp_client.completion(&file_path_str, line, character).await
            }
            LspOp::Hover => {
                let line = arguments.get("line").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                let character = arguments
                    .get("character")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32;
                self.lsp_client.hover(&file_path_str, line, character).await
            }
            LspOp::Definition => {
                let line = arguments.get("line").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                let character = arguments
                    .get("character")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32;
                self.lsp_client.definition(&file_path_str, line, character).await
            }
            LspOp::References => {
                let line = arguments.get("line").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                let character = arguments
                    .get("character")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32;
                let include_decl = arguments
                    .get("include_declaration")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                self.lsp_client
                    .references(&file_path_str, line, character, include_decl)
                    .await
            }
            LspOp::DocumentSymbols => self.lsp_client.document_symbols(&file_path_str).await,
        };

        match result {
            Ok(value) => Ok(crate::tool_registry::ToolResult {
                content: serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".into()),
                is_error: false,
                structured_content: Some(value),
            }),
            Err(e) => Ok(crate::tool_registry::ToolResult {
                content: format!("LSP error: {}", e),
                is_error: true,
                structured_content: None,
            }),
        }
    }

    fn provenance(&self) -> crate::tool_registry::ToolProvenance {
        crate::tool_registry::ToolProvenance::Builtin
    }
}

#[allow(dead_code)]
fn register_lsp_tools(registry: &mut ToolRegistry, lsp_client: &Arc<LspClient>) {
    let tools: Vec<(LspOp, &str, &str, serde_json::Value)> = vec![
        (
            LspOp::Diagnostics,
            "lsp_diagnostics",
            "Get LSP diagnostics (errors, warnings, hints) for a file",
            serde_json::json!({"type":"object","properties":{"file_path":{"type":"string","description":"Path to the source file"}},"required":["file_path"]}),
        ),
        (
            LspOp::Completion,
            "lsp_completion",
            "Get code completion suggestions at a position in a file",
            serde_json::json!({"type":"object","properties":{"file_path":{"type":"string","description":"Path to the source file"},"line":{"type":"integer","description":"0-based line number"},"character":{"type":"integer","description":"0-based character offset"}},"required":["file_path","line","character"]}),
        ),
        (
            LspOp::Hover,
            "lsp_hover",
            "Get type info and documentation for the symbol at a position",
            serde_json::json!({"type":"object","properties":{"file_path":{"type":"string","description":"Path to the source file"},"line":{"type":"integer","description":"0-based line number"},"character":{"type":"integer","description":"0-based character offset"}},"required":["file_path","line","character"]}),
        ),
        (
            LspOp::Definition,
            "lsp_definition",
            "Go to definition — find where a symbol is defined",
            serde_json::json!({"type":"object","properties":{"file_path":{"type":"string","description":"Path to the source file"},"line":{"type":"integer","description":"0-based line number"},"character":{"type":"integer","description":"0-based character offset"}},"required":["file_path","line","character"]}),
        ),
        (
            LspOp::References,
            "lsp_references",
            "Find all references to a symbol",
            serde_json::json!({"type":"object","properties":{"file_path":{"type":"string","description":"Path to the source file"},"line":{"type":"integer","description":"0-based line number"},"character":{"type":"integer","description":"0-based character offset"},"include_declaration":{"type":"boolean","description":"Include the declaration in results (default: true)"}},"required":["file_path","line","character"]}),
        ),
        (
            LspOp::DocumentSymbols,
            "lsp_symbols",
            "List all symbols (functions, classes, variables) in a file",
            serde_json::json!({"type":"object","properties":{"file_path":{"type":"string","description":"Path to the source file"}},"required":["file_path"]}),
        ),
    ];

    for (op, name, desc, schema) in tools {
        let tags: Vec<String> = vec!["lsp".into(), "code".into(), name.to_string()];
        let _ = registry.register(Arc::new(LspTool {
            op,
            tool_definition: loom_types::ToolDefinition {
                name: name.to_string(),
                description: desc.to_string(),
                input_schema: schema,
                tags,
            },
            lsp_client: lsp_client.clone(),
        }));
    }
}

// ============================================================================
// LLM-based entity extraction
// ============================================================================

async fn llm_extract_entities(
    client: &dyn CloudClient,
    text: &str,
    scope: &str,
) -> Result<(Vec<ExtractedEntity>, Vec<ExtractedRelationship>, usize, usize)> {
    let prompt = format!("{}\n\nUser message: {}", LLM_EXTRACTION_PROMPT, text);

    let request = CompletionRequest {
        messages: vec![Message::user(&prompt)],
        tools: vec![],
        tool_choice: None,
        prompt,
        max_tokens: 1024,
        temperature: 0.0,
        top_p: 1.0,
        stop: vec![],
        stream: false,
        thinking_budget: None,
    };

    let response = client.complete(request).await?;
    let prompt_tokens = response.prompt_tokens;
    let completion_tokens = response.completion_tokens;
    let (mut entities, mut relationships) = parse_llm_extraction(&response.text, scope)?;

    // Normalize: USER entity should always be "USER"
    for e in &mut entities {
        if e.name.to_lowercase() == "user" {
            e.name = "USER".into();
            e.entity_type = "Person".into();
        }
    }
    for r in &mut relationships {
        if r.source_name.to_lowercase() == "user" {
            r.source_name = "USER".into();
        }
        if r.target_name.to_lowercase() == "user" {
            r.target_name = "USER".into();
        }
    }

    Ok((entities, relationships, prompt_tokens, completion_tokens))
}

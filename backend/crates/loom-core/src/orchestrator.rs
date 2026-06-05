//! Top-level orchestrator — wires AgentPool, ToolRegistry, McpClient,
//! inference, and the agent loop into a single entry point.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, Result};
use base64::Engine;
use loom_inference::engine::CloudClient;
use loom_lsp::LspClient;
use loom_mcp::McpClient;
use loom_memory::{
    EntityExtractor, ExtractedEntity, ExtractedRelationship, LLM_EXTRACTION_PROMPT,
    RuleBasedEntityExtractor, parse_llm_extraction,
};
use loom_security::merge_multi_permissions;
use loom_skills::SkillPermissionConfig;
use loom_types::{
    AgentConfig, CompletionRequest, ContentPart, Message, ModelBackend, Role, SandboxConfig,
    SessionId, SkillPermissions, StreamDelta,
};
use tokio::sync::{RwLock, mpsc};

// ── Phase 3: Pipeline Scheduler ─────────────────────────────────────────

/// Pipeline stage identifiers used by the scheduler to decide what to run next.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PipelineStage {
    Extraction,
    Generalization,
    Consolidation,
    Forgetting,
    QualityAudit,
}

impl std::fmt::Display for PipelineStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PipelineStage::Extraction => write!(f, "Extraction"),
            PipelineStage::Generalization => write!(f, "Generalization"),
            PipelineStage::Consolidation => write!(f, "Consolidation"),
            PipelineStage::Forgetting => write!(f, "Forgetting"),
            PipelineStage::QualityAudit => write!(f, "QualityAudit"),
        }
    }
}

/// Reports the entities and relationships removed during a forgetting cycle.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ForgettingReport {
    pub cycle_timestamp: String,
    pub nodes_removed: usize,
    pub edges_removed: usize,
    pub cognitions_removed: usize,
    pub min_importance_threshold: f64,
    pub max_age_days: i64,
    pub summary: String,
}

/// Health snapshot of the memory system.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MemoryHealth {
    pub total_nodes: usize,
    pub total_edges: usize,
    pub total_cognitions: usize,
    pub stale_nodes: usize,
    pub orphan_nodes: usize,
    pub layer_distribution: Vec<(String, i64)>,
    pub fragmentation_score: f64,
    pub status: String,
    pub checked_at: String,
}

/// Quality evaluation of memory recall over a lookback period.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MemoryQualityReport {
    pub lookback_days: i64,
    pub total_injections: usize,
    pub total_references: usize,
    pub recall_rate: f64,
    pub top_entities: Vec<String>,
    pub stale_entities: Vec<String>,
    pub quality_score: f64,
    pub recommendations: Vec<String>,
    pub evaluated_at: String,
}

/// Detected behavioral patterns for self-evolution.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BehaviorProfile {
    pub preferred_tools: Vec<(String, usize)>,
    pub frequent_topics: Vec<(String, usize)>,
    pub active_hours: Vec<(u32, usize)>,
    pub avg_turn_tokens: usize,
    pub skill_usage: Vec<(String, usize)>,
    pub extracted_at: String,
}

/// Derive agent behavior hints from the user's rich persona.
/// Returns a concise directive (~80-120 tokens) for the system prompt.
pub fn adapt_behavior(persona_text: &str) -> String {
    if persona_text.is_empty() {
        return String::new();
    }

    let mut hints = Vec::new();

    // Detect language preference from persona
    let has_chinese = persona_text.contains("中文") || persona_text.contains("Chinese");
    let has_english = persona_text.contains("English") || persona_text.contains("英文");
    if has_chinese && !has_english {
        hints.push("Respond in Chinese (中文) unless the user writes in English.");
    } else if has_english && !has_chinese {
        hints.push("Respond in English unless the user writes in Chinese.");
    }

    // Detect working style from persona traits
    if persona_text.contains("code-first") || persona_text.contains("CodeFirst") {
        hints.push("Show code implementations directly rather than describing them abstractly.");
    } else if persona_text.contains("plan-first") || persona_text.contains("PlanFirst") {
        hints.push("Outline the approach and get confirmation before writing implementation code.");
    }

    // Detect verbosity preference
    if persona_text.contains("concise") || persona_text.contains("Concise") {
        hints.push("Keep responses concise — prefer minimal explanations.");
    } else if persona_text.contains("detailed") || persona_text.contains("Detailed") {
        hints.push("Provide detailed explanations with context and reasoning.");
    }

    // Detect expertise level for adaptation
    if persona_text.contains("Beginner") {
        hints.push("Include more explanation and examples as the user is learning.");
    } else if persona_text.contains("Expert") {
        hints.push("Skip basic explanations — the user is experienced and prefers advanced discussions.");
    }

    if hints.is_empty() {
        return String::new();
    }

    format!("## Behavior Adaptation\n{}\n", hints.iter().map(|h| format!("- {}", h)).collect::<Vec<_>>().join("\n"))
}

/// Tracks pipeline events and gates stage transitions.
pub struct PipelineScheduler {
    extraction_count: u64,
    last_generalization: std::time::Instant,
    last_consolidation: std::time::Instant,
    last_forgetting: std::time::Instant,
    last_quality_audit: std::time::Instant,
    generalization_interval: std::time::Duration,
    consolidation_interval: std::time::Duration,
    forgetting_interval: std::time::Duration,
    quality_audit_interval: std::time::Duration,
    extractions_per_generalization: u64,
}

impl PipelineScheduler {
    pub fn new() -> Self {
        let now = std::time::Instant::now();
        Self {
            extraction_count: 0,
            last_generalization: now,
            last_consolidation: now,
            last_forgetting: now,
            last_quality_audit: now,
            generalization_interval: std::time::Duration::from_secs(86400), // 24 hours
            consolidation_interval: std::time::Duration::from_secs(7200),   // 2 hours
            forgetting_interval: std::time::Duration::from_secs(604800),    // 7 days
            quality_audit_interval: std::time::Duration::from_secs(86400),  // 24 hours
            extractions_per_generalization: 50,
        }
    }

    /// Record that an extraction just completed.
    pub fn record_extraction(&mut self) {
        self.extraction_count += 1;
    }

    /// Check whether the given stage should run now based on both
    /// extraction-count thresholds and time-based intervals.
    pub fn should_run(&self, stage: PipelineStage) -> bool {
        let now = std::time::Instant::now();
        match stage {
            PipelineStage::Generalization => {
                self.extraction_count > 0
                    && self
                        .extraction_count
                        .is_multiple_of(self.extractions_per_generalization)
                    && now.duration_since(self.last_generalization) >= self.generalization_interval
            }
            PipelineStage::Consolidation => {
                now.duration_since(self.last_consolidation) >= self.consolidation_interval
            }
            PipelineStage::Forgetting => {
                now.duration_since(self.last_forgetting) >= self.forgetting_interval
            }
            PipelineStage::QualityAudit => {
                now.duration_since(self.last_quality_audit) >= self.quality_audit_interval
            }
            PipelineStage::Extraction => true,
        }
    }

    /// Mark a stage as having just completed so its interval resets.
    pub fn mark_completed(&mut self, stage: PipelineStage) {
        let now = std::time::Instant::now();
        match stage {
            PipelineStage::Generalization => self.last_generalization = now,
            PipelineStage::Consolidation => self.last_consolidation = now,
            PipelineStage::Forgetting => self.last_forgetting = now,
            PipelineStage::QualityAudit => self.last_quality_audit = now,
            PipelineStage::Extraction => {} // no interval gate
        }
    }
}

impl Default for PipelineScheduler {
    fn default() -> Self {
        Self::new()
    }
}

use crate::agent::AgentStatus;
use crate::agent_loop::{
    AgentLoopConfig, TurnResult, build_user_message, run_agent_turn_streaming_with_images,
};
use crate::agent_pool::{AgentPool, AgentSummary};
use crate::event_bus::{AgentEvent, EventBus};
use crate::hooks::{HookContext, HookRegistry};
use crate::slash_router::SlashRouter;
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
    /// Slash-command pre-processor for /skillname interception (Claude Code-style dispatch).
    slash_router: Arc<RwLock<SlashRouter>>,
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
    /// Global default: max LLM iterations per turn (overridable per agent).
    default_max_iterations: Arc<RwLock<usize>>,
    /// Global default: cumulative prompt token budget, 0 = disabled.
    default_max_prompt_budget: Arc<RwLock<usize>>,
    /// Sandbox configuration for file and shell access control.
    sandbox_config: Arc<RwLock<loom_types::config::SandboxConfig>>,
    /// Semaphore to limit concurrent entity extraction tasks (prevents unbounded LLM calls).
    extraction_semaphore: Arc<tokio::sync::Semaphore>,
    /// Separate semaphore for lightweight consolidation tasks to avoid deadlock with
    /// extraction tasks that also hold the extraction_semaphore.
    consolidation_semaphore: Arc<tokio::sync::Semaphore>,
    /// Atomic counter for entity extractions; triggers periodic consolidation every N extractions.
    extraction_count: Arc<AtomicU64>,
    /// Atomic counter for completed consolidations (both periodic and session-close).
    consolidation_count: Arc<AtomicU64>,
    /// Phase 3: Pipeline scheduler gates stage transitions by count + time.
    pipeline_scheduler: Arc<tokio::sync::Mutex<PipelineScheduler>>,
}

/// Trait for memory backends (SqliteEventStore, etc.)
#[allow(clippy::too_many_arguments)]
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
        cached_read_tokens: usize,
        cached_write_tokens: usize,
        context_window: usize,
        // JSON-serialised content of each tool message (tool calls + results).
        tool_msgs_json: &[String],
    ) -> Result<i64>;
    async fn save_interrupted_turn(&self, session_id: &str, user_msg: &str) -> Result<()>;
    async fn load_history(&self, session_id: &str, limit: usize) -> Result<Vec<Message>>;
    async fn delete_message(&self, session_id: &str, index: usize) -> Result<()>;
    async fn extract_cognitions(&self, session_id: &str, text: &str) -> Result<Vec<String>>;
    async fn get_persona(&self) -> Result<String>;
    /// Phase 2: Rich structured persona with confidence scores and evidence counts.
    /// Returns `None` when no sufficient data is available yet.
    async fn get_rich_persona(&self) -> Result<Option<String>> {
        Ok(None)
    }
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
    async fn query_kg_context(
        &self,
        entity_names: &[&str],
        limit: usize,
        scope: &str,
    ) -> Result<String>;
    /// Phase 2: Layer-aware KG context query with optional memory-tier filter.
    /// When `layer` is `None` it falls back to `query_kg_context` for backward
    /// compatibility with stores that have not adopted the layered API.
    async fn query_kg_context_layered(
        &self,
        entity_names: &[&str],
        limit: usize,
        scope: &str,
        _layer: Option<&str>,
    ) -> Result<String> {
        // Default: ignore layer and delegate to the base method.
        self.query_kg_context(entity_names, limit, scope).await
    }
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
    async fn kg_neighbors(
        &self,
        node_name: &str,
        limit: usize,
        scope: Option<&str>,
    ) -> Result<loom_types::KgGraph>;
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
    async fn kg_edges_between(
        &self,
        node_names: &[String],
        scope: Option<&str>,
    ) -> Result<Vec<loom_types::KgEdge>>;
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
    /// Promote high-confidence session-scoped memories to global scope
    /// WITHOUT deleting remaining session-scoped data.
    async fn promote_to_global(
        &self,
        session_id: &str,
        min_confidence: f64,
    ) -> Result<(usize, usize)>;
    /// Promote specific entities and cognitions by name/id to global scope.
    async fn promote_selected(
        &self,
        node_names: &[String],
        cognition_ids: &[i64],
    ) -> Result<(usize, usize)>;
    // Session persistence
    async fn list_sessions(
        &self,
    ) -> Result<Vec<(String, String, usize, Option<String>, Option<String>)>>;
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

    // ── Vector embedding & semantic similarity search ──────────────────────

    /// Store a float32 embedding vector for a named entity node.
    /// Enables semantic similarity search across the knowledge graph.
    async fn embed_entity(&self, _name: &str, _embedding: Vec<f32>) -> Result<()> {
        Ok(())
    }
    /// Search for entities whose stored embeddings are most similar to the
    /// query embedding via cosine similarity.
    async fn search_similar_entities(
        &self,
        _embedding: &[f32],
        _limit: usize,
    ) -> Result<Vec<loom_types::KgNode>> {
        Ok(Vec::new())
    }

    // ── Memory quality feedback loop ───────────────────────────────────────

    /// Record a memory quality injection log entry. Returns the log ID for
    /// later correlation with actual LLM usage via `update_quality_references`.
    async fn record_memory_quality(
        &self,
        _session_id: &str,
        _turn_seq: i64,
        _injected: &[String],
        _duration_ms: i64,
    ) -> Result<i64> {
        Ok(0)
    }
    /// Update which injected memories were actually referenced by the LLM
    /// during the turn, enabling future recall quality scoring.
    async fn update_quality_references(&self, _log_id: i64, _referenced: &[String]) -> Result<()> {
        Ok(())
    }
    /// Holistic memory quality self-evaluation across all dimensions:
    /// injection relevance, entity health, coverage, freshness, and
    /// consolidation effectiveness.  Returns a structured report with a
    /// 0-100 composite health score suitable for the frontend settings page.
    async fn memory_quality_report(
        &self,
        _lookback_days: i64,
    ) -> Result<loom_types::MemoryQualityReport> {
        Ok(loom_types::MemoryQualityReport {
            avg_relevance: 0.0,
            injection_count: 0,
            turns_with_references: 0,
            total_entities: 0,
            duplicate_rate: 0.0,
            stale_entity_count: 0,
            avg_confidence: 0.0,
            entity_types_distribution: vec![],
            layer_distribution: vec![],
            entities_added_recently: 0,
            entities_accessed_recently: 0,
            consolidation_runs: 0,
            total_merged: 0,
            health_score: 0.0,
        })
    }

    // ── Phase 2: Memory consolidation, persona, patterns, layers ─────────────

    /// Run a full memory consolidation cycle (promote/demote/prune across layers).
    /// Returns a JSON-serialised ConsolidationReport.
    async fn run_consolidation_cycle(&self) -> Result<String> {
        Ok("noop".into())
    }

    /// Detect session usage patterns (topics, tools, learning paths, time).
    /// Returns a JSON-serialised SessionPatternReport.
    async fn detect_patterns(&self) -> Result<String> {
        Ok("{}".into())
    }

    /// Get per-layer node counts as Vec<(layer_name, count)>.
    async fn get_layer_stats(&self) -> Result<Vec<(String, i64)>> {
        Ok(vec![])
    }

    /// Promote a single knowledge graph node to a different memory layer.
    async fn promote_to_layer(&self, _node_name: &str, _layer: &str) -> Result<()> {
        Ok(())
    }

    // ── Phase 3: Full pipeline operations ───────────────────────────────────

    /// Run a forgetting cycle: prune low-importance, stale entities.
    /// Returns a JSON-serialised ForgettingReport.
    async fn run_forgetting_cycle(
        &self,
        _min_importance: f64,
        _max_age_days: i64,
    ) -> Result<String> {
        Ok(r#"{"summary":"noop"}"#.into())
    }

    /// Return a health snapshot of the memory system.
    /// Returns a JSON-serialised MemoryHealth.
    async fn get_memory_health(&self) -> Result<String> {
        Ok(r#"{"status":"healthy","fragmentation_score":0.0}"#.into())
    }

    /// Evaluate memory recall quality over a lookback window.
    /// Returns a JSON-serialised MemoryQualityReport.
    async fn evaluate_quality(&self, _lookback_days: i64) -> Result<String> {
        Ok(r#"{"recall_rate":0.0,"quality_score":0.0}"#.into())
    }

    /// Return the current pipeline status (stage, counts, timings).
    /// Returns a JSON-serialised pipeline status object.
    async fn get_pipeline_status(&self) -> Result<String> {
        Ok(r#"{"status":"idle"}"#.into())
    }
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

/// Prompt template for AI-assisted Agent configuration generation.
///
/// Sent to the active cloud model when a user requests an Agent config via
/// natural language description. The LLM must output valid JSON inside a
/// code fence — the extractor falls back to bare `{` if the fence is missing.
const AGENT_CONFIG_GENERATION_PROMPT: &str = r#"你是 openLoom 的 Agent 配置生成器。根据用户的自然语言描述，生成一个完整的 AgentConfig JSON。

AgentConfig 的字段说明：
- name (string, 必填): Agent 名称，简洁有描述性，2-20 个字符，英文或中文
- persona (string, 必填): 自然语言的人格描述，定义 Agent 的核心身份和行为方式。用第二人称"你是..."开头
- system_prompt_override (string, 必填): 自定义系统提示词，包含 Agent 的具体行为规则、工作流程、限制条件等。必须生成，不能为 null。用第二人称
- model (string | null): 指定使用的模型名称，null 表示使用默认模型
- thinking_level (string | null): 思考深度，可选 "low" / "medium" / "high"，null 表示默认
- temperature (number | null): 生成温度 0.0-2.0，null 表示默认
- tool_scope (string | null): 工具范围，null 表示无限制
- allowed_tools (string[] | null): 允许使用的工具列表，null 表示无限制
- disallowed_tools (string[] | null): 禁止使用的工具列表，null 表示无限制
- max_iterations (number | null): 每轮最大迭代次数，null 表示默认 20
- timeout_secs (number | null): 超时秒数，null 表示默认 300
- max_concurrent_subagents (number): 最大并发子代理数，默认 5
- is_primary (boolean): 是否为主代理，默认 false
- memory_enabled (boolean): 是否启用记忆，默认 true

注意：persona 和 system_prompt_override 都必须生成。persona 侧重"身份"（你是谁），system_prompt_override 侧重"行为规则"（你怎么做）。

已有 Agent 名称（请勿重复使用这些名称）：{existing_names}

只输出 JSON，放在 ```json 代码块中，不要包含任何其他解释。"#;

fn extract_entity_candidates(text: &str) -> Vec<String> {
    let mut candidates: Vec<String> = Vec::new();

    // English: split by whitespace, keep capitalized words
    for w in text.split_whitespace() {
        let trimmed = w.trim_matches(|c: char| !c.is_alphabetic());
        if trimmed.len() >= 3 && trimmed.chars().next().is_some_and(|c| c.is_uppercase()) {
            candidates.push(trimmed.to_string());
        }
    }

    // Hardcoded allowlist of common lowercase tech terms
    let tech_allowlist: &[&str] = &[
        "rust",
        "python",
        "typescript",
        "javascript",
        "golang",
        "docker",
        "kubernetes",
        "linux",
        "sqlite",
        "redis",
        "git",
        "react",
        "vue",
        "electron",
        "tauri",
        "node",
        "postgres",
        "llm",
        "mcp",
        "lsp",
    ];
    let lower_text = text.to_lowercase();
    for term in tech_allowlist {
        if lower_text.contains(term) {
            candidates.push(term.to_string());
        }
    }

    // Chinese: sliding window of 2-5 CJK characters on consecutive runs
    let chinese_stopwords: &[&str] = &[
        "的", "了", "是", "在", "和", "与", "或", "而", "我", "你", "他", "她", "它", "们", "这",
        "那", "吗", "呢", "吧", "啊", "哦", "嗯", "一个", "这个", "那个", "什么", "怎么", "因为",
        "所以", "但是", "如果", "虽然", "可以", "需要", "应该", "可能", "已经", "正在", "还是",
        "或者", "以及", "而且", "然后",
    ];

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
                let s: String = chars[cjk_indices[run_start]..=cjk_indices[run_end]]
                    .iter()
                    .collect();
                if !is_cjk_stopword(&s, chinese_stopwords) {
                    candidates.push(s);
                }
            }
            // Sub-ngrams (2-4 chars)
            for n in 2..=5.min(run_len) {
                for i in 0..=(run_len - n) {
                    let s: String = chars
                        [cjk_indices[run_start + i]..=cjk_indices[run_start + i + n - 1]]
                        .iter()
                        .collect();
                    if !is_cjk_stopword(&s, chinese_stopwords) {
                        candidates.push(s);
                    }
                }
            }
        }
        run_start = run_end + 1;
    }

    candidates.sort();
    candidates.dedup();
    candidates.truncate(20);
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

/// Check if a CJK n-gram is a stopword or starts with one.
fn is_cjk_stopword(s: &str, stopwords: &[&str]) -> bool {
    if s.is_empty() {
        return true;
    }
    for sw in stopwords {
        if s == *sw || s.starts_with(sw) {
            return true;
        }
    }
    false
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
        let slash_router = Arc::new(RwLock::new(SlashRouter::new()));
        let _ = registry.register(Arc::new(crate::builtin_tools::UseSkillTool {
            skill_state: skill_state.clone(),
        }));
        let _ = registry.register(Arc::new(crate::builtin_tools::WebSearchTool));
        let _ = registry.register(Arc::new(crate::builtin_tools::WebFetchTool));

        // Register Claude Code-style tool name aliases (always-on, no-op when unused).
        // These allow the model to call e.g. "Read" and have it resolve to "file_read".
        let _ = registry.register_alias("Read", "file_read");
        let _ = registry.register_alias("Write", "file_write");
        let _ = registry.register_alias("Grep", "content_search");
        let _ = registry.register_alias("Glob", "file_list");
        let _ = registry.register_alias("Skill", "use_skill");
        let _ = registry.register_alias("Bash", "shell");
        let _ = registry.register_alias("Delete", "file_delete");

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
            slash_router,
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
            default_max_iterations: Arc::new(RwLock::new(30)),
            default_max_prompt_budget: Arc::new(RwLock::new(0)),
            sandbox_config: Arc::new(RwLock::new(loom_types::config::SandboxConfig::default())),
            extraction_semaphore: Arc::new(tokio::sync::Semaphore::new(3)),
            consolidation_semaphore: Arc::new(tokio::sync::Semaphore::new(2)),
            extraction_count: Arc::new(AtomicU64::new(0)),
            consolidation_count: Arc::new(AtomicU64::new(0)),
            pipeline_scheduler: Arc::new(tokio::sync::Mutex::new(PipelineScheduler::new())),
        }
    }

    /// Phase 3: Spawn the background forgetting check loop.
    /// Runs every hour, fire-and-forget. Does not block construction.
    pub fn spawn_forgetting_loop(&self) {
        let memory_store = self.memory_store.clone();
        let scheduler = self.pipeline_scheduler.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(3600));
            loop {
                interval.tick().await;
                let should_run = {
                    let sched = scheduler.lock().await;
                    sched.should_run(PipelineStage::Forgetting)
                };
                if !should_run {
                    tracing::debug!("forgetting: interval elapsed but scheduler says not yet");
                    continue;
                }
                tracing::info!("forgetting cycle starting (background hourly check)");
                let guard = memory_store.read().await;
                if let Some(ref store) = *guard {
                    match store.run_forgetting_cycle(0.3, 90).await {
                        Ok(report) => {
                            tracing::info!(report = %report, "forgetting cycle completed");
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "forgetting cycle failed");
                        }
                    }
                }
                drop(guard);
                {
                    let mut sched = scheduler.lock().await;
                    sched.mark_completed(PipelineStage::Forgetting);
                }
            }
        });
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

    // ── Global defaults ─────────────────────────────────────────────────

    pub async fn get_default_max_iterations(&self) -> usize {
        *self.default_max_iterations.read().await
    }

    pub async fn set_default_max_iterations(&self, val: usize) {
        *self.default_max_iterations.write().await = val;
    }

    pub async fn get_default_max_prompt_budget(&self) -> usize {
        *self.default_max_prompt_budget.read().await
    }

    pub async fn set_default_max_prompt_budget(&self, val: usize) {
        *self.default_max_prompt_budget.write().await = val;
    }

    /// Get the current sandbox configuration (defaults to disabled).
    pub async fn sandbox_config(&self) -> loom_types::config::SandboxConfig {
        self.sandbox_config.read().await.clone()
    }

    /// Set the sandbox configuration.
    pub async fn set_sandbox_config(&self, config: loom_types::config::SandboxConfig) {
        *self.sandbox_config.write().await = config;
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
    ///
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
            if let Ok(val) = std::env::var(raw)
                && !val.is_empty()
            {
                return Some(val);
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
                tags: vec!["mcp".into(), server.clone()],
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
    /// Also rebuilds the slash-command router for /skillname interception.
    pub async fn set_skills(&self, state: loom_skills::SkillState) {
        self.slash_router
            .write()
            .await
            .rebuild(state.bodies.clone());
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
        self.skill_state
            .read()
            .await
            .bodies
            .keys()
            .cloned()
            .collect()
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
        self.hook_registry.read().await.reload(plugin_manager).await;
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

    /// Holistic memory quality evaluation.  Returns a structured report
    /// with a composite 0-100 health score for the frontend settings page.
    pub async fn memory_quality_report(
        &self,
        lookback_days: i64,
    ) -> Result<loom_types::MemoryQualityReport> {
        if let Some(ref store) = *self.memory_store.read().await {
            store.memory_quality_report(lookback_days).await
        } else {
            Ok(loom_types::MemoryQualityReport {
                avg_relevance: 0.0,
                injection_count: 0,
                turns_with_references: 0,
                total_entities: 0,
                duplicate_rate: 0.0,
                stale_entity_count: 0,
                avg_confidence: 0.0,
                entity_types_distribution: vec![],
                layer_distribution: vec![],
                entities_added_recently: 0,
                entities_accessed_recently: 0,
                consolidation_runs: 0,
                total_merged: 0,
                health_score: 0.0,
            })
        }
    }

    pub async fn kg_neighbors(
        &self,
        node_name: &str,
        limit: usize,
        scope: Option<&str>,
    ) -> Result<loom_types::KgGraph> {
        if let Some(ref store) = *self.memory_store.read().await {
            store.kg_neighbors(node_name, limit, scope).await
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

    pub async fn kg_edges_between(
        &self,
        node_names: &[String],
        scope: Option<&str>,
    ) -> Result<Vec<loom_types::KgEdge>> {
        if let Some(ref store) = *self.memory_store.read().await {
            store.kg_edges_between(node_names, scope).await
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

    /// Promote high-confidence session-scoped KG nodes/edges and cognitions to global scope.
    /// Returns (promoted_nodes, promoted_cognitions).
    /// If node_names/cognition_ids are provided, only promote those specific items.
    pub async fn memory_promote(
        &self,
        session_id: &str,
        min_confidence: f64,
        node_names: &[String],
        cognition_ids: &[i64],
    ) -> Result<(usize, usize)> {
        if let Some(ref store) = *self.memory_store.read().await {
            if !node_names.is_empty() || !cognition_ids.is_empty() {
                store.promote_selected(node_names, cognition_ids).await
            } else {
                store.promote_to_global(session_id, min_confidence).await
            }
        } else {
            Ok((0, 0))
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

    /// Generate a 5-10 character title for a session using the LLM.
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
            "你是一位标题编辑。根据以下对话内容，生成一个 5-10 个汉字的简短会话标题。\n\
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
    pub async fn list_persisted_sessions(
        &self,
    ) -> Vec<(String, String, usize, Option<String>, Option<String>)> {
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
            let _ = s
                .save_session_agent_name(session_id, agent_config_name)
                .await;
        }
    }

    /// Read a persisted session-agent binding from the memory store.
    pub async fn memory_store_session_agent_name(&self, session_id: &str) -> Option<String> {
        let store = self.memory_store.read().await;
        if let Some(ref s) = *store {
            s.get_session_agent_name(session_id).await.ok().flatten()
        } else {
            None
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
                // Strip auxiliary task suffix for price lookup:
                // e.g. "claude-sonnet-4-6 (entity)" → "claude-sonnet-4-6"
                let base_name = model_name
                    .trim_end_matches(" (entity)")
                    .trim_end_matches(" (summary)")
                    .trim_end_matches(" (vision)");
                let (input_price, output_price, cache_read_price, cache_write_price) = configs
                    .get(base_name)
                    .or_else(|| configs.get(model_name))
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
                // prompt = cache-hit + cache-miss; split so cached tokens
                // are NOT double-charged (previously: prompt * input_price PLUS
                // cached_read * cache_read_price — cached tokens paid twice).
                let cache_miss = (prompt - cached_read).max(0.0);
                let cache_hit_cost = cached_read * cache_read_price / 1_000_000.0;
                let cache_write_cost = cached_write * cache_write_price / 1_000_000.0;
                let input_cost = cache_miss * input_price / 1_000_000.0;
                let output_cost = completion * output_price / 1_000_000.0;
                let cost = input_cost + cache_hit_cost + cache_write_cost + output_cost;
                total_cost += cost;
                entry["input_price"] = serde_json::json!(input_price);
                entry["output_price"] = serde_json::json!(output_price);
                entry["cache_read_price"] = serde_json::json!(cache_read_price);
                entry["cache_write_price"] = serde_json::json!(cache_write_price);
                entry["cache_miss_tokens"] = serde_json::json!(cache_miss as i64);
                entry["cache_hit_tokens"] = serde_json::json!(cached_read as i64);
                entry["cache_write_tokens"] = serde_json::json!(cached_write as i64);
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
        parts: &mut [ContentPart],
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
        if let Some(ref s) = *store
            && let Some(cfg) = s.get_agent_config(name).await?
        {
            self.agent_configs
                .write()
                .await
                .insert(name.to_string(), cfg.clone());
            return Ok(cfg);
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

    /// Generate an AgentConfig from a natural language description using the
    /// active cloud model.
    ///
    /// The LLM is prompted with the field definitions, existing agent names
    /// (to avoid duplicates), and the user's description. The response is
    /// parsed as JSON (with fallback extraction from markdown code fences).
    pub async fn agent_config_generate(
        &self,
        description: &str,
    ) -> Result<loom_types::AgentConfig> {
        let existing_names: Vec<String> =
            { self.agent_configs.read().await.keys().cloned().collect() };
        let names_hint = if existing_names.is_empty() {
            "(none)".to_string()
        } else {
            existing_names.join(", ")
        };

        let system_prompt = AGENT_CONFIG_GENERATION_PROMPT.replace("{existing_names}", &names_hint);

        let request = CompletionRequest {
            messages: vec![
                Message {
                    role: Role::System,
                    content: vec![ContentPart::Text {
                        text: system_prompt,
                    }],
                    timestamp: chrono::Utc::now(),
                    usage: None,
                },
                Message {
                    role: Role::User,
                    content: vec![ContentPart::Text {
                        text: format!("用户描述：{}", description),
                    }],
                    timestamp: chrono::Utc::now(),
                    usage: None,
                },
            ],
            tools: vec![],
            tool_choice: None,
            prompt: String::new(),
            max_tokens: 2048,
            temperature: 0.3,
            top_p: 1.0,
            stop: vec![],
            stream: true,
            thinking_budget: None,
        };

        // Get client: try auxiliary first, fall back to main cloud client
        let client = self
            .build_auxiliary_client("entity")
            .await
            .or_else(|| self.cloud_client.try_read().ok().and_then(|g| g.clone()))
            .ok_or_else(|| anyhow::anyhow!("没有可用的模型。请先在设置中配置一个模型。"))?;

        // Use streaming to collect response (works better with local models)
        let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(256);
        let handle = tokio::spawn(async move {
            let mut text = String::new();
            while let Some(token) = rx.recv().await {
                text.push_str(&token);
            }
            text
        });

        if let Err(e) = client.complete_stream(request, tx).await {
            handle.abort();
            anyhow::bail!("模型调用失败: {}. 请确认模型服务正常运行。", e);
        }

        let raw = handle.await.unwrap_or_default().trim().to_string();

        if raw.is_empty() {
            anyhow::bail!("AI 返回了空响应，请重试。提示：请确认已选择模型并模型服务正常运行。");
        }

        // Extract JSON using the same pattern as parse_llm_extraction
        let json_str = if let Some(start) = raw.find("```json") {
            let content = &raw[start + 7..];
            if let Some(end) = content.find("```") {
                &content[..end]
            } else {
                content
            }
        } else if let Some(start) = raw.find('{') {
            &raw[start..]
        } else {
            anyhow::bail!(
                "AI 返回的内容中没有找到 JSON 配置。原始响应: {}",
                &raw[..raw.len().min(500)]
            )
        };

        let mut config: loom_types::AgentConfig =
            serde_json::from_str(json_str.trim()).map_err(|e| {
                anyhow::anyhow!(
                    "AI 生成的配置 JSON 解析失败: {}\nJSON: {}",
                    e,
                    &json_str[..json_str.len().min(300)]
                )
            })?;

        // Validate name
        if config.name.trim().is_empty() {
            anyhow::bail!("AI 生成的配置缺少 name 字段");
        }

        // Auto-generate system_prompt_override if the model left it empty
        if config
            .system_prompt_override
            .as_deref()
            .is_none_or(|s| s.trim().is_empty())
            && !config.persona.trim().is_empty()
        {
            config.system_prompt_override = Some(format!(
                "你是 {}。\n\n{}\n\n行为准则：\n- 始终使用中文回复\n- 优先使用本地工具和本地模型\n- 回答前先确认用户环境",
                config.name, config.persona
            ));
        }

        // Resolve name conflicts by appending a numeric suffix
        if existing_names.contains(&config.name) {
            let base = config.name.clone();
            let mut suffix = 2;
            loop {
                let candidate = format!("{}-{}", base, suffix);
                if !existing_names.contains(&candidate) {
                    config.name = candidate;
                    break;
                }
                suffix += 1;
                if suffix > 100 {
                    anyhow::bail!("无法为 Agent 名称 '{}' 生成不冲突的后缀", base);
                }
            }
        }

        Ok(config)
    }

    /// Optimize an existing agent config using AI based on user instructions.
    /// Reuses the same generation pipeline but with the current config as context.
    pub async fn agent_config_optimize(
        &self,
        current: loom_types::AgentConfig,
        instructions: &str,
    ) -> Result<loom_types::AgentConfig> {
        let current_json = serde_json::to_string_pretty(&current).unwrap_or_default();

        let optimize_prompt = format!(
            r#"你是 openLoom 的 Agent 配置优化器。根据用户的优化指令，改进现有的 AgentConfig JSON。

当前的 AgentConfig:
```json
{}
```

优化规则：
- 保持 name 不变（除非用户明确要求改名）
- 根据用户指令优化 persona 和 system_prompt_override
- 只修改用户提到的部分，其他字段保持不变
- persona 侧重"身份"，system_prompt_override 侧重"行为规则"
- system_prompt_override 必须生成，不能为 null 或空

只输出优化后的完整 JSON，放在 ```json 代码块中，不要包含任何其他解释。"#,
            current_json
        );

        let request = CompletionRequest {
            messages: vec![
                Message {
                    role: Role::System,
                    content: vec![ContentPart::Text {
                        text: optimize_prompt,
                    }],
                    timestamp: chrono::Utc::now(),
                    usage: None,
                },
                Message {
                    role: Role::User,
                    content: vec![ContentPart::Text {
                        text: format!("优化指令：{}", instructions),
                    }],
                    timestamp: chrono::Utc::now(),
                    usage: None,
                },
            ],
            tools: vec![],
            tool_choice: None,
            prompt: String::new(),
            max_tokens: 2048,
            temperature: 0.3,
            top_p: 1.0,
            stop: vec![],
            stream: true,
            thinking_budget: None,
        };

        let client = self
            .build_auxiliary_client("entity")
            .await
            .or_else(|| self.cloud_client.try_read().ok().and_then(|g| g.clone()))
            .ok_or_else(|| anyhow::anyhow!("没有可用的模型。请先在设置中配置一个模型。"))?;

        let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(256);
        let handle = tokio::spawn(async move {
            let mut text = String::new();
            while let Some(token) = rx.recv().await {
                text.push_str(&token);
            }
            text
        });

        if let Err(e) = client.complete_stream(request, tx).await {
            handle.abort();
            anyhow::bail!("模型调用失败: {}. 请确认模型服务正常运行。", e);
        }

        let raw = handle.await.unwrap_or_default().trim().to_string();

        if raw.is_empty() {
            anyhow::bail!("AI 返回了空响应，请重试。提示：请确认已选择模型并模型服务正常运行。");
        }

        let json_str = if let Some(start) = raw.find("```json") {
            let content = &raw[start + 7..];
            if let Some(end) = content.find("```") {
                &content[..end]
            } else {
                content
            }
        } else if let Some(start) = raw.find('{') {
            &raw[start..]
        } else {
            anyhow::bail!(
                "AI 返回的内容中没有找到 JSON。原始响应: {}",
                &raw[..raw.len().min(500)]
            )
        };

        let config: loom_types::AgentConfig =
            serde_json::from_str(json_str.trim()).map_err(|e| {
                anyhow::anyhow!(
                    "优化后的 JSON 解析失败: {}\nJSON: {}",
                    e,
                    &json_str[..json_str.len().min(300)]
                )
            })?;

        // Ensure system_prompt_override is filled
        let mut config = config;
        if config
            .system_prompt_override
            .as_deref()
            .is_none_or(|s| s.trim().is_empty())
            && !config.persona.trim().is_empty()
        {
            config.system_prompt_override = Some(format!(
                "你是 {}。\n\n{}\n\n行为准则：\n- 始终使用中文回复\n- 优先使用本地工具和本地模型\n- 回答前先确认用户环境",
                config.name, config.persona
            ));
        }

        Ok(config)
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
            if let Some(ref old_name) = old_name
                && old_name != name
                && let Ok(old_config) = self.model_config_get(old_name).await
                && old_config.backend.is_local_inference()
                && let Some(ref old_model_id) = old_config.model
            {
                let base = old_config
                    .base_url
                    .as_deref()
                    .unwrap_or(match old_config.backend {
                        loom_types::ModelBackend::LmStudio => "http://localhost:1234/v1",
                        loom_types::ModelBackend::Ollama => "http://localhost:11434/v1",
                        _ => "http://localhost:1234/v1",
                    });
                loom_inference::unload_local_model(base, old_model_id).await;
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
            if let Some(ref client) = existing
                && client.model_name() == model
            {
                tracing::info!(%model, name = %config.name, "cloud client model_name matches — force rebuild anyway to pick up config changes");
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
        config
            .get(key)
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
            match loom_inference::InferenceEngine::connect(&base_url, &model, config.context_size)
                .await
            {
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
                        Some(
                            Arc::new(loom_inference::AnthropicClient::new(key, model, base_url))
                                as Arc<dyn CloudClient>,
                        )
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

    // ── Async entity extraction pipeline ─────────────────────────────────

    /// Spawn a non-blocking entity extraction task via `tokio::spawn`.
    ///
    /// Pipeline: extract_cognitions → RuleBasedEntityExtractor (baseline) →
    /// llm_extract_entities (if available) → feed_knowledge_graph →
    /// save_extracted_entities → publish `MemoryUpdated` event.
    ///
    /// Persona refresh is deferred to the START of the next turn (lazy refresh
    /// in `process_message_with_config` / `process_message_streaming`), which
    /// is more correct since it captures all memory from the previous turn.
    async fn spawn_extraction_pipeline(
        &self,
        session_id: String,
        user_message: String,
        assistant_response: String,
        event_id: i64,
    ) {
        // Build LLM client before spawning (requires &self for config lookup).
        let llm_client: Option<Arc<dyn CloudClient>> = self
            .build_auxiliary_client("entity")
            .await
            .or_else(|| self.cloud_client.try_read().ok().and_then(|g| g.clone()));

        let extract_model = llm_client
            .as_ref()
            .map(|c| c.model_name().to_string())
            .unwrap_or_else(|| {
                format!(
                    "{} (entity)",
                    self.active_model_name
                        .try_read()
                        .ok()
                        .and_then(|g| g.clone())
                        .unwrap_or_default()
                )
            });

        let mem_arc = self.memory_store.clone();
        let event_bus = self.pool.event_bus().clone();
        let sid = session_id.clone();
        let sem = self.extraction_semaphore.clone();
        let con_sem = self.consolidation_semaphore.clone();
        let extraction_count = self.extraction_count.clone();
        let consolidation_count = self.consolidation_count.clone();
        let pipeline_scheduler = self.pipeline_scheduler.clone();

        tokio::spawn(async move {
            // Rate-limit concurrent LLM extraction calls to prevent unbounded
            // resource consumption during rapid chat.
            let _permit = sem.acquire().await;

            // Combine user + assistant text for broader entity coverage.
            // The assistant often mentions new technologies/concepts that should be extracted.
            let combined_text = if assistant_response.is_empty() {
                user_message.clone()
            } else {
                format!("User: {}\nAssistant: {}", user_message, assistant_response)
            };

            // 1. Keyword-based cognition extraction (runs unconditionally)
            {
                let guard = mem_arc.read().await;
                if let Some(ref store) = *guard
                    && let Err(e) = store.extract_cognitions(&session_id, &combined_text).await
                {
                    tracing::warn!(error = %e, "Keyword cognition extraction failed");
                }
            }

            // 2. Rule-based entity extraction (always as baseline)
            let mut all_entities: Vec<ExtractedEntity> = Vec::new();
            let mut all_relationships: Vec<ExtractedRelationship> = Vec::new();

            let extractor = RuleBasedEntityExtractor;
            if let Ok(rule_entities) = extractor.extract_entities(&combined_text, "", &session_id) {
                if let Ok(rule_relationships) =
                    extractor.extract_relationships(&combined_text, &rule_entities, &session_id)
                {
                    all_entities.extend(rule_entities);
                    all_relationships.extend(rule_relationships);
                } else {
                    all_entities.extend(rule_entities);
                }
            }

            // 3. LLM-based entity extraction (if client available)
            if let Some(ref client) = llm_client {
                match llm_extract_entities(
                    client.as_ref(),
                    &user_message,
                    &assistant_response,
                    &session_id,
                )
                .await
                {
                    Ok((mut entities, mut relationships, ext_prompt, ext_completion)) => {
                        // Record entity extraction token usage
                        if ext_prompt > 0 || ext_completion > 0 {
                            let guard = mem_arc.read().await;
                            if let Some(ref store) = *guard {
                                let _ = store
                                    .record_token_usage(
                                        &session_id,
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
                        }
                        all_entities.append(&mut entities);
                        all_relationships.append(&mut relationships);
                        tracing::info!(
                            n_entities = all_entities.len(),
                            n_relationships = all_relationships.len(),
                            "LLM entity extraction complete"
                        );
                    }
                    Err(e) => {
                        tracing::debug!(
                            error = %e,
                            "LLM extraction failed, using rule-based only"
                        );
                    }
                }
            }

            // 4. Feed knowledge graph + save extracted entities
            {
                let guard = mem_arc.read().await;
                if let Some(ref store) = *guard
                    && let Ok((n, e)) = store
                        .feed_knowledge_graph(
                            &all_entities,
                            &all_relationships,
                            event_id,
                            &session_id,
                        )
                        .await
                {
                    let _ = store
                        .save_extracted_entities(&all_entities, &all_relationships, &session_id)
                        .await;
                    if n > 0 || e > 0 {
                        tracing::info!(n, e, "KG updated via async extraction pipeline");
                    }
                }
            }

            // 5. Publish MemoryUpdated event so frontend can refresh KG display
            event_bus.publish(AgentEvent::MemoryUpdated { session_id: sid });

            // 6. Record extraction via scheduler and increment counter.
            {
                let mut sched = pipeline_scheduler.lock().await;
                sched.record_extraction();
            }
            let count = extraction_count.fetch_add(1, Ordering::Relaxed) + 1;

            // 6a. Check if generalization should run (scheduler-gated: count % 50 + time interval).
            let should_generalize = {
                let sched = pipeline_scheduler.lock().await;
                sched.should_run(PipelineStage::Generalization)
            };
            if should_generalize {
                let g_mem = mem_arc.clone();
                let g_sched = pipeline_scheduler.clone();
                tokio::spawn(async move {
                    let guard = g_mem.read().await;
                    if let Some(ref store) = *guard {
                        // Phase 3: Generalization — detect patterns across sessions
                        tracing::info!("Phase 3: running generalization cycle");
                        match store.detect_patterns().await {
                            Ok(patterns_json) => {
                                tracing::info!(
                                    patterns = %patterns_json,
                                    "generalization: patterns detected"
                                );
                                // Create concept nodes from detected topics.
                                // TopicPattern serialises as {"topic":"...","session_count":N,...}
                                // — an object, NOT an array. Use key access, not tuple index.
                                if let Ok(report) =
                                    serde_json::from_str::<serde_json::Value>(&patterns_json)
                                    && let Some(topics) =
                                        report.get("topics").and_then(|v| v.as_array())
                                {
                                    for topic in topics.iter().take(5) {
                                        if let (Some(name), Some(count)) = (
                                            topic.get("topic").and_then(|v| v.as_str()),
                                            topic.get("session_count").and_then(|v| v.as_u64()),
                                        ) {
                                            // Skip common English stopwords that would pollute the KG
                                            let name_lower = name.to_lowercase();
                                            let is_stopword = matches!(
                                                name_lower.as_str(),
                                                "the"
                                                    | "a"
                                                    | "an"
                                                    | "is"
                                                    | "are"
                                                    | "was"
                                                    | "were"
                                                    | "be"
                                                    | "been"
                                                    | "being"
                                                    | "have"
                                                    | "has"
                                                    | "had"
                                                    | "do"
                                                    | "does"
                                                    | "did"
                                                    | "will"
                                                    | "would"
                                                    | "could"
                                                    | "should"
                                                    | "may"
                                                    | "might"
                                                    | "can"
                                                    | "shall"
                                                    | "must"
                                                    | "i"
                                                    | "you"
                                                    | "he"
                                                    | "she"
                                                    | "it"
                                                    | "we"
                                                    | "they"
                                                    | "me"
                                                    | "him"
                                                    | "her"
                                                    | "us"
                                                    | "them"
                                                    | "my"
                                                    | "your"
                                                    | "his"
                                                    | "its"
                                                    | "our"
                                                    | "their"
                                                    | "this"
                                                    | "that"
                                                    | "these"
                                                    | "those"
                                                    | "and"
                                                    | "or"
                                                    | "but"
                                                    | "not"
                                                    | "if"
                                                    | "then"
                                                    | "else"
                                                    | "when"
                                                    | "where"
                                                    | "how"
                                                    | "what"
                                                    | "which"
                                                    | "who"
                                                    | "whom"
                                                    | "to"
                                                    | "from"
                                                    | "in"
                                                    | "on"
                                                    | "at"
                                                    | "by"
                                                    | "for"
                                                    | "with"
                                                    | "about"
                                                    | "as"
                                                    | "of"
                                            );
                                            if is_stopword {
                                                tracing::debug!(
                                                    topic = %name,
                                                    "generalization: skipping stopword topic"
                                                );
                                                continue;
                                            }
                                            // Require topics to appear in at least 2 sessions
                                            if count < 2 {
                                                continue;
                                            }
                                            let entities = vec![loom_memory::ExtractedEntity {
                                                name: format!("Topic::{}", name),
                                                entity_type: "Concept".into(),
                                                description: format!(
                                                    "Emergent topic detected across sessions (mentioned {} times)",
                                                    count
                                                ),
                                                confidence: 0.85,
                                                aliases: vec![],
                                                scope: "global".into(),
                                            }];
                                            let relationships: Vec<
                                                loom_memory::ExtractedRelationship,
                                            > = vec![];
                                            let _ = store
                                                .feed_knowledge_graph(
                                                    &entities,
                                                    &relationships,
                                                    0,
                                                    "global",
                                                )
                                                .await;
                                            let _ = store
                                                .save_extracted_entities(
                                                    &entities,
                                                    &relationships,
                                                    "global",
                                                )
                                                .await;
                                            tracing::info!(
                                                topic = %name,
                                                count = count,
                                                "generalization: created concept node from detected topic"
                                            );
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::warn!(error = %e, "generalization: detect_patterns failed");
                            }
                        }
                    }
                    {
                        let mut sched = g_sched.lock().await;
                        sched.mark_completed(PipelineStage::Generalization);
                    }
                });
            }

            // 6b. Periodic consolidation: count-based trigger (every 50 extractions)
            // AND scheduler time-gate (2-hour minimum interval between consolidations).
            let should_consolidate = {
                let sched = pipeline_scheduler.lock().await;
                count.is_multiple_of(50) && sched.should_run(PipelineStage::Consolidation)
            };
            if should_consolidate {
                let c_mem = mem_arc.clone();
                let c_consolidation_count = consolidation_count.clone();
                let c_sem = con_sem.clone();
                let c_session_id = session_id.clone();
                let c_sched = pipeline_scheduler.clone();
                tokio::spawn(async move {
                    let _permit = c_sem.acquire().await;
                    tracing::info!(
                        extraction_count = count,
                        "triggering periodic consolidation"
                    );
                    let guard = c_mem.read().await;
                    if let Some(ref store) = *guard {
                        // Scope-based promotion: high-confidence session → global
                        match store.promote_to_global(&c_session_id, 0.7).await {
                            Ok((nodes, cogs)) => {
                                let c_count =
                                    c_consolidation_count.fetch_add(1, Ordering::Relaxed) + 1;
                                tracing::info!(
                                    promoted_nodes = nodes,
                                    promoted_cognitions = cogs,
                                    consolidation_count = c_count,
                                    "periodic consolidation: promoted session entities to global"
                                );
                            }
                            Err(e) => {
                                tracing::warn!(error = %e, "periodic consolidation: promote_to_global failed");
                            }
                        }
                        // Prune stale entities to keep KG lean
                        match store.prune_memory().await {
                            Ok(pruned) => {
                                if pruned > 0 {
                                    tracing::info!(
                                        pruned,
                                        "periodic consolidation: pruned stale entities"
                                    );
                                }
                            }
                            Err(e) => {
                                tracing::warn!(error = %e, "periodic consolidation: prune_memory failed");
                            }
                        }
                        // Layer-based L0-L3 consolidation cycle
                        match store.run_consolidation_cycle().await {
                            Ok(json) => {
                                tracing::info!(
                                    report = %json,
                                    "periodic consolidation: L0-L3 layer cycle completed"
                                );
                            }
                            Err(e) => {
                                tracing::warn!(error = %e, "periodic consolidation: run_consolidation_cycle failed");
                            }
                        }
                    }
                    {
                        let mut sched = c_sched.lock().await;
                        sched.mark_completed(PipelineStage::Consolidation);
                    }
                });
            }
        });
        // Note: tokio::JoinHandle does not expose inspect_err in current tokio.
        // Panics in the extraction task are caught by tokio's task boundary
        // and logged by the runtime. The fire-and-forget design is intentional —
        // extraction failures must not block the main agent loop.
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
            false,
        )
        .await
    }

    /// Build the full system prompt by assembling agent persona, system instructions,
    /// user profile, selected skills, available skill list, KG context, and workspace path.
    #[allow(clippy::too_many_arguments)]
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
        // 4a-bis. Always-active skills — auto-inject full SKILL.md content for
        // skills whose manifest declares always_active: true. These activate
        // automatically without user or LLM action.
        {
            let always_active_names: Vec<String> = {
                let state = self.skill_state.read().await;
                state
                    .summaries
                    .iter()
                    .filter(|s| s.always_active)
                    .map(|s| s.name.clone())
                    .collect()
            };
            if !always_active_names.is_empty() {
                let first_time = selected_skills.is_empty();
                let bodies = self.skill_state.read().await.bodies.clone();
                system_prompt.push_str(&format!(
                    "\n\n## Active Skills ({})\nThe following skills are activated automatically for this conversation. Follow their instructions directly — do NOT call use_skill for these.\n",
                    if first_time { "Auto-Activated" } else { "Also Auto-Activated" }
                ));
                for name in &always_active_names {
                    if !selected_skills.contains(name)
                        && let Some(body) = bodies.get(name)
                    {
                        system_prompt.push_str(&format!("\n\n### Skill: {}\n{}", name, body));
                    }
                }
            }
        }
        // 4b. Available skills — name+description list for LLM autonomous use_skill calls.
        // Matches v0.2.17 behavior: simple list injection, no relevance filtering.
        if !available_skills.is_empty() {
            let active_set: std::collections::HashSet<String> = {
                let state = self.skill_state.read().await;
                selected_skills
                    .iter()
                    .cloned()
                    .chain(
                        state
                            .summaries
                            .iter()
                            .filter(|s| s.always_active)
                            .map(|s| s.name.clone()),
                    )
                    .collect()
            };
            let filtered: String = available_skills
                .lines()
                .filter(|line| {
                    if let Some(rest) = line.strip_prefix("- ")
                        && let Some((name, _)) = rest.split_once(':')
                    {
                        return !active_set.contains(name.trim());
                    }
                    true
                })
                .collect::<Vec<_>>()
                .join("\n");
            if !filtered.trim().is_empty() {
                system_prompt.push_str(&format!("\n\n## Available Skills\n{}", filtered));
            }
        }

        // Inject knowledge graph context — layer-aware (Phase 2):
        // Layer 1: Working (current session entities) — max 5
        // Layer 2: Semantic (global persistent entities) — max 2 additional
        {
            let mem_guard = self.memory_store.read().await;
            if let Some(ref store) = *mem_guard {
                let candidates = extract_entity_candidates(user_message);
                let mut entities: Vec<&str> = vec!["USER"];
                for c in &candidates {
                    entities.push(c.as_str());
                }
                entities.truncate(6);

                // Layer 1: Working — current session scope, working-layer entities
                let kg_working = store
                    .query_kg_context_layered(&entities, 5, session_id, Some("working"))
                    .await
                    .unwrap_or_default();
                // Layer 2: Semantic — global persistent scope, semantic-layer entities
                let kg_semantic = if !kg_working.is_empty() {
                    // Only fetch global if working layer already has results
                    store
                        .query_kg_context_layered(&entities, 2, "global", Some("semantic"))
                        .await
                        .unwrap_or_default()
                } else {
                    // Fallback: no working results, get more from global
                    store
                        .query_kg_context_layered(&entities, 5, "global", Some("semantic"))
                        .await
                        .unwrap_or_default()
                };

                // Combine: working first, then semantic (deduped implicitly by
                // query_kg_context's per-entity handling)
                let mut kg_parts: Vec<&str> = Vec::new();
                if !kg_working.is_empty() {
                    kg_parts.push(&kg_working);
                }
                if !kg_semantic.is_empty() && kg_semantic != kg_working {
                    kg_parts.push(&kg_semantic);
                }
                let combined = kg_parts.join("\n");
                if !combined.is_empty() {
                    system_prompt.push_str(&format!("\n\n{}", combined));
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

    /// Shared session-close consolidation: promote high-confidence session
    /// entities to global scope (scope-based), then run a lightweight layer
    /// consolidation cycle (layer-based).  Fire-and-forget via tokio::spawn.
    fn trigger_session_close_consolidation(&self, session_id: &str) {
        let mem = self.memory_store.clone();
        let c_sid = session_id.to_string();
        let c_consolidation_count = self.consolidation_count.clone();
        let c_sem = self.consolidation_semaphore.clone();
        tokio::spawn(async move {
            let _permit = c_sem.acquire().await;
            let guard = mem.read().await;
            if let Some(ref store) = *guard {
                // Scope-based promotion: session-scoped → global
                match store.promote_to_global(&c_sid, 0.6).await {
                    Ok((nodes, cogs)) => {
                        let c_count = c_consolidation_count.fetch_add(1, Ordering::Relaxed) + 1;
                        if nodes > 0 || cogs > 0 {
                            tracing::info!(
                                session_id = %c_sid,
                                promoted_nodes = nodes,
                                promoted_cognitions = cogs,
                                consolidation_count = c_count,
                                "session close consolidation: promoted high-confidence entities to global"
                            );
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            session_id = %c_sid,
                            error = %e,
                            "session close consolidation: promote_to_global failed"
                        );
                    }
                }
                // Layer-based consolidation: run a lightweight L0-L3 cycle
                match store.run_consolidation_cycle().await {
                    Ok(json) => {
                        tracing::info!(
                            session_id = %c_sid,
                            report = %json,
                            "session close: layer consolidation cycle completed"
                        );
                    }
                    Err(e) => {
                        tracing::warn!(
                            session_id = %c_sid,
                            error = %e,
                            "session close: layer consolidation cycle failed"
                        );
                    }
                }
            }
        });
    }

    /// Process a user message with a specific session and agent config.
    /// Uses the Agent state machine: Idle → Thinking → Completed (or Errored).
    /// `attached_images` are ContentPart::Image items to send directly to the model.
    /// `selected_skills` are skill names whose full content should be injected into the system prompt.
    #[allow(clippy::too_many_arguments)]
    pub async fn process_message_with_config(
        &self,
        user_message: &str,
        session_id: &str,
        agent_config: &loom_types::AgentConfig,
        thinking_budget: Option<usize>,
        attached_images: Vec<ContentPart>,
        selected_skills: Vec<String>,
        permission_mode: &str,
        skip_user_message: bool,
    ) -> Result<TurnResult> {
        tracing::info!(
            session_id,
            msg_len = user_message.len(),
            img_count = attached_images.len(),
            "[orchestrator] process_message_with_config ENTER"
        );

        // Lazy-refresh persona from memory store at the START of each turn.
        // This captures all async extraction results from the previous turn
        // (which now runs in a non-blocking tokio::spawn task).
        // Phase 2: use rich persona with confidence scores and layered structure.
        {
            let store = self.memory_store.read().await;
            if let Some(ref s) = *store
                && let Ok(Some(persona)) = s.get_rich_persona().await
                && !persona.is_empty()
            {
                *self.persona_context.write().await = persona;
            }
        }

        // Read shared contexts BEFORE pool.spawn to avoid any lock interaction
        // with the agent pool state (especially when another message's
        // entity-extraction completion is about to write persona_context).
        let user_persona = {
            let p = self.persona_context.read().await;
            let raw = p.clone();
            // Append behavior adaptation hints derived from the persona
            let behavior_hints = adapt_behavior(&raw);
            if behavior_hints.is_empty() {
                raw
            } else {
                format!("{}\n{}", behavior_hints, raw)
            }
        };
        let skills = {
            let state = self.skill_state.read().await;
            state.context.clone()
        };

        // ── Slash-command pre-processing (Claude Code-style dispatch) ──
        // When cc_dispatch is enabled, intercept /skillname before the model
        // sees it. The skill body is injected directly into the system prompt
        // and the slash prefix is stripped from the user message.
        let (effective_message, slash_skill_body) = {
            if agent_config.cc_dispatch {
                let router = self.slash_router.read().await;
                if let Some(intercept) = router.intercept(user_message) {
                    tracing::info!(
                        skill = %intercept.skill_name,
                        "slash command intercepted, injecting skill body"
                    );
                    let skill_header = format!(
                        "## Active Skill (Loaded by /{})\nThe following skill was activated by slash command. Follow its instructions directly — do NOT call use_skill for this.\n\n### Skill: {}\n{}",
                        intercept.skill_name, intercept.skill_name, intercept.skill_body
                    );
                    (intercept.stripped_message, Some(skill_header))
                } else {
                    (user_message.to_string(), None)
                }
            } else {
                (user_message.to_string(), None)
            }
        };
        // Fall back to original message if the stripped version is empty
        let user_message = if effective_message.is_empty() {
            user_message.to_string()
        } else {
            effective_message
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
        // Also include always_active skill names so the agent loop knows
        // the full set of skills injected into the system prompt.
        let mut merged_skill_permissions: Vec<SkillPermissionConfig> = Vec::new();
        let mut effective_selected_skills = selected_skills.clone();
        {
            let state = self.skill_state.read().await;
            for name in &selected_skills {
                if let Some(perms) = state.permissions.get(name) {
                    merged_skill_permissions.push(perms.clone());
                }
            }
            // Append always_active skills that aren't already in the list
            for summary in state.summaries.iter().filter(|s| s.always_active) {
                if !effective_selected_skills.contains(&summary.name) {
                    effective_selected_skills.push(summary.name.clone());
                    if let Some(perms) = state.permissions.get(&summary.name) {
                        merged_skill_permissions.push(perms.clone());
                    }
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

        // ── Memory quality tracking: capture injected entities and start time ──
        let quality_start = std::time::Instant::now();
        let quality_candidates = extract_entity_candidates(&user_message);
        let mut quality_injected_names: Vec<String> = Vec::new();
        for c in &quality_candidates {
            quality_injected_names.push(c.clone());
        }
        quality_injected_names.truncate(6);

        // Build full system prompt via shared method (persona, instructions, profile,
        // selected skills, available skills, KG context, workspace path).
        let mut system_prompt = self
            .build_full_system_prompt(
                agent_config,
                &user_persona,
                &skills,
                &selected_skills,
                &user_message,
                session_id,
                &workspace_path,
            )
            .await;

        // Inject slash-command skill body (if intercepted)
        if let Some(ref body) = slash_skill_body {
            system_prompt.push_str("\n\n");
            system_prompt.push_str(body);
        }

        // Plan mode: inject planning instructions into system prompt
        if permission_mode == "plan" {
            system_prompt.push_str("\n\n## 规划模式\n");
            system_prompt.push_str("你正处于 **规划模式（Plan Mode）**。在此模式下：\n");
            system_prompt.push_str("- **禁止**修改文件、执行 Shell 命令或任何破坏性操作。\n");
            system_prompt
                .push_str("- 你应当深入分析代码库，探索相关文件，创建一个详细的实施方案。\n");
            system_prompt.push_str(
                "- 方案应包含：需要修改的文件、架构决策、分步实施计划、边界情况和潜在风险。\n",
            );
            system_prompt.push_str("- 使用清晰的 Markdown 标题、列表和代码片段呈现方案。\n");
            system_prompt
                .push_str("- 用户审核方案后，会切换到 **Operate（操作）** 模式来开始实施。\n");
            system_prompt
                .push_str("- 你可以自由使用只读工具（Read、Grep、Glob）和搜索工具进行分析。\n");
        }

        // Build default permissions based on permission_mode:
        // - "operate": allow everything (legacy behavior)
        // - "ask": allow everything but agent loop will prompt for medium/high risk
        // - "read_only": deny all writes and shell, allow reads only
        // - "plan": same as read_only — analyze/plan only, no writes
        tracing::info!(
            session_id,
            permission_mode,
            "building base_permissions from permission_mode"
        );
        let base_permissions = match permission_mode {
            "read_only" | "plan" => SkillPermissions {
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
            merge_multi_permissions(merged_skill_permissions.iter().map(Some), &base_permissions)
        };

        // Global defaults (overridable per agent)
        let max_prompt_budget = *self.default_max_prompt_budget.read().await;
        let default_max_iters = *self.default_max_iterations.read().await;

        // Ensure history is loaded from DB if not already in cache (e.g. after restart or session switch)
        if self.session_history(session_id).await.is_empty() {
            tracing::info!(session_id, "[orchestrator] step: loading history from DB");
            if let Err(e) = self.load_history(session_id).await {
                tracing::warn!(session_id = %session_id, error = %e, "Failed to load conversation history from DB");
            }
            tracing::info!(session_id, "[orchestrator] step: history loaded");
        }
        let history = self.session_history(session_id).await;
        tracing::info!(
            session_id,
            hist_len = history.len(),
            "[orchestrator] step: history ready"
        );

        // ── Summary check ──
        let (existing_summary, should_summarize) = {
            let mem = self.memory_store.read().await;
            if let Some(ref store) = *mem {
                let existing = store.get_summary(session_id).await.unwrap_or(None);
                let last_at = store.get_summary_at_count(session_id).await.unwrap_or(0);
                let total_msgs = store
                    .get_message_count(session_id)
                    .await
                    .unwrap_or(history.len());
                let should =
                    loom_memory::SummaryEngine::should_summarize(total_msgs, last_at, 12, 6);
                (existing, should)
            } else {
                (None, false)
            }
        }; // lock released

        let summary = if should_summarize {
            tracing::info!(session_id, "summarization triggered");
            let prompt =
                loom_memory::SummaryEngine::build_prompt(&history, existing_summary.as_deref());
            let request = loom_memory::SummaryEngine::build_request(&prompt);
            // Use auxiliary client if available, fall back to main
            let summary_client: Option<Arc<dyn CloudClient>> =
                if let Some(aux) = self.build_auxiliary_client("summary").await {
                    Some(aux)
                } else {
                    self.cloud_client.read().await.clone()
                };
            if let Some(sc) = summary_client {
                let saved_hash = sc.prefix_hash_snapshot();
                match sc.complete(request).await {
                    Ok(resp) if !resp.text.is_empty() => {
                        sc.prefix_hash_restore(saved_hash);
                        let mem = self.memory_store.read().await;
                        if let Some(ref store) = *mem {
                            let _ = store.save_summary(session_id, &resp.text).await;
                            if resp.prompt_tokens > 0 || resp.completion_tokens > 0 {
                                let _ = store
                                    .record_token_usage(
                                        session_id,
                                        &format!("{} (summary)", sc.model_name()),
                                        resp.prompt_tokens,
                                        resp.completion_tokens,
                                        0,
                                        0,
                                        0,
                                        0,
                                    )
                                    .await;
                            }
                        }
                        tracing::info!(chars = resp.text.len(), "conversation summarized");
                        Some(resp.text)
                    }
                    _ => {
                        sc.prefix_hash_restore(saved_hash);
                        existing_summary
                    }
                }
            } else {
                existing_summary
            }
        } else {
            existing_summary
        };

        let loop_config = AgentLoopConfig {
            system_prompt,
            summary,
            temperature: agent_config.temperature.unwrap_or(0.0),
            max_iterations: agent_config.max_iterations.unwrap_or(default_max_iters),
            // Bump output budget when skills are active — skills often involve
            // large file generation (HTML slides, code, etc.) that 4096 can't fit.
            max_tokens: if effective_selected_skills.is_empty() {
                4096
            } else {
                0
            },
            thinking_budget: if agent_config.cc_dispatch {
                Some(thinking_budget.unwrap_or(4096))
            } else {
                thinking_budget
            },
            model_configs: self.model_configs.read().await.values().cloned().collect(),
            active_model_name: self.active_model_name.read().await.clone(),
            workspace_path: workspace_path.clone(),
            default_permissions,
            max_prompt_budget,
            hook_registry: Some(self.hook_registry.clone()),
            session_id: session_id.to_string(),
            agent_id: agent_id.to_string(),
            key_store: Some(self.key_store.clone()),
            loom_dir: Some(self.data_dir_path().to_path_buf()),
            permission_mode: permission_mode.to_string(),
            cc_dispatch: agent_config.cc_dispatch,
            event_bus: Some(self.pool.event_bus().clone()),
            pending_permissions: Some(self.pending_permissions.clone()),
            // Pass selected_skills so the agent loop can bypass lazy_tools when
            // skill instructions are already injected into the system prompt.
            selected_skills: effective_selected_skills.clone(),
            // Number of available skills — used for soft skill-first routing.
            available_skill_count: {
                let state = self.skill_state.read().await;
                state.summaries.len()
            },
            sandbox: {
                let sc = self.sandbox_config.read().await.clone();
                if sc.enabled {
                    let ws_path = workspace_path.as_ref().map(std::path::PathBuf::from);
                    Some(Arc::new(loom_security::sandbox::SandboxGuard::new(
                        sc, ws_path,
                    )))
                } else {
                    None
                }
            },
            lazy_tools: effective_selected_skills.is_empty(),
            ..Default::default()
        };
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
                        event_bus.publish(AgentEvent::StreamDelta {
                            agent_id: forward_agent_id.clone(),
                            session_id: forward_session_id.clone(),
                            delta: t,
                        });
                    }
                    StreamDelta::Reasoning(t) => {
                        event_bus.publish(AgentEvent::StreamDelta {
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
                            event_bus.publish(AgentEvent::ToolStarted {
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
                        event_bus.publish(AgentEvent::ToolCompleted {
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
                        event_bus.publish(AgentEvent::StreamDelta {
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
                        let (p_tokens, c_tokens, cr_tokens, cw_tokens) = if prompt_tokens > 0
                            && completion_tokens == 0
                        {
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
                        event_bus.publish(AgentEvent::TokenUsage {
                            agent_id: forward_agent_id.clone(),
                            session_id: forward_session_id.clone(),
                            model: usage_model.clone(),
                            prompt_tokens: p_tokens as usize,
                            completion_tokens: c_tokens as usize,
                            cached_tokens: (cr_tokens + cw_tokens) as usize,
                            cache_read_tokens: cr_tokens as usize,
                            cache_write_tokens: cw_tokens as usize,
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
                            tracing::warn!(
                                "[token-stats] memory_store is None, cannot record main model usage"
                            );
                        }
                    }
                    StreamDelta::AuxiliaryUsage {
                        model,
                        prompt_tokens,
                        completion_tokens,
                    } => {
                        // Persist auxiliary model token usage under its own model name
                        event_bus.publish(AgentEvent::TokenUsage {
                            agent_id: forward_agent_id.clone(),
                            session_id: forward_session_id.clone(),
                            model: model.clone(),
                            prompt_tokens: prompt_tokens as usize,
                            completion_tokens: completion_tokens as usize,
                            cached_tokens: 0,
                            cache_read_tokens: 0,
                            cache_write_tokens: 0,
                            latency_ms: 0,
                            context_window: 0,
                        });
                        if let Some(store) = &*memory_store.read().await
                            && let Err(e) = store
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
            // Emit ToolStarted for any tools that had no args chunks
            for (index, (id, name)) in pending_tool_announces {
                if announced_tools.insert(id.clone()) {
                    started_tools.push((id.clone(), name.clone()));
                    let args_str = tool_args_acc.get(&index).cloned().unwrap_or_default();
                    let args: serde_json::Value =
                        serde_json::from_str(&args_str).unwrap_or(serde_json::Value::Null);
                    event_bus.publish(AgentEvent::ToolStarted {
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
                event_bus.publish(AgentEvent::ToolCompleted {
                    agent_id: forward_agent_id.clone(),
                    call_id: call_id.clone(),
                    tool_name: tool_name.clone(),
                    success: true,
                    result: None,
                    structured_content: None,
                });
            }
            // Send StreamEnd when channel closes
            event_bus.publish(AgentEvent::StreamEnd {
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

        // Always run the full agent path — the intent classifier was an
        // over-optimization that caused workspace/skills/persona/KG to be
        // silently dropped in direct-reply mode.
        let needs_tools = true;

        let result = if !needs_tools {
            // Direct reply — single completion, no tools at all.
            // Use a minimal system prompt: no persona, no skill catalog, no KG.
            let slim_history: Vec<Message> = history.iter().rev().take(4).rev().cloned().collect();
            let reg = registry.read().await;
            let base_prompt = self.loop_config.read().await.system_prompt.clone();
            let direct_config = AgentLoopConfig {
                system_prompt: base_prompt, // base instructions only, no persona/skills/KG
                persona: None,
                summary: None,
                kg_context: None,
                lazy_tools: false,
                max_iterations: 1,
                model_configs: loop_config.model_configs.clone(),
                active_model_name: loop_config.active_model_name.clone(),
                thinking_budget: loop_config.thinking_budget,
                ..Default::default()
            };
            run_agent_turn_streaming_with_images(
                client.as_ref(),
                &reg,
                &slim_history,
                &user_msg,
                &attached_images,
                &direct_config,
                delta_tx,
                &Some(vec![]), // no tools at all
                &disallowed,
                &cancel,
            )
            .await
        } else {
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
            let was_interrupted =
                turn.response.contains("[已中断]") || turn.response.contains("[连接中断]");

            // Stop hook is already fired by agent_loop on cancellation; do not double-fire.

            let _ = self
                .pool
                .transition(
                    &agent_id,
                    if was_interrupted {
                        AgentStatus::Killed
                    } else {
                        AgentStatus::Completed
                    },
                    None,
                )
                .await;

            if was_interrupted {
                // Save user message and partial assistant response so context
                // is preserved for the next user message.
                let user_msg_full = build_user_message(&user_msg, &attached_images);
                let mut user_parts = user_msg_full.content.clone();
                let _ = self
                    .convert_images_to_refs(session_id, &mut user_parts)
                    .await;
                if !skip_user_message {
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
                }

                // Build assistant content from the partial TurnResult
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

                // Persist tool messages from the partial turn too
                for tool_msg in &turn.tool_messages {
                    self.add_to_history(session_id, tool_msg.clone()).await;
                }

                // Persist to memory store — use save_turn so both user +
                // assistant content is durable across restarts.
                let mem = self.memory_store.read().await;
                if let Some(ref store) = *mem {
                    let user_content_json =
                        serde_json::to_string(&user_parts).unwrap_or_else(|_| user_msg.clone());
                    let tool_json: Vec<String> = turn
                        .tool_messages
                        .iter()
                        .map(|m| serde_json::to_string(&m.content).unwrap_or_default())
                        .collect();
                    if let Err(e) = store
                        .save_turn(
                            session_id,
                            &user_content_json,
                            &content_json,
                            turn.tool_calls_made,
                            turn.prompt_tokens,
                            turn.completion_tokens,
                            turn.cache_read_tokens,
                            turn.cache_write_tokens,
                            0, // context_window recorded via record_token_usage
                            &tool_json,
                        )
                        .await
                    {
                        tracing::warn!(session_id, error = %e, "failed to persist interrupted turn");
                    }
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
                    let tool_json: Vec<String> = turn
                        .tool_messages
                        .iter()
                        .map(|m| serde_json::to_string(&m.content).unwrap_or_default())
                        .collect();
                    let event_id = store
                        .save_turn(
                            session_id,
                            &user_content_json,
                            &content_json,
                            turn.tool_calls_made,
                            turn.prompt_tokens,
                            turn.completion_tokens,
                            turn.cache_read_tokens,
                            turn.cache_write_tokens,
                            0, // context_window recorded via record_token_usage
                            &tool_json,
                        )
                        .await?;

                    // ── Memory quality logging ──
                    // Record injected entities, duration, and referenced entities for feedback loop
                    let quality_duration_ms = quality_start.elapsed().as_millis() as i64;
                    let assistant_text_for_scan = extract_text(&assistant_parts);
                    let quality_referenced: Vec<String> = quality_injected_names
                        .iter()
                        .filter(|name| {
                            let lower_name = name.to_lowercase();
                            assistant_text_for_scan.to_lowercase().contains(&lower_name)
                        })
                        .cloned()
                        .collect();
                    let quality_log_id = match store
                        .record_memory_quality(
                            session_id,
                            0, // turn_seq computed internally by the store
                            &quality_injected_names,
                            quality_duration_ms,
                        )
                        .await
                    {
                        Ok(id) => id,
                        Err(e) => {
                            tracing::warn!(session_id, error = %e, "failed to record memory quality log");
                            0
                        }
                    };
                    if quality_log_id > 0
                        && !quality_referenced.is_empty()
                        && let Err(e) = store
                            .update_quality_references(quality_log_id, &quality_referenced)
                            .await
                    {
                        tracing::warn!(session_id, error = %e, "failed to update quality references");
                    }

                    // Spawn async entity extraction pipeline (non-blocking)
                    self.spawn_extraction_pipeline(
                        session_id.to_string(),
                        user_message.to_string(),
                        turn.response.clone(),
                        event_id,
                    )
                    .await;
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

        // Phase 2: Session close handling — trigger consolidation
        // (scope-based promotion + layer-based L0-L3 cycle, fire-and-forget).
        self.trigger_session_close_consolidation(&sid);

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
    #[allow(clippy::too_many_arguments)]
    pub async fn process_message_streaming(
        &self,
        user_message: &str,
        delta_tx: mpsc::Sender<StreamDelta>,
        session_id: &str,
        thinking_budget: Option<usize>,
        attached_images: Vec<ContentPart>,
        selected_skills: Vec<String>,
        permission_mode: &str,
    ) -> Result<TurnResult> {
        // Lazy-refresh persona from memory store at the START of each turn.
        // This captures all async extraction results from the previous turn
        // (which now runs in a non-blocking tokio::spawn task).
        // Phase 2: use rich persona with confidence scores and layered structure.
        {
            let store = self.memory_store.read().await;
            if let Some(ref s) = *store
                && let Ok(Some(persona)) = s.get_rich_persona().await
                && !persona.is_empty()
            {
                *self.persona_context.write().await = persona;
            }
        }

        // Read shared contexts BEFORE pool.spawn (same reason as
        // process_message_with_config).
        let user_persona = {
            let p = self.persona_context.read().await;
            let raw = p.clone();
            let behavior_hints = adapt_behavior(&raw);
            if behavior_hints.is_empty() {
                raw
            } else {
                format!("{}\n{}", behavior_hints, raw)
            }
        };
        let skills = {
            let state = self.skill_state.read().await;
            state.context.clone()
        };

        // Resolve agent config for this session (falls back to "default")
        let agent_config = self.resolve_session_agent_config(session_id).await;

        // ── Slash-command pre-processing (Claude Code-style dispatch) ──
        let (effective_message, slash_skill_body) = {
            if agent_config.cc_dispatch {
                let router = self.slash_router.read().await;
                if let Some(intercept) = router.intercept(user_message) {
                    tracing::info!(
                        skill = %intercept.skill_name,
                        "streaming: slash command intercepted, injecting skill body"
                    );
                    let skill_header = format!(
                        "## Active Skill (Loaded by /{})\nThe following skill was activated by slash command. Follow its instructions directly — do NOT call use_skill for this.\n\n### Skill: {}\n{}",
                        intercept.skill_name, intercept.skill_name, intercept.skill_body
                    );
                    (intercept.stripped_message, Some(skill_header))
                } else {
                    (user_message.to_string(), None)
                }
            } else {
                (user_message.to_string(), None)
            }
        };
        let user_message = if effective_message.is_empty() {
            user_message.to_string()
        } else {
            effective_message
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
        // Load history from DB if in-memory cache is empty (e.g. after restart)
        if self.session_history(session_id).await.is_empty()
            && let Err(e) = self.load_history(session_id).await
        {
            tracing::warn!(session_id = %session_id, error = %e, "Failed to load conversation history from DB");
        }
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
            let (summary_client, summary_model) =
                if let Some(aux_client) = self.build_auxiliary_client("summary").await {
                    let model_name = aux_client.model_name().to_string();
                    (aux_client, model_name)
                } else {
                    let model_name = format!("{} (summary)", client.model_name());
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
                    if (sum_prompt > 0 || sum_completion > 0)
                        && let Some(ref store) = *self.memory_store.read().await
                        && let Err(e) = store
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

        // ── Memory quality tracking: capture injected entities and start time ──
        let quality_start = std::time::Instant::now();
        let quality_candidates = extract_entity_candidates(&user_message);
        let mut quality_injected_names: Vec<String> = Vec::new();
        for c in &quality_candidates {
            quality_injected_names.push(c.clone());
        }
        quality_injected_names.truncate(6);

        // ── System prompt assembly (shared method) ──
        let mut system_prompt = self
            .build_full_system_prompt(
                &agent_config,
                &user_persona,
                &skills,
                &selected_skills,
                &user_message,
                session_id,
                &workspace_path,
            )
            .await;

        // Inject slash-command skill body (if intercepted)
        if let Some(ref body) = slash_skill_body {
            system_prompt.push_str("\n\n");
            system_prompt.push_str(body);
        }

        // Collect skill permissions for merged tool-call permissions.
        // Also include always_active skill names so the agent loop knows
        // the full set of skills injected into the system prompt.
        let mut merged_skill_permissions: Vec<SkillPermissionConfig> = Vec::new();
        let mut effective_selected_skills = selected_skills.clone();
        {
            let state = self.skill_state.read().await;
            for name in &selected_skills {
                if let Some(perms) = state.permissions.get(name) {
                    merged_skill_permissions.push(perms.clone());
                }
            }
            for summary in state.summaries.iter().filter(|s| s.always_active) {
                if !effective_selected_skills.contains(&summary.name) {
                    effective_selected_skills.push(summary.name.clone());
                    if let Some(perms) = state.permissions.get(&summary.name) {
                        merged_skill_permissions.push(perms.clone());
                    }
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
            merge_multi_permissions(merged_skill_permissions.iter().map(Some), &base_permissions)
        };

        let allowed = agent_config.allowed_tools.clone();
        let disallowed = agent_config.disallowed_tools.clone();
        let cancel = self.pool.cancel_token(&agent_id).await?;

        // Global defaults (overridable per agent)
        let max_prompt_budget = *self.default_max_prompt_budget.read().await;
        let default_max_iters = *self.default_max_iterations.read().await;

        let config = AgentLoopConfig {
            system_prompt,
            // persona already baked into system_prompt via build_full_system_prompt
            persona: None,
            summary,
            kg_context: None,
            thinking_budget: if agent_config.cc_dispatch {
                Some(thinking_budget.unwrap_or(4096))
            } else {
                thinking_budget
            },
            model_configs: self.model_configs.read().await.values().cloned().collect(),
            active_model_name: self.active_model_name.read().await.clone(),
            workspace_path: workspace_path.clone(),
            default_permissions,
            max_prompt_budget,
            hook_registry: Some(self.hook_registry.clone()),
            session_id: session_id.to_string(),
            agent_id: agent_id.to_string(),
            key_store: Some(self.key_store.clone()),
            loom_dir: Some(self.data_dir_path().to_path_buf()),
            permission_mode: permission_mode.to_string(),
            cc_dispatch: agent_config.cc_dispatch,
            event_bus: Some(self.pool.event_bus().clone()),
            pending_permissions: Some(self.pending_permissions.clone()),
            max_iterations: default_max_iters,
            // Bump output budget when skills are active.
            max_tokens: if effective_selected_skills.is_empty() {
                4096
            } else {
                0
            },
            // When selected_skills is non-empty, bypass lazy_tools so the LLM can
            // act on skill instructions immediately without a request_tools round-trip.
            lazy_tools: effective_selected_skills.is_empty(),
            selected_skills: effective_selected_skills,
            // Number of available skills — used for soft skill-first routing
            // when the model requests web_search.
            available_skill_count: {
                let state = self.skill_state.read().await;
                state.summaries.len()
            },
            sandbox: {
                let sc = self.sandbox_config.read().await.clone();
                if sc.enabled {
                    let ws_path = workspace_path.as_ref().map(std::path::PathBuf::from);
                    Some(Arc::new(loom_security::sandbox::SandboxGuard::new(
                        sc, ws_path,
                    )))
                } else {
                    None
                }
            },
            ..Default::default()
        };
        let result = run_agent_turn_streaming_with_images(
            client.as_ref(),
            &registry,
            &history,
            &user_message,
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
            let was_interrupted =
                turn.response.contains("[已中断]") || turn.response.contains("[连接中断]");

            // Stop hook is already fired by agent_loop on cancellation; do not double-fire.

            let _ = self
                .pool
                .transition(
                    &agent_id,
                    if was_interrupted {
                        AgentStatus::Killed
                    } else {
                        AgentStatus::Completed
                    },
                    None,
                )
                .await;

            if was_interrupted {
                // Save user message and partial assistant response so context
                // is preserved for the next user message.
                let user_msg_full = build_user_message(&user_message, &attached_images);
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

                // Build assistant content from the partial TurnResult
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

                // Persist tool messages from the partial turn too
                for tool_msg in &turn.tool_messages {
                    self.add_to_history(session_id, tool_msg.clone()).await;
                }

                // Persist to memory store — use save_turn so both user +
                // assistant content is durable across restarts.
                let mem = self.memory_store.read().await;
                if let Some(ref store) = *mem {
                    let user_content_json = serde_json::to_string(&user_parts)
                        .unwrap_or_else(|_| user_message.to_string());
                    let tool_json: Vec<String> = turn
                        .tool_messages
                        .iter()
                        .map(|m| serde_json::to_string(&m.content).unwrap_or_default())
                        .collect();
                    if let Err(e) = store
                        .save_turn(
                            session_id,
                            &user_content_json,
                            &content_json,
                            turn.tool_calls_made,
                            turn.prompt_tokens,
                            turn.completion_tokens,
                            turn.cache_read_tokens,
                            turn.cache_write_tokens,
                            0, // context_window recorded via record_token_usage
                            &tool_json,
                        )
                        .await
                    {
                        tracing::warn!(session_id, error = %e, "failed to persist interrupted turn");
                    }
                }
            } else {
                // Convert images to file refs before caching / persisting
                let user_msg_full = build_user_message(&user_message, &attached_images);
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
                    let user_content_json = serde_json::to_string(&user_parts)
                        .unwrap_or_else(|_| user_message.to_string());
                    let tool_json: Vec<String> = turn
                        .tool_messages
                        .iter()
                        .map(|m| serde_json::to_string(&m.content).unwrap_or_default())
                        .collect();
                    let event_id = store
                        .save_turn(
                            session_id,
                            &user_content_json,
                            &content_json,
                            turn.tool_calls_made,
                            turn.prompt_tokens,
                            turn.completion_tokens,
                            turn.cache_read_tokens,
                            turn.cache_write_tokens,
                            0, // context_window recorded via record_token_usage
                            &tool_json,
                        )
                        .await?;

                    // ── Memory quality logging ──
                    let quality_duration_ms = quality_start.elapsed().as_millis() as i64;
                    let assistant_text_for_scan = extract_text(&assistant_parts);
                    let quality_referenced: Vec<String> = quality_injected_names
                        .iter()
                        .filter(|name| {
                            let lower_name = name.to_lowercase();
                            assistant_text_for_scan.to_lowercase().contains(&lower_name)
                        })
                        .cloned()
                        .collect();
                    let quality_log_id = match store
                        .record_memory_quality(
                            session_id,
                            0, // turn_seq computed internally by the store
                            &quality_injected_names,
                            quality_duration_ms,
                        )
                        .await
                    {
                        Ok(id) => id,
                        Err(e) => {
                            tracing::warn!(session_id, error = %e, "failed to record memory quality log");
                            0
                        }
                    };
                    if quality_log_id > 0
                        && !quality_referenced.is_empty()
                        && let Err(e) = store
                            .update_quality_references(quality_log_id, &quality_referenced)
                            .await
                    {
                        tracing::warn!(session_id, error = %e, "failed to update quality references");
                    }

                    // Spawn async entity extraction pipeline (non-blocking)
                    self.spawn_extraction_pipeline(
                        session_id.to_string(),
                        user_message.to_string(),
                        turn.response.clone(),
                        event_id,
                    )
                    .await;
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

        // Phase 2: Session close handling — trigger consolidation
        // (scope-based promotion + layer-based L0-L3 cycle, fire-and-forget).
        self.trigger_session_close_consolidation(&sid);

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
    ) -> tokio::sync::RwLockWriteGuard<'_, HashMap<String, tokio::sync::oneshot::Sender<bool>>>
    {
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

    // === Sandbox Config ===

    /// Load sandbox configuration from `data_dir/sandbox.json`.
    /// Returns default if the file does not exist or cannot be parsed.
    pub async fn load_sandbox_config(&self) -> SandboxConfig {
        let path = self.data_dir.join("sandbox.json");
        match tokio::fs::read_to_string(&path).await {
            Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
            Err(_) => SandboxConfig::default(),
        }
    }

    /// Save sandbox configuration to `data_dir/sandbox.json`.
    pub async fn save_sandbox_config(&self, config: &SandboxConfig) -> Result<()> {
        let _ = tokio::fs::create_dir_all(&self.data_dir).await;
        let path = self.data_dir.join("sandbox.json");
        let contents =
            serde_json::to_string_pretty(config).context("failed to serialize sandbox config")?;
        tokio::fs::write(&path, &contents)
            .await
            .context("failed to write sandbox.json")?;
        // Update in-memory state so enforcement uses the new config immediately
        *self.sandbox_config.write().await = config.clone();
        Ok(())
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
                self.lsp_client
                    .completion(&file_path_str, line, character)
                    .await
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
                self.lsp_client
                    .definition(&file_path_str, line, character)
                    .await
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
    user_text: &str,
    assistant_text: &str,
    scope: &str,
) -> Result<(
    Vec<ExtractedEntity>,
    Vec<ExtractedRelationship>,
    usize,
    usize,
)> {
    let conversation = if assistant_text.is_empty() {
        format!("User message: {}", user_text)
    } else {
        format!("User: {}\nAssistant: {}", user_text, assistant_text)
    };
    let prompt = format!("{}\n\n{}", LLM_EXTRACTION_PROMPT, conversation);

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

pub mod agent_loop;
pub mod config;
pub mod events;
pub mod heartbeat;
pub mod memory_thread;
pub mod persona_watcher;
pub mod rate_limiter;
pub mod session;
pub mod shutdown;
pub mod stream;
pub mod token_store;

use crate::heartbeat::spawn_hub_heartbeat;
use crate::rate_limiter::RateLimiter;
use crate::session::{SessionCommand, spawn_session_thread};
use crate::token_store::{TokenUsageRecord, spawn_token_store_thread};

use std::path::{Path, PathBuf};

use anyhow::Result;
use openloom_cache::NoopCache;
use openloom_inference::{CloudClient, CompletionRequest, InferenceEngine};

/// Scan a directory for .gguf model files, sorted by size (smallest first).
/// Excludes vision projection files (mmproj- prefix) and files < 100 MiB.
fn find_gguf_models(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<(PathBuf, u64)> = match std::fs::read_dir(dir) {
        Ok(entries) => entries
            .filter_map(|e| {
                let e = e.ok()?;
                let p = e.path();
                let name = p.file_name()?.to_str()?;
                if !p.extension().map(|x| x == "gguf").unwrap_or(false) {
                    return None;
                }
                // Skip vision projection files (mmproj- prefix)
                if name.starts_with("mmproj-") {
                    return None;
                }
                let meta = std::fs::metadata(&p).ok()?;
                // Skip files smaller than 100 MiB (likely not usable models)
                if meta.len() < 100 * 1024 * 1024 {
                    return None;
                }
                Some((p, meta.len()))
            })
            .collect(),
        Err(_) => return vec![],
    };
    files.sort_by_key(|(_, size)| *size);
    files.into_iter().map(|(p, _)| p).collect()
}
use openloom_memory::persona::CognitionsPersonaProvider;
use openloom_memory::store::MessageStore;
use openloom_models::*;
use openloom_router::SmartRouter;
use openloom_skills::{Skill, SkillRegistry, builtins};
use openloom_weaver::ContextWeaver;
use std::sync::Mutex;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::time::Instant;
use tokio::sync::{RwLock, broadcast, oneshot};

pub use openloom_models::EngineEvent;

/// Channel payload: a permission request paired with a oneshot sender for the user's response.
pub type PermissionChannelItem = (PermissionRequest, tokio::sync::oneshot::Sender<bool>);

pub(crate) const SYSTEM_INSTRUCTION: &str = "You are openLoom, a coding assistant and AI agent running locally.

## Environment
- Working directory: [cwd]
- Platform: [platform]

## Tool Use
When you need to use a tool, respond with ONLY a JSON block:
{\"tool\": \"<name>\", \"params\": {\"key\": \"value\"}}

One tool call per response. After getting the result, you may call another tool or give your final answer.

## Available Tools

[tools]

## Workflow
1. Read files before editing them.
2. Make minimal, precise edits.
3. Run tests/checks after changes to verify correctness.
4. Search before making assumptions about code structure.

## Rules
- Final answers must be natural language (no JSON).
- Answer in the same language as the user.
- Be concise and direct.";

pub(crate) fn system_instruction() -> String {
    let cwd = std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "unknown".into());
    let platform = if cfg!(target_os = "windows") {
        "Windows"
    } else if cfg!(target_os = "macos") {
        "macOS"
    } else {
        "Linux"
    };
    let project_context = detect_project_context();
    let mut prompt = SYSTEM_INSTRUCTION
        .replace("[cwd]", &cwd)
        .replace("[platform]", platform);
    if !project_context.is_empty() {
        prompt.push_str("\n\n## Project Context\n");
        prompt.push_str(&project_context);
    }
    prompt
}

fn detect_project_context() -> String {
    let cwd = match std::env::current_dir() {
        Ok(p) => p,
        Err(_) => return String::new(),
    };

    let data_dir = dirs::data_dir()
        .unwrap_or_default()
        .join("openLoom");

    let mut context_parts: Vec<String> = Vec::new();

    if cwd.join("Cargo.toml").exists() {
        context_parts.push("- Rust project (Cargo.toml found)".into());
    } else if cwd.join("package.json").exists() {
        context_parts.push("- Node.js project (package.json found)".into());
    } else if cwd.join("pyproject.toml").exists() || cwd.join("setup.py").exists() {
        context_parts.push("- Python project".into());
    } else if cwd.join("go.mod").exists() {
        context_parts.push("- Go project (go.mod found)".into());
    }

    if cwd.join(".git").exists() {
        context_parts.push("- Git repository".into());
    }

    if cwd.join("CLAUDE.md").exists()
        && let Ok(content) = std::fs::read_to_string(cwd.join("CLAUDE.md"))
    {
        let preview: String = content.chars().take(500).collect();
        context_parts.push(format!("- Project instructions (CLAUDE.md):\n{}", preview));
    }

    let loom_ctx = openloom_skills::loom_context::LoomContext::load(&data_dir, &cwd);
    if !loom_ctx.is_empty() {
        context_parts.push(format!("- Project instructions (loom.md):\n{}", loom_ctx));
    }

    context_parts.join("\n")
}

pub struct Engine {
    router: SmartRouter,
    skills: SkillRegistry,
    inference: Arc<InferenceEngine>,
    cloud: Option<Arc<dyn CloudClient>>,
    local_client: Option<Arc<dyn CloudClient>>,
    weaver: ContextWeaver,
    persona: Arc<dyn PersonaProvider>,
    memory_tx: std::sync::mpsc::Sender<memory_thread::ProcessRequest>,
    session_tx: std::sync::mpsc::Sender<SessionCommand>,
    event_bus: broadcast::Sender<EngineEvent>,
    agent_state: Arc<RwLock<AgentState>>,
    interruptible: AtomicBool,
    data_dir: PathBuf,
    db_path: PathBuf,
    config: Arc<RwLock<AppConfig>>,
    start_time: Instant,
    draining: AtomicBool,
    in_flight: AtomicUsize,
    rate_limiter: Mutex<RateLimiter>,
    token_store_tx: std::sync::mpsc::Sender<TokenUsageRecord>,
    model_available: bool,
    last_user_message: Arc<Mutex<Instant>>,
    skip_permissions: bool,
    max_output_tokens: usize,
    context_max_chars: usize,
    perm_request_tx: tokio::sync::mpsc::Sender<PermissionChannelItem>,
    perm_request_rx: std::sync::Mutex<Option<tokio::sync::mpsc::Receiver<PermissionChannelItem>>>,
}

pub struct EngineConfig {
    pub data_dir: PathBuf,
    pub threshold: usize,
    pub cloud_config: Option<openloom_models::ModelConfig>,
    pub local_config: Option<openloom_models::ModelConfig>,
    pub rate_limit_ms: u64,
    pub heartbeat_interval_secs: u64,
    pub heartbeat_idle_threshold_min: u64,
    /// Optional model path override; when set, use this instead of auto-detection.
    pub model_override: Option<PathBuf>,
    /// Project scope identifier derived from working directory (e.g. "project:F:/myApp")
    pub project_scope: String,
    /// When true, skip permission confirmations (--dangerously-skip-permissions)
    pub skip_permissions: bool,
}

impl Engine {
    pub fn new_test(db_path: PathBuf) -> Result<Self> {
        let data_dir = db_path.parent().unwrap().to_path_buf();
        // Create dummy model file so tests don't show "degraded" health status
        let model_dir = data_dir.join("models");
        let _ = std::fs::create_dir_all(&model_dir);
        let model_path = model_dir.join("dummy.gguf");
        let _ = std::fs::write(&model_path, b"test");
        // rate_limit_ms=0 disables rate limiting in tests
        Self::new(EngineConfig {
            data_dir,
            threshold: 3,
            cloud_config: None,
            local_config: None,
            rate_limit_ms: 0,
            heartbeat_interval_secs: 1800,
            heartbeat_idle_threshold_min: 120,
            model_override: Some(model_path),
            project_scope: "global".into(),
            skip_permissions: true,
        })
    }

    pub fn new(config: EngineConfig) -> Result<Self> {
        // Ensure data directory structure exists
        let _ = std::fs::create_dir_all(config.data_dir.join("plugins"));
        let _ = std::fs::create_dir_all(config.data_dir.join("skills"));
        let _ = std::fs::create_dir_all(config.data_dir.join("models"));
        let _ = std::fs::create_dir_all(config.data_dir.join("db"));

        // Create default loom.md if it doesn't exist
        let global_loom = config.data_dir.join("loom.md");
        if !global_loom.exists() {
            let _ = std::fs::write(
                &global_loom,
                "# openLoom Global Instructions\n\n<!-- Add your global instructions here. They will be injected into every conversation. -->\n",
            );
        }

        let model_dir = config.data_dir.join("models");
        let model_path = config.model_override.clone().unwrap_or_else(|| {
            find_gguf_models(&model_dir)
                .first()
                .cloned()
                .unwrap_or_else(|| model_dir.join("model.gguf"))
        });

        // For summarizer, use the second model if auto-detected, or the same
        // override when explicitly given.
        let summarizer_path: Option<PathBuf> = if config.model_override.is_some() {
            None // when model is overridden, don't guess a summarizer
        } else {
            find_gguf_models(&model_dir).get(1).cloned()
        };

        let model_available =
            model_path.exists() && model_path.metadata().map(|m| m.len() > 0).unwrap_or(false);
        if !model_available {
            tracing::warn!(dir = %model_dir.display(), "No .gguf model found, local inference unavailable");
        }

        if model_available {
            tracing::info!(path = %model_path.display(), "Using local model");
        }

        let inference = Arc::new(InferenceEngine::load_blocking(&model_path, 0)?);

        let mut router =
            SmartRouter::new_keywords_only(openloom_router::keywords::default_keyword_rules());
        let mut skills = SkillRegistry::new();
        builtins::register_all(&mut skills);
        for skill in skills.all_skills() {
            let manifest = skill.manifest();
            router.register_skill_triggers(skill.name(), manifest.triggers.clone());
        }

        // Discover and register external skills (plugins + project-local)
        let cwd = std::env::current_dir().unwrap_or_default();
        let external_skills = openloom_skills::plugin_loader::PluginLoader::discover(
            &config.data_dir,
            &cwd,
        );
        for ext_skill in external_skills {
            tracing::info!(name = ext_skill.qualified_name(), "Loaded external skill");
            let manifest = ext_skill.manifest().clone();
            let name = ext_skill.qualified_name().to_string();
            skills.register(Box::new(ext_skill));
            if !manifest.triggers.is_empty() {
                router.register_skill_triggers(&name, manifest.triggers);
            }
        }

        let cloud: Option<Arc<dyn CloudClient>> = config.cloud_config.as_ref().and_then(|cfg| {
            openloom_inference::create_cloud_client(cfg)
                .ok()
                .map(Arc::from)
        });

        let local_client: Option<Arc<dyn CloudClient>> =
            config.local_config.as_ref().and_then(|cfg| {
                openloom_inference::create_cloud_client(cfg)
                    .ok()
                    .map(Arc::from)
            });

        router.set_cloud_available(cloud.is_some() || local_client.is_some());

        let db_path = config.data_dir.join("data").join("db.sqlite");
        let _ = std::fs::create_dir_all(db_path.parent().unwrap());

        // Ensure all migrations are applied before any subsystem opens the DB
        {
            let mut conn = rusqlite::Connection::open(&db_path)?;
            conn.execute_batch("PRAGMA journal_mode=WAL;")?;
            openloom_memory::store::SqliteEventStore::run_migrations(&mut conn)?;
        }

        let persona: Arc<dyn PersonaProvider> = Arc::new(CognitionsPersonaProvider::new(
            db_path.clone(),
            config.project_scope.clone(),
        ));
        let weaver = ContextWeaver::new(Arc::new(NoopCache));

        let (event_tx, _) = broadcast::channel(256);
        let memory_tx = memory_thread::spawn_memory_thread(
            db_path.clone(),
            config.threshold,
            event_tx.clone(),
            summarizer_path,
            config.project_scope.clone(),
        );
        let session_tx = spawn_session_thread(db_path.clone());
        let token_store_tx = spawn_token_store_thread(db_path.clone());

        // Extract heartbeat config before config is partially moved into Self
        let hb_interval = config.heartbeat_interval_secs;
        let hb_idle_threshold = config.heartbeat_idle_threshold_min;

        let (perm_request_tx, perm_request_rx) = tokio::sync::mpsc::channel(1);

        let engine = Self {
            router,
            skills,
            inference,
            cloud,
            local_client,
            weaver,
            persona,
            memory_tx,
            session_tx,
            event_bus: event_tx,
            agent_state: Arc::new(RwLock::new(AgentState::Idle)),
            interruptible: AtomicBool::new(false),
            data_dir: config.data_dir.clone(),
            db_path: db_path.clone(),
            config: Arc::new(RwLock::new(AppConfig::default())),
            start_time: Instant::now(),
            draining: AtomicBool::new(false),
            in_flight: AtomicUsize::new(0),
            rate_limiter: Mutex::new(RateLimiter::new(config.rate_limit_ms)),
            token_store_tx,
            model_available,
            last_user_message: Arc::new(Mutex::new(Instant::now())),
            skip_permissions: config.skip_permissions,
            max_output_tokens: config
                .cloud_config
                .as_ref()
                .or(config.local_config.as_ref())
                .map(|c| c.effective_max_output())
                .unwrap_or(4096),
            context_max_chars: config
                .cloud_config
                .as_ref()
                .or(config.local_config.as_ref())
                .map(|c| c.context_size * 4)
                .unwrap_or(0),
            perm_request_tx,
            perm_request_rx: std::sync::Mutex::new(Some(perm_request_rx)),
        };

        // Spawn persona watcher (from persona_watcher module)
        persona_watcher::spawn(engine.persona.clone(), engine.event_bus.clone());

        // Spawn hub heartbeat (Phase 3A)
        spawn_hub_heartbeat(
            engine.inference.clone(),
            engine.agent_state.clone(),
            engine.event_bus.clone(),
            engine.last_user_message.clone(),
            hb_interval,
            hb_idle_threshold,
        );

        Ok(engine)
    }

    // === Core handler ===

    /// Template response when no cloud API key is configured and the router
    /// would otherwise send chat requests to the local 1.7B model.
    const NO_CLOUD_RESPONSE: &str = "\
我是 openLoom，一个本地优先的 AI 助理。\n\
\n\
我注意到你还没有配置云端 API key。目前本地小模型（Qwen3-1.7B）的定位是意图分类和关键词路由，不适合生成对话回复。\n\
\n\
请设置环境变量 OPENAI_API_KEY 或 ANTHROPIC_API_KEY 后重启，我就能正常对话了。\n\
\n\
当前支持的命令：文件管理、代码协助、网页搜索、日程提醒。";

    pub async fn handle_message(&self, msg: ChatMessage, session_id: &str, mode: openloom_models::Mode) -> Result<ChatResponse> {
        // Rate limiting
        {
            let mut limiter = self.rate_limiter.lock().unwrap();
            limiter.check()?;
        }
        // Track last user message time for heartbeat idle detection
        *self.last_user_message.lock().unwrap() = Instant::now();
        // Drain check
        if self.draining.load(Ordering::SeqCst) {
            return Err(anyhow::anyhow!("Server is shutting down"));
        }
        // Atomic mid-turn check: compare_exchange ensures only one caller enters
        if self
            .interruptible
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(anyhow::anyhow!("Agent is busy, please wait"));
        }
        // C1 fix: do NOT release the gate here -- only at the end of each path

        let out = self.router.classify_sync(&msg.content);

        // C2 fix: feed cognition extraction pipeline (non-blocking)
        let _ = self.memory_tx.send(memory_thread::ProcessRequest {
            session_id: session_id.to_string(),
            text: msg.content.clone(),
            context: out.intent.to_string(),
        });

        // C3 fix: complex intent or skill match -> agent loop
        let mode_cfg = mode.config();
        if mode_cfg.agent_loop && (out.complexity >= 0.8 || out.skill_match.is_some()) {
            return self.agent_loop(&msg, session_id, mode).await;
        }

        // Simple path: track in_flight here (agent_loop tracks its own)
        self.in_flight.fetch_add(1, Ordering::SeqCst);
        let start = Instant::now();
        let skill_ctx = out.skill_match.as_ref().and_then(|name| {
            self.skills
                .find_by_name(name)
                .map(|s| s.context_md().to_string())
        });
        let working_memory = self.get_working_memory(session_id).unwrap_or_default();
        // Persona failure -> empty string fallback
        let persona_summary = self.persona.summarize().await.unwrap_or_default();
        let system = system_instruction();
        let system = if mode_cfg.system_suffix.is_empty() {
            system
        } else {
            format!("{}\n\n{}", system, mode_cfg.system_suffix)
        };
        let assembled = self.weaver.assemble_with_limit(
            &system,
            &msg.content,
            &persona_summary,
            skill_ctx.as_deref(),
            &working_memory,
            self.context_max_chars,
        );

        let response = match out.target_model {
            TargetModel::None => {
                // skill_match.is_some() routes to agent_loop above; this branch is unreachable
                unreachable!(
                    "TargetModel::None with no skill_match -- should have gone to agent_loop"
                )
            }
            TargetModel::Local => {
                if let Some(ref local) = self.local_client {
                    local
                        .complete(CompletionRequest {
                            prompt: assembled.prompt.clone(),
                            ..Default::default()
                        })
                        .await?
                        .text
                } else if self.cloud.is_some() {
                    self.inference
                        .complete(CompletionRequest {
                            prompt: assembled.prompt.clone(),
                            ..Default::default()
                        })
                        .await?
                        .text
                } else {
                    Self::NO_CLOUD_RESPONSE.to_string()
                }
            }
            TargetModel::Cloud => {
                if let Some(ref cloud) = self.cloud {
                    cloud
                        .complete(CompletionRequest {
                            prompt: assembled.prompt.clone(),
                            ..Default::default()
                        })
                        .await?
                        .text
                } else if let Some(ref local) = self.local_client {
                    local
                        .complete(CompletionRequest {
                            prompt: assembled.prompt.clone(),
                            ..Default::default()
                        })
                        .await?
                        .text
                } else {
                    Self::NO_CLOUD_RESPONSE.to_string()
                }
            }
        };

        // save_messages is non-fatal
        let _ = self.save_messages(session_id, &msg, &response);

        let prompt_tokens = self.inference.token_count(&assembled.prompt);
        let completion_tokens = self.inference.token_count(&response);
        let latency_ms = start.elapsed().as_millis() as u64;
        self.in_flight.fetch_sub(1, Ordering::SeqCst);
        let _ = self.event_bus.send(EngineEvent::TokenUsage {
            session_id: session_id.to_string(),
            model: "qwen3-1.7b".into(),
            prompt_tokens,
            completion_tokens,
            cached_tokens: 0,
            latency_ms,
        });
        let _ = self.token_store_tx.send(TokenUsageRecord {
            session_id: session_id.to_string(),
            model: "qwen3-1.7b".into(),
            prompt_tokens,
            completion_tokens,
            cached_tokens: 0,
            latency_ms,
        });

        // C1 fix: reset interruptible flag only at end of simple path
        self.interruptible.store(false, Ordering::SeqCst);

        Ok(ChatResponse {
            response,
            session_id: session_id.to_string(),
            token_usage: TokenUsage {
                prompt_tokens,
                completion_tokens,
                cached_tokens: 0,
                latency_ms,
            },
        })
    }

    // === Message persistence (non-fatal) ===

    fn save_messages(
        &self,
        session_id: &str,
        user_msg: &ChatMessage,
        assistant_response: &str,
    ) -> Result<()> {
        let conn = match rusqlite::Connection::open(&self.db_path) {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("save_messages: {}", e);
                return Ok(());
            }
        };
        let _ = conn.execute_batch("PRAGMA journal_mode=WAL;");
        let store = MessageStore::new(&conn);
        let next_seq = store.max_seq(session_id).unwrap_or(0) + 1;
        let _ = store.insert(session_id, next_seq, "user", &user_msg.content);
        let _ = store.insert(session_id, next_seq + 1, "assistant", assistant_response);
        let _ = self.session_tx.send(SessionCommand::UpdateCount {
            id: session_id.to_string(),
            count: next_seq + 1,
        });
        Ok(())
    }

    fn save_all_messages(
        &self,
        session_id: &str,
        user_msg: &ChatMessage,
        tool_msgs: &[ChatMessage],
        final_response: &str,
    ) -> Result<()> {
        let conn = match rusqlite::Connection::open(&self.db_path) {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("save_all_messages: {}", e);
                return Ok(());
            }
        };
        let _ = conn.execute_batch("PRAGMA journal_mode=WAL;");
        let store = MessageStore::new(&conn);
        let mut seq = store.max_seq(session_id).unwrap_or(0) + 1;
        let _ = store.insert(session_id, seq, "user", &user_msg.content);
        seq += 1;
        for msg in tool_msgs {
            let _ = store.insert(session_id, seq, &msg.role, &msg.content);
            seq += 1;
        }
        let _ = store.insert(session_id, seq, "assistant", final_response);
        let _ = self.session_tx.send(SessionCommand::UpdateCount {
            id: session_id.to_string(),
            count: seq,
        });
        Ok(())
    }

    pub fn get_working_memory(&self, session_id: &str) -> Result<Vec<ChatMessage>> {
        match rusqlite::Connection::open(&self.db_path) {
            Ok(conn) => {
                let store = MessageStore::new(&conn);
                store.all(session_id)
            }
            Err(e) => {
                tracing::warn!("get_working_memory: {}", e);
                Ok(Vec::new())
            }
        }
    }

    // === Public API ===

    pub async fn health_check(&self) -> HealthStatus {
        let gpu = InferenceEngine::detect_gpu();
        HealthStatus {
            status: if self.model_available {
                "ok".into()
            } else {
                "degraded".into()
            },
            uptime: self.start_time.elapsed().as_secs(),
            gpu_info: gpu,
        }
    }

    pub async fn create_session(&self) -> Result<SessionInfo> {
        let (tx, rx) = oneshot::channel();
        self.session_tx
            .send(SessionCommand::Create { reply: tx })
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        rx.await.map_err(|e| anyhow::anyhow!("{}", e))
    }

    pub async fn list_sessions(&self) -> Result<Vec<SessionInfo>> {
        let (tx, rx) = oneshot::channel();
        self.session_tx
            .send(SessionCommand::List { reply: tx })
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        rx.await.map_err(|e| anyhow::anyhow!("{}", e))
    }

    pub async fn list_cognitions(
        &self,
        subject: &str,
        limit: usize,
    ) -> Result<Vec<openloom_memory::store::CognitionRow>> {
        let conn = rusqlite::Connection::open(&self.db_path)?;
        let store = openloom_memory::store::CognitionStore::new(&conn);
        store.query_by_subject(subject, limit)
    }

    pub async fn cognition_snapshots(
        &self,
        cognition_id: i64,
    ) -> Result<Vec<openloom_memory::store::CognitionSnapshot>> {
        let conn = rusqlite::Connection::open(&self.db_path)?;
        let store = openloom_memory::store::CognitionStore::new(&conn);
        store.snapshots_for(cognition_id)
    }

    pub async fn rollback_cognition(&self, cognition_id: i64, version: i64) -> Result<()> {
        let conn = rusqlite::Connection::open(&self.db_path)?;
        let store = openloom_memory::store::CognitionStore::new(&conn);
        let snapshots = store.snapshots_for(cognition_id)?;
        let target = snapshots
            .iter()
            .find(|s| s.version == version)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "version {} not found for cognition {}",
                    version,
                    cognition_id
                )
            })?;
        let (subject, scope): (String, String) = conn.query_row(
            "SELECT subject, scope FROM cognitions WHERE id = ?1",
            rusqlite::params![cognition_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        store.insert(
            &subject,
            &target.trait_name,
            &target.value,
            target.confidence,
            target.evidence_count,
            &scope,
        )?;
        let _ = self.event_bus.send(EngineEvent::CognitionUpdated {
            trait_name: target.trait_name.clone(),
            old_value: String::new(),
            new_value: target.value.clone(),
            confidence: target.confidence,
        });
        Ok(())
    }

    pub async fn persona_summary(&self) -> String {
        self.persona.summarize().await.unwrap_or_default()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<EngineEvent> {
        self.event_bus.subscribe()
    }

    /// Take the permission request receiver. The TUI polls this to show
    /// approval overlays when the agent loop needs user confirmation.
    /// Returns `None` on subsequent calls (the receiver can only be taken once).
    pub fn take_permission_rx(&self) -> Option<tokio::sync::mpsc::Receiver<PermissionChannelItem>> {
        self.perm_request_rx.lock().unwrap().take()
    }

    pub fn list_skills(&self) -> Vec<openloom_skills::SkillInfo> {
        self.skills.list_all()
    }

    pub async fn invoke_skill(
        &self,
        name: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        self.skills.invoke(name, params).await
    }

    pub async fn agent_state(&self) -> AgentState {
        self.agent_state.read().await.clone()
    }

    pub fn cache_stats(&self) -> openloom_cache::CacheStats {
        self.weaver.cache().stats()
    }

    pub fn loom_context(&self) -> String {
        let cwd = std::env::current_dir().unwrap_or_default();
        openloom_skills::loom_context::LoomContext::load(&self.data_dir, &cwd)
    }

    pub fn data_dir(&self) -> &std::path::Path {
        &self.data_dir
    }

    pub fn find_skill_by_name(&self, name: &str) -> Option<String> {
        self.skills
            .find_by_name(name)
            .map(|s| s.context_md().to_string())
    }

    pub fn external_skill_names(&self) -> Vec<(String, String)> {
        self.skills
            .list_all()
            .iter()
            .filter(|s| s.name.contains(':'))
            .map(|s| (s.name.clone(), s.description.clone()))
            .collect()
    }

    pub fn model_display_name(&self) -> String {
        if let Some(ref cloud) = self.cloud {
            return format!("{} ({})", cloud.model_name(), cloud.provider().name());
        }
        if let Some(ref local) = self.local_client {
            return format!("{} ({})", local.model_name(), local.provider().name());
        }
        "no model configured".into()
    }

    pub async fn model_context_size(&self) -> usize {
        let config = self.config.read().await;
        // Find first cloud-capable model with context_size set
        if let Some(cfg) = config.models.iter().find(|m| m.backend.is_cloud_capable()) {
            if cfg.context_size > 4096 {
                return cfg.context_size;
            }
            if let Some(ref model_name) = cfg.model
                && let Some(size) = parse_context_hint(model_name)
            {
                return size;
            }
            return match cfg.backend {
                openloom_models::ModelBackend::Anthropic => 200_000,
                openloom_models::ModelBackend::OpenAI => 128_000,
                openloom_models::ModelBackend::DeepSeek => 64_000,
                openloom_models::ModelBackend::LmStudio => 32_000,
                openloom_models::ModelBackend::Ollama => 32_000,
                _ => 128_000,
            };
        }
        // Fallback: infer from live cloud/local client
        if self.cloud.is_some() {
            return 128_000;
        }
        if self.local_client.is_some() {
            return 32_000;
        }
        config
            .models
            .iter()
            .find(|m| m.backend == openloom_models::ModelBackend::LlamaCpp)
            .map(|m| m.context_size)
            .unwrap_or(4096)
    }

    pub fn token_summary_by_model(&self) -> Result<Vec<openloom_memory::store::ModelUsageSummary>> {
        let conn = rusqlite::Connection::open(&self.db_path)?;
        let store = openloom_memory::store::TokenStore::new(&conn);
        store.summary_by_model()
    }

    pub fn token_usage_today(&self) -> Result<openloom_memory::store::UsageAggregate> {
        let conn = rusqlite::Connection::open(&self.db_path)?;
        let store = openloom_memory::store::TokenStore::new(&conn);
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        store.usage_since(&today)
    }

    pub fn token_usage_session(
        &self,
        session_id: &str,
        limit: usize,
    ) -> Result<Vec<openloom_memory::store::TokenUsageRow>> {
        let conn = rusqlite::Connection::open(&self.db_path)?;
        let store = openloom_memory::store::TokenStore::new(&conn);
        store.query_by_session(session_id, limit)
    }

    pub fn token_recent(&self, limit: usize) -> Result<Vec<openloom_memory::store::TokenUsageRow>> {
        let conn = rusqlite::Connection::open(&self.db_path)?;
        let store = openloom_memory::store::TokenStore::new(&conn);
        store.recent(limit)
    }

    pub async fn model_info(&self) -> ModelInfoResult {
        let (model_id, backend, base_url, api_key_env) = {
            let config = self.config.read().await;
            let cloud = config.models.iter().find(|m| m.backend.is_cloud_capable());
            let local = config
                .models
                .iter()
                .find(|m| m.backend.is_local_inference());
            let active = cloud.or(local);
            (
                active.and_then(|c| c.model.clone()).unwrap_or_default(),
                active
                    .map(|c| c.backend.name().to_string())
                    .unwrap_or_else(|| "none".into()),
                active.and_then(|c| c.base_url.clone()).unwrap_or_default(),
                active
                    .and_then(|c| c.api_key_env.clone())
                    .unwrap_or_default(),
            )
        };
        let ctx_size = self.model_context_size().await;
        let api_key_set = if api_key_env.is_empty() {
            false
        } else {
            std::env::var(&api_key_env).is_ok()
        };

        ModelInfoResult {
            display_name: self.model_display_name(),
            model_id,
            backend,
            base_url,
            api_key_env,
            api_key_set,
            context_size: ctx_size,
        }
    }
}

pub struct ModelInfoResult {
    pub display_name: String,
    pub model_id: String,
    pub backend: String,
    pub base_url: String,
    pub api_key_env: String,
    pub api_key_set: bool,
    pub context_size: usize,
}

fn parse_context_hint(model_name: &str) -> Option<usize> {
    let start = model_name.find('[')?;
    let end = model_name.find(']')?;
    if end <= start + 1 {
        return None;
    }
    let hint = &model_name[start + 1..end];
    let hint_lower = hint.to_lowercase();
    if let Some(num_str) = hint_lower.strip_suffix('m') {
        num_str
            .parse::<f64>()
            .ok()
            .map(|n| (n * 1_000_000.0) as usize)
    } else if let Some(num_str) = hint_lower.strip_suffix('k') {
        num_str.parse::<f64>().ok().map(|n| (n * 1_000.0) as usize)
    } else {
        hint.parse::<usize>().ok()
    }
}

// === Tests ===

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use tempfile::tempdir;

    async fn setup_test_engine() -> (Engine, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let engine = Engine::new_test(db_path).unwrap();
        (engine, dir)
    }

    #[tokio::test]
    async fn test_create_and_list_sessions() {
        let (engine, _dir) = setup_test_engine().await;
        let s1 = engine.create_session().await.unwrap();
        let s2 = engine.create_session().await.unwrap();
        let sessions = engine.list_sessions().await.unwrap();
        assert_eq!(sessions.len(), 2);
        assert!(sessions.iter().any(|s| s.id == s1.id));
        assert!(sessions.iter().any(|s| s.id == s2.id));
    }

    #[tokio::test]
    async fn test_handle_message_llm_path() {
        let (engine, _dir) = setup_test_engine().await;
        let msg = ChatMessage {
            role: "user".into(),
            content: "hello".into(),
            timestamp: Utc::now(),
        };
        let sid = engine.create_session().await.unwrap().id;
        let resp = engine.handle_message(msg, &sid, Mode::Code).await.unwrap();
        assert_eq!(resp.session_id, sid);
    }

    #[tokio::test]
    async fn test_health_check() {
        let (engine, _dir) = setup_test_engine().await;
        let health = engine.health_check().await;
        // new_test creates a dummy model file so status is "ok"
        assert_eq!(health.status, "ok");
        // uptime is reported as seconds; may be 0 when test runs very quickly
        let _ = health.uptime;
    }

    #[tokio::test]
    async fn test_event_bus_subscribe() {
        let (engine, _dir) = setup_test_engine().await;
        let mut rx = engine.subscribe();
        let msg = ChatMessage {
            role: "user".into(),
            content: "hello".into(),
            timestamp: Utc::now(),
        };
        let sid = engine.create_session().await.unwrap().id;
        engine.handle_message(msg, &sid, Mode::Code).await.unwrap();
        let event = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv()).await;
        assert!(event.is_ok(), "should receive TokenUsage event");
    }

    #[tokio::test]
    async fn test_handle_message_skill_path() {
        let (engine, _dir) = setup_test_engine().await;
        let msg = ChatMessage {
            role: "user".into(),
            content: "帮我管理文件".into(),
            timestamp: Utc::now(),
        };
        let sid = engine.create_session().await.unwrap().id;
        let resp = engine.handle_message(msg, &sid, Mode::Code).await.unwrap();
        assert!(!resp.session_id.is_empty());
    }

    fn sync_setup() -> (Engine, tempfile::TempDir) {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(setup_test_engine())
    }

    #[test]
    fn test_parse_tool_call_valid() {
        let (engine, _dir) = sync_setup();
        let result = engine.parse_tool_call("{\"tool\": \"test\", \"params\": {\"k\": \"v\"}}");
        assert!(result.is_some());
        let call = result.unwrap();
        assert_eq!(call.tool, "test");
    }

    #[test]
    fn test_parse_tool_call_with_whitespace() {
        let (engine, _dir) = sync_setup();
        let result = engine.parse_tool_call("  {\"tool\": \"test\", \"params\": {}}");
        assert!(result.is_some());
    }

    #[test]
    fn test_parse_tool_call_malformed_json() {
        let (engine, _dir) = sync_setup();
        let result = engine.parse_tool_call("{\"tool\": \"test\", \"params\": {}");
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_tool_call_no_json() {
        let (engine, _dir) = sync_setup();
        let result = engine.parse_tool_call("This is a normal response");
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_tool_call_nested_braces() {
        let (engine, _dir) = sync_setup();
        let json = "{\"tool\": \"test\", \"params\": {\"nested\": {\"a\": 1}}}";
        let result = engine.parse_tool_call(json);
        assert!(result.is_some());
    }

    #[test]
    fn test_agent_state_defaults_to_idle() {
        let (engine, _dir) = sync_setup();
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let state = rt.block_on(engine.agent_state());
        assert_eq!(state, AgentState::Idle);
    }

    #[tokio::test]
    async fn test_rate_limit_allows_first_request() {
        let (engine, _dir) = setup_test_engine().await;
        let msg = ChatMessage {
            role: "user".into(),
            content: "hello".into(),
            timestamp: Utc::now(),
        };
        let sid = engine.create_session().await.unwrap().id;
        let resp = engine.handle_message(msg, &sid, Mode::Code).await;
        // First request must pass rate limiter (interval=100ms, elapsed=0 but check resets)
        assert!(
            resp.is_ok(),
            "first request should pass rate limiter: {:?}",
            resp.err()
        );
    }

    #[tokio::test]
    async fn test_search_events_empty() {
        let (engine, _dir) = setup_test_engine().await;
        let results = engine.search_events("nonexistent", 10).await;
        assert!(results.is_ok(), "search_events should succeed on empty db");
        assert_eq!(results.unwrap().len(), 0);
    }

    #[tokio::test]
    async fn test_list_events_empty() {
        let (engine, _dir) = setup_test_engine().await;
        let results = engine.list_events(10).await;
        assert!(results.is_ok(), "list_events should succeed on empty db");
        assert_eq!(results.unwrap().len(), 0);
    }
}

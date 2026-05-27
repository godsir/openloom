pub mod agent_loop;
pub mod bridge;
pub mod checkpoint;
pub mod computer_use;
pub mod config;
pub mod cron_scheduler;
pub mod events;
pub mod heartbeat;
pub mod memory_thread;
pub mod persona_watcher;
pub mod plugin_fs;
pub mod rate_limiter;
pub mod secrets;
pub mod session;
pub mod shutdown;
pub mod skill_bundles;
pub mod skill_fs;
pub mod stream;
pub mod token_store;
pub mod vision;

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
use openloom_skills::cron_store::{CronStore, NotificationStore};
use openloom_skills::{Skill, SkillInfo, SkillRegistry, builtins};
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

pub(crate) const SYSTEM_INSTRUCTION: &str =
    "You are openLoom, a coding assistant and AI agent running locally.

## Environment
- Working directory: [cwd]
- Platform: [platform]

## Workflow
1. Read files before editing them.
2. Make minimal, precise edits.
3. Run tests/checks after changes to verify correctness.
4. Search before making assumptions about code structure.

## Rules
- Answer in the same language as the user.
- Be concise and direct.
";

fn detect_project_context(cwd: &std::path::Path, data_dir: &std::path::Path) -> String {
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

    let loom_ctx = openloom_skills::loom_context::LoomContext::load(data_dir, cwd);
    if !loom_ctx.is_empty() {
        context_parts.push(format!("- Project instructions (loom.md):\n{}", loom_ctx));
    }

    context_parts.join("\n")
}

/// Build ToolDefinition array from registered skills for native tool calling.
/// Sanitize a skill name for API tool name requirements: ^[a-zA-Z0-9_-]+$
pub(crate) fn sanitize_tool_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn input_schema_for_skill(name: &str) -> serde_json::Value {
    match name {
        "file_read" => serde_json::json!({
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "File path to read"},
                "offset": {"type": "integer", "description": "Line to start from (0-indexed, default 0)"},
                "limit": {"type": "integer", "description": "Max lines to return (default 2000)"}
            },
            "required": ["path"]
        }),
        "file_write" => serde_json::json!({
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "File path to write to"},
                "content": {"type": "string", "description": "Content to write"}
            },
            "required": ["path"]
        }),
        "file_edit" => serde_json::json!({
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "File path to edit"},
                "old_string": {"type": "string", "description": "Exact string to replace"},
                "new_string": {"type": "string", "description": "Replacement string"},
                "replace_all": {"type": "boolean", "description": "Replace all occurrences (default false)"}
            },
            "required": ["path", "old_string"]
        }),
        "file_search" => serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": {"type": "string", "description": "Glob pattern, e.g. '**/*.rs'"},
                "path": {"type": "string", "description": "Base directory (default current dir)"}
            },
            "required": ["pattern"]
        }),
        "content_search" => serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": {"type": "string", "description": "Regex pattern to search for"},
                "path": {"type": "string", "description": "Base directory (default current dir)"},
                "glob": {"type": "string", "description": "File glob filter, e.g. '**/*.html' (default '**/*')"},
                "max_results": {"type": "integer", "description": "Max results (default 50)"}
            },
            "required": ["pattern"]
        }),
        "shell" => serde_json::json!({
            "type": "object",
            "properties": {
                "command": {"type": "string", "description": "Shell command to execute"},
                "cwd": {"type": "string", "description": "Working directory for command"},
                "timeout_ms": {"type": "integer", "description": "Timeout in ms (default 120000)"}
            },
            "required": ["command"]
        }),
        "web-browser" => serde_json::json!({
            "type": "object",
            "properties": {
                "url": {"type": "string", "description": "URL to fetch via HTTP GET"}
            },
            "required": ["url"]
        }),
        "schedule-reminder" => serde_json::json!({
            "type": "object",
            "properties": {
                "action": {"type": "string", "enum": ["add", "list"]},
                "title": {"type": "string", "description": "Reminder title"},
                "time": {"type": "string", "description": "Reminder time"}
            },
            "required": ["action"]
        }),
        _ => serde_json::json!({
            "type": "object",
            "properties": {},
            "additionalProperties": true
        }),
    }
}

pub(crate) fn build_tool_definitions(skills: &[SkillInfo]) -> Vec<ToolDefinition> {
    let mut seen = std::collections::HashSet::new();
    skills
        .iter()
        .filter_map(|s| {
            let safe_name = sanitize_tool_name(&s.name);
            if !seen.insert(safe_name.clone()) {
                return None;
            }
            let schema = input_schema_for_skill(&s.name);
            let schema = if schema
                .get("properties")
                .is_none_or(|p| p.as_object().is_none_or(|o| o.is_empty()))
            {
                input_schema_for_skill(&safe_name)
            } else {
                schema
            };
            Some(ToolDefinition {
                name: safe_name,
                description: s.description.clone(),
                input_schema: schema,
            })
        })
        .collect()
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
    /// User-configured workspace directory. When set, overrides process cwd in system prompt.
    pub active_cwd: std::sync::RwLock<Option<PathBuf>>,
    #[allow(dead_code)]
    bridge_manager: Option<Arc<crate::bridge::BridgeManager>>,
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
    /// Per-session abort flags. Set to true to cancel an in-progress streaming response.
    abort_flags: Arc<Mutex<std::collections::HashMap<String, Arc<AtomicBool>>>>,
    /// Per-session permission mode ("operate" | "ask" | "read_only").
    /// Default mode (when key is missing or session_id is empty) is "ask".
    permission_modes: Arc<Mutex<std::collections::HashMap<String, String>>>,
    /// Default permission mode for new sessions ("operate" | "ask" | "read_only").
    default_permission_mode: Arc<Mutex<String>>,
    /// Cron job store for scheduled task persistence.
    cron_store: Arc<CronStore>,
    /// Notification store for automation notifications.
    notification_store: Arc<NotificationStore>,
    /// Checkpoint store for file backup before tool edits.
    pub checkpoint_store: Arc<std::sync::Mutex<checkpoint::CheckpointStore>>,
}

impl Engine {
    /// Start the cron scheduler. Must be called after the engine is wrapped in Arc.
    pub fn start_cron_scheduler(self: &Arc<Self>) {
        cron_scheduler::spawn_cron_scheduler(
            self.clone(),
            self.cron_store.clone(),
            self.notification_store.clone(),
        );
    }
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
    /// Build the system instruction with the user-configured workspace directory.
    /// Falls back to process cwd when no workspace is set.
    pub(crate) fn system_instruction(&self) -> String {
        let resolved_cwd = self
            .active_cwd
            .read()
            .unwrap()
            .clone()
            .or_else(|| std::env::current_dir().ok());
        let cwd_path = resolved_cwd.as_deref().unwrap_or(std::path::Path::new("."));
        let cwd_str = cwd_path.display().to_string();

        let platform = if cfg!(target_os = "windows") {
            "Windows"
        } else if cfg!(target_os = "macos") {
            "macOS"
        } else {
            "Linux"
        };
        let project_context = detect_project_context(cwd_path, &self.data_dir);
        let mut prompt = SYSTEM_INSTRUCTION
            .replace("[cwd]", &cwd_str)
            .replace("[platform]", platform);
        if !project_context.is_empty() {
            prompt.push_str("\n\n## Project Context\n");
            prompt.push_str(&project_context);
        }
        prompt
    }

    /// Build a system instruction for bridge conversations.
    pub(crate) fn bridge_system_instruction(
        &self,
        platform: &crate::bridge::Platform,
        sender_name: &str,
    ) -> String {
        let base = self.system_instruction();
        format!(
            "{base}\n\n## Bridge Context\n\
             You are responding via {} (external messaging platform).\n\
             The user's name on this platform is: {sender_name}.\n\
             Keep responses concise and conversational. \
             Do not mention internal tools or file paths.",
            platform.name()
        )
    }

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

        let inference = if model_available {
            Arc::new(InferenceEngine::load_blocking(&model_path, 0)?)
        } else {
            Arc::new(InferenceEngine::dummy())
        };

        let mut router =
            SmartRouter::new_keywords_only(openloom_router::keywords::default_keyword_rules());

        // Load AppConfig from disk (needed for skill registration)
        let app_config_raw: AppConfig = {
            let config_path = config.data_dir.join("config.toml");
            if config_path.exists() {
                match std::fs::read_to_string(&config_path) {
                    Ok(content) => match toml::from_str::<AppConfig>(&content) {
                        Ok(loaded) => {
                            tracing::info!(path = %config_path.display(), "Loaded config from disk");
                            loaded
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "Failed to parse config.toml, using defaults");
                            AppConfig::default()
                        }
                    },
                    Err(e) => {
                        tracing::warn!(error = %e, "Failed to read config.toml, using defaults");
                        AppConfig::default()
                    }
                }
            } else {
                AppConfig::default()
            }
        };
        let app_config: Arc<RwLock<AppConfig>> = Arc::new(RwLock::new(app_config_raw.clone()));

        let mut skills = SkillRegistry::new();
        let cron_store = Arc::new(CronStore::new(&config.data_dir));
        let notification_store = Arc::new(NotificationStore::new(&config.data_dir));
        builtins::register_all(
            &mut skills,
            app_config.clone(),
            &config.data_dir,
            cron_store.clone(),
        );
        for skill in skills.all_skills() {
            let manifest = skill.manifest();
            router.register_skill_triggers(skill.name(), manifest.triggers.clone());
        }

        // Apply tools.disabled from config (read before wrapping in Arc)
        {
            let names: Vec<String> = app_config_raw
                .settings
                .get("tools")
                .and_then(|t| t.get("disabled"))
                .and_then(|d| d.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            if !names.is_empty() {
                tracing::info!(?names, "Applying tool disabled list from config");
                skills.set_disabled(names);
            }
        }

        // Discover and register external skills (plugins + project-local)
        let cwd = std::env::current_dir().unwrap_or_default();
        let external_skills =
            openloom_skills::plugin_loader::PluginLoader::discover(&config.data_dir, &cwd);
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

        // Load secrets into process environment
        secrets::load(&config.data_dir);

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
            active_cwd: std::sync::RwLock::new(None),
            bridge_manager: None,
            db_path: db_path.clone(),
            config: app_config,
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
            abort_flags: Arc::new(Mutex::new(std::collections::HashMap::new())),
            permission_modes: Arc::new(Mutex::new(std::collections::HashMap::new())),
            default_permission_mode: Arc::new(Mutex::new("ask".to_string())),
            cron_store: cron_store.clone(),
            notification_store,
            checkpoint_store: Arc::new(std::sync::Mutex::new(checkpoint::CheckpointStore::new(
                &config.data_dir,
            ))),
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

    /// Process a message from a bridge platform and return the Agent's reply text.
    pub async fn handle_bridge_message(&self, msg: crate::bridge::BridgeMessage) -> Result<String> {
        let _session_id = format!("bridge:{}:{}", msg.platform.name(), msg.chat_id);
        let system = self.bridge_system_instruction(&msg.platform, &msg.sender_name);

        let user_text = match &msg.content {
            crate::bridge::MessageContent::Text(t) => t.clone(),
            crate::bridge::MessageContent::Image { caption, url } => {
                if let Some(c) = caption {
                    format!("[Image: {url}]\n{c}")
                } else {
                    format!("[Image: {url}]")
                }
            }
            crate::bridge::MessageContent::File { name, url, .. } => {
                format!("[File: {name} ({url})]")
            }
            crate::bridge::MessageContent::Audio { url, .. } => {
                format!("[Audio: {url}]")
            }
        };

        if user_text.trim().is_empty() {
            return Ok("(No text content received)".to_string());
        }

        // Combine system prompt + user message into a single prompt string
        let full_prompt = format!("{system}\n\nUser: {user_text}");

        let request = CompletionRequest {
            prompt: full_prompt,
            temperature: 0.7,
            ..Default::default()
        };

        if let Some(ref cloud) = self.cloud {
            let response = cloud.complete(request).await?;
            Ok(response.text)
        } else if let Some(ref local) = self.local_client {
            let response = local.complete(request).await?;
            Ok(response.text)
        } else {
            anyhow::bail!("no inference backend available for bridge message")
        }
    }

    // === Core handler ===

    /// Template response when no model is available at all.
    const NO_MODEL_RESPONSE: &str = "\
当前没有可用的模型。请在设置中配置至少一个云端或本地模型。";

    pub async fn handle_message(
        &self,
        msg: ChatMessage,
        session_id: &str,
        mode: openloom_models::Mode,
        model_pref: openloom_models::ModelPreference,
    ) -> Result<ChatResponse> {
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
            return self.agent_loop(&msg, session_id, mode, model_pref).await;
        }

        // Simple path: track in_flight here (agent_loop tracks its own)
        self.in_flight.fetch_add(1, Ordering::SeqCst);
        let start = Instant::now();
        let skill_ctx = out.skill_match.as_ref().and_then(|name| {
            self.skills
                .find_by_name(name)
                .map(|s| s.context_md().to_string())
        });
        let mut working_memory = self.get_working_memory(session_id).unwrap_or_default();

        // Auto-compact: if history exceeds 80% of context window, summarize the older half.
        // Only affects this LLM call — stored history is NOT modified.
        let history_chars: usize = working_memory
            .iter()
            .map(|m| m.content.chars().count())
            .sum();
        let compact_threshold = (self.context_max_chars as f64 * 0.8) as usize;
        if history_chars > compact_threshold
            && self.context_max_chars > 0
            && working_memory.len() > 4
        {
            let split = working_memory.len() / 2;
            let older_text: String = working_memory[..split]
                .iter()
                .filter(|m| m.role == "user" || m.role == "assistant")
                .map(|m| format!("[{}]: {}", m.role, m.content))
                .collect::<Vec<_>>()
                .join("\n\n");
            if !older_text.is_empty() {
                let compact_prompt = format!(
                    "Summarize this conversation history concisely. Include key decisions, code changes, and important context. Keep under 500 characters.\n\n{}",
                    older_text
                );
                if let Ok(summary) = self.invoke_model_raw(&compact_prompt).await {
                    let compact_msg = ChatMessage {
                        role: "system".into(),
                        content: format!("[Earlier conversation summary]\n{}", summary.trim()),
                        timestamp: chrono::Utc::now(),
                        metadata: None,
                        id: None,
                        seq: None,
                    };
                    working_memory = vec![compact_msg]
                        .into_iter()
                        .chain(working_memory[split..].to_vec())
                        .collect();
                    tracing::info!(session_id, "auto-compacted context for chat path");
                }
            }
        }

        // Persona failure -> empty string fallback
        let persona_summary = self.persona.summarize().await.unwrap_or_default();
        let system = self.system_instruction();
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
                } else if let Some(ref cloud) = self.cloud {
                    // No local model available, fall back to cloud
                    cloud
                        .complete(CompletionRequest {
                            prompt: assembled.prompt.clone(),
                            ..Default::default()
                        })
                        .await?
                        .text
                } else {
                    // Try the user's selected active model as last resort
                    let config = self.config.read().await;
                    let active = config.get_nested("settings.active_model");
                    drop(config);
                    if let Some(active) = active {
                        let mid = active.get("id").and_then(|v| v.as_str()).unwrap_or("");
                        let prov = active
                            .get("provider")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        if !mid.is_empty() && !prov.is_empty() {
                            self.complete_with_model(session_id, &assembled.prompt, mid, prov)
                                .await
                                .unwrap_or_else(|_| Self::NO_MODEL_RESPONSE.to_string())
                        } else {
                            Self::NO_MODEL_RESPONSE.to_string()
                        }
                    } else {
                        Self::NO_MODEL_RESPONSE.to_string()
                    }
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
                    // Try the user's selected active model as last resort
                    let config = self.config.read().await;
                    let active = config.get_nested("settings.active_model");
                    drop(config);
                    if let Some(active) = active {
                        let mid = active.get("id").and_then(|v| v.as_str()).unwrap_or("");
                        let prov = active
                            .get("provider")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        if !mid.is_empty() && !prov.is_empty() {
                            self.complete_with_model(session_id, &assembled.prompt, mid, prov)
                                .await
                                .unwrap_or_else(|_| Self::NO_MODEL_RESPONSE.to_string())
                        } else {
                            Self::NO_MODEL_RESPONSE.to_string()
                        }
                    } else {
                        Self::NO_MODEL_RESPONSE.to_string()
                    }
                }
            }
        };

        // save_messages is non-fatal
        let _ = self.save_messages(session_id, &msg, &response);

        let prompt_tokens = self.inference.token_count(&assembled.prompt);
        let completion_tokens = self.inference.token_count(&response);
        let latency_ms = start.elapsed().as_millis() as u64;
        self.in_flight.fetch_sub(1, Ordering::SeqCst);
        let model_id = self.current_model_id();
        let context_window = self.model_context_size().await;
        let _ = self.event_bus.send(EngineEvent::TokenUsage {
            session_id: session_id.to_string(),
            model: model_id.clone(),
            prompt_tokens,
            completion_tokens,
            cached_tokens: 0,
            latency_ms,
            context_window,
        });
        let _ = self.token_store_tx.send(TokenUsageRecord {
            session_id: session_id.to_string(),
            model: model_id,
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

    pub fn save_messages(
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
        // Ensure metadata column exists (migration may not have run)
        let _ = conn.execute_batch("ALTER TABLE message_history ADD COLUMN metadata TEXT;");
        let store = MessageStore::new(&conn);
        let next_seq = store.max_seq(session_id).unwrap_or(0) + 1;
        let _ = store.insert_with_metadata(
            session_id,
            next_seq,
            "user",
            &user_msg.content,
            user_msg.metadata.as_deref(),
        );
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
        let _ = store.insert_with_metadata(
            session_id,
            seq,
            "user",
            &user_msg.content,
            user_msg.metadata.as_deref(),
        );
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

    /// Compute context usage for a session: (used_tokens, total_window, percent).
    /// Uses the live model config for the total window and includes system instruction
    /// overhead in the used count.
    pub async fn context_usage(&self, session_id: &str) -> (usize, usize, f64) {
        let history = self.get_working_memory(session_id).unwrap_or_default();
        let history_text: String = history
            .iter()
            .map(|m| format!("{}: {}\n", m.role, m.content))
            .collect();
        // Include system instruction overhead
        let system_text = self.system_instruction();
        let full_text = if history_text.is_empty() {
            system_text
        } else {
            system_text + "\n" + &history_text
        };
        // Rough token estimate: 1 token ≈ 4 chars for Latin, 2 chars for CJK.
        // Use a blended estimate: count CJK chars at ratio 1:2, others at 1:4.
        let used = {
            let mut cjk = 0usize;
            let mut other = 0usize;
            for c in full_text.chars() {
                if ('\u{4E00}'..='\u{9FFF}').contains(&c)
                    || ('\u{3400}'..='\u{4DBF}').contains(&c)
                    || ('\u{F900}'..='\u{FAFF}').contains(&c)
                {
                    cjk += 1;
                } else {
                    other += 1;
                }
            }
            (cjk / 2 + other / 4).max(1)
        };

        let total = self.model_context_size().await;
        let percent = (used as f64 / total as f64 * 100.0).min(100.0);
        (used, total, percent)
    }

    /// Complete a chat request using a specific model by ID and provider.
    /// Looks up the ModelConfig from config.models OR settings.providers,
    /// creates a cloud client on the fly, and sends the request.
    pub async fn complete_with_model(
        &self,
        session_id: &str,
        prompt: &str,
        model_id: &str,
        provider: &str,
    ) -> Result<String> {
        // Build message history from the session
        let history = self.get_working_memory(session_id).unwrap_or_default();
        let messages: Vec<Message> = history
            .iter()
            .map(|m| {
                if m.role == "user" {
                    Message::user(&m.content)
                } else {
                    Message::assistant(&m.content)
                }
            })
            .chain(std::iter::once(Message::user(prompt)))
            .collect();

        let config = self.config.read().await;
        let provider_lower = provider.to_lowercase();
        let backend = match provider_lower.as_str() {
            "anthropic" => openloom_models::ModelBackend::Anthropic,
            "openai" => openloom_models::ModelBackend::OpenAI,
            "deepseek" => openloom_models::ModelBackend::DeepSeek,
            "lmstudio" | "lm-studio" => openloom_models::ModelBackend::LmStudio,
            "ollama" => openloom_models::ModelBackend::Ollama,
            _ => openloom_models::ModelBackend::OpenAI,
        };

        // Step 1: Try typed config.models
        let typed_cfg = config
            .models
            .iter()
            .find(|m| {
                let m_id = m.model.as_deref().unwrap_or(&m.name);
                m_id == model_id
                    && (m.backend == backend || m.backend.name().eq_ignore_ascii_case(provider))
            })
            .cloned();

        // Step 2: If not found, try settings.providers
        let settings_cfg = if typed_cfg.is_none() {
            Self::find_model_in_settings(&config.settings, model_id, provider)
        } else {
            None
        };

        drop(config);

        let req = CompletionRequest {
            messages,
            ..Default::default()
        };

        // For LM Studio: proactively load the model before sending the request.
        if backend == openloom_models::ModelBackend::LmStudio {
            let lm_base = typed_cfg
                .as_ref()
                .and_then(|c| c.base_url.clone())
                .unwrap_or_else(|| "http://localhost:1234/v1".to_string());
            let ctx = typed_cfg.as_ref().map(|c| c.context_size).unwrap_or(32000);
            let _ = openloom_inference::ensure_lm_studio_model(&lm_base, model_id, ctx).await;
        }

        // Try typed config first
        if let Some(cfg) = typed_cfg {
            if cfg.backend == openloom_models::ModelBackend::LmStudio {
                let lm_base = cfg
                    .base_url
                    .clone()
                    .unwrap_or_else(|| "http://localhost:1234/v1".to_string());
                let ctx = cfg.context_size;
                let _ = openloom_inference::ensure_lm_studio_model(&lm_base, model_id, ctx).await;
            }
            match openloom_inference::create_cloud_client(&cfg) {
                Ok(client) => {
                    let resp = client.complete(req).await?;
                    return Ok(resp.text);
                }
                Err(e) => {
                    tracing::warn!("create_cloud_client for {model_id}: {e}, falling back");
                }
            }
        }

        // Try settings-based config
        if let Some(cfg) = settings_cfg {
            if cfg.backend == openloom_models::ModelBackend::LmStudio {
                let lm_base = cfg
                    .base_url
                    .clone()
                    .unwrap_or_else(|| "http://localhost:1234/v1".to_string());
                let ctx = cfg.context_size;
                let _ = openloom_inference::ensure_lm_studio_model(&lm_base, model_id, ctx).await;
            }
            match openloom_inference::create_cloud_client(&cfg) {
                Ok(client) => {
                    let resp = client.complete(req).await?;
                    return Ok(resp.text);
                }
                Err(e) => {
                    tracing::warn!(
                        "create_cloud_client from settings for {model_id}: {e}, falling back"
                    );
                }
            }
        }

        // Fallback: try pre-configured clients
        if backend != openloom_models::ModelBackend::LmStudio
            && backend != openloom_models::ModelBackend::Ollama
            && let Some(ref cloud) = self.cloud
        {
            let resp = cloud
                .complete(CompletionRequest {
                    prompt: prompt.to_string(),
                    ..Default::default()
                })
                .await?;
            return Ok(resp.text);
        }
        if let Some(ref local) = self.local_client {
            let resp = local
                .complete(CompletionRequest {
                    prompt: prompt.to_string(),
                    ..Default::default()
                })
                .await?;
            return Ok(resp.text);
        }

        Ok(Self::NO_MODEL_RESPONSE.to_string())
    }

    /// Streaming variant of complete_with_model.
    /// Sends each token as a `StreamDelta` event via event_bus, then fires `StreamEnd`.
    pub async fn complete_with_model_streaming(
        &self,
        session_id: &str,
        prompt: &str,
        images: &[openloom_models::ImagePart],
        model_id: &str,
        provider: &str,
    ) -> Result<()> {
        self.complete_with_model_streaming_meta(
            session_id,
            prompt,
            images,
            None,
            model_id,
            provider,
            openloom_models::Mode::Code,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn complete_with_model_streaming_meta(
        &self,
        session_id: &str,
        prompt: &str,
        images: &[openloom_models::ImagePart],
        metadata: Option<&str>,
        model_id: &str,
        provider: &str,
        mode: openloom_models::Mode,
    ) -> Result<()> {
        // Build message history with system prompt + persona
        let history = self.get_working_memory(session_id).unwrap_or_default();
        let persona_summary = self.persona.summarize().await.unwrap_or_default();
        let base_system = self.system_instruction();
        // Read current permission mode for this session and translate to a directive
        let permission_mode = self.permission_mode(session_id);
        let permission_directive = match permission_mode.as_str() {
            "read_only" => {
                "## Permission Mode: READ-ONLY\n\
You are operating in READ-ONLY mode. You may read, analyze, summarize, and answer questions, \
but you MUST NOT propose any actions that would modify files, run mutating shell commands, \
change configuration, install software, or alter any system state. If the user asks for such \
an action, politely refuse and explain that read-only mode is active. Suggest they switch to \
\"ask\" or \"operate\" mode if mutation is required."
            }
            "ask" => {
                "## Permission Mode: ASK-BEFORE-ACTING\n\
You are operating in ASK mode. Before suggesting any action that would modify files, run \
mutating shell commands, change configuration, install software, or alter system state, \
explicitly state what you intend to do and ask the user to confirm. Do not assume permission \
even if the user seemed to grant it earlier in the conversation — re-confirm for each \
new mutating action."
            }
            _ => {
                "## Permission Mode: AUTO-OPERATE\n\
You are operating in AUTO-OPERATE mode. You may proceed with reasonable actions without \
asking for confirmation each time, but still warn the user about destructive or irreversible \
operations (e.g. force-delete, force-push, dropping databases) before performing them."
            }
        };
        // Read agent identity/ishiki from saved settings
        let agent_system = {
            let cfg = self.config.read().await;
            let settings = &cfg.settings;
            let identity = settings
                .get("identity")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let ishiki = settings
                .get("ishiki")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let mut parts = vec![base_system.clone()];
            parts.push(permission_directive.to_string());
            if !persona_summary.is_empty() {
                parts.push(persona_summary.clone());
            }
            if !identity.is_empty() {
                parts.push(format!("## Identity\n{}", identity));
            }
            if !ishiki.is_empty() {
                parts.push(format!("## Consciousness\n{}", ishiki));
            }
            parts.join("\n\n")
        };

        // Inject skill context into the system prompt so the LLM knows about available tools
        let skill_infos = self.skills.list_all();
        let skill_context: String = skill_infos
            .iter()
            .map(|s| format!("- {}: {}", s.name, s.description))
            .collect::<Vec<_>>()
            .join("\n");
        let agent_system = if skill_context.is_empty() {
            agent_system
        } else {
            format!("{}\n\n## Available Tools\n{}", agent_system, skill_context)
        };

        // ── Determine backend & check vision auxiliary ──
        let (backend, typed_cfg_pre, settings_snapshot) = {
            let config = self.config.read().await;
            let provider_lower = provider.to_lowercase();
            let backend = match provider_lower.as_str() {
                "anthropic" => openloom_models::ModelBackend::Anthropic,
                "openai" => openloom_models::ModelBackend::OpenAI,
                "deepseek" => openloom_models::ModelBackend::DeepSeek,
                "lmstudio" | "lm-studio" => openloom_models::ModelBackend::LmStudio,
                "ollama" => openloom_models::ModelBackend::Ollama,
                _ => openloom_models::ModelBackend::OpenAI,
            };
            let typed_cfg = config
                .models
                .iter()
                .find(|m| {
                    let m_id = m.model.as_deref().unwrap_or(&m.name);
                    m_id == model_id
                        && (m.backend == backend || m.backend.name().eq_ignore_ascii_case(provider))
                })
                .cloned();
            let snapshot = config.settings.clone();
            (backend, typed_cfg, snapshot)
        }; // config lock released

        // ── Auxiliary vision interception ──
        let use_auxiliary = !images.is_empty()
            && vision::should_use_auxiliary_vision(&settings_snapshot, &backend, images);

        let vision_text = if use_auxiliary {
            match vision::prepare_vision_context(&settings_snapshot, prompt, images).await {
                Ok(Some(text)) => {
                    tracing::info!(
                        image_count = images.len(),
                        "vision auxiliary analysis complete"
                    );
                    Some(text)
                }
                Ok(None) => {
                    tracing::info!("vision auxiliary skipped (no model configured)");
                    None
                }
                Err(e) => {
                    tracing::warn!(error = %e, "vision auxiliary model failed, proceeding text-only");
                    Some(format!("[Image analysis unavailable: {}]\n\n", e))
                }
            }
        } else {
            None
        };

        // Re-acquire config for typed_cfg fallback and settings_cfg
        let config = self.config.read().await;
        let typed_cfg = if typed_cfg_pre.is_some() {
            typed_cfg_pre
        } else {
            config
                .models
                .iter()
                .find(|m| {
                    let m_id = m.model.as_deref().unwrap_or(&m.name);
                    m_id == model_id
                        && (m.backend == backend || m.backend.name().eq_ignore_ascii_case(provider))
                })
                .cloned()
        };

        let settings_cfg = if typed_cfg.is_none() {
            Self::find_model_in_settings(&config.settings, model_id, provider)
        } else {
            None
        };

        drop(config);

        let event_bus = self.event_bus().clone();
        let sid = session_id.to_string();

        // ── Build the model client override ──
        let model_client: Option<std::sync::Arc<dyn openloom_inference::CloudClient>> = {
            // For LM Studio: proactively load the model
            if backend == openloom_models::ModelBackend::LmStudio {
                let lm_base = typed_cfg
                    .as_ref()
                    .and_then(|c| c.base_url.clone())
                    .unwrap_or_else(|| "http://localhost:1234/v1".to_string());
                let ctx = typed_cfg.as_ref().map(|c| c.context_size).unwrap_or(32000);
                let _ = openloom_inference::ensure_lm_studio_model(&lm_base, model_id, ctx).await;
            }
            if let Some(ref cfg) = typed_cfg {
                openloom_inference::create_cloud_client(cfg)
                    .ok()
                    .map(std::sync::Arc::from)
            } else if let Some(ref cfg) = settings_cfg {
                if backend == openloom_models::ModelBackend::LmStudio {
                    let lm_base = cfg
                        .base_url
                        .clone()
                        .unwrap_or_else(|| "http://localhost:1234/v1".to_string());
                    let ctx = cfg.context_size;
                    let _ =
                        openloom_inference::ensure_lm_studio_model(&lm_base, model_id, ctx).await;
                }
                openloom_inference::create_cloud_client(cfg)
                    .ok()
                    .map(std::sync::Arc::from)
            } else {
                None
            }
        };

        // ── Build user message (with vision text if available) ──
        let user_content = if let Some(ref vtext) = vision_text {
            format!("{}\n\n{}", vtext, prompt)
        } else {
            prompt.to_string()
        };

        let user_msg = openloom_models::ChatMessage {
            role: "user".into(),
            content: user_content.clone(),
            timestamp: chrono::Utc::now(),
            id: None,
            seq: None,
            metadata: metadata.map(|s| s.to_string()),
        };

        // Feed user message into memory pipeline for cognition extraction
        self.feed_memory_pipeline(&sid, &user_content, "user_message");

        // ── Classify intent: simple queries skip the agent loop ──
        let router_out = self.router.classify_sync(prompt);
        let use_agent_loop = router_out.complexity >= 0.8 || router_out.skill_match.is_some();

        // Feed cognition extraction pipeline (non-blocking)
        let _ = self.memory_tx.send(memory_thread::ProcessRequest {
            session_id: sid.clone(),
            text: prompt.to_string(),
            context: router_out.intent.to_string(),
        });

        if use_agent_loop {
            // ── Complex/skill-match: run the full agent loop with tools ──

            // Create/reset the abort flag for this session
            let abort_flag = self.session_abort_flag(&sid);
            abort_flag.store(false, Ordering::SeqCst);

            let sink = crate::agent_loop::OutputSink::Electron {
                event_bus: event_bus.clone(),
                session_id: sid.clone(),
            };

            let model_pref = openloom_models::ModelPreference::Auto;

            let result = self
                .agent_loop_inner(
                    &user_msg,
                    &sid,
                    sink,
                    Some(agent_system),
                    model_client,
                    mode,
                    model_pref,
                )
                .await;

            // Clean up abort flag
            self.remove_abort_flag(&sid);

            match result {
                Ok(response) => {
                    let _ = event_bus.send(EngineEvent::StreamEnd {
                        session_id: sid.clone(),
                        full_response: response.response.clone(),
                    });
                    if !response.response.is_empty() {
                        self.feed_memory_pipeline(&sid, &response.response, "assistant_response");
                    }
                    Ok(())
                }
                Err(e) => {
                    let _ = event_bus.send(EngineEvent::StreamEnd {
                        session_id: sid.clone(),
                        full_response: format!("[error: {}]", e),
                    });
                    Err(e)
                }
            }
        } else {
            // ── Simple query: fast path without tools (preserves streaming UX) ──

            // Build messages manually for a simple completion (no tools)
            let simple_messages: Vec<Message> = std::iter::once(Message {
                role: Role::System,
                content: vec![openloom_models::ContentPart::Text {
                    text: agent_system.clone(),
                }],
                timestamp: chrono::Utc::now(),
            })
            .chain(history.iter().map(|m| {
                if m.role == "user" {
                    Message::user(&m.content)
                } else {
                    Message::assistant(&m.content)
                }
            }))
            .chain(std::iter::once(if images.is_empty() {
                Message::user(&user_content)
            } else {
                Message::user_with_images(&user_content, images)
            }))
            .collect();

            let req = CompletionRequest {
                messages: simple_messages,
                max_tokens: self.max_output_tokens,
                temperature: 0.0,
                stream: true,
                ..Default::default()
            };

            let (token_tx, mut token_rx) = tokio::sync::mpsc::channel::<String>(256);

            // Spawn token collector: forwards each token as StreamDelta event
            let sid_clone = sid.clone();
            let bus_clone = event_bus.clone();
            let collect_handle = tokio::spawn(async move {
                let mut full = String::new();
                let mut in_thinking = false;
                while let Some(token) = token_rx.recv().await {
                    if token.starts_with('\x00') {
                        continue;
                    }
                    // Reasoning/thinking tokens: emit as ThinkingDelta events
                    if token.starts_with('\x02') {
                        let reasoning = token.strip_prefix('\x02').unwrap_or("");
                        let reasoning =
                            reasoning.strip_prefix("REASONING\x02").unwrap_or(reasoning);
                        if !reasoning.is_empty() {
                            if !in_thinking {
                                in_thinking = true;
                            }
                            let _ = bus_clone.send(EngineEvent::ThinkingDelta {
                                session_id: sid_clone.clone(),
                                delta: reasoning.to_string(),
                            });
                        }
                        continue;
                    }
                    // Regular text token: if we were in thinking mode, close it first
                    if in_thinking {
                        in_thinking = false;
                        let _ = bus_clone.send(EngineEvent::ThinkingEnd {
                            session_id: sid_clone.clone(),
                        });
                    }
                    full.push_str(&token);
                    let _ = bus_clone.send(EngineEvent::StreamDelta {
                        session_id: sid_clone.clone(),
                        delta: token,
                    });
                }
                // Flush any remaining thinking
                if in_thinking {
                    let _ = bus_clone.send(EngineEvent::ThinkingEnd {
                        session_id: sid_clone.clone(),
                    });
                }
                full
            });

            let stream_result = if let Some(ref client) = model_client {
                client.complete_stream(req, token_tx).await
            } else if let Some(ref cloud) = self.cloud {
                cloud
                    .complete_stream(
                        CompletionRequest {
                            prompt: user_content.clone(),
                            stream: true,
                            ..Default::default()
                        },
                        token_tx,
                    )
                    .await
            } else if let Some(ref local) = self.local_client {
                local
                    .complete_stream(
                        CompletionRequest {
                            prompt: user_content.clone(),
                            stream: true,
                            ..Default::default()
                        },
                        token_tx,
                    )
                    .await
            } else {
                Err(anyhow::anyhow!("no model client available"))
            };

            let full_response = collect_handle.await.unwrap_or_default();

            // Save messages
            let _ = self.save_messages(&sid, &user_msg, &full_response);

            // Feed assistant response into memory pipeline
            if !full_response.is_empty() {
                self.feed_memory_pipeline(&sid, &full_response, "assistant_response");
            }

            if let Err(e) = stream_result {
                let _ = event_bus.send(EngineEvent::StreamEnd {
                    session_id: sid.clone(),
                    full_response: format!("[error: {}]", e),
                });
                return Err(e);
            }

            let _ = event_bus.send(EngineEvent::StreamEnd {
                session_id: sid.clone(),
                full_response,
            });

            Ok(())
        }
    }

    /// Find a model in settings.providers and construct a ModelConfig from it.
    pub(crate) fn find_model_in_settings(
        settings: &serde_json::Value,
        model_id: &str,
        provider: &str,
    ) -> Option<openloom_models::ModelConfig> {
        let providers = settings
            .get("providers")
            .or_else(|| settings.get("general").and_then(|g| g.get("providers")))
            .and_then(|p| p.as_object())?;

        let provider_lower = provider.to_lowercase();
        let (prov_key, prov_val) = providers.iter().find(|(k, _)| {
            k.eq_ignore_ascii_case(&provider_lower) || k.eq_ignore_ascii_case(provider)
        })?;

        let models = prov_val.get("models").and_then(|m| m.as_array())?;
        let _model_entry = models.iter().find(|m| {
            if m.is_string() {
                m.as_str() == Some(model_id)
            } else if m.is_object() {
                m.get("id").and_then(|v| v.as_str()) == Some(model_id)
            } else {
                false
            }
        })?;

        // Construct a ModelConfig from the provider info
        let api_key_env = prov_val
            .get("api_key_env")
            .and_then(|v| v.as_str())
            .map(String::from);
        let raw_base_url = prov_val
            .get("base_url")
            .and_then(|v| v.as_str())
            .map(String::from);

        let backend = match provider_lower.as_str() {
            "anthropic" => openloom_models::ModelBackend::Anthropic,
            "openai" => openloom_models::ModelBackend::OpenAI,
            "deepseek" => openloom_models::ModelBackend::DeepSeek,
            "lmstudio" | "lm-studio" => openloom_models::ModelBackend::LmStudio,
            "ollama" => openloom_models::ModelBackend::Ollama,
            _ => openloom_models::ModelBackend::OpenAI,
        };

        // Normalize base_url: LM Studio and compatible local servers need /v1 suffix.
        // If the user stored "http://localhost:1234" without /v1, append it.
        let base_url = raw_base_url.map(|url| {
            let trimmed = url.trim_end_matches('/');
            if (backend == openloom_models::ModelBackend::LmStudio
                || backend == openloom_models::ModelBackend::Ollama
                || backend == openloom_models::ModelBackend::OpenAI)
                && !trimmed.ends_with("/v1")
                && !trimmed.is_empty()
            {
                format!("{}/v1", trimmed)
            } else {
                trimmed.to_string()
            }
        });

        Some(openloom_models::ModelConfig {
            name: prov_key.clone(),
            model: Some(model_id.to_string()),
            model_type: openloom_models::ModelType::Router,
            backend,
            path: None,
            context_size: 128000,
            max_output_tokens: None,
            n_gpu_layers: 0,
            api_key_env,
            base_url,
        })
    }

    // === Public API ===

    pub fn event_bus(&self) -> &broadcast::Sender<EngineEvent> {
        &self.event_bus
    }

    /// Streaming version of complete_with_model: sends tokens to the provided channel.
    pub async fn stream_with_model(
        &self,
        session_id: &str,
        prompt: &str,
        model_id: &str,
        provider: &str,
        token_tx: tokio::sync::mpsc::Sender<String>,
    ) -> anyhow::Result<()> {
        // Try to find and create a streaming client
        let config = self.config.read().await;
        let provider_lower = provider.to_lowercase();
        let backend = match provider_lower.as_str() {
            "anthropic" => openloom_models::ModelBackend::Anthropic,
            "openai" => openloom_models::ModelBackend::OpenAI,
            "deepseek" => openloom_models::ModelBackend::DeepSeek,
            "lmstudio" | "lm-studio" => openloom_models::ModelBackend::LmStudio,
            "ollama" => openloom_models::ModelBackend::Ollama,
            _ => openloom_models::ModelBackend::OpenAI,
        };

        // Step 1: Try typed config.models
        let typed_cfg = config
            .models
            .iter()
            .find(|m| {
                let m_id = m.model.as_deref().unwrap_or(&m.name);
                m_id == model_id
                    && (m.backend == backend || m.backend.name().eq_ignore_ascii_case(provider))
            })
            .cloned();

        // Step 2: Try settings.providers
        let settings_cfg = if typed_cfg.is_none() {
            Self::find_model_in_settings(&config.settings, model_id, provider)
        } else {
            None
        };

        drop(config);

        let history = self.get_working_memory(session_id).unwrap_or_default();
        let messages: Vec<Message> = history
            .iter()
            .map(|m| {
                if m.role == "user" {
                    Message::user(&m.content)
                } else {
                    Message::assistant(&m.content)
                }
            })
            .chain(std::iter::once(Message::user(prompt)))
            .collect();

        let req = CompletionRequest {
            messages,
            stream: true,
            ..Default::default()
        };

        if let Some(cfg) = typed_cfg.or(settings_cfg) {
            match openloom_inference::create_cloud_client(&cfg) {
                Ok(client) => {
                    return client.complete_stream(req, token_tx).await;
                }
                Err(e) => {
                    tracing::warn!("stream create_cloud_client for {model_id}: {e}, falling back");
                }
            }
        }

        // Fallback: non-streaming complete, send as single chunk
        let response = self
            .complete_with_model(session_id, prompt, model_id, provider)
            .await?;
        let _ = token_tx.send(response).await;
        Ok(())
    }

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

    pub async fn archive_session(&self, session_id: &str) -> Result<bool> {
        let (tx, rx) = oneshot::channel();
        self.session_tx
            .send(SessionCommand::Archive {
                id: session_id.to_string(),
                reply: tx,
            })
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        rx.await.map_err(|e| anyhow::anyhow!("{}", e))
    }

    pub async fn delete_session(&self, session_id: &str) -> Result<bool> {
        let (tx, rx) = oneshot::channel();
        self.session_tx
            .send(SessionCommand::Delete {
                id: session_id.to_string(),
                reply: tx,
            })
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        rx.await.map_err(|e| anyhow::anyhow!("{}", e))
    }

    pub fn delete_message(&self, session_id: &str, msg_id: i64) -> Result<bool> {
        let conn = rusqlite::Connection::open(&self.db_path)?;
        let store = MessageStore::new(&conn);
        store.delete_by_id(session_id, msg_id)
    }

    pub async fn list_archived_sessions(&self) -> Result<Vec<SessionInfo>> {
        let (tx, rx) = oneshot::channel();
        self.session_tx
            .send(SessionCommand::ListArchived { reply: tx })
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        rx.await.map_err(|e| anyhow::anyhow!("{}", e))
    }

    pub async fn restore_session(&self, session_id: &str) -> Result<bool> {
        let (tx, rx) = oneshot::channel();
        self.session_tx
            .send(SessionCommand::Restore {
                id: session_id.to_string(),
                reply: tx,
            })
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        rx.await.map_err(|e| anyhow::anyhow!("{}", e))
    }

    pub async fn delete_archived_session(&self, session_id: &str) -> Result<bool> {
        let (tx, rx) = oneshot::channel();
        self.session_tx
            .send(SessionCommand::DeleteArchived {
                id: session_id.to_string(),
                reply: tx,
            })
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        rx.await.map_err(|e| anyhow::anyhow!("{}", e))
    }

    pub async fn cleanup_archived_sessions(&self, max_age_days: u32) -> Result<usize> {
        let (tx, rx) = oneshot::channel();
        self.session_tx
            .send(SessionCommand::Cleanup {
                max_age_days,
                reply: tx,
            })
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        rx.await.map_err(|e| anyhow::anyhow!("{}", e))
    }

    pub async fn rename_session(&self, session_id: &str, title: &str) -> Result<bool> {
        let (tx, rx) = oneshot::channel();
        self.session_tx
            .send(SessionCommand::Rename {
                id: session_id.to_string(),
                title: title.to_string(),
                reply: tx,
            })
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        rx.await.map_err(|e| anyhow::anyhow!("{}", e))
    }

    pub async fn pin_session(&self, session_id: &str, pinned: bool) -> Result<bool> {
        let (tx, rx) = oneshot::channel();
        let pinned_at = if pinned {
            Some(chrono::Utc::now().to_rfc3339())
        } else {
            None
        };
        self.session_tx
            .send(SessionCommand::Pin {
                id: session_id.to_string(),
                pinned_at,
                reply: tx,
            })
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        rx.await.map_err(|e| anyhow::anyhow!("{}", e))
    }

    pub async fn list_cognitions(
        &self,
        subject: &str,
        scope: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<openloom_memory::store::CognitionRow>> {
        let conn = rusqlite::Connection::open(&self.db_path)?;
        let store = openloom_memory::store::CognitionStore::new(&conn);
        store.query_by_subject_and_scope(subject, scope, limit, offset)
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

    pub async fn count_cognitions(&self, subject: &str, scope: Option<&str>) -> Result<usize> {
        let conn = rusqlite::Connection::open(&self.db_path)?;
        let store = openloom_memory::store::CognitionStore::new(&conn);
        store.count_by_subject_and_scope(subject, scope)
    }

    pub async fn delete_cognition(&self, cognition_id: i64) -> Result<()> {
        let conn = rusqlite::Connection::open(&self.db_path)?;
        let store = openloom_memory::store::CognitionStore::new(&conn);
        store.delete(cognition_id)
    }

    pub async fn persona_summary(&self) -> String {
        self.persona.summarize().await.unwrap_or_default()
    }

    /// Feed conversation text into the memory pipeline for automatic extraction.
    pub fn feed_memory_pipeline(&self, session_id: &str, text: &str, context: &str) {
        let _ = self.memory_tx.send(memory_thread::ProcessRequest {
            session_id: session_id.to_string(),
            text: text.to_string(),
            context: context.to_string(),
        });
    }

    /// Use the configured utility model to extract cognitions, scoped to a session.
    /// Reads the user's 小工具模型 (settings.models.models.utility) from config.
    /// Returns the number of cognitions extracted.
    pub async fn extract_cognitions_with_local_model(
        &self,
        text: &str,
        scope: &str,
    ) -> Result<usize> {
        let config = self.config.read().await;
        // Read the user's configured 小工具模型: settings.models.models.utility
        let utility = config
            .settings
            .get("models")
            .and_then(|m| m.get("models"))
            .and_then(|m| m.get("utility"));
        let utility_id = utility.and_then(|u| u.get("id")).and_then(|v| v.as_str());
        let utility_provider = utility
            .and_then(|u| u.get("provider"))
            .and_then(|v| v.as_str());

        let local_model = utility_id.and_then(|id| {
            Self::find_model_in_settings(
                &config.settings,
                id,
                utility_provider.unwrap_or("lmstudio"),
            )
        });
        drop(config);

        let model_cfg = match local_model {
            Some(cfg) => {
                tracing::info!(backend = ?cfg.backend, base_url = ?cfg.base_url, "using local model for cognition extraction");
                cfg
            }
            None => {
                tracing::info!(
                    "no local model configured, using rule-based pipeline for manual record"
                );
                self.feed_memory_pipeline(scope, text, "manual_record");
                return Ok(0);
            }
        };

        let model_id = model_cfg.model.as_deref().unwrap_or(&model_cfg.name);

        // Auto-load the model in LM Studio before sending the request
        if model_cfg.backend == openloom_models::ModelBackend::LmStudio {
            let base_url = model_cfg
                .base_url
                .as_deref()
                .unwrap_or("http://localhost:1234/v1");
            let _ = openloom_inference::ensure_lm_studio_model(
                base_url,
                model_id,
                model_cfg.context_size,
            )
            .await;
        }

        let prompt = format!(
            "Extract key facts about the user from this text. For each fact, output one line:\n\
            trait: value (confidence: 0.0 to 1.0)\n\n\
            Example:\n\
            profession: backend engineer (confidence: 0.95)\n\
            preference: likes Rust (confidence: 0.8)\n\
            tech_stack: Rust, Python (confidence: 0.9)\n\n\
            Text:\n{text}\n\n\
            Output one line per fact. If nothing to extract, output NONE."
        );

        let messages = vec![Message::user(&prompt)];
        let req = CompletionRequest {
            messages,
            ..Default::default()
        };

        let resp_text = match openloom_inference::create_cloud_client(&model_cfg) {
            Ok(client) => match client.complete(req).await {
                Ok(resp) => {
                    tracing::info!(
                        len = resp.text.len(),
                        "local model returned response for cognition extraction"
                    );
                    resp.text
                }
                Err(e) => {
                    tracing::warn!(error = %e, "local model call failed, falling back to rule-based");
                    self.feed_memory_pipeline(scope, text, "manual_record");
                    return Ok(0);
                }
            },
            Err(e) => {
                tracing::warn!(error = %e, "failed to create local client, falling back to rule-based");
                self.feed_memory_pipeline(scope, text, "manual_record");
                return Ok(0);
            }
        };

        tracing::info!(
            lines = resp_text.lines().count(),
            scope,
            "parsing cognition extraction response"
        );

        let mut count = 0;
        let conn = rusqlite::Connection::open(&self.db_path)?;
        let store = openloom_memory::store::CognitionStore::new(&conn);

        for line in resp_text.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed == "NONE" {
                continue;
            }

            // Parse format: trait: value (confidence: 0.XX)
            if let Some((trait_name, rest)) = trimmed.split_once(':') {
                let trait_name = trait_name.trim().to_lowercase().replace(' ', "_");
                if trait_name.is_empty() {
                    continue;
                }

                let (value, confidence) = if let Some(conf_start) = rest.find("(confidence:") {
                    let value = rest[..conf_start].trim();
                    let conf_part = &rest[conf_start + "(confidence:".len()..];
                    let conf_str = conf_part.trim_end_matches(')').trim();
                    let confidence = conf_str.parse::<f64>().unwrap_or(0.7);
                    (value.to_string(), confidence)
                } else {
                    (rest.trim().to_string(), 0.7)
                };

                if !value.is_empty() {
                    if let Err(e) = store.insert("USER", &trait_name, &value, confidence, 1, scope)
                    {
                        tracing::warn!(error = %e, trait_name, scope, "failed to insert cognition");
                    } else {
                        count += 1;
                    }
                }
                continue;
            }

            // Fallback: JSON
            if trimmed.starts_with('{')
                && let Ok(parsed) = serde_json::from_str::<serde_json::Value>(trimmed)
            {
                let trait_name = parsed
                    .get("trait")
                    .and_then(|v| v.as_str())
                    .unwrap_or("general");
                let value = parsed
                    .get("value")
                    .and_then(|v| v.as_str())
                    .unwrap_or(trimmed);
                let confidence = parsed
                    .get("confidence")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.7);
                if let Err(e) = store.insert("USER", trait_name, value, confidence, 1, scope) {
                    tracing::warn!(error = %e, trait_name, scope, "failed to insert cognition");
                } else {
                    count += 1;
                }
            }
        }

        // Also feed text through pipeline for event-level recording
        self.feed_memory_pipeline(scope, text, "manual_record");

        Ok(count)
    }

    /// Auto-extract cognitions from a session's conversation history using the local model.
    pub async fn extract_cognitions_from_session(&self, session_id: &str) -> Result<usize> {
        let history = self.get_working_memory(session_id).unwrap_or_default();
        if history.is_empty() {
            anyhow::bail!("session has no messages");
        }

        // Build a conversation transcript
        let transcript: String = history
            .iter()
            .map(|m| format!("[{}]: {}", m.role, m.content))
            .collect::<Vec<_>>()
            .join("\n");

        // Reuse the same extraction flow, scoped to this session
        self.extract_cognitions_with_local_model(&transcript, session_id)
            .await
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

    pub fn list_all_skills(&self) -> Vec<openloom_skills::SkillInfo> {
        self.skills.list_all_skills()
    }

    pub async fn invoke_skill(
        &self,
        name: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        self.skills.invoke(name, params).await
    }

    // ── User skill management (filesystem-based) ──────────────────────

    /// List all user skills (from skills dir, learned dirs, external paths) with
    /// per-agent enabled state from config.
    pub async fn list_user_skills(&self, agent_id: &str) -> serde_json::Value {
        let mut skills: Vec<serde_json::Value> = Vec::new();
        let mut seen = std::collections::HashSet::new();

        // Get enabled skill names for this agent from config
        let enabled_key = format!("settings.agent.{}.skills.enabled", agent_id);
        let enabled_val = self.get_config(Some(&enabled_key)).await;
        let enabled_set: std::collections::HashSet<String> = enabled_val
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        // 1. Scan skills dir (~/.loom/skills/)
        let skills_dir = self.data_dir.join("skills");
        for skill in crate::skill_fs::scan_skills_dir(&skills_dir, "user") {
            if seen.insert(skill.name.clone()) {
                let enabled = enabled_set.contains(&skill.name);
                skills.push(serde_json::json!({
                    "name": skill.name,
                    "description": skill.description,
                    "filePath": skill.file_path,
                    "baseDir": skill.base_dir,
                    "source": skill.source,
                    "hidden": false,
                    "enabled": enabled,
                    "externalLabel": null,
                    "externalPath": null,
                    "readonly": false,
                }));
            }
        }

        // 2. Scan learned skills (from agent dir)
        let agent_dir = self.data_dir.join("agents").join(agent_id);
        let learned_dir = agent_dir.join("learned-skills");
        for skill in crate::skill_fs::scan_learned_skills_dir(&learned_dir, agent_id) {
            if seen.insert(skill.name.clone()) {
                let enabled = enabled_set.contains(&skill.name);
                skills.push(serde_json::json!({
                    "name": skill.name,
                    "description": skill.description,
                    "filePath": skill.file_path,
                    "baseDir": skill.base_dir,
                    "source": "learned",
                    "hidden": false,
                    "enabled": enabled,
                    "externalLabel": null,
                    "externalPath": null,
                    "readonly": false,
                }));
            }
        }

        // 3. Scan external paths
        let ext_paths = self.get_external_skill_paths().await;
        for skill in crate::skill_fs::scan_external_paths(&ext_paths) {
            if seen.insert(skill.name.clone()) {
                let enabled = enabled_set.contains(&skill.name);
                let readonly = skill
                    .external_label
                    .as_deref()
                    .map(|l| l.starts_with("plugin:"))
                    .unwrap_or(false);
                skills.push(serde_json::json!({
                    "name": skill.name,
                    "description": skill.description,
                    "filePath": skill.file_path,
                    "baseDir": skill.base_dir,
                    "source": "external",
                    "hidden": false,
                    "enabled": enabled,
                    "externalLabel": skill.external_label,
                    "externalPath": skill.external_path,
                    "readonly": readonly,
                }));
            }
        }

        serde_json::json!({ "skills": skills })
    }

    /// Get external skill paths (configured + discovered paths that exist).
    pub async fn get_external_skill_paths(&self) -> Vec<(String, String)> {
        let mut paths: Vec<(String, String)> = Vec::new();

        // 1. Configured custom paths from config
        let config = self
            .get_config(Some("settings.skills.external_paths"))
            .await;
        let configured: Vec<String> = config
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        for p in &configured {
            paths.push((p.clone(), format!("custom:{}", p)));
        }

        // 2. Discovered paths from known tools (Claude Code, Codex, etc.)
        let home = dirs::home_dir().unwrap_or_default();
        for discovered in crate::skill_fs::discover_external_paths(&home) {
            if discovered.exists {
                // Skip if already covered by a configured path
                let already_configured = configured.iter().any(|c| c == &discovered.dir_path);
                if !already_configured {
                    paths.push((discovered.dir_path.clone(), discovered.label.clone()));
                }
            }
        }

        paths
    }

    /// Get external paths info (configured paths + discovered tool paths).
    pub async fn get_external_paths_info(&self) -> serde_json::Value {
        // Read configured paths from config
        let config = self
            .get_config(Some("settings.skills.external_paths"))
            .await;
        let configured: Vec<String> = config
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        let home = dirs::home_dir().unwrap_or_default();
        let discovered: Vec<serde_json::Value> = crate::skill_fs::discover_external_paths(&home)
            .iter()
            .map(|d| {
                serde_json::json!({
                    "dirPath": d.dir_path,
                    "label": d.label,
                    "exists": d.exists,
                })
            })
            .collect();

        serde_json::json!({
            "configured": configured,
            "discovered": discovered,
        })
    }

    /// Set external skill paths.
    pub async fn set_external_skill_paths(&self, paths: Vec<String>) -> Result<()> {
        let value = serde_json::json!(paths);
        self.set_config("settings.skills.external_paths", value)
            .await
    }

    /// Install a skill from a file path.
    pub fn install_user_skill(&self, source_path: &str) -> Result<serde_json::Value> {
        let source = std::path::Path::new(source_path);
        let skills_dir = self.data_dir.join("skills");
        std::fs::create_dir_all(&skills_dir)?;
        let skill = crate::skill_fs::install_skill(source, &skills_dir)?;
        Ok(serde_json::json!({
            "skill": {
                "name": skill.name,
                "description": skill.description,
                "filePath": skill.file_path,
                "baseDir": skill.base_dir,
            }
        }))
    }

    /// Delete a user skill by name.
    pub fn delete_user_skill(&self, name: &str) -> Result<bool> {
        let skills_dir = self.data_dir.join("skills");
        crate::skill_fs::delete_skill(&skills_dir, name)
    }

    /// Toggle a skill's enabled state for an agent.
    pub async fn toggle_user_skill(&self, agent_id: &str, name: &str, enabled: bool) -> Result<()> {
        let key = format!("settings.agent.{}.skills.enabled", agent_id);
        let current = self.get_config(Some(&key)).await;
        let mut names: Vec<String> = current
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        if enabled {
            if !names.contains(&name.to_string()) {
                names.push(name.to_string());
            }
        } else {
            names.retain(|n| n != name);
        }

        self.set_config(&key, serde_json::json!(names)).await
    }

    // ── Bundle management ─────────────────────────────────────────────

    /// List all skill bundles with per-agent enabled state.
    pub fn list_skill_bundles(&self, _agent_id: &str) -> serde_json::Value {
        let bundles: Vec<serde_json::Value> = crate::skill_bundles::list_bundles(&self.data_dir)
            .iter()
            .map(|b| {
                serde_json::json!({
                    "id": b.id,
                    "name": b.name,
                    "skillNames": b.skill_names,
                    "source": b.source,
                    "agentId": b.agent_id,
                    "sourcePackage": b.source_package,
                    "createdAt": b.created_at,
                    "updatedAt": b.updated_at,
                })
            })
            .collect();
        serde_json::json!({ "bundles": bundles })
    }

    /// Create a new skill bundle.
    pub fn create_skill_bundle(
        &self,
        name: &str,
        skill_names: Vec<String>,
    ) -> Result<serde_json::Value> {
        let bundle = crate::skill_bundles::create_bundle(&self.data_dir, name, skill_names)?;
        Ok(serde_json::json!({
            "bundle": {
                "id": bundle.id,
                "name": bundle.name,
                "skillNames": bundle.skill_names,
            }
        }))
    }

    /// Update a skill bundle.
    pub fn update_skill_bundle(
        &self,
        id: &str,
        name: Option<&str>,
        skill_names: Option<Vec<String>>,
    ) -> Result<serde_json::Value> {
        let bundle = crate::skill_bundles::update_bundle(&self.data_dir, id, name, skill_names)?;
        Ok(serde_json::json!({
            "bundle": {
                "id": bundle.id,
                "name": bundle.name,
                "skillNames": bundle.skill_names,
            }
        }))
    }

    /// Delete a skill bundle.
    pub fn delete_skill_bundle(&self, id: &str) -> Result<bool> {
        crate::skill_bundles::delete_bundle(&self.data_dir, id)
    }

    /// Reorder skill bundles.
    pub fn reorder_skill_bundles(&self, bundle_ids: &[String]) -> Result<serde_json::Value> {
        let bundles = crate::skill_bundles::reorder_bundles(&self.data_dir, bundle_ids)?;
        let result: Vec<serde_json::Value> = bundles
            .iter()
            .map(|b| {
                serde_json::json!({
                    "id": b.id,
                    "name": b.name,
                    "skillNames": b.skill_names,
                })
            })
            .collect();
        Ok(serde_json::json!({ "bundles": result }))
    }

    /// Toggle a bundle's enabled state for an agent.
    pub fn toggle_skill_bundle(&self, id: &str, agent_id: &str, enabled: bool) -> Result<()> {
        crate::skill_bundles::set_bundle_enabled(&self.data_dir, id, agent_id, enabled)
    }

    /// Export a skill bundle as zip.
    pub fn export_skill_bundle(&self, id: &str) -> Result<serde_json::Value> {
        let skills_dir = self.data_dir.join("skills");
        let result = crate::skill_bundles::export_bundle(&self.data_dir, &skills_dir, id)?;
        Ok(serde_json::json!({
            "filePath": result.file_path,
            "fileName": result.file_name,
            "bundle": {
                "id": result.bundle.id,
                "name": result.bundle.name,
                "skillCount": result.bundle.skill_count,
            },
            "warnings": result.warnings.iter().map(|w| serde_json::json!({
                "type": w.r#type,
                "name": w.name,
            })).collect::<Vec<_>>(),
        }))
    }

    // ── Plugin management ─────────────────────────────────────────────

    /// List all installed plugins.
    pub fn list_plugins(&self) -> serde_json::Value {
        let plugins_dir = self.data_dir.join("plugins");
        let plugins = crate::plugin_fs::scan_plugins_dir(&plugins_dir);

        let plugins_json: Vec<serde_json::Value> = plugins
            .iter()
            .map(|p| {
                serde_json::json!({
                    "id": p.id,
                    "name": p.name,
                    "version": p.version,
                    "description": p.description,
                    "source": p.source,
                    "status": p.status,
                    "trust": p.trust,
                    "hidden": p.hidden,
                    "pluginDir": p.plugin_dir,
                    "manifestFormat": p.manifest_format,
                    "contributions": p.contributions,
                    "configSchema": p.config_schema,
                    "uiHostCapabilities": p.ui_host_capabilities,
                    "author": p.author,
                    "repository": p.repository,
                    "category": p.category,
                })
            })
            .collect();

        serde_json::json!({ "plugins": plugins_json })
    }

    /// Get plugin config for a specific plugin.
    pub async fn get_plugin_config(&self, plugin_id: &str) -> serde_json::Value {
        let key = format!("settings.plugins.{}.config", plugin_id);
        self.get_config(Some(&key)).await
    }

    /// Set plugin config for a specific plugin.
    pub async fn set_plugin_config(
        &self,
        plugin_id: &str,
        config: serde_json::Value,
    ) -> Result<()> {
        let key = format!("settings.plugins.{}.config", plugin_id);
        self.set_config(&key, config).await
    }

    /// Get plugin diagnostics.
    pub fn get_plugin_diagnostics(&self) -> serde_json::Value {
        let plugins_dir = self.data_dir.join("plugins");
        let plugins = crate::plugin_fs::scan_plugins_dir(&plugins_dir);

        let diagnostics: Vec<serde_json::Value> = plugins
            .iter()
            .map(|p| {
                let skills_dir = p.plugin_dir.clone() + "/skills";
                let tools_dir = p.plugin_dir.clone() + "/tools";
                let commands_dir = p.plugin_dir.clone() + "/commands";

                let list_files = |dir: &str| -> Vec<String> {
                    let path = Path::new(dir);
                    if !path.exists() {
                        return vec![];
                    }
                    std::fs::read_dir(path)
                        .map(|entries| {
                            entries
                                .flatten()
                                .filter_map(|e| {
                                    let name = e.file_name().to_string_lossy().to_string();
                                    if name.starts_with('.') {
                                        None
                                    } else {
                                        Some(name)
                                    }
                                })
                                .collect()
                        })
                        .unwrap_or_default()
                };

                serde_json::json!({
                    "id": p.id,
                    "name": p.name,
                    "version": p.version,
                    "status": p.status,
                    "pluginDir": p.plugin_dir,
                    "tools": list_files(&tools_dir),
                    "commands": list_files(&commands_dir),
                    "skills": list_files(&skills_dir),
                    "configKeys": Vec::<String>::new(),
                })
            })
            .collect();

        serde_json::json!({ "diagnostics": diagnostics })
    }

    /// Get plugin settings (global).
    pub async fn get_plugin_settings(&self) -> serde_json::Value {
        let config = self.get_config(Some("settings.plugins")).await;
        let allow_full = config
            .get("allow_full_access")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let dev_tools = config
            .get("plugin_dev_tools_enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let plugins_dir = config
            .get("plugins_dir")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        serde_json::json!({
            "allow_full_access": allow_full,
            "plugin_dev_tools_enabled": dev_tools,
            "plugins_dir": plugins_dir,
        })
    }

    /// Update plugin settings (global).
    pub async fn set_plugin_settings(&self, settings: serde_json::Value) -> Result<()> {
        // Merge with existing
        let mut existing = self.get_config(Some("settings.plugins")).await;
        if let Some(obj) = existing.as_object_mut() {
            if let Some(new_obj) = settings.as_object() {
                for (k, v) in new_obj {
                    obj.insert(k.clone(), v.clone());
                }
            }
        } else {
            existing = settings;
        }
        self.set_config("settings.plugins", existing).await
    }

    /// Install a plugin from a source path.
    pub fn install_plugin(&self, source_path: &str) -> Result<serde_json::Value> {
        let source = Path::new(source_path);
        let plugins_dir = self.data_dir.join("plugins");
        std::fs::create_dir_all(&plugins_dir)?;
        let plugin = crate::plugin_fs::install_plugin(source, &plugins_dir)?;
        Ok(serde_json::json!({
            "plugin": {
                "id": plugin.id,
                "name": plugin.name,
                "version": plugin.version,
                "status": plugin.status,
            }
        }))
    }

    /// Remove a plugin by ID.
    pub fn remove_plugin(&self, id: &str) -> Result<bool> {
        let plugins_dir = self.data_dir.join("plugins");
        crate::plugin_fs::remove_plugin(&plugins_dir, id)
    }

    /// Enable or disable a plugin.
    pub async fn set_plugin_enabled(&self, id: &str, enabled: bool) -> Result<()> {
        let key = "settings.plugins.disabled";
        let current = self.get_config(Some(key)).await;
        let mut disabled: Vec<String> = current
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        if enabled {
            disabled.retain(|d| d != id);
        } else if !disabled.contains(&id.to_string()) {
            disabled.push(id.to_string());
        }

        self.set_config(key, serde_json::json!(disabled)).await
    }

    // ── Plugin marketplace ──────────────────────────────────────────────

    /// Get marketplace sources from config.
    pub async fn get_marketplace_sources(&self) -> Vec<crate::plugin_fs::MarketplaceSource> {
        let config = self
            .get_config(Some("settings.plugins.marketplace_sources"))
            .await;
        let sources: Vec<serde_json::Value> = config.as_array().cloned().unwrap_or_default();

        if sources.is_empty() {
            // Default: check if marketplace dir has content
            let mp_dir = crate::plugin_fs::marketplace_dir(&self.data_dir);
            if mp_dir.exists() {
                let has_content = std::fs::read_dir(&mp_dir)
                    .map(|mut entries| {
                        entries.any(|e| {
                            e.ok().is_some_and(|e| {
                                e.path().is_dir()
                                    && !e.file_name().to_string_lossy().starts_with('.')
                            })
                        })
                    })
                    .unwrap_or(false);
                if has_content {
                    return vec![crate::plugin_fs::MarketplaceSource {
                        kind: "local".to_string(),
                        url: None,
                        path: Some(mp_dir.to_string_lossy().to_string()),
                        configured: true,
                        name: "Local Marketplace".to_string(),
                    }];
                }
            }
            return vec![];
        }

        sources
            .iter()
            .map(|s| crate::plugin_fs::MarketplaceSource {
                kind: s
                    .get("kind")
                    .and_then(|v| v.as_str())
                    .unwrap_or("local")
                    .to_string(),
                url: s.get("url").and_then(|v| v.as_str()).map(String::from),
                path: s.get("path").and_then(|v| v.as_str()).map(String::from),
                configured: true,
                name: s
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Marketplace")
                    .to_string(),
            })
            .collect()
    }

    /// Add a marketplace source (Git URL or local path). Clones Git repos.
    pub async fn add_marketplace_source(&self, url: &str, name: &str) -> Result<serde_json::Value> {
        let mp_dir = crate::plugin_fs::marketplace_dir(&self.data_dir);
        std::fs::create_dir_all(&mp_dir)?;

        let source_dir =
            if url.starts_with("http://") || url.starts_with("https://") || url.starts_with("git@")
            {
                // Git clone
                let target = mp_dir.join(slugify_str(name));
                if target.exists() {
                    // git pull
                    let output = std::process::Command::new("git")
                        .args(["-C", &target.to_string_lossy(), "pull", "--ff-only"])
                        .output();
                    if let Err(e) = output {
                        tracing::warn!(error = %e, "git pull failed for marketplace source");
                    }
                } else {
                    let output = std::process::Command::new("git")
                        .args(["clone", url, &target.to_string_lossy()])
                        .output()
                        .map_err(|e| anyhow::anyhow!("git clone failed: {}", e))?;
                    if !output.status.success() {
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        anyhow::bail!("git clone failed: {}", stderr);
                    }
                }
                target
            } else {
                // Local path
                let p = std::path::Path::new(url);
                if !p.exists() {
                    anyhow::bail!("local path does not exist: {}", url);
                }
                p.to_path_buf()
            };

        // Save source to config
        let mut existing: Vec<serde_json::Value> = self
            .get_config(Some("settings.plugins.marketplace_sources"))
            .await
            .as_array()
            .cloned()
            .unwrap_or_default();

        // Remove existing entry with same name
        existing.retain(|s| s.get("name").and_then(|v| v.as_str()) != Some(name));

        existing.push(serde_json::json!({
            "kind": if url.starts_with("http") || url.starts_with("git@") { "git" } else { "local" },
            "name": name,
            "url": if url.starts_with("http") || url.starts_with("git@") { serde_json::json!(url) } else { serde_json::Value::Null },
            "path": source_dir.to_string_lossy().to_string(),
        }));

        self.set_config(
            "settings.plugins.marketplace_sources",
            serde_json::json!(existing),
        )
        .await?;

        Ok(serde_json::json!({
            "name": name,
            "path": source_dir.to_string_lossy().to_string(),
        }))
    }

    /// Remove a marketplace source by name.
    pub async fn remove_marketplace_source(&self, name: &str) -> Result<()> {
        let mut existing: Vec<serde_json::Value> = self
            .get_config(Some("settings.plugins.marketplace_sources"))
            .await
            .as_array()
            .cloned()
            .unwrap_or_default();

        existing.retain(|s| s.get("name").and_then(|v| v.as_str()) != Some(name));
        self.set_config(
            "settings.plugins.marketplace_sources",
            serde_json::json!(existing),
        )
        .await?;

        // Remove cloned directory
        let mp_dir = crate::plugin_fs::marketplace_dir(&self.data_dir);
        let target = mp_dir.join(slugify_str(name));
        if target.exists() {
            let _ = std::fs::remove_dir_all(&target);
        }
        Ok(())
    }

    /// Set/replace all marketplace sources.
    pub async fn set_marketplace_sources(&self, sources: Vec<serde_json::Value>) -> Result<()> {
        self.set_config(
            "settings.plugins.marketplace_sources",
            serde_json::json!(sources),
        )
        .await
    }

    /// Refresh marketplace sources (git pull all).
    pub async fn refresh_marketplace(&self) -> Result<()> {
        let sources = self.get_marketplace_sources().await;
        for source in &sources {
            if source.kind == "git"
                && let Some(ref path_str) = source.path
            {
                let target = std::path::Path::new(path_str);
                if target.join(".git").exists() {
                    let output = std::process::Command::new("git")
                        .args(["-C", path_str, "pull", "--ff-only"])
                        .output();
                    match output {
                        Ok(o) if o.status.success() => {
                            tracing::info!(source = %source.name, "marketplace source refreshed");
                        }
                        Ok(o) => {
                            tracing::warn!(source = %source.name, stderr = %String::from_utf8_lossy(&o.stderr), "git pull failed");
                        }
                        Err(e) => {
                            tracing::warn!(source = %source.name, error = %e, "git pull error");
                        }
                    }
                }
            }
        }
        Ok(())
    }

    // ── Computer Use ───────────────────────────────────────────────────

    /// Get computer use status and settings.
    pub async fn get_computer_use_status(&self) -> serde_json::Value {
        let settings = self.get_config(Some("settings.computer-use")).await;
        let enabled = settings
            .get("enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let status = crate::computer_use::check_status();
        let available = status
            .get("available")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let reason = status
            .get("reason")
            .and_then(|v| v.as_str())
            .map(String::from);
        let error = status
            .get("error")
            .and_then(|v| v.as_str())
            .map(String::from);
        let permissions = status
            .get("permissions")
            .cloned()
            .unwrap_or(serde_json::json!([]));

        serde_json::json!({
            "selectedProviderId": if available { serde_json::json!("loom-sandbox") } else { serde_json::Value::Null },
            "status": {
                "enabled": enabled && available,
                "activeLease": null,
                "providers": [
                    {
                        "providerId": "loom-sandbox",
                        "status": {
                            "available": available,
                            "reason": reason,
                            "error": error,
                            "permissions": permissions
                        }
                    }
                ]
            },
            "settings": {
                "enabled": enabled,
                "app_approvals": []
            }
        })
    }

    // ── Bridge ─────────────────────────────────────────────────────────

    /// Get bridge status for an agent.
    pub async fn get_bridge_status(&self, agent_id: &str) -> serde_json::Value {
        let bridge_key = format!("settings.agent.{}.bridge", agent_id);
        let config = self.get_config(Some(&bridge_key)).await;
        let global = self.get_config(Some("settings.bridge")).await;

        crate::bridge::build_status(&config, &global, agent_id)
    }

    /// Test bridge platform credentials.
    pub async fn test_bridge_platform(
        &self,
        platform: &str,
        credentials: serde_json::Value,
    ) -> Result<serde_json::Value> {
        crate::bridge::test_platform(platform, &credentials).await
    }

    // ── Computer Use runtime ───────────────────────────────────────────

    /// List open app windows for computer use.
    pub fn list_computer_use_apps(&self) -> Result<serde_json::Value> {
        crate::computer_use::list_apps()
    }

    /// Get app state (screenshot + accessibility tree).
    pub fn get_computer_use_app_state(
        &self,
        target: serde_json::Value,
    ) -> Result<serde_json::Value> {
        crate::computer_use::get_app_state(&target)
    }

    /// Perform a computer use action.
    pub fn perform_computer_use_action(
        &self,
        target: serde_json::Value,
        action: serde_json::Value,
    ) -> Result<serde_json::Value> {
        crate::computer_use::perform_action(&target, &action)
    }

    /// Build the computer use tool definition for LLM tool calling.
    pub fn computer_use_tool_definition(&self) -> serde_json::Value {
        serde_json::json!({
            "name": "computer",
            "description": "Control a Windows application using accessibility APIs. Actions: list_apps, get_app_state, click_element, type_text, scroll, stop.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["list_apps", "get_app_state", "click_element", "type_text", "scroll", "stop"],
                        "description": "The action to perform."
                    },
                    "process_id": {
                        "type": "number",
                        "description": "Target process ID (for get_app_state and element actions)."
                    },
                    "window_id": {
                        "type": "number",
                        "description": "Target window handle."
                    },
                    "app_name": {
                        "type": "string",
                        "description": "App name to search for."
                    },
                    "element_id": {
                        "type": "string",
                        "description": "Element ID from the last get_app_state response (e.g. 'uia:5')."
                    },
                    "text": {
                        "type": "string",
                        "description": "Text to type into the element."
                    },
                    "direction": {
                        "type": "string",
                        "enum": ["up", "down", "left", "right"],
                        "description": "Scroll direction."
                    }
                },
                "required": ["action"]
            }
        })
    }

    /// List marketplace plugins from all configured sources.
    pub async fn list_marketplace_plugins(&self) -> serde_json::Value {
        let mp_dir = crate::plugin_fs::marketplace_dir(&self.data_dir);
        let mut plugins: Vec<crate::plugin_fs::MarketplacePlugin> = Vec::new();
        let mut warnings: Vec<String> = Vec::new();

        let sources = self.get_marketplace_sources().await;
        let has_sources = !sources.is_empty();

        // Scan top-level marketplace dir (local plugins placed directly)
        if mp_dir.exists() {
            let top_level = crate::plugin_fs::scan_marketplace_dir(&mp_dir);
            plugins.extend(top_level);
        }

        // Scan cloned repo sources (marketplace/<source-name>/)
        if mp_dir.exists()
            && let Ok(entries) = std::fs::read_dir(&mp_dir)
        {
            for entry in entries.flatten() {
                let source_dir = entry.path();
                if !source_dir.is_dir() {
                    continue;
                }
                let dir_name = source_dir
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                if dir_name.starts_with('.') {
                    continue;
                }
                if crate::plugin_fs::find_manifest_in_dir(&source_dir).is_some() {
                    continue;
                }

                // 1) Try marketplace.json first (primary listing format)
                let marketplace_json = source_dir.join(".claude-plugin").join("marketplace.json");
                if marketplace_json.exists() {
                    match crate::plugin_fs::parse_marketplace_listing(
                        &marketplace_json,
                        &source_dir,
                    ) {
                        Ok(listed) => {
                            tracing::info!(source = %dir_name, count = listed.len(), "loaded marketplace.json");
                            plugins.extend(listed);
                            continue;
                        }
                        Err(e) => {
                            warnings.push(format!(
                                "源「{}」marketplace.json 解析失败: {}",
                                dir_name, e
                            ));
                        }
                    }
                }

                // 2) Fallback: scan plugins/ and external_plugins/ subdirectories
                let mut found = false;
                for sub in &["plugins", "external_plugins"] {
                    let sub_dir = source_dir.join(sub);
                    if sub_dir.exists() && sub_dir.is_dir() {
                        let nested = crate::plugin_fs::scan_marketplace_dir(&sub_dir);
                        plugins.extend(nested);
                        found = true;
                    }
                }
                if !found {
                    // Deep scan other subdirectories
                    if let Ok(sub_entries) = std::fs::read_dir(&source_dir) {
                        for sub_entry in sub_entries.flatten() {
                            let sub_path = sub_entry.path();
                            if !sub_path.is_dir() {
                                continue;
                            }
                            let sn = sub_path
                                .file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_default();
                            if sn.starts_with('.')
                                || sn == "plugins"
                                || sn == "external_plugins"
                                || sn == "scripts"
                                || sn == "tests"
                                || sn == ".github"
                            {
                                continue;
                            }
                            if crate::plugin_fs::find_manifest_in_dir(&sub_path).is_some() {
                                if let Ok(entry) = crate::plugin_fs::load_plugin(&sub_path) {
                                    let ver = entry.version.clone();
                                    plugins.push(crate::plugin_fs::MarketplacePlugin {
                                        id: entry.id.clone(),
                                        name: entry.name,
                                        publisher: entry.author,
                                        version: Some(ver.clone()),
                                        description: if entry.description.is_empty() {
                                            None
                                        } else {
                                            Some(entry.description)
                                        },
                                        trust: entry.trust,
                                        contributions: entry.contributions,
                                        repository: entry.repository,
                                        compatibility: crate::plugin_fs::MarketplaceCompatibility {
                                            min_app_version: None,
                                        },
                                        distribution: Some(
                                            crate::plugin_fs::MarketplaceDistribution {
                                                kind: "source".to_string(),
                                                path: sub_path.to_string_lossy().to_string(),
                                            },
                                        ),
                                        installed: false,
                                        installed_version: None,
                                        latest_version: Some(ver),
                                        update_available: false,
                                        can_install: true,
                                        install_action: "install".to_string(),
                                        compatible: true,
                                    });
                                    found = true;
                                }
                            } else {
                                let nested = crate::plugin_fs::scan_marketplace_dir(&sub_path);
                                plugins.extend(nested);
                                found = true;
                            }
                        }
                    }
                }
                if !found
                    && sources
                        .iter()
                        .any(|s| s.path.as_deref() == Some(source_dir.to_string_lossy().as_ref()))
                {
                    warnings.push(format!("源「{}」未发现插件或 marketplace.json", dir_name));
                }
            }
        }

        // Check for sources whose paths don't exist (clone failed, repo moved, etc.)
        for source in &sources {
            if let Some(ref path_str) = source.path {
                let p = std::path::Path::new(path_str);
                if !p.exists() {
                    warnings.push(format!(
                        "源「{}」路径不存在（clone 可能失败，请检查URL是否正确）: {}",
                        source.name, path_str
                    ));
                }
            }
        }

        // Dedup by ID
        let mut seen = std::collections::HashSet::new();
        plugins.retain(|p| seen.insert(p.id.clone()));

        // Cross-reference with installed plugins
        let plugins_dir = self.data_dir.join("plugins");
        let installed = crate::plugin_fs::scan_plugins_dir(&plugins_dir);
        crate::plugin_fs::cross_reference_marketplace(&mut plugins, &installed);

        let plugins_json: Vec<serde_json::Value> = plugins
            .iter()
            .map(|p| {
                serde_json::json!({
                    "id": p.id,
                    "name": p.name,
                    "publisher": p.publisher,
                    "version": p.version,
                    "description": p.description,
                    "trust": p.trust,
                    "contributions": p.contributions,
                    "repository": p.repository,
                    "compatibility": {
                        "minAppVersion": p.compatibility.min_app_version,
                    },
                    "distribution": p.distribution.as_ref().map(|d| serde_json::json!({
                        "kind": d.kind,
                        "path": d.path,
                    })),
                    "installed": p.installed,
                    "installedVersion": p.installed_version,
                    "latestVersion": p.latest_version,
                    "updateAvailable": p.update_available,
                    "canInstall": p.can_install,
                    "installAction": p.install_action,
                    "compatible": p.compatible,
                })
            })
            .collect();

        serde_json::json!({
            "source": {
                "kind": "local",
                "configured": has_sources,
                "path": mp_dir.to_string_lossy().to_string(),
            },
            "plugins": plugins_json,
            "warnings": warnings,
        })
    }

    /// Get marketplace plugin readme.
    pub fn get_marketplace_plugin_readme(&self, id: &str) -> serde_json::Value {
        let mp_dir = crate::plugin_fs::marketplace_dir(&self.data_dir);
        let plugins = crate::plugin_fs::scan_marketplace_dir(&mp_dir);
        let plugin = plugins.iter().find(|p| p.id == id);

        let dist_path = plugin.and_then(|p| {
            p.distribution
                .as_ref()
                .map(|d| std::path::PathBuf::from(&d.path))
        });

        let readme = dist_path.and_then(|dir| {
            // Try README.md, readme.md, README
            for name in &["README.md", "readme.md", "README"] {
                let path = dir.join(name);
                if let Ok(content) = std::fs::read_to_string(&path) {
                    return Some(content);
                }
            }
            None
        });

        serde_json::json!({
            "markdown": readme.unwrap_or_default(),
        })
    }

    /// Install a plugin from marketplace by ID.
    /// Uses the same comprehensive scan as list_marketplace_plugins so
    /// plugins discovered via marketplace.json or nested repo subdirectories
    /// can also be installed.
    pub async fn install_marketplace_plugin(&self, id: &str) -> Result<serde_json::Value> {
        let mp_dir = crate::plugin_fs::marketplace_dir(&self.data_dir);

        // ── Step 1: Try listing JSON to find the plugin ──
        let listing = self.list_marketplace_plugins().await;
        let plugins_json = listing.get("plugins").and_then(|p| p.as_array());

        let dist_path = plugins_json
            .and_then(|arr| {
                arr.iter()
                    .find(|p| p.get("id").and_then(|v| v.as_str()) == Some(id))
            })
            .and_then(|p| {
                p.get("distribution")
                    .and_then(|d| d.get("path"))
                    .and_then(|v| v.as_str())
                    .map(String::from)
            });

        // ── Step 2: If distribution path missing, scan filesystem directly ──
        let dist_path = if let Some(ref path) = dist_path {
            path.clone()
        } else {
            // Fallback: do the same comprehensive scan but keep MarketplacePlugin
            // structs so we can access distribution.path directly.
            let _sources = self.get_marketplace_sources().await;
            let mut all: Vec<crate::plugin_fs::MarketplacePlugin> = Vec::new();

            if mp_dir.exists() {
                all.extend(crate::plugin_fs::scan_marketplace_dir(&mp_dir));
            }

            if mp_dir.exists() {
                if let Ok(entries) = std::fs::read_dir(&mp_dir) {
                    for entry in entries.flatten() {
                        let source_dir = entry.path();
                        if !source_dir.is_dir() {
                            continue;
                        }
                        let dn = source_dir
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_default();
                        if dn.starts_with('.') {
                            continue;
                        }
                        if crate::plugin_fs::find_manifest_in_dir(&source_dir).is_some() {
                            continue;
                        }

                        let mj = source_dir.join(".claude-plugin").join("marketplace.json");
                        if mj.exists() {
                            if let Ok(listed) =
                                crate::plugin_fs::parse_marketplace_listing(&mj, &source_dir)
                            {
                                all.extend(listed);
                                continue;
                            }
                        }
                        for sub in &["plugins", "external_plugins"] {
                            let sd = source_dir.join(sub);
                            if sd.exists() && sd.is_dir() {
                                all.extend(crate::plugin_fs::scan_marketplace_dir(&sd));
                            }
                        }
                        if let Ok(sub_entries) = std::fs::read_dir(&source_dir) {
                            for sub_entry in sub_entries.flatten() {
                                let sp = sub_entry.path();
                                if !sp.is_dir() {
                                    continue;
                                }
                                let sn = sp
                                    .file_name()
                                    .map(|n| n.to_string_lossy().to_string())
                                    .unwrap_or_default();
                                if sn.starts_with('.')
                                    || sn == "plugins"
                                    || sn == "external_plugins"
                                    || sn == "scripts"
                                    || sn == "tests"
                                    || sn == ".github"
                                {
                                    continue;
                                }
                                if crate::plugin_fs::find_manifest_in_dir(&sp).is_some() {
                                    if let Ok(entry) = crate::plugin_fs::load_plugin(&sp) {
                                        all.push(crate::plugin_fs::MarketplacePlugin {
                                            id: entry.id.clone(),
                                            name: entry.name,
                                            publisher: entry.author,
                                            version: Some(entry.version.clone()),
                                            description: if entry.description.is_empty() {
                                                None
                                            } else {
                                                Some(entry.description)
                                            },
                                            trust: entry.trust,
                                            contributions: entry.contributions,
                                            repository: entry.repository,
                                            compatibility:
                                                crate::plugin_fs::MarketplaceCompatibility {
                                                    min_app_version: None,
                                                },
                                            distribution: Some(
                                                crate::plugin_fs::MarketplaceDistribution {
                                                    kind: "source".to_string(),
                                                    path: sp.to_string_lossy().to_string(),
                                                },
                                            ),
                                            installed: false,
                                            installed_version: None,
                                            latest_version: Some(entry.version),
                                            update_available: false,
                                            can_install: true,
                                            install_action: "install".to_string(),
                                            compatible: true,
                                        });
                                    }
                                } else {
                                    all.extend(crate::plugin_fs::scan_marketplace_dir(&sp));
                                }
                            }
                        }
                    }
                }
            }

            all.iter()
                .find(|p| p.id == id)
                .and_then(|p| p.distribution.as_ref().map(|d| d.path.clone()))
                .ok_or_else(|| anyhow::anyhow!("marketplace plugin not found: {}", id))?
        };

        let plugins_dir = self.data_dir.join("plugins");
        std::fs::create_dir_all(&plugins_dir)?;
        let entry =
            crate::plugin_fs::install_plugin(std::path::Path::new(&dist_path), &plugins_dir)?;

        Ok(serde_json::json!({
            "name": entry.name,
            "id": entry.id,
        }))
    }

    /// Translate text to the target language using any configured model.
    pub async fn translate(&self, text: &str, target: &str) -> Result<String> {
        // Find a usable model from config — prefer the active model setting,
        // then fall back to any cloud-capable or local model in the config list,
        // then check settings.providers for custom provider models.
        let config = self.config.read().await;
        let (model_id, backend): (String, String) = {
            // 1) Check settings.active_model
            if let Some(active) = config.settings.get("active_model") {
                let id = active.get("id").and_then(|v| v.as_str()).unwrap_or("");
                let prov = active
                    .get("provider")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if !id.is_empty() && !prov.is_empty() {
                    (id.to_string(), prov.to_string())
                } else {
                    (String::new(), String::new())
                }
            } else {
                (String::new(), String::new())
            }
        };
        drop(config);

        let (model_id, backend) = if !model_id.is_empty() {
            (model_id, backend)
        } else {
            // 2) Fall back: scan config.models + settings.providers
            let config = self.config.read().await;
            let found = config
                .models
                .iter()
                .find_map(|m| {
                    let id = m.model.as_deref().unwrap_or(&m.name);
                    if id.is_empty() {
                        return None;
                    }
                    Some((id.to_string(), m.backend.name().to_string()))
                })
                .or_else(|| {
                    config.settings.get("providers").and_then(|providers| {
                        providers.as_object().and_then(|obj| {
                            obj.iter().find_map(|(name, prov)| {
                                let models = prov.get("models").and_then(|v| v.as_array())?;
                                let first = models.first()?;
                                let id = if let Some(s) = first.as_str() {
                                    s
                                } else {
                                    first.get("id")?.as_str()?
                                };
                                Some((id.to_string(), name.clone()))
                            })
                        })
                    })
                });
            drop(config);
            found.ok_or_else(|| {
                anyhow::anyhow!(
                    "No model configured. Please add a model in Settings → Providers first."
                )
            })?
        };

        let prompt = format!(
            "Translate the following text to {target}. IMPORTANT: output ONLY the translated text, \
             preserve all markdown formatting (headings, lists, code blocks, links), \
             do NOT add any explanations or notes.\n\n{text}",
        );

        let result = self
            .complete_with_model("_translate", &prompt, &model_id, &backend)
            .await?;
        Ok(result.trim().to_string())
    }

    pub async fn agent_state(&self) -> AgentState {
        self.agent_state.read().await.clone()
    }

    /// Signal an interrupt to the engine, clearing the busy flag so a new
    /// turn can start. Used by TurnInterrupt RPC mapping.
    pub fn interrupt(&self) {
        self.interruptible
            .store(false, std::sync::atomic::Ordering::SeqCst);
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

    pub fn db_path(&self) -> &std::path::Path {
        &self.db_path
    }

    pub fn read_settings(&self) -> serde_json::Value {
        self.config.blocking_read().settings.clone()
    }

    /// Request abort of the streaming response for the given session.
    /// Returns true if a flag was found and set (session was streaming).
    pub fn abort_session(&self, session_id: &str) -> bool {
        let flags = self.abort_flags.lock().unwrap();
        if let Some(flag) = flags.get(session_id) {
            flag.store(true, Ordering::SeqCst);
            true
        } else {
            false
        }
    }

    /// Compact a session's working memory by summarizing the conversation.
    /// Returns the number of messages replaced.
    pub async fn compact_session(&self, session_id: &str) -> Result<usize> {
        let history = self.get_working_memory(session_id).unwrap_or_default();
        if history.is_empty() {
            return Ok(0);
        }

        // Extract user/assistant messages (skip tool messages)
        let conversation: Vec<String> = history
            .iter()
            .filter(|m| m.role == "user" || m.role == "assistant")
            .map(|m| format!("[{}]: {}", m.role, m.content))
            .collect();

        if conversation.is_empty() {
            return Ok(0);
        }

        let count = conversation.len();
        let text = conversation.join("\n\n");

        let prompt = format!(
            "Summarize the following conversation into a concise summary under 500 characters. Include key decisions, code changes, and important context. Respond with only the summary, no preamble.\n\n{}",
            text
        );

        // Use the cloud client or local inference for summarization
        let summary = match self.invoke_model_raw(&prompt).await {
            Ok(s) => s.trim().to_string(),
            Err(e) => {
                tracing::warn!(error = %e, "Compaction summarization failed, using heuristic truncation");
                // Fallback: keep last 500 chars
                let fallback: String = text
                    .chars()
                    .rev()
                    .take(500)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect();
                format!("[Compacted context]\n\n...{}", fallback)
            }
        };

        // Replace working memory with the summary
        let compact_msg = ChatMessage {
            role: "system".into(),
            content: format!("[Compacted session context]\n\n{}", summary),
            timestamp: chrono::Utc::now(),
            id: None,
            seq: None,
            metadata: None,
        };

        let conn = rusqlite::Connection::open(&self.db_path)?;
        // Delete old messages for this session
        conn.execute(
            "DELETE FROM message_history WHERE session_id = ?1",
            rusqlite::params![session_id],
        )?;
        // Insert compacted summary
        let store = openloom_memory::store::MessageStore::new(&conn);
        store.insert(session_id, 1, &compact_msg.role, &compact_msg.content)?;

        tracing::info!(session_id, count, "Session compacted");
        Ok(count)
    }

    /// Get or create the abort flag for a session. The flag is removed when streaming ends.
    fn session_abort_flag(&self, session_id: &str) -> Arc<AtomicBool> {
        let mut flags = self.abort_flags.lock().unwrap();
        flags
            .entry(session_id.to_string())
            .or_insert_with(|| Arc::new(AtomicBool::new(false)))
            .clone()
    }

    fn remove_abort_flag(&self, session_id: &str) {
        let mut flags = self.abort_flags.lock().unwrap();
        flags.remove(session_id);
    }

    /// Get the permission mode for a session. Falls back to default mode.
    pub fn permission_mode(&self, session_id: &str) -> String {
        if !session_id.is_empty() {
            let modes = self.permission_modes.lock().unwrap();
            if let Some(m) = modes.get(session_id) {
                return m.clone();
            }
        }
        self.default_permission_mode.lock().unwrap().clone()
    }

    /// Set the permission mode for a specific session, or set the default for new sessions.
    /// Valid modes: "operate", "ask", "read_only". Other values default to "ask".
    pub fn set_permission_mode(
        &self,
        session_id: &str,
        mode: &str,
        pending_new_session: bool,
    ) -> String {
        let normalized = match mode {
            "operate" | "ask" | "read_only" => mode.to_string(),
            _ => "ask".to_string(),
        };
        if pending_new_session || session_id.is_empty() {
            *self.default_permission_mode.lock().unwrap() = normalized.clone();
        } else {
            let mut modes = self.permission_modes.lock().unwrap();
            modes.insert(session_id.to_string(), normalized.clone());
        }
        normalized
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

    /// Returns the raw model ID string (e.g. "deepseek-v4-pro[1m]") for use in
    /// protocol responses. Prefers cloud over local. Returns empty string when
    /// no model is configured so callers can supply their own fallback.
    pub fn current_model_id(&self) -> String {
        if let Some(ref cloud) = self.cloud {
            return cloud.model_name().to_string();
        }
        if let Some(ref local) = self.local_client {
            return local.model_name().to_string();
        }
        String::new()
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
            .find(|m| m.backend.is_local_inference())
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
            // Prefer the user's saved active model from settings, falling back to
            // the first cloud-capable (or local) model in the config list.
            let active = resolve_active_model(&config.models, &config.settings).or_else(|| {
                let cloud = config.models.iter().find(|m| m.backend.is_cloud_capable());
                let local = config
                    .models
                    .iter()
                    .find(|m| m.backend.is_local_inference());
                cloud.or(local)
            });
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

/// Resolve the active model from `settings.active_model` against the typed model configs.
/// Returns None if no active model is set or the referenced model is not found.
fn resolve_active_model<'a>(
    models: &'a [openloom_models::ModelConfig],
    settings: &serde_json::Value,
) -> Option<&'a openloom_models::ModelConfig> {
    let active = settings.get("active_model")?;
    let id = active.get("id")?.as_str()?;
    let provider = active
        .get("provider")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    models.iter().find(|m| {
        let m_id = m.model.as_deref().unwrap_or("");
        m_id == id && m.backend.name().eq_ignore_ascii_case(provider)
    })
}

fn slugify_str(value: &str) -> String {
    value
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
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
            id: None,
            metadata: None,
            seq: None,
        };
        let sid = engine.create_session().await.unwrap().id;
        let resp = engine
            .handle_message(
                msg,
                &sid,
                Mode::Code,
                openloom_models::ModelPreference::default(),
            )
            .await
            .unwrap();
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
            id: None,
            metadata: None,
            seq: None,
        };
        let sid = engine.create_session().await.unwrap().id;
        engine
            .handle_message(
                msg,
                &sid,
                Mode::Code,
                openloom_models::ModelPreference::default(),
            )
            .await
            .unwrap();
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
            id: None,
            metadata: None,
            seq: None,
        };
        let sid = engine.create_session().await.unwrap().id;
        let resp = engine
            .handle_message(
                msg,
                &sid,
                Mode::Code,
                openloom_models::ModelPreference::default(),
            )
            .await
            .unwrap();
        assert!(!resp.session_id.is_empty());
    }

    fn sync_setup() -> (Engine, tempfile::TempDir) {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(setup_test_engine())
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
            id: None,
            metadata: None,
            seq: None,
        };
        let sid = engine.create_session().await.unwrap().id;
        let resp = engine
            .handle_message(
                msg,
                &sid,
                Mode::Code,
                openloom_models::ModelPreference::default(),
            )
            .await;
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

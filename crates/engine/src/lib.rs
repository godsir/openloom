pub mod memory_thread;

use anyhow::Result;
use openloom_cache::{NoopCache};
use openloom_inference::{CloudClient, CompletionRequest, InferenceEngine};
use openloom_models::*;
use openloom_models::NoopPersonaProvider;
use openloom_router::SmartRouter;
use openloom_skills::{SkillRegistry, builtins};
use openloom_weaver::ContextWeaver;
use openloom_memory::store::SessionStore;
use std::path::PathBuf;
use std::sync::{Arc, mpsc};
use tokio::sync::{broadcast, oneshot};

// Re-export EngineEvent from models
pub use openloom_models::EngineEvent;

const SYSTEM_INSTRUCTION: &str = "You are openLoom, a private AI assistant running locally.";

pub struct Engine {
    router: SmartRouter,
    skills: SkillRegistry,
    inference: Arc<InferenceEngine>,
    cloud: Option<Arc<dyn CloudClient>>,
    weaver: ContextWeaver,
    memory_tx: mpsc::Sender<memory_thread::ProcessRequest>,
    session_tx: mpsc::Sender<SessionCommand>,
    event_bus: broadcast::Sender<EngineEvent>,
    db_path: PathBuf,
}

#[allow(dead_code)]
enum SessionCommand {
    Create { reply: oneshot::Sender<SessionInfo> },
    List { reply: oneshot::Sender<Vec<SessionInfo>> },
    UpdateCount { id: String, count: usize },
}

pub struct EngineConfig {
    pub data_dir: PathBuf,
    pub threshold: usize,
    pub cloud_config: Option<openloom_models::ModelConfig>,
}

fn spawn_session_thread(db_path: PathBuf) -> mpsc::Sender<SessionCommand> {
    let (tx, rx) = mpsc::channel::<SessionCommand>();
    std::thread::spawn(move || {
        let conn = rusqlite::Connection::open(&db_path).expect("session db open");
        conn.execute_batch("PRAGMA journal_mode=WAL;").unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY,
                created_at TEXT NOT NULL,
                message_count INTEGER DEFAULT 0
            );"
        ).unwrap();
        let store = SessionStore::new(&conn);
        for cmd in rx {
            match cmd {
                SessionCommand::Create { reply } => {
                    let id = uuid::Uuid::new_v4().to_string();
                    let info = SessionInfo { id: id.clone(), created_at: chrono::Utc::now(), message_count: 0 };
                    let _ = store.insert(&info.id, info.created_at);
                    let _ = reply.send(info);
                }
                SessionCommand::List { reply } => {
                    let sessions = store.list_all(100).unwrap_or_default();
                    let _ = reply.send(sessions);
                }
                SessionCommand::UpdateCount { id, count } => {
                    let _ = store.update_message_count(&id, count);
                }
            }
        }
    });
    tx
}

impl Engine {
    /// Create a test Engine with a temp database
    pub fn new_test(db_path: PathBuf) -> Result<Self> {
        Self::new(EngineConfig {
            data_dir: db_path.parent().unwrap().to_path_buf(),
            threshold: 3,
            cloud_config: None,
        })
    }

    /// Create the Engine with all subsystems
    pub fn new(config: EngineConfig) -> Result<Self> {
        // 1. Inference engine (Phase 1: placeholder, no model file needed)
        let inference = Arc::new(InferenceEngine::load_blocking(
            &config
                .data_dir
                .join("models")
                .join("qwen3-1.7b-q4_k_m.gguf"),
            0,
        )?);

        // 2. Router (keyword-first)
        let mut router =
            SmartRouter::new_keywords_only(openloom_router::keywords::default_keyword_rules());

        // 3. Skill registry with built-in skills
        let mut skills = SkillRegistry::new();
        builtins::register_all(&mut skills);

        // Wire skill triggers into router
        for skill in skills.all_skills() {
            let manifest = skill.manifest();
            router.register_skill_triggers(skill.name(), manifest.triggers.clone());
        }

        // 4. Cloud client
        let cloud: Option<Arc<dyn CloudClient>> = config.cloud_config.as_ref().and_then(|cfg| {
            openloom_inference::create_cloud_client(cfg).ok().map(Arc::from)
        });
        router.set_cloud_available(cloud.is_some());

        // 5. Weaver with stubs
        let weaver = ContextWeaver::new(Arc::new(NoopCache), Arc::new(NoopPersonaProvider));

        // 6. EventBus
        let (event_tx, _) = broadcast::channel(256);

        // 7. Memory pipeline in dedicated thread
        let db_path = config.data_dir.join("data").join("db.sqlite");
        let _ = std::fs::create_dir_all(db_path.parent().unwrap());

        let memory_tx = memory_thread::spawn_memory_thread(db_path.clone(), config.threshold, event_tx.clone());

        // 8. Session persistence thread
        let session_tx = spawn_session_thread(db_path.clone());

        Ok(Self {
            router,
            skills,
            inference,
            cloud,
            weaver,
            memory_tx,
            session_tx,
            event_bus: event_tx,
            db_path,
        })
    }

    /// Core request handler
    pub async fn handle_message(&self, msg: ChatMessage, session_id: &str) -> Result<ChatResponse> {
        // 1. Classify intent
        let out = self.router.classify_sync(&msg.content);

        // 2. Gather context for prompt assembly
        let skill_ctx = out.skill_match.as_ref().and_then(|name| {
            self.skills.find_by_name(name).map(|s| s.context_md().to_string())
        });
        let working_memory = self.get_working_memory(session_id)?;
        let assembled = self.weaver.assemble(SYSTEM_INSTRUCTION, &msg.content, skill_ctx.as_deref(), &working_memory);

        // 3. Execute based on target model
        let response = match out.target_model {
            TargetModel::None => {
                let name = out.skill_match.as_ref()
                    .ok_or_else(|| anyhow::anyhow!("skill_match is None but target_model is None"))?;
                self.skills.invoke(name, serde_json::json!({"text": msg.content})).await?.to_string()
            }
            TargetModel::Local => {
                self.inference.complete(CompletionRequest { prompt: assembled.prompt.clone(), ..Default::default() }).await?.text
            }
            TargetModel::Cloud => {
                if let Some(ref cloud) = self.cloud {
                    cloud.complete(CompletionRequest { prompt: assembled.prompt.clone(), ..Default::default() }).await?.text
                } else {
                    self.inference.complete(CompletionRequest { prompt: assembled.prompt.clone(), ..Default::default() }).await?.text
                }
            }
        };

        // 4. Background: memory pipeline (fire-and-forget via channel)
        let _ = self.memory_tx.send(memory_thread::ProcessRequest {
            session_id: session_id.to_string(),
            text: msg.content.clone(),
            context: out.intent.to_string(),
        });

        // 5. Broadcast token usage event
        let prompt_tokens = self.inference.token_count(&assembled.prompt);
        let completion_tokens = self.inference.token_count(&response);
        let _ = self.event_bus.send(EngineEvent::TokenUsage {
            session_id: session_id.to_string(),
            model: "qwen3-1.7b".into(),
            prompt_tokens,
            completion_tokens,
        });

        Ok(ChatResponse {
            response,
            session_id: session_id.to_string(),
            token_usage: TokenUsage {
                prompt_tokens,
                completion_tokens,
            },
        })
    }

    fn get_working_memory(&self, _session_id: &str) -> Result<Vec<ChatMessage>> {
        Ok(Vec::new())
    }

    pub async fn health_check(&self) -> HealthStatus {
        let gpu = InferenceEngine::detect_gpu();
        HealthStatus {
            status: "ok".into(),
            uptime: 0,
            gpu_info: gpu,
        }
    }

    pub async fn create_session(&self) -> Result<SessionInfo> {
        let (tx, rx) = oneshot::channel();
        self.session_tx.send(SessionCommand::Create { reply: tx }).map_err(|e| anyhow::anyhow!("{}", e))?;
        rx.await.map_err(|e| anyhow::anyhow!("{}", e))
    }

    pub async fn list_sessions(&self) -> Result<Vec<SessionInfo>> {
        let (tx, rx) = oneshot::channel();
        self.session_tx.send(SessionCommand::List { reply: tx }).map_err(|e| anyhow::anyhow!("{}", e))?;
        rx.await.map_err(|e| anyhow::anyhow!("{}", e))
    }

    pub async fn list_cognitions(&self, subject: &str, limit: usize) -> Result<Vec<openloom_memory::store::CognitionRow>> {
        let conn = rusqlite::Connection::open(&self.db_path)?;
        let store = openloom_memory::store::CognitionStore::new(&conn);
        store.query_by_subject(subject, limit)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<EngineEvent> {
        self.event_bus.subscribe()
    }

    pub fn list_skills(&self) -> Vec<openloom_skills::SkillInfo> {
        self.skills.list_all()
    }

    pub async fn invoke_skill(&self, name: &str, params: serde_json::Value) -> Result<serde_json::Value> {
        self.skills.invoke(name, params).await
    }

    pub async fn shutdown(&self) -> Result<()> {
        tracing::info!("engine shutting down");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        };
        let sid = engine.create_session().await.unwrap().id;
        let resp = engine.handle_message(msg, &sid).await.unwrap();
        assert_eq!(resp.session_id, sid);
    }

    #[tokio::test]
    async fn test_health_check() {
        let (engine, _dir) = setup_test_engine().await;
        let health = engine.health_check().await;
        assert_eq!(health.status, "ok");
    }

    #[tokio::test]
    async fn test_event_bus_subscribe() {
        let (engine, _dir) = setup_test_engine().await;
        let mut rx = engine.subscribe();
        let msg = ChatMessage {
            role: "user".into(),
            content: "hello".into(),
        };
        let sid = engine.create_session().await.unwrap().id;
        engine.handle_message(msg, &sid).await.unwrap();
        let event = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv()).await;
        assert!(event.is_ok(), "should receive TokenUsage event");
    }

    #[tokio::test]
    async fn test_handle_message_skill_path() {
        let (engine, _dir) = setup_test_engine().await;
        let msg = ChatMessage {
            role: "user".into(),
            content: "帮我管理文件".into(),
        };
        let sid = engine.create_session().await.unwrap().id;
        let resp = engine.handle_message(msg, &sid).await.unwrap();
        assert!(!resp.session_id.is_empty());
    }
}

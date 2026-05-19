pub mod memory_thread;

use anyhow::Result;
use openloom_inference::{CompletionRequest, InferenceEngine};
use openloom_models::*;
use openloom_router::SmartRouter;
use openloom_skills::{SkillRegistry, builtins};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use tokio::sync::broadcast;

// Re-export EngineEvent from models
pub use openloom_models::EngineEvent;

pub struct Engine {
    router: SmartRouter,
    skills: SkillRegistry,
    inference: Arc<InferenceEngine>,
    memory_tx: std::sync::mpsc::Sender<memory_thread::ProcessRequest>,
    sessions: Arc<RwLock<HashMap<String, SessionInfo>>>,
    event_bus: broadcast::Sender<EngineEvent>,
}

pub struct EngineConfig {
    pub data_dir: PathBuf,
    pub threshold: usize,
}

impl Engine {
    /// Create a test Engine with a temp database
    pub fn new_test(db_path: PathBuf) -> Result<Self> {
        Self::new(EngineConfig {
            data_dir: db_path.parent().unwrap().to_path_buf(),
            threshold: 3,
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

        // 4. Memory pipeline in dedicated thread
        let db_path = config.data_dir.join("data").join("db.sqlite");
        let _ = std::fs::create_dir_all(db_path.parent().unwrap());

        // 5. EventBus
        let (event_tx, _) = broadcast::channel(256);

        let memory_tx = memory_thread::spawn_memory_thread(db_path, config.threshold, event_tx.clone());

        // 6. Wire skill triggers into router (router is mut after step 2)

        Ok(Self {
            router,
            skills,
            inference,
            memory_tx,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            event_bus: event_tx,
        })
    }

    /// Core request handler
    pub async fn handle_message(&self, msg: ChatMessage, session_id: &str) -> Result<ChatResponse> {
        // 1. Classify intent
        let out = self.router.classify_sync(&msg.content);

        // 2. Execute based on target model
        let response = match out.target_model {
            TargetModel::None => {
                let skill_name = out.skill_match.as_ref().ok_or_else(|| {
                    anyhow::anyhow!("skill_match is None but target_model is None")
                })?;
                let params =
                    serde_json::json!({"text": msg.content, "intent": out.intent.to_string()});
                self.skills.invoke(skill_name, params).await?.to_string()
            }
            TargetModel::Local => {
                let req = CompletionRequest {
                    prompt: msg.content.clone(),
                    ..Default::default()
                };
                self.inference.complete(req).await?.text
            }
        };

        // 3. Background: memory pipeline (fire-and-forget via channel)
        let _ = self.memory_tx.send(memory_thread::ProcessRequest {
            session_id: session_id.to_string(),
            text: msg.content.clone(),
            context: out.intent.to_string(),
        });

        let prompt_tokens = self.inference.token_count(&msg.content);
        let completion_tokens = self.inference.token_count(&response);

        // 4. Broadcast token usage event
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

    pub async fn health_check(&self) -> HealthStatus {
        let gpu = InferenceEngine::detect_gpu();
        HealthStatus {
            status: "ok".into(),
            uptime: 0,
            gpu_info: gpu,
        }
    }

    pub async fn create_session(&self) -> Result<SessionInfo> {
        let id = uuid::Uuid::new_v4().to_string();
        let info = SessionInfo {
            id: id.clone(),
            created_at: chrono::Utc::now(),
            message_count: 0,
        };
        self.sessions.write().unwrap().insert(id, info.clone());
        Ok(info)
    }

    pub async fn list_sessions(&self) -> Result<Vec<SessionInfo>> {
        Ok(self.sessions.read().unwrap().values().cloned().collect())
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

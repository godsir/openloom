pub mod memory_thread;

use anyhow::Result;
use openloom_cache::NoopCache;
use openloom_inference::{CloudClient, CompletionRequest, InferenceEngine};
use openloom_models::*;
use openloom_memory::persona::CognitionsPersonaProvider;
use openloom_memory::store::{MessageStore, SessionStore};
use openloom_router::SmartRouter;
use openloom_skills::{SkillRegistry, builtins};
use openloom_weaver::ContextWeaver;
use std::path::PathBuf;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use tokio::sync::{broadcast, oneshot, RwLock};
use chrono::Utc;

pub use openloom_models::EngineEvent;

const SYSTEM_INSTRUCTION: &str = "You are openLoom, a private AI assistant running locally.
When you need to use a tool, respond with ONLY a JSON block on a single line:
{\"tool\": \"<skill_name>\", \"params\": {\"key\": \"value\"}}
Available tools: [tools]
When you have the final answer, respond in natural language without JSON.";

pub struct Engine {
    router: SmartRouter,
    skills: SkillRegistry,
    inference: Arc<InferenceEngine>,
    cloud: Option<Arc<dyn CloudClient>>,
    weaver: ContextWeaver,
    persona: Arc<dyn PersonaProvider>,
    memory_tx: std::sync::mpsc::Sender<memory_thread::ProcessRequest>,
    session_tx: std::sync::mpsc::Sender<SessionCommand>,
    event_bus: broadcast::Sender<EngineEvent>,
    agent_state: Arc<RwLock<AgentState>>,
    interruptible: AtomicBool,
    db_path: PathBuf,
}

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

fn spawn_session_thread(db_path: PathBuf) -> std::sync::mpsc::Sender<SessionCommand> {
    let (tx, rx) = std::sync::mpsc::channel::<SessionCommand>();
    std::thread::spawn(move || {
        let conn = rusqlite::Connection::open(&db_path).expect("session db open");
        conn.execute_batch("PRAGMA journal_mode=WAL;").unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY, created_at TEXT NOT NULL, message_count INTEGER DEFAULT 0
            );"
        ).unwrap();
        let store = SessionStore::new(&conn);
        for cmd in rx {
            match cmd {
                SessionCommand::Create { reply } => {
                    let id = uuid::Uuid::new_v4().to_string();
                    let info = SessionInfo { id: id.clone(), created_at: Utc::now(), message_count: 0 };
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
    pub fn new_test(db_path: PathBuf) -> Result<Self> {
        Self::new(EngineConfig {
            data_dir: db_path.parent().unwrap().to_path_buf(),
            threshold: 3,
            cloud_config: None,
        })
    }

    pub fn new(config: EngineConfig) -> Result<Self> {
        let inference = Arc::new(InferenceEngine::load_blocking(
            &config.data_dir.join("models").join("qwen3-1.7b-q4_k_m.gguf"), 0,
        )?);

        let mut router = SmartRouter::new_keywords_only(openloom_router::keywords::default_keyword_rules());
        let mut skills = SkillRegistry::new();
        builtins::register_all(&mut skills);
        for skill in skills.all_skills() {
            let manifest = skill.manifest();
            router.register_skill_triggers(skill.name(), manifest.triggers.clone());
        }

        let cloud: Option<Arc<dyn CloudClient>> = config.cloud_config.as_ref()
            .and_then(|cfg| openloom_inference::create_cloud_client(cfg).ok().map(Arc::from));
        router.set_cloud_available(cloud.is_some());

        let db_path = config.data_dir.join("data").join("db.sqlite");
        let _ = std::fs::create_dir_all(db_path.parent().unwrap());

        let persona: Arc<dyn PersonaProvider> = Arc::new(CognitionsPersonaProvider::new(db_path.clone()));
        let weaver = ContextWeaver::new(Arc::new(NoopCache));

        let (event_tx, _) = broadcast::channel(256);
        let memory_tx = memory_thread::spawn_memory_thread(db_path.clone(), config.threshold, event_tx.clone());
        let session_tx = spawn_session_thread(db_path.clone());

        let engine = Self {
            router, skills, inference, cloud, weaver, persona, memory_tx, session_tx,
            event_bus: event_tx,
            agent_state: Arc::new(RwLock::new(AgentState::Idle)),
            interruptible: AtomicBool::new(false),
            db_path,
        };

        engine.spawn_persona_watcher();
        Ok(engine)
    }

    // === Persona watcher ===

    fn spawn_persona_watcher(&self) {
        let persona = self.persona.clone();
        let mut rx = self.event_bus.subscribe();
        tokio::spawn(async move {
            while rx.recv().await.is_ok() {
                persona.invalidate();
            }
        });
    }

    // === Core handler ===

    pub async fn handle_message(&self, msg: ChatMessage, session_id: &str) -> Result<ChatResponse> {
        // Atomic mid-turn check: compare_exchange ensures only one caller enters
        if self.interruptible.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
            return Err(anyhow::anyhow!("Agent is busy, please wait"));
        }
        // C1 fix: do NOT release the gate here — only at the end of each path

        let out = self.router.classify_sync(&msg.content);

        // C2 fix: feed cognition extraction pipeline (non-blocking)
        let _ = self.memory_tx.send(memory_thread::ProcessRequest {
            session_id: session_id.to_string(),
            text: msg.content.clone(),
            context: out.intent.to_string(),
        });

        // C3 fix: complex intent or skill match -> agent loop
        if out.complexity >= 0.8 || out.skill_match.is_some() {
            return self.agent_loop(&msg, session_id).await;
        }

        // Simple path: direct dispatch based on router's target_model
        let skill_ctx = out.skill_match.as_ref()
            .and_then(|name| self.skills.find_by_name(name).map(|s| s.context_md().to_string()));
        let working_memory = self.get_working_memory(session_id).unwrap_or_default();
        // Persona failure → empty string fallback
        let persona_summary = self.persona.summarize().await.unwrap_or_default();
        let assembled = self.weaver.assemble(
            SYSTEM_INSTRUCTION, &msg.content, &persona_summary, skill_ctx.as_deref(), &working_memory,
        );

        let response = match out.target_model {
            TargetModel::None => {
                let name = out.skill_match.as_ref()
                    .ok_or_else(|| anyhow::anyhow!("skill_match is None"))?;
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

        // save_messages is non-fatal
        let _ = self.save_messages(session_id, &msg, &response);

        let prompt_tokens = self.inference.token_count(&assembled.prompt);
        let completion_tokens = self.inference.token_count(&response);
        let _ = self.event_bus.send(EngineEvent::TokenUsage {
            session_id: session_id.to_string(),
            model: "qwen3-1.7b".into(),
            prompt_tokens,
            completion_tokens,
        });

        // C1 fix: reset interruptible flag only at end of simple path
        self.interruptible.store(false, Ordering::SeqCst);

        return Ok(ChatResponse {
            response,
            session_id: session_id.to_string(),
            token_usage: TokenUsage { prompt_tokens, completion_tokens },
        });
    }

    // === Agent Loop ===

    async fn agent_loop(&self, msg: &ChatMessage, session_id: &str) -> Result<ChatResponse> {
        *self.agent_state.write().await = AgentState::Thinking;
        self.interruptible.store(true, Ordering::SeqCst);

        let mut history: Vec<ChatMessage> = self.get_working_memory(session_id).unwrap_or_default();
        history.push(msg.clone());

        // Build skill list string for system prompt injection
        let skill_list = self.build_skill_list_string();

        let mut all_tool_messages: Vec<ChatMessage> = Vec::new();
        let mut last_response = String::new();

        let outcome = tokio::time::timeout(
            std::time::Duration::from_secs(120),
            async {
                for _iteration in 0..3 {
                    let persona_summary = self.persona.summarize().await.unwrap_or_default();
                    let system_with_tools = SYSTEM_INSTRUCTION.replace("[tools]", &skill_list);
                    let assembled = self.weaver.assemble(
                        &system_with_tools, "", &persona_summary, None, &history,
                    );

                    let response = self.invoke_model_raw(&assembled.prompt).await?;

                    if let Some(tool_call) = self.parse_tool_call(&response) {
                        *self.agent_state.write().await = AgentState::Acting;
                        let result = match self.execute_tool(&tool_call).await {
                            Ok(output) => output,
                            Err(e) => format!("Tool error: {}", e),
                        };
                        let ts = Utc::now();
                        history.push(ChatMessage { role: "assistant".into(), content: response.clone(), timestamp: ts });
                        history.push(ChatMessage { role: "tool".into(), content: result.clone(), timestamp: ts });
                        all_tool_messages.push(ChatMessage { role: "assistant".into(), content: response, timestamp: ts });
                        all_tool_messages.push(ChatMessage { role: "tool".into(), content: result, timestamp: ts });
                    } else {
                        last_response = response;
                        break;
                    }
                }
                Ok::<_, anyhow::Error>(last_response)
            },
        ).await;

        *self.agent_state.write().await = AgentState::Idle;
        self.interruptible.store(false, Ordering::SeqCst);

        match outcome {
            Ok(Ok(ref response)) if response.is_empty() => {
                Err(anyhow::anyhow!("Agent loop produced no response after 3 iterations"))
            }
            Ok(Ok(response)) => {
                let _ = self.save_all_messages(session_id, msg, &all_tool_messages, &response);

                let prompt_tokens = self.inference.token_count(&msg.content);
                let completion_tokens = self.inference.token_count(&response);
                let _ = self.event_bus.send(EngineEvent::TokenUsage {
                    session_id: session_id.to_string(),
                    model: "agent-loop".into(),
                    prompt_tokens,
                    completion_tokens,
                });
                Ok(ChatResponse {
                    response,
                    session_id: session_id.to_string(),
                    token_usage: TokenUsage { prompt_tokens, completion_tokens },
                })
            }
            Ok(Err(e)) => Err(e),
            Err(_elapsed) => Err(anyhow::anyhow!("Agent loop timed out after 120s")),
        }
    }

    fn build_skill_list_string(&self) -> String {
        let skills = self.skills.list_all();
        if skills.is_empty() {
            return "None".into();
        }
        skills.iter()
            .map(|s| format!("{}: {}", s.name, s.description))
            .collect::<Vec<_>>()
            .join(", ")
    }

    async fn invoke_model_raw(&self, prompt: &str) -> Result<String> {
        if let Some(ref cloud) = self.cloud {
            match cloud.complete(CompletionRequest { prompt: prompt.to_string(), ..Default::default() }).await {
                Ok(r) => return Ok(r.text),
                Err(e) => tracing::warn!("Cloud failed, falling back to local: {}", e),
            }
        }
        self.inference.complete(CompletionRequest { prompt: prompt.to_string(), ..Default::default() })
            .await
            .map(|r| r.text)
    }

    fn parse_tool_call(&self, response: &str) -> Option<ToolCall> {
        let trimmed = response.trim();
        if let Some(start) = trimmed.find("{\"tool\"") {
            let slice = &trimmed[start..];
            let mut depth = 0;
            let mut end = 0;
            for (i, ch) in slice.char_indices() {
                match ch {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            end = i;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            if end > 0 {
                let json_str = &slice[..=end];
                match serde_json::from_str::<ToolCall>(json_str) {
                    Ok(call) => return Some(call),
                    Err(e) => {
                        tracing::warn!("Failed to parse tool call JSON: {} — raw: {}", e, json_str);
                        return None;
                    }
                }
            }
        }
        None
    }

    async fn execute_tool(&self, call: &ToolCall) -> Result<String> {
        self.skills.invoke(&call.tool, call.params.clone()).await.map(|v| v.to_string())
    }

    // === Message persistence (non-fatal) ===

    fn save_messages(&self, session_id: &str, user_msg: &ChatMessage, assistant_response: &str) -> Result<()> {
        let conn = match rusqlite::Connection::open(&self.db_path) {
            Ok(c) => c,
            Err(e) => { tracing::error!("save_messages: {}", e); return Ok(()); }
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

    fn save_all_messages(&self, session_id: &str, user_msg: &ChatMessage, tool_msgs: &[ChatMessage], final_response: &str) -> Result<()> {
        let conn = match rusqlite::Connection::open(&self.db_path) {
            Ok(c) => c,
            Err(e) => { tracing::error!("save_all_messages: {}", e); return Ok(()); }
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

    fn get_working_memory(&self, session_id: &str) -> Result<Vec<ChatMessage>> {
        match rusqlite::Connection::open(&self.db_path) {
            Ok(conn) => {
                let store = MessageStore::new(&conn);
                store.recent(session_id, 20)
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
        HealthStatus { status: "ok".into(), uptime: 0, gpu_info: gpu }
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

    pub async fn persona_summary(&self) -> String {
        self.persona.summarize().await.unwrap_or_default()
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

    pub async fn agent_state(&self) -> AgentState {
        self.agent_state.read().await.clone()
    }

    pub async fn shutdown(&self) -> Result<()> {
        tracing::info!("engine shutting down");
        Ok(())
    }
}

// === Tests ===

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
        let msg = ChatMessage { role: "user".into(), content: "hello".into(), timestamp: Utc::now() };
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
        let msg = ChatMessage { role: "user".into(), content: "hello".into(), timestamp: Utc::now() };
        let sid = engine.create_session().await.unwrap().id;
        engine.handle_message(msg, &sid).await.unwrap();
        let event = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv()).await;
        assert!(event.is_ok(), "should receive TokenUsage event");
    }

    #[tokio::test]
    async fn test_handle_message_skill_path() {
        let (engine, _dir) = setup_test_engine().await;
        let msg = ChatMessage { role: "user".into(), content: "帮我管理文件".into(), timestamp: Utc::now() };
        let sid = engine.create_session().await.unwrap().id;
        let resp = engine.handle_message(msg, &sid).await.unwrap();
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
        let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
        let state = rt.block_on(engine.agent_state());
        assert_eq!(state, AgentState::Idle);
    }
}

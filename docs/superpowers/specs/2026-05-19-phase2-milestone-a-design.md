# Phase 2 Milestone A: Infrastructure + Smart Prompts — 设计规范

**版本:** 1.0
**日期:** 2026-05-19
**状态:** 设计完成，待实现
**前置:** Phase 1 (已完成), Phase 1 patches (已完成)

---

## 1. 目标

搭建 Phase 2 的基础设施层，实现 "智能提示"——让引擎从裸传用户文本升级为 4 合 1 上下文编织。

**核心交付:**
- 云端模型接入（Anthropic / OpenAI / DeepSeek）
- 多会话持久化（跨重启保留）
- 认知写入（规则版 CognitionUpdate → cognitions 表）
- Context Weaver（4 步 prompt 组装）
- KV Cache trait 契约（Phase 3 填入实现）

**不做:**
- Persona Projector（Milestone B，等 cognitions 有数据）
- Agent Loop（Milestone B）
- 8B LLM 认知提取（Milestone C）
- KV Cache 磁盘持久化（Phase 3）

---

## 2. Crate 变更

### 2.1 新增 Crate

| Crate | 路径 | 职责 |
|-------|------|------|
| `cache` | `crates/cache/` | `KvCache` trait + `NoopCache` stub（Phase 3 填入真实实现） |
| `weaver` | `crates/weaver/` | `ContextWeaver` — 4 步 prompt 组装 + `PersonaProvider` trait stub |

### 2.2 现有 Crate 变更

| Crate | 变更 |
|-------|------|
| `models` | `TargetModel` 加回 `Cloud`；`ModelConfig` 加 `model` 字段；加 `PersonaProvider` trait |
| `inference` | 新增 `CloudClient` trait + `AnthropicClient` + `OpenAIClient` |
| `memory` | `store.rs` 新增 `CognitionStore` / `SessionStore` / `TokenStore`；`pipeline.rs` 认知触发改为调用 `CognitionStore::insert()` |
| `engine` | 加 `cloud: Option<Arc<dyn CloudClient>>`；session 改为专用线程+通道；`handle_message` 集成 Weaver；加 `list_skills`/`invoke_skill`（已有） |
| `server` | 新增 JSON-RPC：`session.list`/`session.create`、`memory.cognitions`、`agent.status` |
| `cli` | 新增 `session list`/`session create`、`memory cognitions` |

---

## 3. 详细设计

### 3.1 crates/models/ — 类型扩展

```rust
// === TargetModel 加回 Cloud ===
pub enum TargetModel {
    Local,
    None,
    Cloud,  // NEW: route to cloud API
}

// === ModelConfig 加 model 字段 ===
pub struct ModelConfig {
    pub name: String,
    pub model_type: ModelType,
    pub backend: ModelBackend,
    pub model: Option<String>,       // NEW: "claude-sonnet-4-6" etc.
    pub path: Option<String>,
    pub context_size: usize,
    pub n_gpu_layers: usize,
    pub api_key_env: Option<String>,
}

// === PersonaProvider trait (Milestone A stub, Milestone B 真实实现) ===
#[async_trait]
pub trait PersonaProvider: Send + Sync {
    async fn summarize(&self) -> Result<String>;  // ~50 token one-sentence profile
}
```

### 3.2 crates/inference/ — CloudClient trait

```rust
use reqwest::Client;
use async_trait::async_trait;
use tokio::sync::mpsc;

#[async_trait]
pub trait CloudClient: Send + Sync {
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse>;
    async fn complete_stream(&self, req: CompletionRequest, tx: mpsc::Sender<String>) -> Result<()>;
    fn provider(&self) -> ModelBackend;
    fn model_name(&self) -> &str;
}

pub struct AnthropicClient {
    api_key: String,
    model: String,
    http: Client,
}

pub struct OpenAIClient {
    api_key: String,
    model: String,
    base_url: String,   // https://api.openai.com or https://api.deepseek.com
    http: Client,
}

impl CloudClient for AnthropicClient { ... }
impl CloudClient for OpenAIClient { ... }

/// Factory
pub fn create_cloud_client(config: &ModelConfig) -> Result<Box<dyn CloudClient>> {
    let api_key = std::env::var(config.api_key_env.as_deref().unwrap_or(""))
        .context("API key not found")?;
    let model = config.model.clone().unwrap_or_default();
    match config.backend {
        ModelBackend::Anthropic => Ok(Box::new(AnthropicClient::new(api_key, model))),
        ModelBackend::OpenAI => Ok(Box::new(OpenAIClient::new(api_key, model, "https://api.openai.com/v1".into()))),
        ModelBackend::DeepSeek => Ok(Box::new(OpenAIClient::new(api_key, model, "https://api.deepseek.com/v1".into()))),
        ModelBackend::LlamaCpp => anyhow::bail!("LlamaCpp is not a cloud backend"),
    }
}
```

**错误处理:** 每个 `complete()` 内部实现指数退避重试（最多 3 次），返回 `Err` 时由 Engine 决定是否降级到备用模型。

### 3.3 crates/memory/ — V2 表读写层

```rust
// === CognitionStore ===
pub struct CognitionStore { conn: Connection }

impl CognitionStore {
    pub fn new(conn: Connection) -> Self;
    pub fn insert(&self, subject: &str, trait_name: &str, value: &str, confidence: f64, evidence_count: usize) -> Result<i64>;
    pub fn query_by_subject(&self, subject: &str, limit: usize) -> Result<Vec<CognitionRow>>;
    pub fn latest_version(&self, subject: &str, trait_name: &str) -> Option<i64>;
}

// === SessionStore ===
pub struct SessionStore { conn: Connection }

impl SessionStore {
    pub fn new(conn: Connection) -> Self;
    pub fn insert(&self, id: &str, created_at: &str) -> Result<()>;
    pub fn list_all(&self, limit: usize) -> Result<Vec<SessionInfo>>;
    pub fn update_message_count(&self, id: &str, count: usize) -> Result<()>;
}

// === TokenStore ===
pub struct TokenStore { conn: Connection }

impl TokenStore {
    pub fn insert(&self, session_id: &str, model: &str, prompt_tokens: usize, completion_tokens: usize, latency_ms: u64) -> Result<()>;
    pub fn query_by_session(&self, session_id: &str, limit: usize) -> Result<Vec<TokenUsageRow>>;
    pub fn total_usage(&self) -> Result<(usize, usize)>;  // (prompt, completion)
}
```

这些 Store 不管理 Connection 生命周期——由调用方（Engine 的 memory_thread 和 session_thread）传入 Connection。

### 3.4 crates/cache/ — KvCache trait

```rust
// crates/cache/src/lib.rs

pub struct CachedPrefix {
    pub blocks: Vec<u8>,       // Q4 KV blocks (Phase 3)
    pub token_count: usize,
}

pub trait KvCache: Send + Sync {
    fn lookup(&self, prefix_hash: u64) -> Option<CachedPrefix>;
    fn store(&self, prefix_hash: u64, blocks: CachedPrefix);
}

/// Phase 2 stub: always returns None (cache miss), silently discards stores
pub struct NoopCache;

impl KvCache for NoopCache {
    fn lookup(&self, _hash: u64) -> Option<CachedPrefix> { None }
    fn store(&self, _hash: u64, _blocks: CachedPrefix) { /* noop */ }
}
```

### 3.5 crates/weaver/ — Context Weaver

```rust
// crates/weaver/src/lib.rs

pub struct ContextWeaver {
    cache: Arc<dyn KvCache>,
    persona: Arc<dyn PersonaProvider>,
}

pub struct AssembledPrompt {
    pub prompt: String,
    pub static_prefix_len: usize,  // for cache alignment
}

impl ContextWeaver {
    pub fn new(cache: Arc<dyn KvCache>, persona: Arc<dyn PersonaProvider>) -> Self;

    /// 4-step assembly:
    /// 1. KV Cache lookup (stub: always miss → compute prefix inline)
    /// 2. Persona summary (stub: empty string in Milestone A)
    /// 3. Skill context.md injection (≤200 tokens)
    /// 4. Working memory assembly (~200 tokens)
    pub fn assemble(
        &self,
        system_instruction: &str,
        user_message: &str,
        skill_context: Option<&str>,
        working_memory: &[ChatMessage],
    ) -> AssembledPrompt;
}
```

**编织策略（对齐 Phase 3 缓存）:**
- 静态前缀放最前面：`system_instruction` + `persona_summary`（Phase 3 缓存这部分 KV）
- 动态内容放最后面：`skill_context` + `working_memory` + `user_message`
- 通过 append 而非 modify 保护缓存

### 3.6 crates/engine/ — Engine 变更

```rust
pub struct Engine {
    router: SmartRouter,
    skills: SkillRegistry,
    inference: Arc<InferenceEngine>,
    cloud: Option<Arc<dyn CloudClient>>,        // NEW
    weaver: ContextWeaver,                       // NEW
    memory_tx: mpsc::Sender<ProcessRequest>,
    session_tx: mpsc::Sender<SessionCommand>,    // NEW (replaces HashMap)
    sessions: Arc<RwLock<HashMap<String, SessionInfo>>>,  // kept as cache
    event_bus: broadcast::Sender<EngineEvent>,
}

enum SessionCommand {
    Create { reply: oneshot::Sender<SessionInfo> },
    List { reply: oneshot::Sender<Vec<SessionInfo>> },
    UpdateCount { id: String, count: usize },
}
```

**handle_message 变更:**
```rust
async fn handle_message(&self, msg: ChatMessage, session_id: &str) -> Result<ChatResponse> {
    let out = self.router.classify_sync(&msg.content);

    // Get skill context if matched
    let skill_ctx = out.skill_match.as_ref().and_then(|name| {
        self.skills.find_by_name(name).map(|s| s.context_md())
    });

    // Assemble prompt via Weaver
    let assembled = self.weaver.assemble(
        SYSTEM_INSTRUCTION,
        &msg.content,
        skill_ctx,
        &self.get_working_memory(session_id)?,
    );

    // Route: Cloud vs Local
    let response = match out.target_model {
        TargetModel::None => {
            let name = out.skill_match.as_ref().unwrap();
            self.skills.invoke(name, serde_json::json!({"text": msg.content})).await?.to_string()
        }
        TargetModel::Local => {
            self.inference.complete(CompletionRequest {
                prompt: assembled.prompt,
                ..Default::default()
            }).await?.text
        }
        TargetModel::Cloud => {
            if let Some(ref cloud) = self.cloud {
                cloud.complete(CompletionRequest {
                    prompt: assembled.prompt,
                    ..Default::default()
                }).await?.text
            } else {
                // Fallback to local
                self.inference.complete(CompletionRequest {
                    prompt: assembled.prompt,
                    ..Default::default()
                }).await?.text
            }
        }
    };

    // Background memory + token recording (unchanged)
    // ...
}
```

**Session 线程（同 memory_thread 模式）:**
```
Engine (async) ──mpsc::channel──▶ SessionThread (std::thread, owns Connection + SessionStore)
  create_session()                  oneshot reply
  list_sessions()
```

### 3.7 规则版 Cognition Updater

现有 `memory_thread.rs` 已经广播 `EngineEvent::CognitionUpdated`。Milestone A 新增：收到 cognition 触发时，**同时写入 cognitions 表**。

```rust
// memory_thread.rs 改造
if let Some(cog) = result.cognition_triggered {
    // 写入 cognitions 表
    let _ = cognition_store.insert(
        "USER",
        &cog.trait_name,
        &cog.summary,
        cog.confidence,
        cog.evidence_count,
    );
    // 广播事件（已有）
    let _ = event_tx.send(EngineEvent::CognitionUpdated { ... });
}
```

### 3.8 JSON-RPC 新增方法

| 方法 | 说明 |
|------|------|
| `session.list` | `{}` → `{sessions: [{id, created_at, message_count}]}` |
| `session.create` | `{}` → `{id, created_at}` |
| `memory.cognitions` | `{subject?, limit?}` → `{cognitions: [{trait, value, confidence}]}` |
| `agent.status` | `{}` → `{state: "idle", active_session, model_info}` |

### 3.9 CLI 新增命令

| 命令 | 说明 |
|------|------|
| `openloom session list` | 列出所有会话 |
| `openloom session create` | 创建新会话 |
| `openloom memory cognitions [--subject USER]` | 查看认知图谱 |

---

## 4. 依赖关系图

```
cli → engine + server
server → engine + models
engine → weaver + router + skills + inference + memory + cache + models
weaver → cache + models
inference → models (CloudClient trait in inference)
cache → (独立，无依赖)
memory → models
```

---

## 5. 测试策略

| 层级 | 内容 |
|------|------|
| 单元测试 | CloudClient mock 测试；Weaver 组装正确性（4 步 prompt 拼接）；SessionStore/CognitionStore CRUD；NoopCache 返回 None |
| 集成测试 | Engine handle_message 经 Weaver → Cloud/Local 全链路；Session 线程 create/list 往返；Cognition 触发 → 表写入 → 查询 |
| Clippy | `-D warnings` 零警告 |

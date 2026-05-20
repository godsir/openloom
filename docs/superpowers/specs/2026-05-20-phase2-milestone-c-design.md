# Phase 2 Milestone C: Backend Completion — 设计规范

**版本:** 1.0
**日期:** 2026-05-20
**状态:** 设计完成，待实现
**前置:** Phase 2 Milestone B (已完成)

---

## 1. 目标

补齐 Phase 2 所有后端缺口：类型系统完整性、JSON-RPC 全方法真实返回、CLI 全命令实装、WebSocket 推送通知、信号处理与优雅关闭、配置系统读写。

**核心交付:**
- 类型系统：AgentStateChanged 使用强类型、ClassifyOutput 加路由理由、TokenUsage 加 cached_tokens/latency_ms、AppConfig 扩展
- JSON-RPC：memory.query / cache.stats / config.get / config.set / system.shutdown 全部真实实现
- WebSocket 推送：Server 订阅 EventBus，4 种通知推送到客户端
- CLI 实装：skill list / memory events / config get/set 返回真实数据
- 运维基础：SIGTERM/SIGINT 信号处理、Engine::shutdown() 真正清理、请求限流、模型文件存在性检查
- 配置系统：config.toml 读写、示例文件

**明确不做（归入 Phase 3）:**
- llama-cpp 真实模型加载（需 GGUF 文件分发方案）
- 8B LLM 认知提取
- KV Cache 磁盘持久化
- WASM Skill 安装/卸载
- 权限模型强制执行
- Config 热重载

---

## 2. Crate 变更

| Crate | 变更 |
|-------|------|
| `models` | TokenUsage 加 cached_tokens/latency_ms；AgentStateChanged 改为 AgentState 枚举；ClassifyOutput 加 route_reason；AppConfig 加 cache/agent/persona 段；StoragePrefs.data_dir 改为 PathBuf；CompletionRequest 加 top_p/stop；新增 RateLimitConfig |
| `inference` | CompletionRequest 加 top_p/stop 字段；CompletionResponse 加 latency_ms |
| `memory` | store.rs 暴露事件查询 API；TokenStore 已在 store.rs 中定义，无需变更 |
| `engine` | TokenStore::insert 在 handle_message 中调用；EngineConfig 加 rate_limit 字段；Engine 加 start_time 字段计算 uptime；shutdown() 真实清理；model 文件存在性检查 |
| `server` | WebSocket handler 订阅 EventBus 推送通知；dispatch.rs 补齐 5 个 stub 方法 |
| `cli` | skill list 查询 engine；memory events 查询 event store；config get/set 读写 config.toml |
| `cache` | 加 stats() 方法到 KvCache trait（NoopCache 返回零值） |

---

## 3. 详细设计

### 3.1 类型系统修复

#### T1: AgentStateChanged 用 AgentState 枚举

```rust
// models/lib.rs — EngineEvent::AgentStateChanged
AgentStateChanged {
    old_state: AgentState,  // was: String
    new_state: AgentState,  // was: String
},
```

Engine 中构造 AgentStateChanged 事件时传入 `AgentState` 枚举值而非字符串。

#### H2: TokenUsage 扩展

```rust
pub struct TokenUsage {
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub cached_tokens: usize,   // NEW
    pub latency_ms: u64,        // NEW
}
```

Engine handle_message() 中记录 `Instant::now()` 在调用前后，计算 latency_ms。

#### T3: ClassifyOutput 加 route_reason

```rust
pub struct ClassifyOutput {
    pub intent: Intent,
    pub complexity: f32,
    pub skill_match: Option<String>,
    pub confidence: f32,
    pub cache_hit: bool,
    pub target_model: TargetModel,
    pub route_reason: String,   // NEW: "keyword_match" | "skill_trigger" | "cloud_fallback" | "default"
}
```

Router classify_sync() 中根据实际走的分支填入 route_reason。

#### T7: AppConfig 扩展

```rust
pub struct AppConfig {
    pub models: Vec<ModelConfig>,
    pub router: RouterPrefs,
    pub server: ServerPrefs,
    pub storage: StoragePrefs,
    pub logging: LoggingPrefs,
    pub cache: CachePrefs,       // NEW
    pub agent: AgentPrefs,       // NEW
    pub persona: PersonaPrefs,   // NEW
}

pub struct CachePrefs {
    pub block_size: usize,       // default: 1024
    pub max_blocks: usize,       // default: 32
    pub total_budget_mb: usize,  // default: 5120
}

pub struct AgentPrefs {
    pub max_iterations: usize,   // default: 3
    pub timeout_secs: u64,       // default: 120
}

pub struct PersonaPrefs {
    pub top_n: usize,            // default: 5
    pub recency_decay_days: u32, // default: 30
}
```

#### T8: StoragePrefs.data_dir → PathBuf

```rust
pub struct StoragePrefs {
    pub data_dir: PathBuf,   // was: Option<String>
}
```

#### CompletionRequest 扩展

```rust
// inference/src/lib.rs
pub struct CompletionRequest {
    pub prompt: String,
    pub max_tokens: usize,
    pub temperature: f32,
    pub top_p: f32,            // NEW, default 1.0
    pub stop: Vec<String>,     // NEW, default empty
    pub stream: bool,
}
```

#### Engine struct 完整定义（变更后）

新增字段一览：

```rust
pub struct Engine {
    // Existing (unchanged)
    router: SmartRouter,
    skills: SkillRegistry,
    inference: Arc<InferenceEngine>,
    cloud: Option<Arc<dyn CloudClient>>,
    weaver: ContextWeaver,
    persona: Arc<dyn PersonaProvider>,
    memory_tx: mpsc::Sender<memory_thread::ProcessRequest>,
    session_tx: mpsc::Sender<SessionCommand>,
    event_bus: broadcast::Sender<EngineEvent>,
    agent_state: Arc<RwLock<AgentState>>,
    interruptible: AtomicBool,
    db_path: PathBuf,

    // NEW in Milestone C
    config: Arc<RwLock<AppConfig>>,           // config.get/set 读写
    start_time: Instant,                      // health_check uptime
    draining: AtomicBool,                     // shutdown 排空标记
    in_flight: AtomicUsize,                   // 当前处理中的请求数
    rate_limiter: Mutex<RateLimiter>,         // token bucket 限流
    token_store_tx: mpsc::Sender<TokenUsageRecord>,  // token 持久化线程
    model_available: bool,                    // 模型文件存在性
}
```

### 3.2 JSON-RPC 补齐

#### memory.query

实现 FTS5 全文搜索 events 表：

```rust
// dispatch.rs
"memory.query" => {
    let query = params.as_ref()
        .and_then(|p| p.get("query"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let limit = params.as_ref()
        .and_then(|p| p.get("limit"))
        .and_then(|v| v.as_u64())
        .unwrap_or(20) as usize;
    let events = engine.search_events(query, limit).await?;
    Ok(serde_json::json!({"events": events, "cognitions": []}))
}
```

Engine 新增 `search_events()` 方法，打开 SQLite 连接执行 `SELECT * FROM events_fts WHERE events_fts MATCH ? LIMIT ?`。

#### cache.stats

KvCache trait 加 `stats()` 方法：

```rust
pub trait KvCache: Send + Sync {
    fn lookup(&self, prefix_hash: u64) -> Option<CachedPrefix>;
    fn store(&self, prefix_hash: u64, blocks: CachedPrefix);
    fn stats(&self) -> CacheStats;  // NEW
}

pub struct CacheStats {
    pub hit_rate: f64,
    pub block_count: usize,
    pub total_size_mb: f64,
}
```

NoopCache::stats() 返回全零。Phase 3 真实实现返回实际值。

#### config.get / config.set

Engine 持有 `Arc<RwLock<AppConfig>>`，初始化时从 config.toml 加载：

```rust
// dispatch.rs
"config.get" => {
    let key = params.as_ref().and_then(|p| p.get("key")).and_then(|v| v.as_str());
    let config = engine.get_config(key).await;
    Ok(serde_json::to_value(config)?)
}
"config.set" => {
    let key = params["key"].as_str().unwrap_or("");
    let value = params["value"].clone();
    engine.set_config(key, value).await?;
    Ok(serde_json::json!({"ok": true}))
}
```

Engine::set_config() 修改内存配置并写入 config.toml。

**Server 端 config 加载：** `Server::new()` 接收 config_path，在 `serve()` 中调用 `load_config()` 加载 `AppConfig`，存入 `Engine.config`。CLI 的 `Serve` 命令将 `--config` 参数传递给 `Server::new()`：

```rust
// server/src/lib.rs
impl Server {
    pub fn new(engine: Engine, config_path: Option<PathBuf>) -> Self { ... }
}

// cli/main.rs Commands::Serve
let engine = Engine::new(EngineConfig {
    data_dir,
    threshold: app_config.agent.max_iterations,
    cloud_config: app_config.models.iter().find(|m| m.backend != ModelBackend::LlamaCpp).cloned(),
})?;
let server = Server::new(engine, config.as_ref().map(PathBuf::from));
server.serve(port).await?;
```

#### system.shutdown

```rust
"system.shutdown" => {
    engine.shutdown().await?;
    Ok(serde_json::json!({"ok": true}))
}
```

### 3.3 WebSocket 推送通知

Server 启动时 spawn 一个后台 task，监听 EventBus 并将事件转为 JSON-RPC Notification（无 id 字段）推送：

```rust
// server/src/ws.rs 新增
fn spawn_event_pusher(engine: Arc<Engine>) -> broadcast::Sender<EngineEvent> {
    let mut rx = engine.subscribe();
    let (tx, _) = broadcast::channel(256);
    let push_tx = tx.clone();
    tokio::spawn(async move {
        while let Ok(event) = rx.recv().await {
            let notification = event_to_notification(&event);
            let _ = push_tx.send(notification);
        }
    });
    tx
}

fn event_to_notification(event: &EngineEvent) -> String {
    match event {
        EngineEvent::CognitionUpdated { trait_name, new_value, confidence, .. } => {
            serde_json::json!({
                "jsonrpc": "2.0",
                "method": "cognition.updated",
                "params": { "trait": trait_name, "new_value": new_value, "confidence": confidence }
            }).to_string()
        }
        EngineEvent::AgentStateChanged { old_state, new_state } => {
            serde_json::json!({
                "jsonrpc": "2.0",
                "method": "agent.state_changed",
                "params": { "old_state": old_state, "new_state": new_state }
            }).to_string()
        }
        // ... TokenUsage, Error 同理
    }
}
```

WebSocket handler 同时监听两个 channel：客户端消息 + 推送通知，用 `tokio::select!` 合并。

### 3.4 CLI 实装

#### skill list

```rust
SkillAction::List => {
    let engine = build_engine(&config_path)?;
    let skills = engine.list_skills();
    for s in &skills {
        println!("{} - {} (triggers: {:?})", s.name, s.description, s.triggers);
    }
}
```

#### memory events

```rust
MemoryAction::Events { limit } => {
    let engine = build_engine(&config_path)?;
    let events = engine.list_events(limit).await?;
    for e in &events {
        println!("[{}] {}: {} (conf: {:.0}%)",
            e.timestamp, e.event_type, e.action, e.confidence * 100.0);
    }
}
```

Engine 新增 `list_events()` 方法，查询 events 表。

#### config get/set

```rust
ConfigAction::Get { key } => {
    let config = load_config(None);
    match key {
        Some(k) => println!("{} = {:?}", k, config.get_nested(&k)),
        None => println!("{}", toml::to_string_pretty(&config)?),
    }
}
ConfigAction::Set { key, value } => {
    let mut config = load_config(None);
    config.set_nested(&key, &value)?;
    std::fs::write(config_path(None), toml::to_string(&config)?)?;
    println!("{} = {}", key, value);
}
```

### 3.5 信号处理与关闭

```rust
// engine/lib.rs
impl Engine {
    pub async fn shutdown(&self) -> Result<()> {
        tracing::info!("engine shutting down");

        // 1. Set draining flag (reject new requests)
        self.draining.store(true, Ordering::SeqCst);

        // 2. Wait for in-flight requests to complete (max 5s)
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while self.in_flight.load(Ordering::SeqCst) > 0 {
            if tokio::time::Instant::now() > deadline {
                tracing::warn!("shutdown timeout, forcing close");
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        // 3. Close SQLite WAL checkpoint
        if let Ok(conn) = rusqlite::Connection::open(&self.db_path) {
            let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
        }

        // 4. Drop inference engine (frees GPU memory)
        // (handled by Arc drop)

        tracing::info!("engine shutdown complete");
        Ok(())
    }
}
```

CLI 和 Server 入口注册信号 handler：

```rust
// cli/main.rs
tokio::spawn(async move {
    tokio::signal::ctrl_c().await.ok();
    tracing::info!("SIGINT received");
    engine.shutdown().await.ok();
    std::process::exit(0);
});
```

### 3.6 请求限流

```rust
// engine/lib.rs — Engine struct 新增字段
rate_limiter: tokio::sync::Mutex<RateLimiter>,

struct RateLimiter {
    last_request: Instant,
    min_interval: Duration,  // default 100ms
}

impl RateLimiter {
    fn check(&mut self) -> Result<()> {
        let elapsed = self.last_request.elapsed();
        if elapsed < self.min_interval {
            anyhow::bail!("rate limit exceeded, retry in {}ms",
                (self.min_interval - elapsed).as_millis());
        }
        self.last_request = Instant::now();
        Ok(())
    }
}
```

### 3.7 模型文件检查

Engine::new() 中检查 GGUF 模型文件是否存在，不存在则 `tracing::warn!` 并设置 `model_available: false`。后续请求走 `TargetModel::None`（skill 直执行）或 Cloud fallback。

### 3.8 Token 使用持久化

handle_message() 返回前写入 `token_usage` 表：

```rust
let _ = self.token_store_tx.send(TokenUsageRecord {
    session_id: session_id.to_string(),
    model: "qwen3-1.7b".into(),
    prompt_tokens,
    completion_tokens,
    cached_tokens: 0,
    latency_ms,
});
```

新增 token_store 后台线程（同 session_thread 模式），接收 TokenUsageRecord 并调用 TokenStore::insert()。

---

## 4. 数据流

```
用户消息 → Engine::handle_message()
  ├── rate_limiter.check()
  ├── Router::classify_sync() → ClassifyOutput (含 route_reason)
  ├── agent_loop / direct response
  ├── TokenStore::insert() (持久化 token 使用)
  ├── EventBus 广播 (TokenUsage, CognitionUpdated, AgentStateChanged)
  │     └── WebSocket push task → JSON-RPC Notification → 客户端
  └── return ChatResponse

信号 (SIGTERM/SIGINT) → Engine::shutdown()
  ├── draining flag
  ├── 排空 in-flight 请求
  ├── SQLite WAL checkpoint
  └── std::process::exit(0)
```

---

## 5. 错误处理

| 场景 | 策略 |
|------|------|
| config.toml 解析失败 | 降级使用默认值，tracing::warn! |
| FTS5 搜索失败 | 返回空结果，tracing::warn! |
| token_usage 写入失败 | 非致命，tracing::warn! |
| shutdown 排空超时 | 5 秒超时后强制关闭 |
| 模型文件不存在 | tracing::warn!，走 skill 直执行或 Cloud fallback |
| 限流触发 | 返回 Error "rate limit exceeded" |

---

## 6. 测试策略

| 层级 | 内容 |
|------|------|
| 单元测试 | TokenUsage 序列化(含新字段)；ClassifyOutput route_reason；AppConfig 默认值(含新段)；RateLimiter 拒绝/放行；AgentStateChanged 序列化 |
| 集成测试 | config get/set 读写往返；FTS5 搜索事件；WebSocket 通知推送；信号处理 graceful shutdown |
| Clippy | `-D warnings` 零警告 |

---

## 7. 文件结构

```
F:/openLoom/
├── crates/
│   ├── models/src/lib.rs              ← [Modify] T1,H2,T3,T7,T8 + RateLimitConfig
│   ├── inference/src/lib.rs           ← [Modify] CompletionRequest + top_p/stop
│   ├── memory/src/store.rs            ← [Modify] + EventRow, query_fts()
│   ├── cache/src/lib.rs               ← [Modify] + CacheStats, KvCache::stats()
│   ├── engine/src/
│   │   ├── lib.rs                     ← [Modify] + rate_limiter, + start_time, + draining, + in_flight, + config, + model_check, + token_store thread
│   │   └── memory_thread.rs           ← 不变
│   ├── server/src/
│   │   ├── dispatch.rs                ← [Modify] + 5 method impls
│   │   ├── ws.rs                      ← [Modify] + event push notifications
│   │   └── lib.rs                     ← [Modify] + config loading
│   └── cli/src/main.rs                ← [Modify] skill list, memory events, config get/set live data
├── config.example.toml                ← [Create]
└── docs/superpowers/specs/
    └── 2026-05-20-phase2-milestone-c-design.md  ← 本文件
```

---

## 8. 依赖关系

```
server/push → Engine::subscribe() → EventBus
dispatch::config.get/set → Engine::get_config()/set_config() → AppConfig RwLock + config.toml write
dispatch::memory.query → Engine::search_events() → SQLite FTS5
dispatch::cache.stats → KvCache::stats() → NoopCache (Phase 3 real)
cli::skill list → Engine::list_skills()
cli::memory events → Engine::list_events() → SQLite events table
cli::config get/set → config.toml read/write
signal handler → Engine::shutdown() → drain + WAL checkpoint
```

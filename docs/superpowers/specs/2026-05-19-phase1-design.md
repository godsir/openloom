# Phase 1: Smart Router + Skill Engine — 设计规范

**版本:** 1.1
**日期:** 2026-05-19
**状态:** 设计完成，待实现
**前置:** Phase 0 Memory Kernel MVP (已完成)

---

## 1. 目标

实现 "80% 请求不动大模型" — 通过本地 Qwen3-1.7B 意图分类 + Skill 引擎懒加载，将大部分用户请求在 Router 或 Skill 层处理完毕，不触及云端大模型。

**核心指标:**
- Router 意图分类准确率 ≥ 80%
- 80% 请求由 Router + Skill 直接处理，不调云端模型
- Skill context.md 注入 ≤ 200 tokens（对比传统方案 15-20K 工具定义）
- Engine sidecar 崩溃自动恢复（5 次指数退避重试）

---

## 2. 架构总览

### 2.1 Crate 布局

```
crates/
├── memory/          ← Phase 0: 事件管线 (复用)
├── models/          ← 共享类型 (扩展)
├── inference/       ← NEW: llama-cpp-2 封装
├── router/          ← NEW: 意图分类 + 复杂度 + 技能匹配
├── skills/          ← NEW: Skill trait + Registry + CLI Bridge + builtins/
├── engine/          ← NEW: EventBus + 请求派发 (轻量)
├── server/          ← NEW: Axum HTTP + WS + JSON-RPC 2.0
└── cli/             ← CLI 入口 (扩展)

skills-repo/         ← 内置 Skill 模块 (Phase 2 WASM 产物，Phase 1 在 skills/builtins/)
```

### 2.2 依赖图

```
cli → engine + server
server → engine + models (显式)
engine → router + skills + inference + memory + models
router → inference + models
skills → models
inference → models
memory → models
```

### 2.3 请求处理流

```
用户输入 (CLI 或 Electron)
  │
  ▼
Router.classify(text)  [关键词优先→模型兜底]
  ├─ intent: Chat | FileOperation | WebSearch | CodeAssist | Schedule | ...
  ├─ complexity: 0.0 ~ 1.0
  ├─ skill_match: Some("file-manager") | None
  ├─ confidence: 0.0 ~ 1.0
  ├─ cache_hit: bool (Phase 3 启用，默认 false)
  └─ target_model: Local | None

  ├─ target_model == None (skill 匹配, 低复杂度)
  │    └─▶ SkillRegistry.invoke(skill_name, params) → 结果
  │
  └─ target_model == Local
       └─▶ InferenceEngine.complete(prompt) → LLM 响应
           │
           └─▶ [后台] memory_tx.send(ProcessRequest) → MemoryPipeline.process()
                └─ 事件提取 + 模式聚合 (独立线程，channel 通信)
```

**关于 Cloud 路径:** Phase 1 不支持云端模型。当 Router 置信度 < 0.7 时，降级到 `Local` 并附带低置信度标记，Phase 2 引入云端适配后扩展 `TargetModel::Cloud`。

---

## 3. Crate 详细设计

### 3.1 crates/models/ — 共享类型 (扩展)

**新增依赖:** `chrono = { version = "0.4", features = ["serde"] }`, `serde = { version = "1", features = ["derive"] }`, `serde_json = "1"`

**Phase 0 保留:**
```rust
enum ModelType { Router, Summarizer, Reasoning }
```

**Phase 0 扩展 (新增字段均有 Default):**
```rust
struct ModelConfig {
    pub name: String,
    pub model_type: ModelType,
    pub backend: ModelBackend,                    // NEW, default: LlamaCpp
    pub path: Option<String>,
    pub context_size: usize,
    pub n_gpu_layers: usize,                     // NEW, default: 0 (CPU only)
    pub api_key_env: Option<String>,             // NEW
}

enum ModelBackend { LlamaCpp, Anthropic, OpenAI, DeepSeek }
```

**Phase 1 新增类型:**
```rust
// === JSON-RPC 协议类型 ===
struct JsonRpcRequest { jsonrpc: String, method: String, params: Value, id: u64 }
struct JsonRpcResponse { jsonrpc: String, result: Option<Value>, error: Option<JsonRpcError>, id: u64 }
struct JsonRpcError { code: ErrorCode, message: String, data: Option<Value> }

enum ErrorCode {
    ParseError = -32700, InvalidRequest = -32600, MethodNotFound = -32601,
    InternalError = -32603, ModelUnavailable = -32000, SkillFailed = -32001,
    PermissionDenied = -32002, Timeout = -32003,
}

// === Router 类型 (从 models 导出，router 和 engine 共享) ===
enum Intent { Chat, FileOperation, WebSearch, CodeAssist, Schedule, Question, Other }
enum TargetModel { Local, None }  // Phase 2 加 Cloud

struct ClassifyOutput {
    intent: Intent,
    complexity: f32,
    skill_match: Option<String>,
    confidence: f32,
    cache_hit: bool,             // Phase 3, default false
    target_model: TargetModel,
}

// === 引擎类型 ===
struct ChatMessage { role: String, content: String }
struct ChatResponse { response: String, session_id: String, token_usage: TokenUsage }
struct TokenUsage { prompt_tokens: usize, completion_tokens: usize }
struct SessionInfo { id: String, created_at: DateTime<Utc>, message_count: usize }
struct HealthStatus { status: String, uptime: u64, gpu_info: GpuInfo }
struct SystemEvent { event_type: String, payload: Value }

// === GPU 信息 (inference 和 models 共享) ===
struct GpuInfo { vendor: String, vram_mb: u64, supported: bool }

// === 引擎事件 (广播给所有订阅者) ===
#[serde(rename_all = "snake_case")]
enum EngineEvent {
    CognitionUpdated { trait_name: String, old_value: String, new_value: String, confidence: f64 },
    AgentStateChanged { old_state: String, new_state: String },
    TokenUsage { session_id: String, model: String, prompt_tokens: usize, completion_tokens: usize },
    Error { code: ErrorCode, message: String, subsystem: String },
}
// serde 自动将 CamelCase 变体映射为 snake_case JSON 字段：
//   CognitionUpdated → "cognition_updated" → server 推送 "cognition.updated"
// server 将下划线替换为点号作为通知方法名

// === 配置类型 ===
struct AppConfig {
    models: Vec<ModelConfig>,
    router: RouterPrefs,
    server: ServerPrefs,
    storage: StoragePrefs,
    logging: LoggingPrefs,
}
struct RouterPrefs { keyword_threshold: f32, fallback_threshold: f32 }
struct ServerPrefs { host: String }          // port 由 --port 或 OS 分配
struct StoragePrefs { data_dir: PathBuf }    // SQLite 和日志根目录
struct LoggingPrefs { level: String, log_content: bool, dir: PathBuf }
```

### 3.2 crates/inference/ — 模型推理

**职责:** 封装 llama-cpp-2，加载 GGUF 模型，提供文本补全。

**依赖:** `llama-cpp-2 = "=0.1.146"`, `tokio = { version = "1", features = ["rt"] }`, openloom-models
**Feature flags:** `cuda`, `metal`, `vulkan` (workspace 级透传)

```rust
struct InferenceEngine { /* llama-cpp-2 context + model */ }

struct CompletionRequest {
    prompt: String,
    max_tokens: usize,        // default: 2048
    temperature: f32,         // default: 0.7
    stream: bool,             // default: false
}
impl Default for CompletionRequest { ... }

struct CompletionResponse { text: String, prompt_tokens: usize, completion_tokens: usize }

impl InferenceEngine {
    /// 加载 GGUF 模型。GPU 不可用时自动 CPU fallback (n_gpu_layers = 0)
    async fn load(model_path: &Path, n_gpu_layers: usize) -> Result<Self>;

    /// 同步补全
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse>;

    /// 流式补全，token 通过 mpsc::Sender 推送
    async fn complete_stream(&self, req: CompletionRequest, tx: mpsc::Sender<String>) -> Result<()>;

    /// 检测 GPU 信息
    fn detect_gpu() -> GpuInfo;  // GpuInfo 定义在 openloom_models

    /// 使用当前加载模型的 tokenizer 计算 token 数
    fn token_count(&self, text: &str) -> usize;
}
```

**错误恢复:**
- 模型加载失败 → `tracing::error!` + 返回 `Err`，由 Engine 决定是否降级
- 推理超时 (30s) → 取消推理，返回 `Err(Timeout)`，不 crash
- GPU 不可用 → 自动降级 CPU (n_gpu_layers = 0)，打印 warning

### 3.3 crates/router/ — 智能路由

**职责:** 意图分类 + 复杂度评分 + 技能匹配。关键词优先（零 token），未命中时调用 Qwen3-1.7B。

**依赖:** openloom-inference, `regex = "1"`, `serde = { version = "1", features = ["derive"] }`, openloom-models

```rust
struct KeywordRule {
    pattern: String,         // 正则或关键词
    intent: Intent,          // 从 models 导入
    confidence: f32,
}

struct RouterConfig {
    model_path: PathBuf,
    keyword_rules: Vec<KeywordRule>,
    keyword_threshold: f32,        // 关键词匹配置信度阈值，默认 0.85
    fallback_threshold: f32,       // 模型分类低置信度阈值，默认 0.7
}

impl SmartRouter {
    async fn new(config: RouterConfig, engine: Arc<InferenceEngine>) -> Result<Self>;

    /// 分类用户文本：关键词匹配 → 命中直接返回 → 未命中调 Qwen3-1.7B
    async fn classify(&self, text: &str) -> ClassifyOutput;

    /// 分类系统事件 (Phase 2 完整启用，Phase 1 提供类型契约)
    async fn classify_event(&self, event: &SystemEvent) -> ClassifyOutput;

    /// 注册技能触发词 (Engine::new() 中调用)
    fn register_skill_triggers(&mut self, name: &str, triggers: Vec<String>);
}
```

**内部流程:**
1. 关键词匹配 (零 token) → 置信度 ≥ `keyword_threshold` → 直接返回 `target_model = None`
2. 未命中 → 构造分类 prompt → Qwen3-1.7B 推理 (~50-100 tokens)
3. 模型置信度 ≥ `fallback_threshold` → `target_model = Local (or None with skill_match)`
4. 模型置信度 < `fallback_threshold` → `target_model = Local` + 低置信度标记 (Phase 1 无 Cloud)

**错误恢复:**
- 关键词无匹配且本地模型不可用 → `target_model = Local`, `intent = Chat`, 让 Engine 直接调推理
- 模型推理返回格式错误 → 规则引擎作为最终兜底，返回 `Chat / Local / 0.5`

### 3.4 crates/skills/ — 技能引擎

**职责:** Skill 注册/发现/执行 + CLI Bridge PATH 工具发现。

**依赖:** `serde = { version = "1", features = ["derive"] }`, `serde_json = "1"`, openloom-models, `wasmtime` (Phase 2)

**内置 Skill 源码路径:** `crates/skills/src/builtins/{file_manager,info_retriever,schedule_reminder,code_assistant,web_browser}.rs`

```rust
/// Skill trait — Phase 1 用 Rust native 实现，Phase 2 编译为 WASM
trait Skill: Send + Sync {
    fn name(&self) -> &str;
    fn manifest(&self) -> &SkillManifest;
    async fn invoke(&self, params: serde_json::Value) -> Result<serde_json::Value>;
    fn context_md(&self) -> &str;   // ≤ 200 tokens, 仅在激活时注入
}

struct SkillPermissions {
    fs_read: Vec<String>,
    fs_write: Vec<String>,
    network: Vec<String>,
    shell: bool,
    subprocess: bool,
    max_memory_mb: u32,
    max_runtime_sec: u32,
}

struct SkillManifest {
    name: String,
    description: String,
    triggers: Vec<String>,               // Router 匹配触发词
    permissions: SkillPermissions,       // Phase 2/3 强制执行
    min_engine_version: String,
}

struct SkillInfo { name: String, description: String, triggers: Vec<String> }

impl SkillRegistry {
    fn new() -> Self;
    fn register(&mut self, skill: Box<dyn Skill>);
    fn find_by_trigger(&self, text: &str) -> Option<&dyn Skill>;
    fn list_all(&self) -> Vec<SkillInfo>;
    async fn invoke(&self, name: &str, params: serde_json::Value) -> Result<serde_json::Value>;
}

impl CliBridge {
    fn discover_path_tools() -> Vec<CliTool>;
    fn parse_help(binary: &str) -> CliTool;
}

struct CliTool { name: String, description: String, binary: String }
```

**Phase 1 内置 Skill (Native Rust 实现):**
- `FileManager` — 文件读写/搜索/列表
- `InfoRetriever` — 信息检索/知识查询
- `ScheduleReminder` — 日程提醒
- `CodeAssistant` — 代码辅助
- `WebBrowser` — 网页浏览

**错误恢复:** Skill panic → 被 Engine 的 `catch_unwind` 捕获 (wasmtime 30s 超时于 Phase 2 启用)，返回 `Err(SkillFailed)` + 日志。

### 3.5 crates/engine/ — 编排引擎

**职责:** EventBus + 请求派发 + 生命周期管理。Phase 1 轻量版本，Phase 2 加入 Agent Loop。

**依赖:** `tokio = { version = "1", features = ["rt", "sync", "macros"] }`, openloom-router, openloom-skills, openloom-inference, openloom-memory, openloom-models

```rust
/// Engine 通过 channel 与 MemoryPipeline 通信，避免 Send 问题
/// (rusqlite::Connection 不是 Send，不能直接传给 spawn_blocking)
struct ProcessRequest {
    session_id: String,
    text: String,
    context: String,
}

struct Engine {
    router: SmartRouter,
    skills: SkillRegistry,
    inference: Arc<InferenceEngine>,
    memory_tx: mpsc::UnboundedSender<ProcessRequest>,  // 发到独立线程
    sessions: Arc<RwLock<HashMap<String, SessionInfo>>>,  // 内存存储，Phase 2 持久化
    event_bus: broadcast::Sender<EngineEvent>,
}

// EngineEvent 从 models 重导出
pub use openloom_models::EngineEvent;

struct EngineConfig {
    models: Vec<ModelConfig>,
    router_cfg: RouterConfig,
    data_dir: PathBuf,
    threshold: usize,            // MemoryPipeline 聚合阈值
}

impl Engine {
    async fn new(config: EngineConfig) -> Result<Self> {
        // 1. 加载 inference engine (Qwen3-1.7B)
        // 2. 创建 SmartRouter + 加载模型
        // 3. 注册 5 个内置 Skill
        // 4. 启动 MemoryPipeline 独立线程，返回 memory_tx
        // 5. 迭代 skills: router.register_skill_triggers(s.name(), s.manifest().triggers)
        // 6. 创建 event_bus + sessions
        // 7. 执行 refinery 迁移
    }

    /// 启动 EventBus + 后台任务
    async fn start(&self) -> Result<()>;

    /// 核心请求处理
    async fn handle_message(&self, msg: ChatMessage, session_id: &str) -> Result<ChatResponse> {
        // 1. 路由分类
        let out = self.router.classify(&msg.content).await?;

        // 2. 执行
        let response = match out.target_model {
            TargetModel::None => {
                // Skill 路径
                let params = serde_json::json!({"text": msg.content});
                match self.skills.invoke(out.skill_match.as_ref().unwrap(), params).await {
                    Ok(v) => v.to_string(),
                    Err(e) => {
                        tracing::error!(skill = %out.skill_match.as_ref().unwrap(), error = %e, "skill failed");
                        return Err(e);
                    }
                }
            }
            TargetModel::Local => {
                // LLM 路径
                let req = CompletionRequest { prompt: msg.content.clone(), ..Default::default() };
                self.inference.complete(req).await?.text
            }
        };

        // 3. 后台: 记忆管线 (通过 channel，不阻塞)
        let _ = self.memory_tx.send(ProcessRequest {
            session_id: session_id.to_string(),
            text: msg.content.clone(),
            context: format!("{}", out.intent),  // Intent impl Display
        });

        // 4. 广播 token_usage 事件
        let _ = self.event_bus.send(EngineEvent::TokenUsage {
            session_id: session_id.to_string(),
            model: "qwen3-1.7b".into(),
            prompt_tokens: self.inference.token_count(&msg.content),
            completion_tokens: self.inference.token_count(&response),
        });

        Ok(ChatResponse {
            response,
            session_id: session_id.to_string(),
            token_usage: TokenUsage { prompt_tokens: 0, completion_tokens: 0 },
        })
    }

    async fn health_check(&self) -> HealthStatus;
    async fn create_session(&self) -> Result<SessionInfo>;  // UUID + 插入 sessions map
    async fn list_sessions(&self) -> Result<Vec<SessionInfo>>;
    async fn shutdown(&self) -> Result<()>;
    fn subscribe(&self) -> broadcast::Receiver<EngineEvent>;
}
```

**MemoryPipeline 独立线程模型 (解决 rusqlite Connection 不 Send 的问题):**

```
// Engine::new() 中启动:
let (tx, rx) = mpsc::unbounded_channel::<ProcessRequest>();
std::thread::spawn(move || {
    let mut pipeline = MemoryPipeline::new(extractor, aggregator, store, threshold);
    for req in rx {
        if let Err(e) = pipeline.process(&req.session_id, &req.text, &req.context) {
            tracing::error!(session = %req.session_id, error = %e, "memory pipeline error");
        }
    }
});
// tx 存入 Engine.memory_tx
```

**关键集成点 — Router↔Skills 触发词注册 (Engine::new()):**
```rust
for skill in self.skills.all_skills() {
    self.router.register_skill_triggers(
        skill.name(),
        skill.manifest().triggers.clone(),
    );
}
```

**错误恢复 (子系统级):**
- Router 分类失败 → 降级为 `Chat / Local`，直接推理
- Skill invoke 失败 → `tracing::error!` + 返回 JSON-RPC `SkillFailed` 错误
- MemoryPipeline 失败 → `tracing::error!`，不影响用户响应 (fire-and-forget)
- 推理超时 → 30s 超时 + 返回 `ModelUnavailable`
- Config 缺失/损坏 → 使用全默认值启动，打印 warning
- 迁移失败 → `tracing::error!` 并退出，打印修复指引

### 3.6 crates/server/ — HTTP + WS 服务

**职责:** Axum HTTP + WebSocket + SSE + JSON-RPC 2.0 端点。

**依赖:** `axum = { version = "0.7", features = ["ws"] }`, `tower = "0.4"`, `serde = { version = "1", features = ["derive"] }`, `serde_json = "1"`, openloom-engine, openloom-models (显式)

**CLI 创建 Engine 并注入 Server:**
```
CLI: let engine = Engine::new(config).await?;
     match command {
         Serve => Server::new(engine).serve(port).await?,
         Chat  => run_chat_tui(engine).await?,
         Run   => engine.handle_message(msg, sid).await?,
     }
```

**端点:**
| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/health` | 健康检查 → `engine.health_check()` |
| GET | `/ws` | WebSocket upgrade，JSON-RPC 2.0 帧 |
| GET | `/sse/{session_id}` | SSE token 流，前端直连绕过 IPC |
| POST | `/api` | JSON-RPC 2.0 HTTP fallback |

**JSON-RPC 方法 (Phase 1):**
| 方法 | 参数 | 返回值 | 说明 |
|------|------|--------|------|
| `chat.send` | `{messages, session_id?, stream: bool}` | `{response, session_id, token_usage}` | stream=true 时返回 SSE URL |
| `skill.invoke` | `{skill_name, params}` | `{result}` | |
| `skill.list` | `{}` | `{skills: [{name, description, triggers}]}` | |
| `memory.query` | `{query, limit?}` | `{events, cognitions}` | Phase 2 加入向量搜索 |
| `memory.persona` | `{}` | `{summary, traits}` | |
| `agent.status` | `{}` | `{state, active_session, model_info}` | |
| `cache.stats` | `{}` | `{hit_rate: 0, ...}` | Phase 1 返回占位 |
| `config.get` | `{key?}` | `{config}` | |
| `config.set` | `{key, value}` | `{ok: bool}` | |
| `system.health` | `{}` | `{status, uptime, gpu_info}` | |
| `system.shutdown` | `{}` | `{ok: bool}` | |

**服务端通知 (EventBus 订阅 → WS 推送):**

EngineEvent 序列化为 snake_case → server 将 `_` 替换为 `.` 作为通知方法名:
| EngineEvent 变体 | 通知方法 |
|------|---------|
| `CognitionUpdated` | `cognition.updated` → `{trait, old_value, new_value, confidence}` |
| `AgentStateChanged` | `agent.state_changed` → `{old_state, new_state}` |
| `TokenUsage` | `token.usage` → `{session_id, model, prompt_tokens, completion_tokens}` |
| `Error` | `error` → `{code, message, subsystem}` |

### 3.7 crates/cli/ — CLI 扩展

**新增依赖:** `tokio = { version = "1", features = ["rt-multi-thread", "macros"] }`, `dirs = "5"`

**配置加载:** 所有命令启动时从 `~/.openloom/config.toml` 加载配置 (通过 `dirs` crate 解析平台路径)。支持 `--config <path>` 覆盖。

**Phase 0 保留:**
| 命令 | 说明 |
|------|------|
| `openloom analyze --input <file> --output <file>` | 离线分析对话日志 |

**Phase 1 新增:**
| 命令 | 说明 |
|------|------|
| `openloom serve [--port 0] [--config <path>]` | 启动 Engine 服务 (Electron sidecar) |
| `openloom chat [--config <path>]` | 交互式对话 (TUI) |
| `openloom run "任务" [--config <path>]` | 单次任务执行 |
| `openloom skill list` | 列出已安装 Skill |
| `openloom skill install <path>` | 安装 Skill |
| `openloom skill remove <name>` | 移除 Skill |
| `openloom memory persona` | 查看认知画像 |
| `openloom memory events [--limit N]` | 查看最近事件 |
| `openloom memory cognitions` | 查看认知图谱 |
| `openloom config get [key]` | 读取配置 |
| `openloom config set <key> <value>` | 修改配置 |
| `openloom config path` | 显示配置文件路径 |
| `openloom doctor` | 系统诊断 (GPU/模型/DB 状态) |
| `openloom version` | 版本信息 |

---

## 4. Sidecar 生命周期

Electron 主进程管理 `openloom serve` 子进程:

| 阶段 | 机制 |
|------|------|
| **启动** | `spawn openloom serve --port 0`，OS 分配端口 |
| **就绪信号** | Engine 向 stdout 写入 `{"type":"ready","port":19876}` JSON 行 |
| **就绪超时** | Electron 10 秒未收到 → 启动失败，提示用户检查日志 |
| **健康检查** | WS ping/pong (5s 超时) + `GET /health` |
| **崩溃恢复** | 指数退避 1s→2s→4s→8s→max 30s，最多 5 次 |
| **优雅关闭** | `before-quit` → `system.shutdown` RPC → 排空请求 → 5s 后 SIGKILL |
| **僵尸清理** | 启动时检查 `~/.openloom/run/` 下遗留的 port/pid 文件并清理 |

---

## 5. 配置管理

配置文件: `~/.openloom/config.toml` (TOML，通过 `dirs` 解析)

```toml
# 模型配置
[[models]]
name = "router"
path = "qwen3-1.7b-q4_k_m.gguf"
model_type = "Router"
backend = "LlamaCpp"
n_gpu_layers = 32
context_size = 4096

# 云端模型 (Phase 2 启用)
# [[models]]
# name = "cloud-primary"
# model_type = "Reasoning"
# backend = "Anthropic"
# model = "claude-sonnet-4-6"
# api_key_env = "ANTHROPIC_API_KEY"

# Router 配置
[router]
keyword_threshold = 0.85
fallback_threshold = 0.7

# 服务配置
[server]
host = "127.0.0.1"

# 存储配置
[storage]
data_dir = "~/.openloom/data"

# 日志
[logging]
level = "INFO"
log_content = false
dir = "~/.openloom/logs"
```

**配置行为:**
- 配置文件缺失 → 使用全默认值启动，打印 warning
- 配置格式错误 → 打印 parse error + 行号，退出
- 未知字段 → serde 默认忽略 (不报错)
- 新增字段必有默认值，废弃字段保留 2 个大版本后才删除

**字段默认值:**
- `ModelConfig.backend` = `LlamaCpp`
- `ModelConfig.n_gpu_layers` = `0` (CPU only)
- `ModelConfig.api_key_env` = `None`
- `RouterPrefs.keyword_threshold` = `0.85`
- `RouterPrefs.fallback_threshold` = `0.7`
- `ServerPrefs.host` = `"127.0.0.1"`
- `StoragePrefs.data_dir` = 平台默认 (Windows: `%APPDATA%/openLoom/data`, macOS: `~/Library/Application Support/openLoom/data`, Linux: `~/.local/share/openLoom/data`)
- `LoggingPrefs.level` = `"INFO"`
- `LoggingPrefs.log_content` = `false`

---

## 6. 数据迁移

### 6.1 Phase 0 → Phase 1 迁移策略

Phase 0 使用内联 `CREATE TABLE IF NOT EXISTS` 创建 schema。Phase 1 引入 `refinery` crate (`refinery = { version = "0.8", features = ["rusqlite"] }`) 管理迁移。

**过渡方案:**
1. 删除 Phase 0 `SqliteEventStore::migrate()` 中的内联 DDL
2. Phase 1 的 V1 迁移脚本与 Phase 0 内联 DDL 完全一致 (events 表 + events_fts + 触发器)
3. refinery 执行 `CREATE TABLE IF NOT EXISTS` 时，如果 Phase 0 已创建表，则为安全 no-op
4. V2 迁移新增 cognitions、sessions、token_usage 表

**迁移失败处理:**
- 迁移脚本出错 → `tracing::error!` 打印具体错误 + SQL 语句 + 修复指引，Engine 退出
- 磁盘满 → 同上，提示清理磁盘
- 提供 `openloom doctor` 命令检查数据库完整性 + 迁移状态

### 6.2 迁移脚本

`V1__initial.sql` — 与 Phase 0 内联 DDL 一致:
```sql
CREATE TABLE IF NOT EXISTS events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp TEXT NOT NULL,
    type TEXT NOT NULL,
    action TEXT NOT NULL,
    context TEXT NOT NULL DEFAULT '',
    confidence REAL NOT NULL,
    source_session TEXT,
    source_text TEXT NOT NULL DEFAULT '',
    payload TEXT
);

CREATE VIRTUAL TABLE IF NOT EXISTS events_fts
USING fts5(type, action, context, source_text, content='events', content_rowid='id');

-- 触发器 (IF NOT EXISTS 不支持 trigger，用 CREATE OR REPLACE 替代风险)
CREATE TRIGGER IF NOT EXISTS events_ai AFTER INSERT ON events BEGIN
    INSERT INTO events_fts(rowid, type, action, context, source_text)
    VALUES (new.id, new.type, new.action, new.context, new.source_text);
END;

CREATE TRIGGER IF NOT EXISTS events_ad AFTER DELETE ON events BEGIN
    INSERT INTO events_fts(events_fts, rowid, type, action, context, source_text)
    VALUES('delete', old.id, old.type, old.action, old.context, old.source_text);
END;
```

`V2__add_cognitions_sessions.sql` — Phase 1 新增:
```sql
CREATE TABLE IF NOT EXISTS cognitions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    subject TEXT NOT NULL,
    trait TEXT NOT NULL,
    value TEXT NOT NULL,
    confidence REAL,
    evidence_count INTEGER,
    first_seen INTEGER,
    last_updated INTEGER,
    version INTEGER DEFAULT 1
);

CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY,
    created_at TEXT NOT NULL,
    message_count INTEGER DEFAULT 0
);

CREATE TABLE IF NOT EXISTS token_usage (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp TEXT NOT NULL,
    session_id TEXT,
    model TEXT NOT NULL,
    prompt_tokens INTEGER,
    completion_tokens INTEGER,
    cached_tokens INTEGER DEFAULT 0,
    latency_ms INTEGER
);
```

---

## 7. Token 监控

每次模型调用记录到 `token_usage` 表 (Section 6 DDL)。

**CLI 面板 (chat 模式 TUI 底部状态栏):**
- 当前会话 token 用量 (本地)
- Router 命中率 = Router 处理数 / 总请求数
- 节省率 = (假想全走云端的用量 - 实际用量) / 假想用量

**Web 仪表盘 (Electron 渲染进程):**
- 通过 `token.usage` 通知实时更新
- 显示内容: 累计本地/云端 token、节省率百分比、Router 命中率折线图
- 作为聊天界面的侧边栏面板或独立 Tab

**Tracing 埋点 (关键路径 span):**
- `router.classify` — 延迟 + 分类结果
- `engine.handle_message` — 延迟 + 派发路径 (skill/llm)
- `inference.complete` — 延迟 + token 数
- `memory.process` — 延迟 + 事件数

---

## 8. Electron 壳

### 8.1 主进程

- Sidecar 生命周期管理 (Section 4)
- 系统托盘图标 + 菜单
- 原生对话框 (文件选择、设置)
- contextBridge 暴露 `window.openloom` API:
  - `window.openloom.send(method, params)` → Promise (JSON-RPC over WS)
  - `window.openloom.subscribe(event, callback)` → 取消订阅函数
  - `window.openloom.sseUrl(sessionId)` → SSE URL 字符串

### 8.2 渲染进程 (React 19 + Tailwind)

- **侧边栏:** 会话列表 (新建/切换) + 认知画像摘要 (通过 `memory.persona`)
- **聊天区:** Markdown 渲染 + 流式 token 显示 (SSE)
- **设置面板:** 模型配置 (编辑 config.toml)
- **Token 仪表盘:** 实时节省率 + Router 命中率 (通过 `token.usage` 通知)

### 8.3 安全配置

```
contextIsolation: true
nodeIntegration: false
sandbox: true
webviewTag: false
CSP: default-src 'self'; connect-src ws://127.0.0.1:* http://127.0.0.1:*; script-src 'self'
```

---

## 9. 测试策略

| 层级 | 内容 |
|------|------|
| **单元测试** | 每个 crate 独立测试 |
| **集成测试** | 10 个场景 + 端到端 (见下方) |
| **性能基准** | `criterion`: Router 延迟 (p50/p95)、Token 节省率 |
| **CI** | GitHub Actions: `cargo test --workspace` + `cargo clippy -- -D warnings` + `cargo fmt --check` + `cargo audit` |

### 9.1 单元测试清单

| Crate | 测试内容 |
|-------|---------|
| models | JSON-RPC 序列化往返、ErrorCode 枚举值、EngineEvent→snake_case 映射 |
| inference | GGUF 加载、complete 返回 token 数正确、detect_gpu 不 panic、token_count 一致性 |
| router | 关键词匹配准确率 ≥ 90%、空输入不 panic、未匹配降级到 Chat、skill 触发词注册 |
| skills | Skill trait invoke、find_by_trigger 匹配、list_all 返回全部、未注册 skill 返回 Err |
| engine | handle_message skill 路径、handle_message LLM 路径、memory channel 不阻塞、session 创建/列表 |
| server | JSON-RPC 请求解析、无效 method 返回 -32601、WS ping/pong、/health 返回 HealthStatus |

### 9.2 集成测试场景 (10 个)

| # | 场景 | 验证点 |
|---|------|--------|
| 1 | "帮我写个 Python 脚本" → skill 匹配 code_assistant → invoke 成功 | Router→Skill 路径 |
| 2 | "今天天气怎么样" → 未匹配 skill → LLM 推理 → 响应返回 | Router→LLM 路径 |
| 3 | 空输入 "" → 不 panic，返回空响应 | 边界情况 |
| 4 | 连续 5 次相同意图 → Router 分类一致 | 分类稳定性 |
| 5 | 长文本 (>2000 chars) → 关键词仍命中 → 不超时 | 性能边界 |
| 6 | engine.handle_message → memory channel 收到 ProcessRequest | 记忆管线非阻塞 |
| 7 | server /health → `{status: "ok", uptime, gpu_info}` | 健康检查 |
| 8 | server chat.send JSON-RPC → `{response, session_id, token_usage}` | JSON-RPC 往返 |
| 9 | server 发送无效 JSON → 返回 ParseError -32700 | 错误处理 |
| 10 | engine.create_session → list_sessions 包含新 session | session 管理 |

---

## 10. 不做的事项 (Phase 2+)

- 完整 Agent Loop (ReAct 循环)
- 认知自动化 (Cognition Updater 调用 8B 模型)
- Persona Projector (一句话认知画像)
- Context Weaver (KV Cache + 四合一编织)
- KV Cache 持久化
- 云端模型适配层 (`TargetModel::Cloud`)
- 多 Session 持久化 (Phase 1 内存存储)
- WASM 编译管线 (Skill 用 Rust trait 实现)
- 权限强制执行 (数据模型存在，暂不执行)
- sqlite-vec 向量索引
- Tauri 替代方案评估

---

## 11. 关键风险

| 风险 | 缓解 |
|------|------|
| llama-cpp-2 与 Windows 11 的兼容性 | 优先验证 GPU 检测 + GGUF 加载；支持 CPU fallback |
| Qwen3-1.7B 意图分类准确度 | 关键词优先作为快速路径；置信度 < 0.7 降级本地推理 (非云端) |
| wasmtime 集成复杂度 | 推迟到 Phase 2，Phase 1 用 Rust trait |
| MemoryPipeline rusqlite Connection 不 Send | 独立线程 + channel 通信，不传 MemoryPipeline 跨线程 |
| Electron 内存占用 ~200MB | 设计上可接受，相比竞品仍属轻量 |
| Phase 0 → refinery 迁移兼容 | V1 与 Phase 0 内联 DDL 完全一致，CREATE IF NOT EXISTS 为安全 no-op |

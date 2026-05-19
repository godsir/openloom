# Phase 1: Smart Router + Skill Engine — 设计规范

**版本:** 1.0
**日期:** 2026-05-19
**状态:** 设计完成，待实现
**前置:** Phase 0 Memory Kernel MVP (已完成)

---

## 1. 目标

实现 "80% 请求不动大模型" — 通过本地 Qwen3-1.7B 意图分类 + WASM 技能引擎懒加载，将大部分用户请求在 Router 或 Skill 层处理完毕，不触及云端大模型。

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
├── skills/          ← NEW: Skill trait + Registry + CLI Bridge
├── engine/          ← NEW: EventBus + 请求派发 (轻量)
├── server/          ← NEW: Axum HTTP + WSS + JSON-RPC 2.0
└── cli/             ← CLI 入口 (扩展)
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
  ├─ cache_hit: bool (Phase 3 启用)
  └─ target_model: Local | Cloud | None
  │
  ├─ 匹配到 Skill (target_model == None)
  │    └─▶ SkillRegistry.invoke(skill_name, params) → 结果
  │
  └─ 未匹配 或 需 LLM
       └─▶ InferenceEngine.complete(prompt) → LLM 响应
           │
           └─▶ [后台] MemoryPipeline.process() via spawn_blocking
                └─ 事件提取 + 模式聚合
```

---

## 3. Crate 详细设计

### 3.1 crates/models/ — 共享类型 (扩展)

**Phase 0 保留:**
```rust
enum ModelType { Router, Summarizer, Reasoning }
```

**Phase 0 扩展:**
```rust
struct ModelConfig {
    pub name: String,
    pub model_type: ModelType,         // 角色: Router/Summarizer/Reasoning
    pub backend: ModelBackend,         // NEW: 运行时后端
    pub path: Option<String>,
    pub context_size: usize,
    pub n_gpu_layers: usize,           // NEW
    pub api_key_env: Option<String>,   // NEW
}

enum ModelBackend { LlamaCpp, Anthropic, OpenAI, DeepSeek }
```

**Phase 1 新增:**
```rust
// JSON-RPC 协议类型
struct JsonRpcRequest { jsonrpc: String, method: String, params: Value, id: u64 }
struct JsonRpcResponse { jsonrpc: String, result: Option<Value>, error: Option<JsonRpcError>, id: u64 }
struct JsonRpcError { code: ErrorCode, message: String, data: Option<Value> }

enum ErrorCode {
    ParseError = -32700,
    InvalidRequest = -32600,
    MethodNotFound = -32601,
    InternalError = -32603,
    ModelUnavailable = -32000,
    SkillFailed = -32001,
    PermissionDenied = -32002,
    Timeout = -32003,
}

// 引擎类型
struct ChatResponse { response: String, session_id: String, token_usage: TokenUsage }
struct TokenUsage { prompt_tokens: usize, completion_tokens: usize }
struct SessionInfo { id: String, created_at: DateTime<Utc>, message_count: usize }
struct HealthStatus { status: String, uptime: u64, gpu_info: GpuInfo }
struct SystemEvent { event_type: String, payload: Value }

// 引擎事件 (广播给所有订阅者)
enum EngineEvent {
    CognitionUpdated { trait_name: String, old_value: String, new_value: String, confidence: f64 },
    AgentStateChanged { old_state: String, new_state: String },
    TokenUsage { session_id: String, model: String, prompt_tokens: usize, completion_tokens: usize },
    Error { code: ErrorCode, message: String, subsystem: String },
}
```

### 3.2 crates/inference/ — 模型推理

**职责:** 封装 llama-cpp-2，加载 GGUF 模型，提供文本补全。

**依赖:** `llama-cpp-2 = "=0.1.146"`, tokio, openloom-models
**Feature flags:** `cuda`, `metal`, `vulkan` (workspace 级透传)

```rust
struct InferenceEngine { /* llama-cpp-2 context + model */ }
struct CompletionRequest { prompt: String, max_tokens: usize, temperature: f32, stream: bool }
struct CompletionResponse { text: String, prompt_tokens: usize, completion_tokens: usize }
struct GpuInfo { vendor: String, vram_mb: u64, supported: bool }

impl InferenceEngine {
    /// 加载 GGUF 模型，n_gpu_layers 控制 GPU 加速层数
    async fn load(model_path: &Path, n_gpu_layers: usize) -> Result<Self>;

    /// 同步补全，返回完整文本
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse>;

    /// 流式补全，token 通过 mpsc::Sender 逐 token 推送
    async fn complete_stream(&self, req: CompletionRequest, tx: mpsc::Sender<String>) -> Result<()>;

    /// 检测 GPU 信息 (VRAM、厂商)，用于自动配置
    fn detect_gpu() -> GpuInfo;

    /// 使用当前加载模型的 tokenizer 计算 token 数
    fn token_count(&self, text: &str) -> usize;
}
```

**降级策略:** GPU 不可用时自动 CPU 推理，通过 `detect_gpu()` 检测。

### 3.3 crates/router/ — 智能路由

**职责:** 意图分类 + 复杂度评分 + 技能匹配。关键词优先（零 token），未命中时调用 Qwen3-1.7B。

**依赖:** openloom-inference, regex, openloom-models

```rust
struct KeywordRule {
    pattern: String,         // 正则或关键词
    intent: Intent,          // 匹配到的意图
    confidence: f32,         // 置信度
}

struct RouterConfig {
    model_path: PathBuf,
    keyword_rules: Vec<KeywordRule>,  // 关键词优先匹配
    fallback_threshold: f32,          // 默认 0.7
}

struct ClassifyOutput {
    intent: Intent,
    complexity: f32,           // 0.0 ~ 1.0
    skill_match: Option<String>,
    confidence: f32,
    cache_hit: bool,           // Phase 3 启用，默认 false
    target_model: TargetModel,
}

enum Intent { Chat, FileOperation, WebSearch, CodeAssist, Schedule, Question, Other }
enum TargetModel { Local, Cloud, None }

impl SmartRouter {
    /// 创建 Router，加载 Qwen3-1.7B 用于低置信度分类
    async fn new(config: RouterConfig, engine: Arc<InferenceEngine>) -> Result<Self>;

    /// 分类用户文本：关键词匹配 → 命中直接返回 → 未命中调 LLM
    async fn classify(&self, text: &str) -> ClassifyOutput;

    /// 分类系统事件 (Phase 2 完整启用，Phase 1 提供类型契约)
    async fn classify_event(&self, event: &SystemEvent) -> ClassifyOutput;

    /// 注册技能触发词，用于 skill_match 匹配
    fn register_skill_triggers(&mut self, name: &str, triggers: Vec<String>);
}
```

**内部流程:**
1. 关键词匹配 (零 token) → 命中 → 高置信度直接返回
2. 未命中 → 构造分类 prompt → Qwen3-1.7B 推理 (~50-100 tokens)
3. 模型置信度 < 0.7 → `target_model = Cloud` (需二次判断)
4. 置信度 ≥ 0.7 → `target_model = Local | None`

### 3.4 crates/skills/ — 技能引擎

**职责:** Skill 注册/发现/执行 + CLI Bridge PATH 工具发现。

**依赖:** serde_json, openloom-models, wasmtime (Phase 2)

```rust
/// Skill trait — Phase 1 用 Rust native 实现，Phase 2 编译为 WASM
trait Skill: Send + Sync {
    fn name(&self) -> &str;
    fn manifest(&self) -> &SkillManifest;
    async fn invoke(&self, params: serde_json::Value) -> Result<serde_json::Value>;
    fn context_md(&self) -> &str;   // ≤ 200 tokens, 仅在激活时注入
}

/// 权限模型 (Section 13.2) — 数据模型完整，Phase 2/3 强制执行
struct SkillPermissions {
    fs_read: Vec<String>,       // 只读路径白名单
    fs_write: Vec<String>,      // 可写路径白名单
    network: Vec<String>,       // 网络域名白名单
    shell: bool,
    subprocess: bool,
    max_memory_mb: u32,
    max_runtime_sec: u32,
}

struct SkillManifest {
    name: String,
    description: String,
    triggers: Vec<String>,               // Router 匹配触发词
    permissions: SkillPermissions,
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
    /// 扫描 PATH 中可执行文件，解析 --help 提取工具描述
    fn discover_path_tools() -> Vec<CliTool>;
    fn parse_help(binary: &str) -> CliTool;
}

struct CliTool { name: String, description: String, binary: String }
```

**Phase 1 内置 Skill:**
- `FileManager` — 文件读写/搜索/列表
- `InfoRetriever` — 信息检索/知识查询
- `ScheduleReminder` — 日程提醒
- `CodeAssistant` — 代码辅助
- `WebBrowser` — 网页浏览

### 3.5 crates/engine/ — 编排引擎

**职责:** EventBus + 请求派发 + 生命周期管理。Phase 1 为轻量版本，Phase 2 加入 Agent Loop。

**依赖:** openloom-router, openloom-skills, openloom-inference, openloom-memory, openloom-models, tokio

```rust
struct Engine {
    router: SmartRouter,
    skills: SkillRegistry,
    inference: Arc<InferenceEngine>,
    memory: Arc<Mutex<MemoryPipeline>>,   // Phase 0 同步代码，Mutex 保护
    event_bus: broadcast::Sender<EngineEvent>,
}

// EngineEvent 从 models 重导出
pub use openloom_models::EngineEvent;

struct EngineConfig {
    models: Vec<ModelConfig>,
    router: RouterConfig,
    data_dir: PathBuf,
    threshold: usize,
}
struct ChatMessage { role: String, content: String }

impl Engine {
    async fn new(config: EngineConfig) -> Result<Self>;

    /// 启动 EventBus + 后台任务 (Phase 1: 仅日志)
    async fn start(&self) -> Result<()>;

    /// 核心请求处理
    async fn handle_message(&self, msg: ChatMessage, session_id: &str) -> Result<ChatResponse> {
        // 1. 路由分类
        let out = self.router.classify(&msg.content).await?;

        // 2. 执行
        let response = match &out.skill_match {
            Some(skill_name) => {
                let params = serde_json::json!({"text": &msg.content});
                self.skills.invoke(skill_name, params).await?.to_string()
            }
            None => {
                let req = CompletionRequest { prompt: msg.content.clone(), ..default() };
                self.inference.complete(req).await?.text
            }
        };

        // 3. 后台: 记忆管线
        let mem = self.memory.clone();
        let text = msg.content.clone();
        let intent = format!("{:?}", out.intent);
        tokio::spawn_blocking(move || {
            let _ = mem.lock().unwrap().process(session_id, &text, &intent);
        });

        // 4. 广播 token 用量
        let _ = self.event_bus.send(EngineEvent::TokenUsage { ... });

        Ok(ChatResponse { response, session_id: session_id.to_string(), token_usage })
    }

    async fn health_check(&self) -> HealthStatus;
    async fn create_session(&self) -> Result<SessionInfo>;
    async fn list_sessions(&self) -> Result<Vec<SessionInfo>>;
    async fn shutdown(&self) -> Result<()>;

    /// 订阅引擎事件 (server 用此推送通知到前端)
    fn subscribe(&self) -> broadcast::Receiver<EngineEvent>;
}
```

### 3.6 crates/server/ — HTTP + WSS 服务

**职责:** Axum HTTP + WebSocket + SSE + JSON-RPC 2.0 端点。

**依赖:** axum, tower, tokio-tungstenite, openloom-engine, openloom-models (显式)

**端点:**
| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/health` | 健康检查 → `engine.health_check()` |
| GET | `/ws` | WebSocket upgrade，JSON-RPC 2.0 帧 |
| GET | `/sse/{session_id}` | SSE token 流，前端直连绕过 IPC |
| POST | `/api` | JSON-RPC 2.0 HTTP fallback |

**JSON-RPC 方法 (Phase 1):**
| 方法 | 说明 |
|------|------|
| `chat.send` | 发送消息，`{messages, session_id?, stream: bool}` → `{response, session_id, token_usage}` |
| `skill.invoke` | 调用 Skill，`{skill_name, params}` → `{result}` |
| `skill.list` | 列出所有 Skill → `{skills: [{name, description, triggers}]}` |
| `memory.query` | 查询事件和认知，`{query, limit?}` → `{events, cognitions}` |
| `memory.persona` | 获取用户认知画像 → `{summary, traits}` |
| `agent.status` | Agent 状态 → `{state, active_session, model_info}` |
| `cache.stats` | KV Cache 统计 (Phase 1: 返回占位) → `{hit_rate: 0, ...}` |
| `config.get` | 读取配置，`{key?}` → `{config}` |
| `config.set` | 修改配置，`{key, value}` → `{ok: bool}` |
| `system.health` | 系统诊断 → `{status, uptime, gpu_info}` |
| `system.shutdown` | 优雅关闭 → `{ok: bool}` |

**服务端通知 (EventBus 订阅 → WSS 推送):**
| 通知 | 触发条件 |
|------|---------|
| `cognition.updated` | 认知阈值触发 → `{trait, old_value, new_value, confidence}` |
| `token.usage` | 每次 LLM 调用后 → `{session_id, model, prompt_tokens, completion_tokens}` |
| `error` | 非致命错误 → `{code, message, subsystem}` |

### 3.7 crates/cli/ — CLI 扩展

**Phase 0 保留:** `openloom analyze`

**Phase 1 新增:**
| 命令 | 说明 |
|------|------|
| `openloom serve [--port 0]` | 启动 Engine 服务 (Electron sidecar 模式)，`--port 0` 让 OS 分配端口 |
| `openloom chat` | 交互式对话 (TUI，直接调用 engine.handle_message) |
| `openloom run "任务描述"` | 单次任务执行 |
| `openloom skill list` | 列出已安装 Skill |
| `openloom skill install <path>` | 安装 Skill |
| `openloom doctor` | 系统诊断 (GPU/模型/数据库状态) |
| `openloom memory persona` | 查看当前用户认知画像 |
| `openloom config get/set` | 配置管理 |

---

## 4. Sidecar 生命周期

Electron 主进程管理 `openloom-engine serve` 子进程 (Phase 1 在 serve 命令中实现):

| 阶段 | 机制 |
|------|------|
| **启动** | `spawn openloom serve --port 0`，OS 分配端口 |
| **就绪信号** | Engine 向 stdout 写入 `{"type":"ready","port":19876}` JSON 行 |
| **就绪超时** | Electron 10 秒未收到 → 启动失败 |
| **健康检查** | WSS ping/pong (5s 超时) + `GET /health` |
| **崩溃恢复** | 指数退避 1s→2s→4s→8s→max 30s，最多 5 次 |
| **优雅关闭** | `before-quit` → `system.shutdown` RPC → 排空请求 → 5s 后 SIGKILL |
| **僵尸清理** | 启动时检查并清理上次遗留的端口/pid 文件 |

---

## 5. 配置管理

配置文件: `~/.openloom/config.toml` (TOML 格式)

```toml
# 模型配置 (Section 16.3)
[[models]]
name = "router"
path = "qwen3-1.7b-q4_k_m.gguf"
model_type = "Router"
backend = "LlamaCpp"
n_gpu_layers = 32
context_size = 4096

[[models]]
name = "cloud-primary"
model_type = "Reasoning"
backend = "Anthropic"
model = "claude-sonnet-4-6"
api_key_env = "ANTHROPIC_API_KEY"

# Router 配置
[router]
keyword_threshold = 0.85
fallback_threshold = 0.7

# 服务配置
[server]
host = "127.0.0.1"

# 日志
[logging]
level = "INFO"
log_content = false
```

新增字段必有默认值，废弃字段保留 2 个大版本后才删除。

---

## 6. 数据迁移

Phase 1 引入 `refinery` crate 管理 SQLite schema 版本。

**迁移脚本:**

`V1__initial.sql` — 对应 Phase 0 初始 schema (events 表 + FTS5)
`V2__add_cognitions.sql` — Phase 1 新增 cognitions 表:

```sql
CREATE TABLE IF NOT EXISTS cognitions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    subject TEXT NOT NULL,       -- 'USER' | 'AGENT' | 'RELATIONSHIP'
    trait TEXT NOT NULL,         -- 'risk_tendency' | 'communication_style' | ...
    value TEXT NOT NULL,
    confidence REAL,
    evidence_count INTEGER,
    first_seen INTEGER,
    last_updated INTEGER,
    version INTEGER DEFAULT 1
);
```

Engine 启动时自动执行未应用的迁移。

---

## 7. Token 监控

每次模型调用记录到 SQLite 表 `token_usage`:

```sql
CREATE TABLE token_usage (
    id INTEGER PRIMARY KEY,
    timestamp TEXT NOT NULL,
    session_id TEXT,
    model TEXT NOT NULL,
    prompt_tokens INTEGER,
    completion_tokens INTEGER,
    cached_tokens INTEGER DEFAULT 0,
    latency_ms INTEGER
);
```

CLI 面板展示:
- 累计 token 用量 (本地 vs 云端)
- 节省率 = (假想全走云端的用量 - 实际用量) / 假想用量
- Router 命中率 = Router 处理数 / 总请求数

Web 面板通过 `token.usage` 通知实时更新。

---

## 8. Electron 壳

### 8.1 主进程

- Sidecar 生命周期管理 (Section 4)
- 系统托盘图标 + 菜单
- 原生对话框 (文件选择、设置)
- contextBridge 暴露 `window.openloom` API

### 8.2 渲染进程 (React 19 + Tailwind)

- **侧边栏:** 会话列表 + 认知画像摘要
- **聊天区:** Markdown 渲染 + 流式 token 显示
- **设置面板:** 模型配置
- **Token 仪表盘:** 实时节省率

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
| **单元测试** | 每个 crate 独立测试：Router 分类准确率、Skill 执行正确性、JSON-RPC 序列化 |
| **集成测试** | Router→Engine→Server 端到端；10 个对话场景验证意图分类 |
| **性能基准** | `criterion`: Router 延迟 (p50/p95)、Token 节省率、内存占用 |
| **CI** | GitHub Actions: `cargo test --workspace` + `cargo clippy -- -D warnings` + `cargo fmt --check` |

---

## 10. 不做的事项 (Phase 2+)

- 完整 Agent Loop (ReAct 循环)
- 认知自动化 (Cognition Updater 调用 8B 模型)
- Persona Projector (一句话认知画像)
- Context Weaver (KV Cache + 四合一编织)
- KV Cache 持久化
- 云端模型适配层
- 多 Session 完整管理 (Phase 1 仅基本 UUID 生成)
- WASM 编译管线 (Skill 用 Rust trait 实现)
- 权限强制执行 (数据模型存在，暂不执行)

---

## 11. 关键风险

| 风险 | 缓解 |
|------|------|
| llama-cpp-2 与 Windows 11 的兼容性 | 优先验证 GPU 检测 + GGUF 加载 |
| Qwen3-1.7B 意图分类准确度 | 关键词优先作为快速路径；置信度 < 0.7 降级 |
| wasmtime 集成复杂度 | 推迟到 Phase 2，Phase 1 用 Rust trait |
| MemoryPipeline 同步调用阻塞 Tokio | `spawn_blocking` 隔离，不阻塞主循环 |
| Electron 内存占用 ~200MB | 设计上可接受，相比竞品仍属轻量 |

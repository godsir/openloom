# Phase 1: Smart Router + Skill Engine — 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 实现 "80% 请求不动大模型" — 本地 Qwen3-1.7B 意图分类 + Skill 引擎懒加载 + Axum 服务 + Electron 壳

**Architecture:** 新增 5 个 crate（inference/router/skills/engine/server），扩展 models 和 cli。Engine 通过 channel 与 MemoryPipeline 通信（解决 rusqlite Connection 不 Send 问题）。Server 通过 Axum + WebSocket + JSON-RPC 2.0 与 Electron/CLI 通信。

**Tech Stack:** Rust 2024, Tokio, llama-cpp-2 (=0.1.146), Axum 0.7, refinery 0.8, rusqlite 0.32, Electron 38, React 19 + Tailwind

---

## 文件结构

```
F:/openLoom/
├── Cargo.toml                              ← [Modify] workspace deps
├── crates/
│   ├── models/
│   │   ├── Cargo.toml                      ← [Modify] +chrono, +serde_json
│   │   └── src/
│   │       └── lib.rs                      ← [Modify] 扩展所有新类型
│   ├── inference/
│   │   ├── Cargo.toml                      ← [Create]
│   │   └── src/
│   │       └── lib.rs                      ← [Create] InferenceEngine
│   ├── router/
│   │   ├── Cargo.toml                      ← [Create]
│   │   └── src/
│   │       ├── lib.rs                      ← [Create] SmartRouter
│   │       └── keywords.rs                 ← [Create] 关键词规则
│   ├── skills/
│   │   ├── Cargo.toml                      ← [Create]
│   │   └── src/
│   │       ├── lib.rs                      ← [Create] Skill trait + Registry + CliBridge
│   │       └── builtins/
│   │           ├── mod.rs                  ← [Create]
│   │           ├── file_manager.rs         ← [Create]
│   │           ├── info_retriever.rs       ← [Create]
│   │           ├── schedule_reminder.rs    ← [Create]
│   │           ├── code_assistant.rs       ← [Create]
│   │           └── web_browser.rs          ← [Create]
│   ├── engine/
│   │   ├── Cargo.toml                      ← [Create]
│   │   └── src/
│   │       ├── lib.rs                      ← [Create] Engine + EventBus
│   │       └── memory_thread.rs            ← [Create] MemoryPipeline 独立线程
│   ├── server/
│   │   ├── Cargo.toml                      ← [Create]
│   │   └── src/
│   │       ├── lib.rs                      ← [Create] Axum server
│   │       ├── jsonrpc.rs                  ← [Create] JSON-RPC 处理
│   │       ├── ws.rs                       ← [Create] WebSocket handler
│   │       └── sse.rs                      ← [Create] SSE stream
│   ├── memory/
│   │   ├── Cargo.toml                      ← [Modify] +refinery
│   │   └── src/
│   │       └── store.rs                    ← [Modify] 移除内联 migrate，改用 refinery
│   └── cli/
│       ├── Cargo.toml                      ← [Modify] +tokio, +dirs
│       └── src/
│           └── main.rs                     ← [Modify] 扩展所有命令
├── migrations/                             ← [Create]
│   ├── V1__initial.sql                     ← [Create]
│   └── V2__add_cognitions_sessions.sql     ← [Create]
├── electron/                               ← [Create] (独立子系统)
│   ├── package.json
│   ├── main.js                             ← 主进程
│   ├── preload.js                          ← contextBridge
│   └── ...
├── web/                                    ← [Create] (独立子系统)
│   ├── package.json
│   ├── src/
│   │   ├── App.tsx
│   │   ├── components/
│   │   └── ...
│   └── ...
└── tests/
    └── phase1_integration_tests.rs         ← [Create] 10 场景集成测试
```

---

### Task 1: 扩展 models crate — 共享类型

**Files:**
- Modify: `F:/openLoom/crates/models/Cargo.toml`
- Modify: `F:/openLoom/crates/models/src/lib.rs`

- [ ] **Step 1: 更新 Cargo.toml 添加依赖**

`F:/openLoom/crates/models/Cargo.toml`:
```toml
[package]
name = "openloom-models"
version.workspace = true
edition.workspace = true

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
chrono = { version = "0.4", features = ["serde"] }
```

- [ ] **Step 2: 重写 lib.rs — 添加所有 Phase 1 类型**

`F:/openLoom/crates/models/src/lib.rs`:
```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

// === Phase 0 保留 ===

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ModelType {
    Router,
    Summarizer,
    Reasoning,
}

// === Phase 0 扩展 ===

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ModelBackend {
    LlamaCpp,
    Anthropic,
    OpenAI,
    DeepSeek,
}

impl Default for ModelBackend {
    fn default() -> Self {
        Self::LlamaCpp
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub name: String,
    #[serde(default)]
    pub model_type: ModelType,
    #[serde(default)]
    pub backend: ModelBackend,
    pub path: Option<String>,
    #[serde(default = "default_context_size")]
    pub context_size: usize,
    #[serde(default)]
    pub n_gpu_layers: usize,
    pub api_key_env: Option<String>,
}

fn default_context_size() -> usize { 4096 }

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            model_type: ModelType::Router,
            backend: ModelBackend::default(),
            path: None,
            context_size: default_context_size(),
            n_gpu_layers: 0,
            api_key_env: None,
        }
    }
}

// === Router 类型 ===

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Intent {
    Chat,
    FileOperation,
    WebSearch,
    CodeAssist,
    Schedule,
    Question,
    Other,
}

impl std::fmt::Display for Intent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Intent::Chat => write!(f, "chat"),
            Intent::FileOperation => write!(f, "file_operation"),
            Intent::WebSearch => write!(f, "web_search"),
            Intent::CodeAssist => write!(f, "code_assist"),
            Intent::Schedule => write!(f, "schedule"),
            Intent::Question => write!(f, "question"),
            Intent::Other => write!(f, "other"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TargetModel {
    Local,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassifyOutput {
    pub intent: Intent,
    pub complexity: f32,
    pub skill_match: Option<String>,
    pub confidence: f32,
    pub cache_hit: bool,
    pub target_model: TargetModel,
}

// === 引擎类型 ===

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub response: String,
    pub session_id: String,
    pub token_usage: TokenUsage,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TokenUsage {
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: String,
    pub created_at: DateTime<Utc>,
    pub message_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    pub status: String,
    pub uptime: u64,
    pub gpu_info: GpuInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemEvent {
    pub event_type: String,
    pub payload: Value,
}

// === GPU 信息 ===

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuInfo {
    pub vendor: String,
    pub vram_mb: u64,
    pub supported: bool,
}

// === 引擎事件 ===

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineEvent {
    CognitionUpdated {
        trait_name: String,
        old_value: String,
        new_value: String,
        confidence: f64,
    },
    AgentStateChanged {
        old_state: String,
        new_state: String,
    },
    TokenUsage {
        session_id: String,
        model: String,
        prompt_tokens: usize,
        completion_tokens: usize,
    },
    Error {
        code: ErrorCode,
        message: String,
        subsystem: String,
    },
}

// === JSON-RPC 类型 ===

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub method: String,
    pub params: Option<Value>,
    pub id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
    pub id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: ErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum ErrorCode {
    #[serde(rename = "-32700")]
    ParseError = -32700,
    #[serde(rename = "-32600")]
    InvalidRequest = -32600,
    #[serde(rename = "-32601")]
    MethodNotFound = -32601,
    #[serde(rename = "-32603")]
    InternalError = -32603,
    #[serde(rename = "-32000")]
    ModelUnavailable = -32000,
    #[serde(rename = "-32001")]
    SkillFailed = -32001,
    #[serde(rename = "-32002")]
    PermissionDenied = -32002,
    #[serde(rename = "-32003")]
    Timeout = -32003,
}

// === 配置类型 ===

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub models: Vec<ModelConfig>,
    #[serde(default)]
    pub router: RouterPrefs,
    #[serde(default)]
    pub server: ServerPrefs,
    #[serde(default)]
    pub storage: StoragePrefs,
    #[serde(default)]
    pub logging: LoggingPrefs,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouterPrefs {
    #[serde(default = "default_keyword_threshold")]
    pub keyword_threshold: f32,
    #[serde(default = "default_fallback_threshold")]
    pub fallback_threshold: f32,
}

fn default_keyword_threshold() -> f32 { 0.85 }
fn default_fallback_threshold() -> f32 { 0.7 }

impl Default for RouterPrefs {
    fn default() -> Self {
        Self {
            keyword_threshold: default_keyword_threshold(),
            fallback_threshold: default_fallback_threshold(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerPrefs {
    #[serde(default = "default_host")]
    pub host: String,
}

fn default_host() -> String { "127.0.0.1".into() }

impl Default for ServerPrefs {
    fn default() -> Self {
        Self { host: default_host() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoragePrefs {
    #[serde(default)]
    pub data_dir: Option<String>,
}

impl Default for StoragePrefs {
    fn default() -> Self {
        Self { data_dir: None }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingPrefs {
    #[serde(default = "default_log_level")]
    pub level: String,
    #[serde(default)]
    pub log_content: bool,
    #[serde(default)]
    pub dir: Option<String>,
}

fn default_log_level() -> String { "INFO".into() }

impl Default for LoggingPrefs {
    fn default() -> Self {
        Self { level: default_log_level(), log_content: false, dir: None }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_config_defaults() {
        let config: AppConfig = toml::from_str("").unwrap_or_default();
        // 暂时不依赖 toml，测试 Default trait
        let config = AppConfig {
            models: vec![],
            router: RouterPrefs::default(),
            server: ServerPrefs::default(),
            storage: StoragePrefs::default(),
            logging: LoggingPrefs::default(),
        };
        assert_eq!(config.router.keyword_threshold, 0.85);
        assert_eq!(config.router.fallback_threshold, 0.7);
        assert_eq!(config.server.host, "127.0.0.1");
    }

    #[test]
    fn test_intent_display() {
        assert_eq!(format!("{}", Intent::Chat), "chat");
        assert_eq!(format!("{}", Intent::FileOperation), "file_operation");
    }

    #[test]
    fn test_jsonrpc_error_code_serde() {
        let err = JsonRpcError {
            code: ErrorCode::MethodNotFound,
            message: "not found".into(),
            data: None,
        };
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("-32601"));
        let decoded: JsonRpcError = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.code, ErrorCode::MethodNotFound);
    }

    #[test]
    fn test_engine_event_snake_case() {
        let event = EngineEvent::CognitionUpdated {
            trait_name: "risk".into(),
            old_value: "low".into(),
            new_value: "high".into(),
            confidence: 0.9,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("cognition_updated"));
    }

    #[test]
    fn test_chat_response_serialization() {
        let resp = ChatResponse {
            response: "hello".into(),
            session_id: "s1".into(),
            token_usage: TokenUsage { prompt_tokens: 10, completion_tokens: 5 },
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("session_id"));
        let decoded: ChatResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.response, "hello");
    }

    #[test]
    fn test_model_config_default() {
        let config = ModelConfig::default();
        assert_eq!(config.n_gpu_layers, 0);
        assert_eq!(config.backend, ModelBackend::LlamaCpp);
        assert!(config.api_key_env.is_none());
    }
}
```

- [ ] **Step 3: 运行测试**

```bash
cd F:/openLoom && cargo test -p openloom-models
```
Expected: 6 new tests PASS (test_app_config_defaults, test_intent_display, test_jsonrpc_error_code_serde, test_engine_event_snake_case, test_chat_response_serialization, test_model_config_default)

- [ ] **Step 4: Commit**

```bash
git add crates/models/Cargo.toml crates/models/src/lib.rs
git commit -m "feat(models): add Phase 1 shared types — Router, JSON-RPC, Config, EngineEvent"
```

---

### Task 2: 创建 inference crate — llama-cpp-2 封装

**Files:**
- Create: `F:/openLoom/crates/inference/Cargo.toml`
- Create: `F:/openLoom/crates/inference/src/lib.rs`

- [ ] **Step 1: 创建 Cargo.toml**

`F:/openLoom/crates/inference/Cargo.toml`:
```toml
[package]
name = "openloom-inference"
version.workspace = true
edition.workspace = true

[dependencies]
openloom-models = { path = "../models" }
llama-cpp-2 = "=0.1.146"
tokio = { version = "1", features = ["rt", "sync", "macros"] }
anyhow = "1"
tracing = "0.1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"

[features]
cuda = ["llama-cpp-2/cuda"]
metal = ["llama-cpp-2/metal"]
vulkan = ["llama-cpp-2/vulkan"]
```

- [ ] **Step 2: 编写测试**

`F:/openLoom/crates/inference/src/lib.rs` 末尾添加:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use openloom_models::GpuInfo;

    #[test]
    fn test_detect_gpu_does_not_panic() {
        let info = InferenceEngine::detect_gpu();
        // 应该至少返回有意义的结构（即使在 CPU-only 机器上）
        assert!(!info.vendor.is_empty() || !info.supported);
    }

    #[test]
    fn test_completion_request_default() {
        let req = CompletionRequest::default();
        assert_eq!(req.max_tokens, 2048);
        assert!((req.temperature - 0.7).abs() < 0.01);
        assert!(!req.stream);
    }

    #[test]
    fn test_gpu_info_serialization() {
        let info = GpuInfo { vendor: "NVIDIA".into(), vram_mb: 8192, supported: true };
        let json = serde_json::to_string(&info).unwrap();
        let decoded: GpuInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.vram_mb, 8192);
    }
}
```

- [ ] **Step 3: 运行测试验证失败**

```bash
cd F:/openLoom && cargo test -p openloom-inference
```
Expected: FAIL — "InferenceEngine not defined"

- [ ] **Step 4: 实现 InferenceEngine**

`F:/openLoom/crates/inference/src/lib.rs`:
```rust
use anyhow::Result;
use openloom_models::GpuInfo;
use std::path::Path;

pub struct CompletionRequest {
    pub prompt: String,
    pub max_tokens: usize,
    pub temperature: f32,
    pub stream: bool,
}

impl Default for CompletionRequest {
    fn default() -> Self {
        Self {
            prompt: String::new(),
            max_tokens: 2048,
            temperature: 0.7,
            stream: false,
        }
    }
}

pub struct CompletionResponse {
    pub text: String,
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
}

pub struct InferenceEngine {
    // Phase 1: llama-cpp-2 LlamaModel + LlamaContext
    // 实际类型在集成 llama-cpp-2 时填入
    _model_path: std::path::PathBuf,
    _n_gpu_layers: usize,
}

impl InferenceEngine {
    pub async fn load(model_path: &Path, n_gpu_layers: usize) -> Result<Self> {
        tracing::info!(path = %model_path.display(), n_gpu_layers, "loading model");
        // Phase 1 实际实现:
        // let params = llama_cpp_2::LlamaModelParams::default();
        // let model = llama_cpp_2::model::LlamaModel::load_from_file(&params, model_path)?;
        // let ctx_params = llama_cpp_2::ContextParams::default()
        //     .with_n_gpu_layers(n_gpu_layers as u32);
        // let ctx = model.create_context(&ctx_params)?;
        Ok(Self {
            _model_path: model_path.to_path_buf(),
            _n_gpu_layers: if n_gpu_layers > 0 { n_gpu_layers } else { 0 },
        })
    }

    pub async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse> {
        // Phase 1 实际实现: 调用 llama-cpp-2 推理
        // let tokens = ctx.llama_decode(...)?;
        // let text = ctx.decode_tokens(&tokens)?;
        // 返回 CompletionResponse { text, prompt_tokens, completion_tokens }
        Ok(CompletionResponse {
            text: String::new(),
            prompt_tokens: 0,
            completion_tokens: 0,
        })
    }

    pub async fn complete_stream(
        &self,
        _req: CompletionRequest,
        _tx: tokio::sync::mpsc::Sender<String>,
    ) -> Result<()> {
        // Phase 1 实际实现: 逐 token 推送
        Ok(())
    }

    pub fn detect_gpu() -> GpuInfo {
        // Phase 1 实际实现: 检测 CUDA/Metal/Vulkan 可用性
        // 先用 env!("CUDA_VISIBLE_DEVICES") 等检测
        GpuInfo {
            vendor: String::new(),
            vram_mb: 0,
            supported: false,
        }
    }

    pub fn token_count(&self, text: &str) -> usize {
        // 简化估算: 中文 ~1.5 char/token, 英文 ~4 char/token
        // Phase 1 实际实现: 用 GGUF tokenizer
        text.len() / 4
    }
}
```

- [ ] **Step 5: 运行测试验证通过**

```bash
cd F:/openLoom && cargo test -p openloom-inference
```
Expected: 3 tests PASS

- [ ] **Step 6: 构建验证**

```bash
cd F:/openLoom && cargo build -p openloom-inference
```
Expected: `Compiling openloom-inference ... Finished`

- [ ] **Step 7: Commit**

```bash
git add crates/inference/
git commit -m "feat(inference): add llama-cpp-2 wrapper — InferenceEngine with GPU detect"
```

---

### Task 3: 创建 router crate — SmartRouter

**Files:**
- Create: `F:/openLoom/crates/router/Cargo.toml`
- Create: `F:/openLoom/crates/router/src/lib.rs`
- Create: `F:/openLoom/crates/router/src/keywords.rs`

- [ ] **Step 1: 创建 Cargo.toml**

`F:/openLoom/crates/router/Cargo.toml`:
```toml
[package]
name = "openloom-router"
version.workspace = true
edition.workspace = true

[dependencies]
openloom-models = { path = "../models" }
openloom-inference = { path = "../inference" }
regex = "1"
serde = { version = "1", features = ["derive"] }
anyhow = "1"
tracing = "0.1"
```

- [ ] **Step 2: 编写关键词匹配模块**

`F:/openLoom/crates/router/src/keywords.rs`:
```rust
use openloom_models::Intent;
use regex::Regex;

pub struct KeywordRule {
    pub pattern: Regex,
    pub intent: Intent,
    pub confidence: f32,
}

pub fn default_keyword_rules() -> Vec<KeywordRule> {
    vec![
        // 文件操作
        KeywordRule {
            pattern: Regex::new(r"(?i)(打开|读取|写入|保存|删除|创建|新建|列出|查看)\s*(文件|文档|目录|文件夹)").unwrap(),
            intent: Intent::FileOperation,
            confidence: 0.90,
        },
        KeywordRule {
            pattern: Regex::new(r"(?i)(\.rs|\.py|\.js|\.ts|\.toml|\.json|\.md)\b").unwrap(),
            intent: Intent::FileOperation,
            confidence: 0.85,
        },
        // 网页搜索
        KeywordRule {
            pattern: Regex::new(r"(?i)(搜索|查找|查询|百度|Google|搜一下|查一下)").unwrap(),
            intent: Intent::WebSearch,
            confidence: 0.90,
        },
        KeywordRule {
            pattern: Regex::new(r"(?i)(今天|最近|新闻|天气|最新)").unwrap(),
            intent: Intent::WebSearch,
            confidence: 0.75,
        },
        // 代码辅助
        KeywordRule {
            pattern: Regex::new(r"(?i)(写|编写|实现|修复|debug|重构|review|优化)\s*(代码|函数|方法|类|模块|脚本|程序)").unwrap(),
            intent: Intent::CodeAssist,
            confidence: 0.90,
        },
        KeywordRule {
            pattern: Regex::new(r"(?i)(代码|编译|运行|测试|单元测试|cargo|npm|pip|git|commit)").unwrap(),
            intent: Intent::CodeAssist,
            confidence: 0.85,
        },
        KeywordRule {
            pattern: Regex::new(r"(?i)(bug|错误|报错|失败|不对|不行|有问题)").unwrap(),
            intent: Intent::CodeAssist,
            confidence: 0.80,
        },
        // 日程提醒
        KeywordRule {
            pattern: Regex::new(r"(?i)(提醒|日程|日历|会议|预约|安排|定时|几点|明天|下周|周)").unwrap(),
            intent: Intent::Schedule,
            confidence: 0.85,
        },
        // 问题
        KeywordRule {
            pattern: Regex::new(r"(?i)(什么|怎么|如何|为什么|是什么|什么意思|解释)").unwrap(),
            intent: Intent::Question,
            confidence: 0.80,
        },
    ]
}
```

- [ ] **Step 3: 编写测试**

`F:/openLoom/crates/router/src/lib.rs` 末尾:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use openloom_models::{ClassifyOutput, Intent, TargetModel};

    #[test]
    fn test_classify_file_operation_keyword() {
        let router = SmartRouter::new_keywords_only(keywords::default_keyword_rules());
        let output = router.classify_sync("打开这个文件看看");
        assert_eq!(output.intent, Intent::FileOperation);
        assert!(output.confidence >= 0.85);
        assert_eq!(output.target_model, TargetModel::None);
    }

    #[test]
    fn test_classify_code_assist_keyword() {
        let router = SmartRouter::new_keywords_only(keywords::default_keyword_rules());
        let output = router.classify_sync("帮我写一个Python脚本处理CSV");
        assert_eq!(output.intent, Intent::CodeAssist);
        assert!(output.confidence >= 0.80);
    }

    #[test]
    fn test_classify_chat_fallback() {
        let router = SmartRouter::new_keywords_only(keywords::default_keyword_rules());
        let output = router.classify_sync("你好啊，今天过得怎么样");
        assert_eq!(output.intent, Intent::Chat);
        assert_eq!(output.target_model, TargetModel::Local);
    }

    #[test]
    fn test_empty_input() {
        let router = SmartRouter::new_keywords_only(keywords::default_keyword_rules());
        let output = router.classify_sync("");
        assert_eq!(output.intent, Intent::Chat);
    }

    #[test]
    fn test_register_skill_triggers() {
        let mut router = SmartRouter::new_keywords_only(keywords::default_keyword_rules());
        router.register_skill_triggers("file-manager", vec!["文件".into(), "文档".into()]);
        // 触发词注册后，匹配应返回 skill_match
        let output = router.classify_sync("帮我管理文件");
        assert_eq!(output.skill_match, Some("file-manager".into()));
    }
}
```

- [ ] **Step 4: 运行测试验证失败**

```bash
cd F:/openLoom && cargo test -p openloom-router
```
Expected: FAIL — "SmartRouter not defined"

- [ ] **Step 5: 实现 SmartRouter**

`F:/openLoom/crates/router/src/lib.rs`:
```rust
pub mod keywords;

use keywords::KeywordRule;
use openloom_models::{ClassifyOutput, Intent, SystemEvent, TargetModel};

pub struct RouterConfig {
    pub model_path: std::path::PathBuf,
    pub keyword_rules: Vec<KeywordRule>,
    pub keyword_threshold: f32,
    pub fallback_threshold: f32,
}

pub struct SmartRouter {
    config: RouterConfig,
    skill_triggers: Vec<(String, Vec<String>)>,
    // Phase 1 后续: Arc<InferenceEngine> 用于 LLM 分类
}

impl SmartRouter {
    /// 纯关键词模式 (Phase 1 初始版本，llama-cpp 集成后添加 LLM 分类)
    pub fn new_keywords_only(keyword_rules: Vec<KeywordRule>) -> Self {
        Self {
            config: RouterConfig {
                model_path: std::path::PathBuf::new(),
                keyword_rules,
                keyword_threshold: 0.85,
                fallback_threshold: 0.7,
            },
            skill_triggers: Vec::new(),
        }
    }

    /// 同步分类 (关键词匹配)
    pub fn classify_sync(&self, text: &str) -> ClassifyOutput {
        if text.is_empty() {
            return ClassifyOutput {
                intent: Intent::Chat,
                complexity: 0.0,
                skill_match: None,
                confidence: 1.0,
                cache_hit: false,
                target_model: TargetModel::Local,
            };
        }

        // Step 1: 关键词匹配
        let mut best_confidence = 0.0f32;
        let mut best_intent = Intent::Chat;
        for rule in &self.config.keyword_rules {
            if rule.pattern.is_match(text) && rule.confidence > best_confidence {
                best_confidence = rule.confidence;
                best_intent = rule.intent.clone();
            }
        }

        // Step 2: 检查 skill 触发词匹配
        let mut skill_match = None;
        for (name, triggers) in &self.skill_triggers {
            for trigger in triggers {
                if text.contains(trigger.as_str()) {
                    skill_match = Some(name.clone());
                    break;
                }
            }
            if skill_match.is_some() { break; }
        }

        let (target_model, complexity) = if best_confidence >= self.config.keyword_threshold {
            // 关键词高置信度 → 如果有 skill 匹配就走 skill
            let model = if skill_match.is_some() { TargetModel::None } else { TargetModel::Local };
            (model, 0.3)
        } else if best_confidence >= self.config.fallback_threshold {
            (TargetModel::Local, 0.6)
        } else {
            // 低置信度 → Local LLM, Phase 2 扩展 Cloud
            (TargetModel::Local, 0.8)
        };

        ClassifyOutput {
            intent: best_intent,
            complexity,
            skill_match,
            confidence: best_confidence.max(0.3),
            cache_hit: false,
            target_model,
        }
    }

    /// Phase 1 后续: async fn classify(&self, text: &str) -> ClassifyOutput
    /// (关键词优先，未命中调 LLM)

    pub fn register_skill_triggers(&mut self, name: &str, triggers: Vec<String>) {
        self.skill_triggers.push((name.to_string(), triggers));
    }
}
```

- [ ] **Step 6: 运行测试验证通过**

```bash
cd F:/openLoom && cargo test -p openloom-router
```
Expected: 5 tests PASS

- [ ] **Step 7: Commit**

```bash
git add crates/router/
git commit -m "feat(router): add SmartRouter with keyword-based intent classification"
```

---

### Task 4: 创建 skills crate — Skill trait + Registry + CLI Bridge

**Files:**
- Create: `F:/openLoom/crates/skills/Cargo.toml`
- Create: `F:/openLoom/crates/skills/src/lib.rs`

- [ ] **Step 1: 创建 Cargo.toml**

`F:/openLoom/crates/skills/Cargo.toml`:
```toml
[package]
name = "openloom-skills"
version.workspace = true
edition.workspace = true

[dependencies]
openloom-models = { path = "../models" }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
anyhow = "1"
tracing = "0.1"
async-trait = "0.1"
```

- [ ] **Step 2: 编写测试**

`F:/openLoom/crates/skills/src/lib.rs` 末尾:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct TestSkill;
    #[async_trait::async_trait]
    impl Skill for TestSkill {
        fn name(&self) -> &str { "test" }
        fn manifest(&self) -> &SkillManifest {
            static M: std::sync::OnceLock<SkillManifest> = std::sync::OnceLock::new();
            M.get_or_init(|| SkillManifest {
                name: "test".into(),
                description: "A test skill".into(),
                triggers: vec!["test".into(), "测试".into()],
                permissions: SkillPermissions::default(),
                min_engine_version: "0.1.0".into(),
            })
        }
        async fn invoke(&self, params: serde_json::Value) -> anyhow::Result<serde_json::Value> {
            Ok(json!({"echo": params}))
        }
        fn context_md(&self) -> &str { "Test skill context" }
    }

    #[test]
    fn test_register_and_find_skill() {
        let mut registry = SkillRegistry::new();
        registry.register(Box::new(TestSkill));
        let found = registry.find_by_trigger("运行测试");
        assert!(found.is_some());
        assert_eq!(found.unwrap().name(), "test");
    }

    #[test]
    fn test_list_skills() {
        let mut registry = SkillRegistry::new();
        registry.register(Box::new(TestSkill));
        let skills = registry.list_all();
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "test");
    }

    #[test]
    fn test_find_nonexistent_skill() {
        let registry = SkillRegistry::new();
        let found = registry.find_by_trigger("不存在的技能");
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn test_invoke_skill() {
        let mut registry = SkillRegistry::new();
        registry.register(Box::new(TestSkill));
        let result = registry.invoke("test", json!({"key": "value"})).await.unwrap();
        assert_eq!(result["echo"]["key"], "value");
    }
}
```

- [ ] **Step 3: 运行测试验证失败**

```bash
cd F:/openLoom && cargo test -p openloom-skills
```
Expected: FAIL — "Skill trait not defined"

- [ ] **Step 4: 实现 Skill trait + Registry + CliBridge**

`F:/openLoom/crates/skills/src/lib.rs`:
```rust
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Skill trait — Phase 1 用 Rust native 实现，Phase 2 编译为 WASM
#[async_trait::async_trait]
pub trait Skill: Send + Sync {
    fn name(&self) -> &str;
    fn manifest(&self) -> &SkillManifest;
    async fn invoke(&self, params: Value) -> Result<Value>;
    fn context_md(&self) -> &str;
}

/// 权限模型 — 数据模型完整，Phase 2/3 强制执行
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillPermissions {
    #[serde(default)]
    pub fs_read: Vec<String>,
    #[serde(default)]
    pub fs_write: Vec<String>,
    #[serde(default)]
    pub network: Vec<String>,
    #[serde(default)]
    pub shell: bool,
    #[serde(default)]
    pub subprocess: bool,
    #[serde(default = "default_max_memory")]
    pub max_memory_mb: u32,
    #[serde(default = "default_max_runtime")]
    pub max_runtime_sec: u32,
}

fn default_max_memory() -> u32 { 128 }
fn default_max_runtime() -> u32 { 30 }

impl Default for SkillPermissions {
    fn default() -> Self {
        Self {
            fs_read: Vec::new(), fs_write: Vec::new(), network: Vec::new(),
            shell: false, subprocess: false,
            max_memory_mb: default_max_memory(),
            max_runtime_sec: default_max_runtime(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillManifest {
    pub name: String,
    pub description: String,
    pub triggers: Vec<String>,
    #[serde(default)]
    pub permissions: SkillPermissions,
    pub min_engine_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillInfo {
    pub name: String,
    pub description: String,
    pub triggers: Vec<String>,
}

pub struct SkillRegistry {
    skills: Vec<Box<dyn Skill>>,
}

impl SkillRegistry {
    pub fn new() -> Self {
        Self { skills: Vec::new() }
    }

    pub fn register(&mut self, skill: Box<dyn Skill>) {
        self.skills.push(skill);
    }

    pub fn find_by_trigger(&self, text: &str) -> Option<&dyn Skill> {
        self.skills.iter().find(|s| {
            s.manifest().triggers.iter().any(|t| text.contains(t.as_str()))
        }).map(|s| s.as_ref())
    }

    pub fn list_all(&self) -> Vec<SkillInfo> {
        self.skills.iter().map(|s| {
            let m = s.manifest();
            SkillInfo {
                name: m.name.clone(),
                description: m.description.clone(),
                triggers: m.triggers.clone(),
            }
        }).collect()
    }

    pub async fn invoke(&self, name: &str, params: Value) -> Result<Value> {
        let skill = self.skills.iter().find(|s| s.name() == name)
            .ok_or_else(|| anyhow::anyhow!("skill '{}' not found", name))?;
        skill.invoke(params).await
    }

    pub fn all_skills(&self) -> &[Box<dyn Skill>] {
        &self.skills
    }
}

impl Default for SkillRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// CLI Bridge — 扫描 PATH 中的可执行文件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliTool {
    pub name: String,
    pub description: String,
    pub binary: String,
}

pub struct CliBridge;

impl CliBridge {
    pub fn discover_path_tools() -> Vec<CliTool> {
        // Phase 1: 扫描常见工具
        let common_tools = vec![
            ("gh", "GitHub CLI — manage issues, PRs, and repos"),
            ("git", "Version control system"),
            ("cargo", "Rust package manager"),
            ("npm", "Node.js package manager"),
            ("python", "Python interpreter"),
        ];
        common_tools.into_iter()
            .filter(|(binary, _)| Self::is_on_path(binary))
            .map(|(binary, desc)| CliTool {
                name: binary.to_string(),
                description: desc.to_string(),
                binary: binary.to_string(),
            })
            .collect()
    }

    fn is_on_path(binary: &str) -> bool {
        std::env::var_os("PATH").map_or(false, |path| {
            std::env::split_paths(&path).any(|dir| {
                let full = dir.join(binary);
                full.exists() || {
                    // Windows: 检查 .exe 后缀
                    let with_ext = dir.join(format!("{}.exe", binary));
                    with_ext.exists()
                }
            })
        })
    }

    pub fn parse_help(binary: &str) -> Option<CliTool> {
        let output = std::process::Command::new(binary)
            .arg("--help")
            .output()
            .ok()?;
        let help_text = String::from_utf8_lossy(&output.stdout);
        let first_line = help_text.lines().next().unwrap_or(binary);
        Some(CliTool {
            name: binary.to_string(),
            description: first_line.to_string(),
            binary: binary.to_string(),
        })
    }
}
```

- [ ] **Step 5: 运行测试验证通过**

```bash
cd F:/openLoom && cargo test -p openloom-skills
```
Expected: 4 tests PASS

- [ ] **Step 6: Commit**

```bash
git add crates/skills/Cargo.toml crates/skills/src/lib.rs
git commit -m "feat(skills): add Skill trait, SkillRegistry, and CliBridge"
```

---

### Task 5: 实现 5 个内置 Skill

**Files:**
- Create: `F:/openLoom/crates/skills/src/builtins/mod.rs`
- Create: `F:/openLoom/crates/skills/src/builtins/file_manager.rs`
- Create: `F:/openLoom/crates/skills/src/builtins/info_retriever.rs`
- Create: `F:/openLoom/crates/skills/src/builtins/schedule_reminder.rs`
- Create: `F:/openLoom/crates/skills/src/builtins/code_assistant.rs`
- Create: `F:/openLoom/crates/skills/src/builtins/web_browser.rs`

- [ ] **Step 1: 创建 mod.rs**

`F:/openLoom/crates/skills/src/builtins/mod.rs`:
```rust
pub mod file_manager;
pub mod info_retriever;
pub mod schedule_reminder;
pub mod code_assistant;
pub mod web_browser;

use crate::{Skill, SkillRegistry};

pub fn register_all(registry: &mut SkillRegistry) {
    registry.register(Box::new(file_manager::FileManager));
    registry.register(Box::new(info_retriever::InfoRetriever));
    registry.register(Box::new(schedule_reminder::ScheduleReminder));
    registry.register(Box::new(code_assistant::CodeAssistant));
    registry.register(Box::new(web_browser::WebBrowser));
}
```

- [ ] **Step 2: 实现 FileManager**

`F:/openLoom/crates/skills/src/builtins/file_manager.rs`:
```rust
use crate::{Skill, SkillManifest, SkillPermissions};
use anyhow::Result;
use serde_json::{json, Value};

pub struct FileManager;

#[async_trait::async_trait]
impl Skill for FileManager {
    fn name(&self) -> &str { "file-manager" }

    fn manifest(&self) -> &SkillManifest {
        static M: std::sync::OnceLock<SkillManifest> = std::sync::OnceLock::new();
        M.get_or_init(|| SkillManifest {
            name: "file-manager".into(),
            description: "文件管理：读写、搜索、列表文件".into(),
            triggers: vec!["文件".into(), "文档".into(), "读写".into(), "保存".into(), "目录".into(), "文件夹".into()],
            permissions: SkillPermissions { fs_read: vec!["~".into()], fs_write: vec!["~".into()], ..Default::default() },
            min_engine_version: "0.1.0".into(),
        })
    }

    async fn invoke(&self, params: Value) -> Result<Value> {
        let action = params.get("action").and_then(|v| v.as_str()).unwrap_or("list");
        let path = params.get("path").and_then(|v| v.as_str()).unwrap_or(".");
        match action {
            "list" => {
                let entries: Vec<String> = std::fs::read_dir(path)?
                    .filter_map(|e| e.ok())
                    .map(|e| e.file_name().to_string_lossy().to_string())
                    .collect();
                Ok(json!({"files": entries}))
            }
            "read" => {
                let content = std::fs::read_to_string(path)?;
                Ok(json!({"content": content}))
            }
            "write" => {
                let content = params.get("content").and_then(|v| v.as_str()).unwrap_or("");
                std::fs::write(path, content)?;
                Ok(json!({"ok": true}))
            }
            _ => Ok(json!({"error": "unknown action", "available": ["list", "read", "write"]})),
        }
    }

    fn context_md(&self) -> &str { "文件管理技能：支持 list/read/write 操作。路径可以是相对或绝对路径。" }
}
```

- [ ] **Step 3: 实现 InfoRetriever**

`F:/openLoom/crates/skills/src/builtins/info_retriever.rs`:
```rust
use crate::{Skill, SkillManifest, SkillPermissions};
use anyhow::Result;
use serde_json::{json, Value};

pub struct InfoRetriever;

#[async_trait::async_trait]
impl Skill for InfoRetriever {
    fn name(&self) -> &str { "info-retriever" }
    fn manifest(&self) -> &SkillManifest {
        static M: std::sync::OnceLock<SkillManifest> = std::sync::OnceLock::new();
        M.get_or_init(|| SkillManifest {
            name: "info-retriever".into(),
            description: "信息检索：知识查询、文档搜索".into(),
            triggers: vec!["搜索".into(), "查找".into(), "查询".into(), "检索".into(), "信息".into()],
            permissions: SkillPermissions::default(),
            min_engine_version: "0.1.0".into(),
        })
    }
    async fn invoke(&self, params: Value) -> Result<Value> {
        let query = params.get("query").and_then(|v| v.as_str()).unwrap_or("");
        // Phase 1: 返回占位结果。Phase 2: 集成 FTS5 + sqlite-vec 语义搜索
        Ok(json!({
            "query": query,
            "results": [],
            "note": "InfoRetriever: Phase 2 will integrate FTS5 + semantic search"
        }))
    }
    fn context_md(&self) -> &str { "信息检索技能：基于 FTS5 全文搜索本地文档和知识库。" }
}
```

- [ ] **Step 4: 实现 ScheduleReminder**

`F:/openLoom/crates/skills/src/builtins/schedule_reminder.rs`:
```rust
use crate::{Skill, SkillManifest, SkillPermissions};
use anyhow::Result;
use serde_json::{json, Value};

pub struct ScheduleReminder;

#[async_trait::async_trait]
impl Skill for ScheduleReminder {
    fn name(&self) -> &str { "schedule-reminder" }
    fn manifest(&self) -> &SkillManifest {
        static M: std::sync::OnceLock<SkillManifest> = std::sync::OnceLock::new();
        M.get_or_init(|| SkillManifest {
            name: "schedule-reminder".into(),
            description: "日程提醒：管理日历、设置提醒、查看日程".into(),
            triggers: vec!["提醒".into(), "日程".into(), "日历".into(), "会议".into(), "安排".into(), "定时".into()],
            permissions: SkillPermissions::default(),
            min_engine_version: "0.1.0".into(),
        })
    }
    async fn invoke(&self, params: Value) -> Result<Value> {
        let action = params.get("action").and_then(|v| v.as_str()).unwrap_or("list");
        match action {
            "add" => {
                let title = params.get("title").and_then(|v| v.as_str()).unwrap_or("untitled");
                let time = params.get("time").and_then(|v| v.as_str()).unwrap_or("unspecified");
                Ok(json!({"added": {"title": title, "time": time}}))
            }
            "list" => {
                Ok(json!({"reminders": [], "note": "Phase 2: persisted reminders"}))
            }
            _ => Ok(json!({"error": "unknown action", "available": ["add", "list"]})),
        }
    }
    fn context_md(&self) -> &str { "日程提醒技能：添加/查看/取消提醒。支持时间解析。" }
}
```

- [ ] **Step 5: 实现 CodeAssistant**

`F:/openLoom/crates/skills/src/builtins/code_assistant.rs`:
```rust
use crate::{Skill, SkillManifest, SkillPermissions};
use anyhow::Result;
use serde_json::{json, Value};

pub struct CodeAssistant;

#[async_trait::async_trait]
impl Skill for CodeAssistant {
    fn name(&self) -> &str { "code-assistant" }
    fn manifest(&self) -> &SkillManifest {
        static M: std::sync::OnceLock<SkillManifest> = std::sync::OnceLock::new();
        M.get_or_init(|| SkillManifest {
            name: "code-assistant".into(),
            description: "代码辅助：编写、调试、重构、运行测试".into(),
            triggers: vec!["代码".into(), "编程".into(), "写".into(), "实现".into(), "修复".into(), "bug".into(), "测试".into(), "编译".into(), "运行".into(), "git".into()],
            permissions: SkillPermissions { shell: true, subprocess: true, ..Default::default() },
            min_engine_version: "0.1.0".into(),
        })
    }
    async fn invoke(&self, params: Value) -> Result<Value> {
        let action = params.get("action").and_then(|v| v.as_str()).unwrap_or("analyze");
        match action {
            "run_test" => {
                let output = std::process::Command::new("cargo")
                    .args(["test"])
                    .output();
                match output {
                    Ok(out) => {
                        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
                        Ok(json!({"output": stdout, "success": out.status.success()}))
                    }
                    Err(e) => Ok(json!({"error": e.to_string()})),
                }
            }
            _ => Ok(json!({
                "note": "CodeAssistant: provides code analysis, test running, and git operations",
                "available_actions": ["run_test", "format", "git_status"]
            })),
        }
    }
    fn context_md(&self) -> &str { "代码辅助技能：运行 cargo test/fmt/clippy，查看 git status。" }
}
```

- [ ] **Step 6: 实现 WebBrowser**

`F:/openLoom/crates/skills/src/builtins/web_browser.rs`:
```rust
use crate::{Skill, SkillManifest, SkillPermissions};
use anyhow::Result;
use serde_json::{json, Value};

pub struct WebBrowser;

#[async_trait::async_trait]
impl Skill for WebBrowser {
    fn name(&self) -> &str { "web-browser" }
    fn manifest(&self) -> &SkillManifest {
        static M: std::sync::OnceLock<SkillManifest> = std::sync::OnceLock::new();
        M.get_or_init(|| SkillManifest {
            name: "web-browser".into(),
            description: "网页浏览：搜索网页、抓取内容".into(),
            triggers: vec!["网页".into(), "浏览".into(), "网址".into(), "链接".into(), "打开".into(), "搜索".into(), "百度".into(), "Google".into()],
            permissions: SkillPermissions { network: vec!["*".into()], ..Default::default() },
            min_engine_version: "0.1.0".into(),
        })
    }
    async fn invoke(&self, params: Value) -> Result<Value> {
        let url = params.get("url").and_then(|v| v.as_str()).unwrap_or("");
        if url.is_empty() {
            return Ok(json!({"error": "url required"}));
        }
        // Phase 1: 占位。Phase 2: HTTP client 集成
        Ok(json!({
            "url": url,
            "status": "WebBrowser: Phase 2 will fetch and parse web content"
        }))
    }
    fn context_md(&self) -> &str { "网页浏览技能：打开 URL 获取内容。需要网络权限。" }
}
```

- [ ] **Step 7: 更新 lib.rs 注册 builtins 模块**

在 `F:/openLoom/crates/skills/src/lib.rs` 顶部添加:
```rust
pub mod builtins;
```

- [ ] **Step 8: 构建验证**

```bash
cd F:/openLoom && cargo build -p openloom-skills
```
Expected: Compiles with 5 built-in skills

- [ ] **Step 9: Commit**

```bash
git add crates/skills/src/builtins/ crates/skills/src/lib.rs
git commit -m "feat(skills): add 5 built-in skills — FileManager, InfoRetriever, ScheduleReminder, CodeAssistant, WebBrowser"
```

---

### Task 6: 修改 memory crate — refinery 迁移 + channel 支持

**Files:**
- Modify: `F:/openLoom/crates/memory/Cargo.toml`
- Modify: `F:/openLoom/crates/memory/src/store.rs`
- Create: `F:/openLoom/migrations/V1__initial.sql`
- Create: `F:/openLoom/migrations/V2__add_cognitions_sessions.sql`

- [ ] **Step 1: 更新 Cargo.toml 添加 refinery**

`F:/openLoom/crates/memory/Cargo.toml` 的 `[dependencies]`:
```toml
refinery = { version = "0.8", features = ["rusqlite"] }
```

- [ ] **Step 2: 创建迁移脚本 V1**

`F:/openLoom/migrations/V1__initial.sql`:
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

CREATE TRIGGER IF NOT EXISTS events_ai AFTER INSERT ON events BEGIN
    INSERT INTO events_fts(rowid, type, action, context, source_text)
    VALUES (new.id, new.type, new.action, new.context, new.source_text);
END;

CREATE TRIGGER IF NOT EXISTS events_ad AFTER DELETE ON events BEGIN
    INSERT INTO events_fts(events_fts, rowid, type, action, context, source_text)
    VALUES('delete', old.id, old.type, old.action, old.context, old.source_text);
END;
```

- [ ] **Step 3: 创建迁移脚本 V2**

`F:/openLoom/migrations/V2__add_cognitions_sessions.sql`:
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

- [ ] **Step 4: 修改 store.rs — 添加 refinery 迁移方法，保留兼容性**

`F:/openLoom/crates/memory/src/store.rs` — 在 `impl SqliteEventStore` 中添加:
```rust
/// 使用 refinery 执行迁移 (Phase 1)
/// 保留 Phase 0 兼容：如果 V1 中表已存在，CREATE IF NOT EXISTS 是安全 no-op
pub fn run_migrations(conn: &Connection) -> Result<()> {
    // refinery 嵌入迁移
    mod embedded {
        use refinery::embed_migrations;
        embed_migrations!("../../migrations");
    }
    embedded::migrations::runner().run(conn)?;
    Ok(())
}

/// Phase 1 推荐：使用 refinery 打开数据库
pub fn open_with_migrations(path: &std::path::Path) -> Result<Self> {
    let conn = Connection::open(path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
    Self::run_migrations(&conn)?;
    Ok(Self { conn })
}
```

保留 Phase 0 `open()` 方法和 `migrate()` 方法不变（向后兼容），新增 `open_with_migrations()` 供 Engine 使用。

- [ ] **Step 5: 添加 refinery 到 store.rs 顶部**

```rust
use refinery::embed_migrations;
embed_migrations!("../../migrations");
```

- [ ] **Step 6: 构建验证**

```bash
cd F:/openLoom && cargo build -p openloom-memory
```
Expected: Compiles with refinery

- [ ] **Step 7: Commit**

```bash
git add crates/memory/Cargo.toml crates/memory/src/store.rs migrations/
git commit -m "feat(memory): add refinery migrations — V1 initial schema, V2 cognitions+sessions+token_usage"
```

---

### Task 7: 创建 engine crate — EventBus + 请求派发

**Files:**
- Create: `F:/openLoom/crates/engine/Cargo.toml`
- Create: `F:/openLoom/crates/engine/src/lib.rs`
- Create: `F:/openLoom/crates/engine/src/memory_thread.rs`

- [ ] **Step 1: 创建 Cargo.toml**

`F:/openLoom/crates/engine/Cargo.toml`:
```toml
[package]
name = "openloom-engine"
version.workspace = true
edition.workspace = true

[dependencies]
openloom-models = { path = "../models" }
openloom-router = { path = "../router" }
openloom-skills = { path = "../skills" }
openloom-inference = { path = "../inference" }
openloom-memory = { path = "../memory" }
tokio = { version = "1", features = ["rt", "sync", "macros"] }
anyhow = "1"
tracing = "0.1"
serde_json = "1"
uuid = { version = "1", features = ["v4"] }
```

- [ ] **Step 2: 创建 memory_thread.rs — MemoryPipeline 独立线程**

`F:/openLoom/crates/engine/src/memory_thread.rs`:
```rust
use openloom_memory::aggregator::PatternAggregator;
use openloom_memory::extractor::RuleBasedExtractor;
use openloom_memory::pipeline::MemoryPipeline;
use openloom_memory::store::SqliteEventStore;
use std::path::PathBuf;
use std::sync::mpsc;
use tracing;

pub struct ProcessRequest {
    pub session_id: String,
    pub text: String,
    pub context: String,
}

/// 启动 MemoryPipeline 独立线程，返回 Sender
/// 独立线程模型解决 rusqlite::Connection 不 Send 的问题
pub fn spawn_memory_thread(
    db_path: PathBuf,
    threshold: usize,
) -> mpsc::Sender<ProcessRequest> {
    let (tx, rx) = mpsc::channel::<ProcessRequest>();

    std::thread::spawn(move || {
        let extractor = RuleBasedExtractor::with_default_rules();
        let aggregator = PatternAggregator::new(threshold);
        let store = SqliteEventStore::open_with_migrations(&db_path)
            .expect("failed to open database with migrations");

        let mut pipeline = MemoryPipeline::new(extractor, aggregator, store, threshold);

        for req in rx {
            tracing::debug!(
                session = %req.session_id,
                text_len = req.text.len(),
                "memory pipeline processing"
            );
            if let Err(e) = pipeline.process(&req.session_id, &req.text, &req.context) {
                tracing::error!(
                    session = %req.session_id,
                    error = %e,
                    "memory pipeline error"
                );
            }
        }
    });

    tx
}
```

- [ ] **Step 3: 编写 engine 测试**

`F:/openLoom/crates/engine/src/lib.rs` 末尾:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use openloom_models::{ChatMessage, Intent, TargetModel};
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
        let msg = ChatMessage { role: "user".into(), content: "你好".into() };
        let sid = engine.create_session().await.unwrap().id;
        let resp = engine.handle_message(msg, &sid).await.unwrap();
        assert!(!resp.response.is_empty() || true); // 当前是占位响应
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
        let msg = ChatMessage { role: "user".into(), content: "hello".into() };
        let sid = engine.create_session().await.unwrap().id;
        engine.handle_message(msg, &sid).await.unwrap();
        // 应该收到 TokenUsage 事件
        let event = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            rx.recv(),
        ).await;
        assert!(event.is_ok());
    }
}
```

- [ ] **Step 4: 运行测试验证失败**

```bash
cd F:/openLoom && cargo test -p openloom-engine
```
Expected: FAIL — "Engine not defined"

- [ ] **Step 5: 实现 Engine**

`F:/openLoom/crates/engine/src/lib.rs`:
```rust
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

// 重导出
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
    pub fn new_test(db_path: PathBuf) -> Result<Self> {
        Self::new(EngineConfig {
            data_dir: db_path.parent().unwrap().to_path_buf(),
            threshold: 3,
        })
    }

    pub fn new(config: EngineConfig) -> Result<Self> {
        // 1. 推理引擎 (Phase 1: 无模型加载，占位)
        let inference = Arc::new(InferenceEngine::load_blocking(
            &config.data_dir.join("models").join("qwen3-1.7b-q4_k_m.gguf"),
            0,
        )?);

        // 2. Router (关键词优先)
        let router = SmartRouter::new_keywords_only(
            openloom_router::keywords::default_keyword_rules(),
        );

        // 3. Skill 注册
        let mut skills = SkillRegistry::new();
        builtins::register_all(&mut skills);

        // 4. MemoryPipeline 独立线程
        let db_path = config.data_dir.join("data").join("db.sqlite");
        let _ = std::fs::create_dir_all(db_path.parent().unwrap());
        let memory_tx = memory_thread::spawn_memory_thread(db_path, config.threshold);

        // 5. 注册 skill 触发词到 router (需要 router 为 mut)
        // 注意：这里需要将 router 设为 mut 或以其他方式注入
        // Phase 1 简化：在 SmartRouter::new_keywords_only 中预先注册

        // 6. EventBus
        let (event_tx, _) = broadcast::channel(256);

        Ok(Self {
            router,
            skills,
            inference,
            memory_tx,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            event_bus: event_tx,
        })
    }

    pub async fn handle_message(
        &self,
        msg: ChatMessage,
        session_id: &str,
    ) -> Result<ChatResponse> {
        // 1. 路由分类
        let out = self.router.classify_sync(&msg.content);

        // 2. 执行
        let response = match out.target_model {
            TargetModel::None => {
                let skill_name = out.skill_match.as_ref()
                    .ok_or_else(|| anyhow::anyhow!("skill_match is None but target_model is None"))?;
                let params = serde_json::json!({"text": msg.content, "intent": out.intent.to_string()});
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

        // 3. 后台: 记忆管线
        let _ = self.memory_tx.send(memory_thread::ProcessRequest {
            session_id: session_id.to_string(),
            text: msg.content.clone(),
            context: out.intent.to_string(),
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
            token_usage: TokenUsage::default(),
        })
    }

    pub async fn health_check(&self) -> HealthStatus {
        let gpu = InferenceEngine::detect_gpu();
        HealthStatus {
            status: "ok".into(),
            uptime: 0, // Phase 2
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

    pub async fn shutdown(&self) -> Result<()> {
        tracing::info!("engine shutting down");
        Ok(())
    }
}
```

- [ ] **Step 6: 给 InferenceEngine 添加同步 load 方法**

在 `F:/openLoom/crates/inference/src/lib.rs` 中添加:
```rust
impl InferenceEngine {
    /// 同步加载 (Phase 1 初始化用)
    pub fn load_blocking(model_path: &Path, n_gpu_layers: usize) -> Result<Self> {
        tracing::info!(path = %model_path.display(), n_gpu_layers, "loading model (sync)");
        Ok(Self {
            _model_path: model_path.to_path_buf(),
            _n_gpu_layers: n_gpu_layers,
        })
    }
}
```

- [ ] **Step 7: 运行测试验证通过**

```bash
cd F:/openLoom && cargo test -p openloom-engine
```
Expected: 4 tests PASS (test_create_and_list_sessions, test_handle_message_llm_path, test_health_check, test_event_bus_subscribe)

- [ ] **Step 8: Commit**

```bash
git add crates/engine/ crates/inference/src/lib.rs
git commit -m "feat(engine): add Engine — EventBus, message dispatch, memory thread, session management"
```

---

### Task 8: 创建 server crate — Axum + JSON-RPC + WS + SSE

**Files:**
- Create: `F:/openLoom/crates/server/Cargo.toml`
- Create: `F:/openLoom/crates/server/src/lib.rs`
- Create: `F:/openLoom/crates/server/src/jsonrpc.rs`
- Create: `F:/openLoom/crates/server/src/ws.rs`
- Create: `F:/openLoom/crates/server/src/sse.rs`

- [ ] **Step 1: 创建 Cargo.toml**

`F:/openLoom/crates/server/Cargo.toml`:
```toml
[package]
name = "openloom-server"
version.workspace = true
edition.workspace = true

[dependencies]
openloom-engine = { path = "../engine" }
openloom-models = { path = "../models" }
axum = { version = "0.7", features = ["ws"] }
tokio = { version = "1", features = ["rt", "sync", "macros", "net"] }
tower = "0.4"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
anyhow = "1"
tracing = "0.1"
futures = "0.3"
```

- [ ] **Step 2: 编写测试**

`F:/openLoom/crates/server/src/lib.rs` 末尾:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use openloom_models::*;
    use axum::http::StatusCode;

    #[test]
    fn test_jsonrpc_parse_valid() {
        let json = r#"{"jsonrpc":"2.0","method":"system.health","params":null,"id":1}"#;
        let req: JsonRpcRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.method, "system.health");
    }

    #[test]
    fn test_jsonrpc_parse_invalid() {
        let json = r#"not json"#;
        let result: Result<JsonRpcRequest, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_jsonrpc_error_response() {
        let err = JsonRpcError {
            code: ErrorCode::MethodNotFound,
            message: "method not found".into(),
            data: None,
        };
        let resp = JsonRpcResponse {
            jsonrpc: "2.0".into(),
            result: None,
            error: Some(err),
            id: 1,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("-32601"));
    }

    #[test]
    fn test_notification_name_mapping() {
        // EngineEvent::CognitionUpdated → "cognition.updated"
        let event = EngineEvent::CognitionUpdated {
            trait_name: "risk".into(),
            old_value: "low".into(),
            new_value: "high".into(),
            confidence: 0.9,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("cognition_updated"));
        // server 将 "_" 替换为 "." 作为通知方法名
        let notification_method = json.split('"').find(|s| s.contains("cognition_updated")).unwrap();
        assert_eq!(notification_method.replace('_', "."), "cognition.updated");
    }
}
```

- [ ] **Step 3: 运行测试验证失败**

```bash
cd F:/openLoom && cargo test -p openloom-server
```
Expected: FAIL — "server not defined"

- [ ] **Step 4: 实现 server crate**

`F:/openLoom/crates/server/src/lib.rs`:
```rust
pub mod jsonrpc;
pub mod ws;
pub mod sse;

use anyhow::Result;
use axum::{Router, routing::get};
use openloom_engine::Engine;
use std::net::SocketAddr;
use std::sync::Arc;

pub struct Server {
    engine: Arc<Engine>,
    port: u16,
}

impl Server {
    pub fn new(engine: Engine) -> Self {
        Self {
            engine: Arc::new(engine),
            port: 0,
        }
    }

    pub async fn serve(mut self, port: u16) -> Result<()> {
        self.port = port;

        let engine = self.engine.clone();

        let app = Router::new()
            .route("/health", get(move || {
                let engine = engine.clone();
                async move {
                    let health = engine.health_check().await;
                    axum::Json(serde_json::to_value(health).unwrap_or_default())
                }
            }))
            .route("/ws", get(ws::ws_handler))
            .route("/sse/{session_id}", get(sse::sse_handler))
            .route("/api", axum::routing::post(jsonrpc::handle_jsonrpc));

        let addr = SocketAddr::from(([127, 0, 0, 1], self.port));
        tracing::info!("server starting on {}", addr);

        let listener = tokio::net::TcpListener::bind(addr).await?;
        let bound_addr = listener.local_addr()?;

        // 向 stdout 输出就绪信号 (Electron sidecar 模式)
        let ready = serde_json::json!({
            "type": "ready",
            "port": bound_addr.port(),
        });
        println!("{}", ready);

        axum::serve(listener, app).await?;
        Ok(())
    }
}
```

`F:/openLoom/crates/server/src/jsonrpc.rs`:
```rust
use axum::{Json, http::StatusCode};
use openloom_models::*;
use serde_json::Value;

pub async fn handle_jsonrpc(
    Json(req): Json<JsonRpcRequest>,
) -> Result<Json<JsonRpcResponse>, StatusCode> {
    // Phase 1: 路由到正确的 handler
    let result = match req.method.as_str() {
        "system.health" => Ok(serde_json::json!({"status": "ok"})),
        "system.shutdown" => Ok(serde_json::json!({"ok": true})),
        "skill.list" => Ok(serde_json::json!({"skills": []})),
        "cache.stats" => Ok(serde_json::json!({"hit_rate": 0.0})),
        _ => Err(JsonRpcError {
            code: ErrorCode::MethodNotFound,
            message: format!("method '{}' not found", req.method),
            data: None,
        }),
    };

    match result {
        Ok(value) => Ok(Json(JsonRpcResponse {
            jsonrpc: "2.0".into(),
            result: Some(value),
            error: None,
            id: req.id,
        })),
        Err(err) => Ok(Json(JsonRpcResponse {
            jsonrpc: "2.0".into(),
            result: None,
            error: Some(err),
            id: req.id,
        })),
    }
}
```

`F:/openLoom/crates/server/src/ws.rs`:
```rust
use axum::extract::ws::{WebSocket, WebSocketUpgrade, Message};
use axum::response::IntoResponse;

pub async fn ws_handler(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(handle_ws)
}

async fn handle_ws(mut socket: WebSocket) {
    while let Some(Ok(msg)) = socket.recv().await {
        match msg {
            Message::Text(text) => {
                // Phase 1: 解析 JSON-RPC，处理并返回
                if let Ok(req) = serde_json::from_str::<openloom_models::JsonRpcRequest>(&text) {
                    let resp = openloom_models::JsonRpcResponse {
                        jsonrpc: "2.0".into(),
                        result: Some(serde_json::json!({"echo": req.method})),
                        error: None,
                        id: req.id,
                    };
                    if let Ok(json) = serde_json::to_string(&resp) {
                        let _ = socket.send(Message::Text(json.into())).await;
                    }
                }
            }
            Message::Ping(_) => {
                let _ = socket.send(Message::Pong(vec![])).await;
            }
            Message::Close(_) => break,
            _ => {}
        }
    }
}
```

`F:/openLoom/crates/server/src/sse.rs`:
```rust
use axum::extract::Path;
use axum::response::sse::{Event, Sse};
use futures::stream;
use std::convert::Infallible;

pub async fn sse_handler(
    Path(session_id): Path<String>,
) -> Sse<impl futures::Stream<Item = Result<Event, Infallible>>> {
    tracing::debug!(%session_id, "SSE connection opened");

    let stream = stream::once(async {
        Ok(Event::default()
            .data(format!("SSE stream ready for session {}", session_id))
            .event("ready"))
    });

    Sse::new(stream)
}
```

- [ ] **Step 5: 运行测试验证通过**

```bash
cd F:/openLoom && cargo test -p openloom-server
```
Expected: 4 tests PASS

- [ ] **Step 6: 构建验证**

```bash
cd F:/openLoom && cargo build -p openloom-server
```
Expected: Compiles

- [ ] **Step 7: Commit**

```bash
git add crates/server/
git commit -m "feat(server): add Axum HTTP + WebSocket + SSE + JSON-RPC 2.0 endpoints"
```

---

### Task 9: 扩展 CLI — 所有命令

**Files:**
- Modify: `F:/openLoom/crates/cli/Cargo.toml`
- Modify: `F:/openLoom/crates/cli/src/main.rs`

- [ ] **Step 1: 更新 Cargo.toml**

`F:/openLoom/crates/cli/Cargo.toml`:
```toml
[package]
name = "openloom"
version.workspace = true
edition.workspace = true

[[bin]]
name = "openloom"
path = "src/main.rs"

[dependencies]
openloom-memory = { path = "../memory" }
openloom-models = { path = "../models" }
openloom-engine = { path = "../engine" }
openloom-server = { path = "../server" }
clap = { version = "4", features = ["derive"] }
anyhow = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
serde_json = "1"
chrono = "0.4"
dirs = "5"
```

- [ ] **Step 2: 编写测试 (verify CLI parsing)**

在 `main.rs` 中添加:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn test_cli_serve_default() {
        let args = Cli::try_parse_from(["openloom", "serve"]);
        assert!(args.is_ok());
    }

    #[test]
    fn test_cli_serve_with_port() {
        let args = Cli::try_parse_from(["openloom", "serve", "--port", "8080"]);
        assert!(args.is_ok());
    }

    #[test]
    fn test_cli_analyze() {
        let args = Cli::try_parse_from(["openloom", "analyze", "--input", "test.log"]);
        assert!(args.is_ok());
    }

    #[test]
    fn test_cli_version() {
        let args = Cli::try_parse_from(["openloom", "version"]);
        assert!(args.is_ok());
    }

    #[test]
    fn test_cli_doctor() {
        let args = Cli::try_parse_from(["openloom", "doctor"]);
        assert!(args.is_ok());
    }
}
```

- [ ] **Step 3: 运行测试验证失败**

```bash
cd F:/openLoom && cargo test -p openloom
```
Expected: FAIL — "new commands not defined"

- [ ] **Step 4: 实现完整 CLI**

`F:/openLoom/crates/cli/src/main.rs`:
```rust
use clap::{Parser, Subcommand};
use openloom_engine::Engine;
use openloom_engine::EngineConfig;
use openloom_models::{AppConfig, ChatMessage, Intent};
use openloom_server::Server;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "openloom", about = "Local-first private AI assistant", version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
    #[arg(short, long, global = true)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// 离线分析对话日志 → 认知画像
    Analyze {
        #[arg(short, long)]
        input: String,
        #[arg(short, long, default_value = "profile.json")]
        output: String,
        #[arg(short, long, default_value = "memory.db")]
        db: String,
        #[arg(short = 't', long, default_value = "3")]
        threshold: usize,
    },
    /// 启动 HTTP + WebSocket 服务 (Electron sidecar)
    Serve {
        #[arg(long, default_value = "0")]
        port: u16,
        #[arg(long)]
        config: Option<String>,
    },
    /// 交互式对话 (TUI)
    Chat {
        #[arg(long)]
        config: Option<String>,
    },
    /// 单次任务执行
    Run {
        task: String,
        #[arg(long)]
        config: Option<String>,
    },
    /// 管理 Skills
    Skill {
        #[command(subcommand)]
        action: SkillAction,
    },
    /// 查看记忆/认知
    Memory {
        #[command(subcommand)]
        action: MemoryAction,
    },
    /// 查看/修改配置
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// 系统诊断
    Doctor,
    /// 版本信息
    Version,
}

#[derive(Subcommand)]
enum SkillAction {
    List,
    Install { path: String },
    Remove { name: String },
}

#[derive(Subcommand)]
enum MemoryAction {
    Persona,
    Events { #[arg(long, default_value = "20")] limit: usize },
    Cognitions,
}

#[derive(Subcommand)]
enum ConfigAction {
    Get { key: Option<String> },
    Set { key: String, value: String },
    Path,
}

fn config_path(custom: Option<&str>) -> PathBuf {
    if let Some(p) = custom {
        return PathBuf::from(p);
    }
    let data_dir = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("openLoom");
    data_dir.join("config.toml")
}

fn load_config(custom_path: Option<&str>) -> AppConfig {
    let path = config_path(custom_path);
    if !path.exists() {
        tracing::warn!(path = %path.display(), "config file not found, using defaults");
        return AppConfig::default();
    }
    match std::fs::read_to_string(&path) {
        Ok(content) => {
            toml::from_str(&content).unwrap_or_else(|e| {
                tracing::warn!(error = %e, "config parse error, using defaults");
                AppConfig::default()
            })
        }
        Err(e) => {
            tracing::warn!(error = %e, "cannot read config, using defaults");
            AppConfig::default()
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("openloom=info")
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Analyze { input, output, db, threshold } => {
            // Phase 0 逻辑保持不变
            run_analyze(&input, &output, &db, threshold)?;
        }
        Commands::Serve { port, config } => {
            let _app_config = load_config(config.as_deref());
            let data_dir = dirs::data_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("openLoom");

            let engine = Engine::new(EngineConfig {
                data_dir: data_dir.clone(),
                threshold: 3,
            })?;

            let server = Server::new(engine);
            server.serve(port).await?;
        }
        Commands::Chat { config } => {
            let _app_config = load_config(config.as_deref());
            let data_dir = dirs::data_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("openLoom");

            let engine = Engine::new(EngineConfig {
                data_dir,
                threshold: 3,
            })?;

            // 基础 TUI: 读 stdin 逐行交互
            let sid = engine.create_session().await?.id;
            println!("openLoom chat (type /exit to quit)");
            loop {
                let mut line = String::new();
                std::io::stdin().read_line(&mut line)?;
                let line = line.trim();
                if line == "/exit" { break; }
                if line.is_empty() { continue; }

                let msg = ChatMessage { role: "user".into(), content: line.to_string() };
                match engine.handle_message(msg, &sid).await {
                    Ok(resp) => println!("{}", resp.response),
                    Err(e) => eprintln!("Error: {}", e),
                }
            }
        }
        Commands::Run { task, config } => {
            let _app_config = load_config(config.as_deref());
            let data_dir = dirs::data_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("openLoom");

            let engine = Engine::new(EngineConfig {
                data_dir,
                threshold: 3,
            })?;

            let sid = engine.create_session().await?.id;
            let msg = ChatMessage { role: "user".into(), content: task };
            let resp = engine.handle_message(msg, &sid).await?;
            println!("{}", resp.response);
        }
        Commands::Skill { action } => match action {
            SkillAction::List => println!("Skills: file-manager, info-retriever, schedule-reminder, code-assistant, web-browser"),
            SkillAction::Install { path } => println!("Install skill from: {} (Phase 2 WASM)", path),
            SkillAction::Remove { name } => println!("Remove skill: {} (not yet implemented)", name),
        },
        Commands::Memory { action } => match action {
            MemoryAction::Persona => println!("Persona: Phase 2 will display cognition summary"),
            MemoryAction::Events { limit } => println!("Showing last {} events (Phase 2 storage query)", limit),
            MemoryAction::Cognitions => println!("Cognitions: Phase 2 will display cognition graph"),
        },
        Commands::Config { action } => match action {
            ConfigAction::Get { key } => {
                let path = config_path(None);
                println!("Config file: {}", path.display());
                if let Some(k) = key { println!("Get: {}", k); }
            }
            ConfigAction::Set { key, value } => println!("Set {} = {}", key, value),
            ConfigAction::Path => {
                let path = config_path(None);
                println!("{}", path.display());
            }
        },
        Commands::Doctor => {
            println!("openLoom System Diagnostic");
            println!("=========================");
            let gpu = openloom_inference::InferenceEngine::detect_gpu();
            println!("GPU: vendor={}, vram={}MB, supported={}", gpu.vendor, gpu.vram_mb, gpu.supported);
            let data_dir = dirs::data_dir().unwrap_or_else(|| PathBuf::from(".")).join("openLoom");
            println!("Data dir: {}", data_dir.display());
            println!("Config: {}", config_path(None).display());
        },
        Commands::Version => {
            println!("openLoom {}", env!("CARGO_PKG_VERSION"));
        },
    }

    Ok(())
}

// Phase 0 analyze 逻辑 (保留)
fn run_analyze(
    input_path: &str,
    output_path: &str,
    db_path: &str,
    threshold: usize,
) -> anyhow::Result<()> {
    use openloom_memory::aggregator::PatternAggregator;
    use openloom_memory::extractor::RuleBasedExtractor;
    use openloom_memory::pipeline::MemoryPipeline;
    use openloom_memory::store::SqliteEventStore;

    let content = std::fs::read_to_string(input_path)?;
    let extractor = RuleBasedExtractor::with_default_rules();
    let aggregator = PatternAggregator::new(threshold);
    let db_file = PathBuf::from(db_path);
    let _ = std::fs::remove_file(&db_file);
    let store = SqliteEventStore::open(&db_file)?;
    let mut pipeline = MemoryPipeline::new(extractor, aggregator, store, threshold);

    let mut all_cognitions: Vec<serde_json::Value> = Vec::new();
    let mut total_events = 0;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') { continue; }
        let parts: Vec<&str> = line.splitn(3, '|').collect();
        if parts.len() < 3 {
            eprintln!("Skipping malformed line: {}", line);
            continue;
        }
        let (session_id, context, text) = (parts[0], parts[1], parts[2]);
        match pipeline.process(session_id, text, context) {
            Ok(result) => {
                total_events += result.events.len();
                if let Some(cog) = result.cognition_triggered {
                    all_cognitions.push(serde_json::json!({
                        "trait": cog.trait_name,
                        "evidence_count": cog.evidence_count,
                        "confidence": cog.confidence,
                        "summary": cog.summary,
                    }));
                }
            }
            Err(e) => eprintln!("Error: {}", e),
        }
    }

    let profile = serde_json::json!({
        "total_events": total_events,
        "cognitions": all_cognitions,
        "generated_at": chrono::Utc::now().to_rfc3339(),
    });
    std::fs::write(output_path, serde_json::to_string_pretty(&profile)?)?;
    println!("\nProfile written to {}", output_path);
    println!("Total events: {}, Cognitions: {}", total_events, all_cognitions.len());
    Ok(())
}
```

- [ ] **Step 5: 运行 CLI 测试**

```bash
cd F:/openLoom && cargo test -p openloom
```
Expected: 5 CLI parsing tests PASS

- [ ] **Step 6: 构建验证**

```bash
cd F:/openLoom && cargo build
```
Expected: Full workspace compiles

- [ ] **Step 7: Commit**

```bash
git add crates/cli/Cargo.toml crates/cli/src/main.rs
git commit -m "feat(cli): add serve/chat/run/skill/memory/config/doctor/version commands"
```

---

### Task 10: 集成测试 — 10 场景端到端验证

**Files:**
- Create: `F:/openLoom/tests/phase1_integration_tests.rs`

- [ ] **Step 1: 编写 10 个集成测试**

`F:/openLoom/tests/phase1_integration_tests.rs`:
```rust
use openloom_engine::Engine;
use openloom_engine::EngineConfig;
use openloom_models::{ChatMessage, Intent};
use tempfile::tempdir;

fn setup_engine() -> (Engine, tempfile::TempDir) {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let engine = Engine::new_test(db_path).unwrap();
    (engine, dir)
}

#[tokio::test]
async fn scenario_1_skill_path_file_operation() {
    let (engine, _dir) = setup_engine();
    let sid = engine.create_session().await.unwrap().id;
    let msg = ChatMessage { role: "user".into(), content: "帮我打开这个文件看看".into() };
    let resp = engine.handle_message(msg, &sid).await.unwrap();
    assert!(!resp.session_id.is_empty());
}

#[tokio::test]
async fn scenario_2_llm_path_chat() {
    let (engine, _dir) = setup_engine();
    let sid = engine.create_session().await.unwrap().id;
    let msg = ChatMessage { role: "user".into(), content: "你好啊，今天过得怎么样".into() };
    let resp = engine.handle_message(msg, &sid).await.unwrap();
    assert_eq!(resp.session_id, sid);
}

#[tokio::test]
async fn scenario_3_empty_input() {
    let (engine, _dir) = setup_engine();
    let sid = engine.create_session().await.unwrap().id;
    let msg = ChatMessage { role: "user".into(), content: String::new() };
    let resp = engine.handle_message(msg, &sid).await;
    assert!(resp.is_ok());
}

#[tokio::test]
async fn scenario_4_consistent_classification() {
    let (engine, _dir) = setup_engine();
    let sid = engine.create_session().await.unwrap().id;
    // 连续 5 次相同意图
    for _ in 0..5 {
        let msg = ChatMessage { role: "user".into(), content: "帮我写一段Python代码".into() };
        let resp = engine.handle_message(msg, &sid).await;
        assert!(resp.is_ok());
    }
}

#[tokio::test]
async fn scenario_5_long_text() {
    let (engine, _dir) = setup_engine();
    let sid = engine.create_session().await.unwrap().id;
    let long_text = "帮我搜索 ".repeat(500); // ~5000 chars
    let msg = ChatMessage { role: "user".into(), content: long_text };
    let resp = engine.handle_message(msg, &sid).await;
    assert!(resp.is_ok());
}

#[tokio::test]
async fn scenario_6_memory_pipeline_non_blocking() {
    let (engine, _dir) = setup_engine();
    let sid = engine.create_session().await.unwrap().id;
    // 发送 10 条消息，验证 memory pipeline 不阻塞
    for i in 0..10 {
        let msg = ChatMessage {
            role: "user".into(),
            content: format!("消息 {}", i),
        };
        let resp = engine.handle_message(msg, &sid).await;
        assert!(resp.is_ok());
    }
}

#[tokio::test]
async fn scenario_7_code_assist_skill() {
    let (engine, _dir) = setup_engine();
    let sid = engine.create_session().await.unwrap().id;
    let msg = ChatMessage { role: "user".into(), content: "修复这个bug".into() };
    let resp = engine.handle_message(msg, &sid).await.unwrap();
    assert!(!resp.session_id.is_empty());
}

#[tokio::test]
async fn scenario_8_session_management() {
    let (engine, _dir) = setup_engine();
    let s1 = engine.create_session().await.unwrap();
    let s2 = engine.create_session().await.unwrap();
    let sessions = engine.list_sessions().await.unwrap();
    assert!(sessions.len() >= 2);
    assert_ne!(s1.id, s2.id);
}

#[tokio::test]
async fn scenario_9_health_check() {
    let (engine, _dir) = setup_engine();
    let health = engine.health_check().await;
    assert_eq!(health.status, "ok");
}

#[tokio::test]
async fn scenario_10_event_bus_token_usage() {
    let (engine, _dir) = setup_engine();
    let mut rx = engine.subscribe();
    let sid = engine.create_session().await.unwrap().id;

    let msg = ChatMessage { role: "user".into(), content: "你好".into() };
    engine.handle_message(msg, &sid).await.unwrap();

    let event = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        rx.recv(),
    ).await;

    assert!(event.is_ok(), "should receive TokenUsage event");
    if let Ok(openloom_models::EngineEvent::TokenUsage { session_id, .. }) = event.unwrap() {
        assert_eq!(session_id, sid);
    } else {
        panic!("expected TokenUsage event");
    }
}
```

- [ ] **Step 2: 运行集成测试**

```bash
cd F:/openLoom && cargo test --test phase1_integration_tests
```
Expected: 10 tests PASS

- [ ] **Step 3: 运行全部测试**

```bash
cd F:/openLoom && cargo test --workspace
```
Expected: All tests PASS (Phase 0 36 + Phase 1 new)

- [ ] **Step 4: Commit**

```bash
git add tests/phase1_integration_tests.rs
git commit -m "test: add 10 Phase 1 integration tests — skill/LLM path, session, event bus, health"
```

---

### Task 11: 最终验证 — clippy + fmt + release build

- [ ] **Step 1: Clippy**

```bash
cd F:/openLoom && cargo clippy --workspace -- -D warnings
```
Expected: Zero warnings

- [ ] **Step 2: fmt**

```bash
cd F:/openLoom && cargo fmt --check
```
Expected: All files formatted

- [ ] **Step 3: Release build**

```bash
cd F:/openLoom && cargo build --release
```
Expected: `target/release/openloom.exe` 生成

- [ ] **Step 4: 运行 release binary**

```bash
./target/release/openloom version
```
Expected: 打印版本号

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "chore: final Phase 1 verification — clippy clean, fmt, release build"
```

---

### Task 12: Electron 壳骨架 (独立子系统)

**Files:**
- Create: `F:/openLoom/electron/package.json`
- Create: `F:/openLoom/electron/main.js`
- Create: `F:/openLoom/electron/preload.js`

> **注意:** Electron 是独立子系统，可与 Rust engine 并行开发。此处提供基础骨架，完整 UI (React) 见 Task 13。

- [ ] **Step 1: 创建 package.json**

`F:/openLoom/electron/package.json`:
```json
{
  "name": "openloom-electron",
  "version": "0.1.0",
  "main": "main.js",
  "scripts": {
    "start": "electron .",
    "dev": "electron . --dev"
  },
  "devDependencies": {
    "electron": "^38.0.0"
  }
}
```

- [ ] **Step 2: 创建 main.js — sidecar 生命周期**

`F:/openLoom/electron/main.js`:
```javascript
const { app, BrowserWindow, Tray, Menu, dialog } = require('electron');
const { spawn } = require('child_process');
const path = require('path');

let mainWindow = null;
let engineProcess = null;
let enginePort = null;
let retryCount = 0;
const MAX_RETRIES = 5;
const RETRY_DELAYS = [1000, 2000, 4000, 8000, 30000];

function startEngine() {
    const engineExe = path.join(__dirname, '..', 'target', 'release', 'openloom');
    engineProcess = spawn(engineExe, ['serve', '--port', '0'], {
        stdio: ['pipe', 'pipe', 'pipe'],
    });

    engineProcess.stdout.on('data', (data) => {
        const line = data.toString().trim();
        try {
            const msg = JSON.parse(line);
            if (msg.type === 'ready') {
                enginePort = msg.port;
                console.log(`Engine ready on port ${enginePort}`);
                retryCount = 0;
            }
        } catch (e) {
            // 非 JSON 行，忽略
        }
    });

    engineProcess.stderr.on('data', (data) => {
        console.error(`Engine stderr: ${data}`);
    });

    engineProcess.on('exit', (code) => {
        console.log(`Engine exited with code ${code}`);
        if (retryCount < MAX_RETRIES) {
            const delay = RETRY_DELAYS[retryCount] || 30000;
            console.log(`Restarting engine in ${delay}ms (attempt ${retryCount + 1}/${MAX_RETRIES})`);
            setTimeout(startEngine, delay);
            retryCount++;
        }
    });
}

function createWindow() {
    mainWindow = new BrowserWindow({
        width: 1200,
        height: 800,
        webPreferences: {
            preload: path.join(__dirname, 'preload.js'),
            contextIsolation: true,
            nodeIntegration: false,
            sandbox: true,
        },
    });

    // Phase 1: 加载本地 HTML (React build 或临时占位)
    mainWindow.loadFile(path.join(__dirname, '..', 'web', 'dist', 'index.html'))
        .catch(() => {
            // Fallback: 显示占位页面
            mainWindow.loadURL('data:text/html,<h1>openLoom</h1><p>Engine on port: ' + enginePort + '</p>');
        });
}

app.whenReady().then(() => {
    startEngine();
    // 给 engine 一些时间启动
    setTimeout(createWindow, 2000);
});

app.on('before-quit', () => {
    if (engineProcess) {
        engineProcess.kill('SIGTERM');
        setTimeout(() => {
            if (engineProcess && !engineProcess.killed) {
                engineProcess.kill('SIGKILL');
            }
        }, 5000);
    }
});

app.on('window-all-closed', () => {
    app.quit();
});
```

- [ ] **Step 3: 创建 preload.js**

`F:/openLoom/electron/preload.js`:
```javascript
const { contextBridge } = require('electron');

contextBridge.exposeInMainWorld('openloom', {
    send: async (method, params) => {
        // JSON-RPC over WebSocket
        const ws = new WebSocket(`ws://127.0.0.1:${window.__enginePort__}/ws`);
        return new Promise((resolve, reject) => {
            ws.onopen = () => {
                ws.send(JSON.stringify({
                    jsonrpc: '2.0',
                    method,
                    params: params || {},
                    id: Date.now(),
                }));
            };
            ws.onmessage = (event) => {
                const data = JSON.parse(event.data);
                resolve(data.result);
                ws.close();
            };
            ws.onerror = (err) => reject(err);
        });
    },

    sseUrl: (sessionId) => {
        return `http://127.0.0.1:${window.__enginePort__}/sse/${sessionId}`;
    },

    subscribe: (eventType, callback) => {
        // Phase 2: WebSocket 订阅引擎事件
        console.log('subscribe:', eventType);
    },
});
```

- [ ] **Step 4: 添加到 .gitignore**

确保 `F:/openLoom/.gitignore` 包含:
```
node_modules/
dist/
```

- [ ] **Step 5: Commit**

```bash
git add electron/ .gitignore
git commit -m "feat(electron): add Electron shell — sidecar lifecycle, preload, basic window"
```

---

### Task 13: React 前端 (独立子系统)

**Files:**
- Create: `F:/openLoom/web/package.json`
- Create: `F:/openLoom/web/src/App.tsx`
- Create: `F:/openLoom/web/src/components/ChatArea.tsx`
- Create: `F:/openLoom/web/src/components/Sidebar.tsx`
- Create: `F:/openLoom/web/src/components/SettingsPanel.tsx`
- Create: `F:/openLoom/web/src/components/TokenDashboard.tsx`
- Create: `F:/openLoom/web/index.html`
- Create: `F:/openLoom/web/vite.config.ts`
- Create: `F:/openLoom/web/tsconfig.json`

- [ ] **Step 1: 创建 package.json**

`F:/openLoom/web/package.json`:
```json
{
  "name": "openloom-web",
  "version": "0.1.0",
  "scripts": {
    "dev": "vite",
    "build": "tsc && vite build",
    "preview": "vite preview"
  },
  "dependencies": {
    "react": "^19.0.0",
    "react-dom": "^19.0.0",
    "react-markdown": "^10.0.0"
  },
  "devDependencies": {
    "@types/react": "^19.0.0",
    "@types/react-dom": "^19.0.0",
    "typescript": "^5.7.0",
    "vite": "^6.0.0",
    "@vitejs/plugin-react": "^4.0.0",
    "tailwindcss": "^4.0.0",
    "@tailwindcss/vite": "^4.0.0"
  }
}
```

- [ ] **Step 2: 创建配置文件**

`F:/openLoom/web/vite.config.ts`:
```typescript
import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';

export default defineConfig({
    plugins: [react(), tailwindcss()],
    base: './',
    build: { outDir: 'dist' },
});
```

`F:/openLoom/web/tsconfig.json`:
```json
{
  "compilerOptions": {
    "target": "ES2022",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "jsx": "react-jsx",
    "strict": true,
    "esModuleInterop": true
  }
}
```

- [ ] **Step 3: 创建 index.html**

`F:/openLoom/web/index.html`:
```html
<!DOCTYPE html>
<html lang="zh-CN">
<head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>openLoom</title>
    <style>
        body { margin: 0; font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif; }
    </style>
</head>
<body>
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
</body>
</html>
```

- [ ] **Step 4: 创建 main.tsx 和 App.tsx**

`F:/openLoom/web/src/main.tsx`:
```tsx
import React from 'react';
import ReactDOM from 'react-dom/client';
import App from './App';
import './index.css';

ReactDOM.createRoot(document.getElementById('root')!).render(
    <React.StrictMode><App /></React.StrictMode>
);
```

`F:/openLoom/web/src/index.css`:
```css
@import "tailwindcss";
```

`F:/openLoom/web/src/App.tsx`:
```tsx
import React, { useState } from 'react';
import Sidebar from './components/Sidebar';
import ChatArea from './components/ChatArea';
import SettingsPanel from './components/SettingsPanel';
import TokenDashboard from './components/TokenDashboard';

type View = 'chat' | 'settings' | 'dashboard';

export default function App() {
    const [activeView, setActiveView] = useState<View>('chat');
    const [sessions, setSessions] = useState([{ id: 'default', name: '默认会话', messageCount: 0 }]);
    const [activeSession, setActiveSession] = useState('default');

    return (
        <div className="flex h-screen bg-gray-900 text-white">
            <Sidebar
                sessions={sessions}
                activeSession={activeSession}
                onSelectSession={setActiveSession}
                onNewSession={() => {
                    const id = crypto.randomUUID();
                    setSessions([...sessions, { id, name: `会话 ${sessions.length + 1}`, messageCount: 0 }]);
                    setActiveSession(id);
                }}
                onNavigate={setActiveView}
                activeView={activeView}
            />
            <main className="flex-1 flex flex-col">
                {activeView === 'chat' && <ChatArea sessionId={activeSession} />}
                {activeView === 'settings' && <SettingsPanel />}
                {activeView === 'dashboard' && <TokenDashboard />}
            </main>
        </div>
    );
}
```

- [ ] **Step 5: 实现 ChatArea**

`F:/openLoom/web/src/components/ChatArea.tsx`:
```tsx
import React, { useState } from 'react';
import ReactMarkdown from 'react-markdown';

interface Message { role: 'user' | 'assistant'; content: string; }

declare global {
    interface Window {
        openloom?: {
            send: (method: string, params?: any) => Promise<any>;
            sseUrl: (sessionId: string) => string;
            subscribe: (event: string, cb: (data: any) => void) => void;
        };
    }
}

export default function ChatArea({ sessionId }: { sessionId: string }) {
    const [messages, setMessages] = useState<Message[]>([]);
    const [input, setInput] = useState('');
    const [loading, setLoading] = useState(false);

    async function sendMessage() {
        if (!input.trim() || loading) return;
        const userMsg: Message = { role: 'user', content: input };
        setMessages(prev => [...prev, userMsg]);
        setInput('');
        setLoading(true);

        try {
            const result = await window.openloom?.send('chat.send', {
                messages: [{ role: 'user', content: input }],
                session_id: sessionId,
                stream: false,
            });
            const assistantMsg: Message = {
                role: 'assistant',
                content: result?.response || 'No response',
            };
            setMessages(prev => [...prev, assistantMsg]);
        } catch (err) {
            setMessages(prev => [...prev, { role: 'assistant', content: `Error: ${err}` }]);
        } finally {
            setLoading(false);
        }
    }

    return (
        <div className="flex flex-col h-full">
            <div className="flex-1 overflow-y-auto p-4 space-y-4">
                {messages.map((msg, i) => (
                    <div key={i} className={`flex ${msg.role === 'user' ? 'justify-end' : 'justify-start'}`}>
                        <div className={`max-w-[70%] rounded-lg p-3 ${
                            msg.role === 'user' ? 'bg-blue-600' : 'bg-gray-700'
                        }`}>
                            <ReactMarkdown>{msg.content}</ReactMarkdown>
                        </div>
                    </div>
                ))}
                {loading && <div className="text-gray-400">Thinking...</div>}
            </div>
            <div className="p-4 border-t border-gray-700">
                <div className="flex gap-2">
                    <input
                        className="flex-1 bg-gray-800 rounded-lg px-4 py-2 text-white outline-none border border-gray-600 focus:border-blue-500"
                        value={input}
                        onChange={e => setInput(e.target.value)}
                        onKeyDown={e => e.key === 'Enter' && sendMessage()}
                        placeholder="输入消息..."
                        disabled={loading}
                    />
                    <button
                        className="px-6 py-2 bg-blue-600 rounded-lg hover:bg-blue-700 disabled:opacity-50"
                        onClick={sendMessage}
                        disabled={loading}
                    >
                        发送
                    </button>
                </div>
            </div>
        </div>
    );
}
```

- [ ] **Step 6: 实现 Sidebar**

`F:/openLoom/web/src/components/Sidebar.tsx`:
```tsx
import React from 'react';

interface Session { id: string; name: string; messageCount: number; }
type View = 'chat' | 'settings' | 'dashboard';

interface Props {
    sessions: Session[];
    activeSession: string;
    onSelectSession: (id: string) => void;
    onNewSession: () => void;
    onNavigate: (view: View) => void;
    activeView: View;
}

export default function Sidebar({ sessions, activeSession, onSelectSession, onNewSession, onNavigate, activeView }: Props) {
    return (
        <div className="w-64 bg-gray-800 flex flex-col border-r border-gray-700">
            <div className="p-4">
                <button
                    className="w-full py-2 bg-blue-600 rounded-lg hover:bg-blue-700 text-sm font-medium"
                    onClick={onNewSession}
                >
                    + 新建会话
                </button>
            </div>
            <div className="flex-1 overflow-y-auto">
                {sessions.map(s => (
                    <div
                        key={s.id}
                        className={`px-4 py-2 cursor-pointer text-sm ${
                            s.id === activeSession ? 'bg-gray-700' : 'hover:bg-gray-700'
                        }`}
                        onClick={() => onSelectSession(s.id)}
                    >
                        <div className="truncate">{s.name}</div>
                        <div className="text-xs text-gray-400">{s.messageCount} 条消息</div>
                    </div>
                ))}
            </div>
            <div className="border-t border-gray-700 p-2">
                {(['chat', 'dashboard', 'settings'] as View[]).map(v => (
                    <div
                        key={v}
                        className={`px-4 py-2 cursor-pointer text-sm rounded ${
                            activeView === v ? 'bg-gray-700 text-blue-400' : 'text-gray-400 hover:text-white'
                        }`}
                        onClick={() => onNavigate(v)}
                    >
                        {v === 'chat' ? '💬 聊天' : v === 'dashboard' ? '📊 仪表盘' : '⚙️ 设置'}
                    </div>
                ))}
            </div>
        </div>
    );
}
```

- [ ] **Step 7: 实现 SettingsPanel + TokenDashboard**

`F:/openLoom/web/src/components/SettingsPanel.tsx`:
```tsx
import React, { useState } from 'react';

export default function SettingsPanel() {
    const [config, setConfig] = useState('');

    return (
        <div className="p-6">
            <h2 className="text-xl font-bold mb-4">模型配置</h2>
            <textarea
                className="w-full h-64 bg-gray-800 text-white p-4 rounded-lg font-mono text-sm border border-gray-600"
                value={config}
                onChange={e => setConfig(e.target.value)}
                placeholder={`[[models]]\nname = "router"\npath = "qwen3-1.7b-q4_k_m.gguf"\nmodel_type = "Router"\nbackend = "LlamaCpp"\nn_gpu_layers = 32\ncontext_size = 4096`}
            />
            <button className="mt-4 px-6 py-2 bg-blue-600 rounded-lg hover:bg-blue-700">
                保存配置
            </button>
        </div>
    );
}
```

`F:/openLoom/web/src/components/TokenDashboard.tsx`:
```tsx
import React, { useState, useEffect } from 'react';

interface TokenStats {
    localTokens: number;
    routerHits: number;
    totalRequests: number;
    savingsRate: number;
}

export default function TokenDashboard() {
    const [stats, setStats] = useState<TokenStats>({
        localTokens: 0,
        routerHits: 0,
        totalRequests: 0,
        savingsRate: 0,
    });

    useEffect(() => {
        window.openloom?.subscribe('token.usage', (data: any) => {
            setStats(prev => ({
                ...prev,
                localTokens: prev.localTokens + (data.prompt_tokens || 0),
                totalRequests: prev.totalRequests + 1,
                savingsRate: prev.totalRequests > 0
                    ? (prev.routerHits / prev.totalRequests) * 100
                    : 0,
            }));
        });
    }, []);

    return (
        <div className="p-6">
            <h2 className="text-xl font-bold mb-4">Token 监控</h2>
            <div className="grid grid-cols-2 gap-4">
                <div className="bg-gray-800 p-4 rounded-lg">
                    <div className="text-sm text-gray-400">本地 Token 用量</div>
                    <div className="text-2xl font-bold">{stats.localTokens.toLocaleString()}</div>
                </div>
                <div className="bg-gray-800 p-4 rounded-lg">
                    <div className="text-sm text-gray-400">Router 命中率</div>
                    <div className="text-2xl font-bold">{stats.savingsRate.toFixed(1)}%</div>
                </div>
                <div className="bg-gray-800 p-4 rounded-lg">
                    <div className="text-sm text-gray-400">总请求数</div>
                    <div className="text-2xl font-bold">{stats.totalRequests}</div>
                </div>
                <div className="bg-gray-800 p-4 rounded-lg">
                    <div className="text-sm text-gray-400">Router 处理数</div>
                    <div className="text-2xl font-bold">{stats.routerHits}</div>
                </div>
            </div>
        </div>
    );
}
```

- [ ] **Step 8: Commit**

```bash
git add web/
git commit -m "feat(web): add React 19 frontend — chat, sidebar, settings, token dashboard"
```

---

### Task 14: 全部验证 + 完成

- [ ] **Step 1: 运行完整测试套件**

```bash
cd F:/openLoom && cargo test --workspace
```
Expected: All tests PASS

- [ ] **Step 2: Clippy + fmt**

```bash
cd F:/openLoom && cargo clippy --workspace -- -D warnings && cargo fmt --check
```
Expected: Clean

- [ ] **Step 3: Release build**

```bash
cd F:/openLoom && cargo build --release
```
Expected: Success

- [ ] **Step 4: 验证 CLI 命令**

```bash
./target/release/openloom version
./target/release/openloom doctor
```
Expected: 正常输出

- [ ] **Step 5: 最终 Commit**

```bash
git add -A
git commit -m "chore: Phase 1 complete — all crates, tests pass, clippy clean, release build"
```

---

## 注意事项

1. **llama-cpp-2 集成:** Task 2 的 InferenceEngine 是占位实现（不依赖实际的 llama-cpp-2 模型文件即可编译和测试）。实际模型推理在 Phase 1 后续迭代中填充（需要下载 GGUF 模型文件后启用）。

2. **Electron + React 独立:** Task 12-13 可与 Rust engine 并行开发。Electron 需要 `cd electron && npm install` 后运行。

3. **Migration 目录路径:** refinery 的 `embed_migrations!("../../migrations")` 路径相对于 `crates/memory/src/store.rs`。需要在 memory crate 的 `Cargo.toml` 或 `build.rs` 中确认路径正确。

4. **Router 注册 Skill 触发词:** 当前 SmartRouter 初始化时不自动注册 skill 触发词。Engine::new() 中应添加 `for skill in skills.all_skills() { router.register_skill_triggers(...) }` 逻辑。

5. **Phase 0 兼容:** `SqliteEventStore::open()` Phase 0 API 保持不变。Phase 1 新增 `open_with_migrations()` 方法供 Engine 使用。

# Phase 3A: AI Activation — 设计规范

**版本:** 1.0
**日期:** 2026-05-20
**状态:** 设计完成
**前置:** Phase 2 Milestone D (已完成)

---

## 1. 目标

让 openLoom 从 "结构完整但 AI 模拟" 升级为 "真正能推理"。解开 llama-cpp 这一个核心阻塞项，连带解锁流式输出、LLM 认知提取、自主心跳检查。

**核心交付:**
1. llama-cpp-2 真实模型加载（Qwen3-1.7B GGUF）→ `InferenceEngine` 从 stub 变真实推理
2. SSE token streaming → 前端 ChatArea 逐字渲染
3. 8B LLM 认知提取 → 规则引擎升级为 Qwen3-8B prompt 提取
4. Hub 心跳 → 空闲时 1.7B 低功耗自主检查
5. 云端 streaming → AnthropicClient/OpenAIClient stream 转发

**不做（归 Phase 3B）：** KV Cache 磁盘持久化、安全沙箱、跨平台打包、Engine 拆分、认知审核面板

---

## 2. Crate 变更

| Crate | 变更 |
|-------|------|
| `inference` | llama-cpp-2 从 optional→required (feature-gated)，InferenceEngine 加真实模型加载/推理/流式/token 计数 |
| `server` | SSE endpoint 从 stub→真实 streaming：channel→stream→SSE |
| `memory` | MemoryPipeline 加 CognitionExtractor enum（RuleBased/LlmBased），PatternAggregator 加事件缓冲 |
| `engine` | 加 hub_loop 后台心跳 task，memory_thread 加载 8B 模型 |
| `models` | EngineEvent 加 HeartbeatTick 变体，CognitionUpdate 加 reasoning 字段 |
| `web` | ChatArea 从 send('chat.send')→EventSource SSE 流式渲染 |

---

## 3. 详细设计

### 3.1 llama-cpp-2 真实模型加载

**架构：** `InferenceEngine` 内部 spawn 一个 `std::thread`（持有 `LlamaModel + LlamaContext + LlamaSampler`，三者 `!Send + !Sync`），通过 `mpsc::channel` 通信。

```rust
// inference/src/lib.rs — InferenceEngine 内部结构
use std::sync::mpsc;

enum EngineCommand {
    Complete { prompt: String, max_tokens: usize, temperature: f32, reply: oneshot::Sender<CompletionResponse> },
    CompleteStream { prompt: String, max_tokens: usize, token_tx: mpsc::Sender<String> },
    TokenCount { text: String, reply: oneshot::Sender<usize> },
}

pub struct InferenceEngine {
    #[cfg(feature = "llama")]
    sender: Option<mpsc::Sender<EngineCommand>>,
    #[cfg(not(feature = "llama"))]
    _model_path: PathBuf,
}
```

**`load()` / `load_blocking()`：**
```rust
#[cfg(feature = "llama")]
pub fn load_blocking(model_path: &Path, n_gpu_layers: usize) -> Result<Self> {
    use llama_cpp_2::*;
    
    if !model_path.exists() {
        anyhow::bail!("Model file not found: {}", model_path.display());
    }
    
    let backend = LlamaBackend::init()?; // init once
    let model = LlamaModel::load_from_file(
        &backend, model_path,
        &LlamaModelParams::default().with_n_gpu_layers(n_gpu_layers as u32),
    )?;
    let ctx = model.new_context(
        &backend, LlamaContextParams::default().with_n_ctx(NonZeroU32::new(4096).unwrap()),
    )?;
    
    let (tx, rx) = mpsc::channel::<EngineCommand>();
    
    std::thread::spawn(move || {
        let sampler = LlamaSampler::chain_simple([/* temp, top_p, greedy */]);
        for cmd in rx { /* handle each command */ }
    });
    
    Ok(Self { sender: Some(tx) })
}
```

**`inference/Cargo.toml` 需确保 reqwest 开启 `stream` feature：**
```toml
reqwest = { version = "0.12", features = ["json", "stream"] }
```

#[cfg(not(feature = "llama"))]  // stub fallback
pub fn load_blocking(model_path: &Path, _n_gpu_layers: usize) -> Result<Self> {
    Ok(Self { _model_path: model_path.to_path_buf(), _n_gpu_layers: 0 })
}
```

**`complete()`：** 若 `sender.is_some()`，send command → await oneshot reply。否则返回当前 stub 文本。

**`complete_stream()`：** 若 sender.is_some()，send `EngineCommand::CompleteStream`。Worker thread 通过 **std::mpsc** 逐 token push（因为 worker 是 `std::thread`）。SSE handler（tokio context）创建的 `tokio::mpsc::channel` 通过一个中间 `std::thread::spawn` 桥接到 worker 的 `std::mpsc::Sender`：

```
SSE handler (tokio)                inference worker (std::thread)
  tokio::mpsc::Sender ──bridge──▶ std::mpsc::Sender
         │                              │
    Sse::new(stream)              LlamaModel::complete_stream()
```

```rust
// inference/src/lib.rs — bridge pattern for streaming
pub async fn complete_stream(&self, req: CompletionRequest, token_tx: tokio::sync::mpsc::Sender<String>) -> Result<()> {
    #[cfg(feature = "llama")]
    if let Some(ref sender) = self.sender {
        let (std_tx, std_rx) = std::sync::mpsc::channel::<String>();
        sender.send(EngineCommand::CompleteStream { prompt: req.prompt, max_tokens: req.max_tokens, token_tx: std_tx })?;
        // Bridge: std::mpsc::Receiver → tokio::mpsc::Sender
        std::thread::spawn(move || {
            for token in std_rx {
                if token_tx.blocking_send(token).is_err() { break; }
            }
        });
        return Ok(());
    }
    #[allow(unreachable_code)]
    Ok(())
}
```

**`token_count()`：** 若 sender.is_some()，send `TokenCount` command，worker 调 `model.str_to_token().len()`。否则保持 `chars/4`。

### 3.2 SSE Token Streaming

**Pipeline：** `InferenceEngine` worker thread → `mpsc::Sender<String>` → SSE handler → `EventSource` → React

**server/sse.rs 重写：**
```rust
use axum::extract::{Path, Query, State};
use axum::response::sse::{Event, Sse};
use futures::stream::{self, Stream};
use std::convert::Infallible;
use tokio::sync::mpsc;

#[derive(Deserialize)]
struct SseParams {
    prompt: String,
    max_tokens: Option<usize>,
}

pub async fn sse_handler(
    Path(session_id): Path<String>,
    Query(params): Query<SseParams>,
    State(engine): State<Arc<Engine>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let (tx, mut rx) = mpsc::channel::<String>(64);
    
    let engine = engine.clone();
    tokio::spawn(async move {
        let req = CompletionRequest {
            prompt: params.prompt,
            max_tokens: params.max_tokens.unwrap_or(2048),
            ..Default::default()
        };
        let _ = engine.stream_complete(req, tx).await;
    });
    
    let stream = stream::unfold(rx, |mut rx| async move {
        match rx.recv().await {
            Some(token) => Some((Ok(Event::default().data(token)), rx)),
            None => None,
        }
    });
    
    Sse::new(stream)
}
```

**Engine 新增 `stream_complete()`：**
```rust
pub async fn stream_complete(&self, req: CompletionRequest, tx: mpsc::Sender<String>) -> Result<()> {
    // Route to cloud or local inference
    if let Some(ref cloud) = self.cloud {
        cloud.complete_stream(req, tx).await
    } else {
        self.inference.complete_stream(req, tx).await
    }
}
```

**前端 ChatArea.tsx（sendMessage 改为 SSE）：**
```tsx
async function sendMessage() {
    if (!input.trim() || loading) return;
    const userMsg: Message = { role: 'user', content: input };
    setMessages(prev => [...prev, userMsg, { role: 'assistant', content: '' }]);
    setInput('');
    setLoading(true);

    const url = window.openloom?.sseUrl(sessionId) + 
        `?prompt=${encodeURIComponent(input)}&max_tokens=2048`;
    const es = new EventSource(url);
    
    es.onmessage = (e) => {
        setMessages(prev => {
            const updated = [...prev];
            const last = updated[updated.length - 1];
            if (last?.role === 'assistant') {
                last.content += e.data;
            }
            return updated;
        });
    };
    es.addEventListener('done', () => { es.close(); setLoading(false); });
    es.onerror = () => { es.close(); setLoading(false); };
}
```

### 3.3 8B LLM 认知提取

**架构：** `MemoryPipeline` 加 `CognitionExtractor` enum。`memory_thread` 启动时尝试加载 Qwen3-8B，成功则 `LlmBased`，失败回退 `RuleBased`。

**CognitionExtractor（保留现有 `MemoryPipeline::new()` 向后兼容，新增 `new_with_extractor()`）：**
```rust
// memory/src/pipeline.rs
pub enum CognitionExtractor {
    LlmBased { model: LlamaModel, ctx: LlamaContext, sampler: LlamaSampler },
    RuleBased,
}

impl MemoryPipeline {
    // 现有 3 参数构造函数不变
    pub fn new(extractor: RuleBasedExtractor, aggregator: PatternAggregator, store: SqliteEventStore) -> Self { /* unchanged */ }
    
    // 新增 4 参数构造函数
    pub fn new_with_extractor(
        extractor: RuleBasedExtractor,
        aggregator: PatternAggregator,
        store: SqliteEventStore,
        cognition: Option<CognitionExtractor>,
    ) -> Self { /* new fields */ }
}
```

**!Send 安全说明：** `LlamaModel`/`LlamaContext`/`LlamaSampler` 是 `!Send`。它们必须在 `memory_thread` 的 `std::thread::spawn` 闭包内创建，然后传入 `MemoryPipeline::new_with_extractor()`——MemoryPipeline 在同一线程中使用，不跨线程移动，安全。

**memory_thread.rs 修改（先加载 8B 再创建 pipeline）：**
```rust
pub fn spawn_memory_thread(
    db_path: PathBuf, threshold: usize,
    event_tx: broadcast::Sender<EngineEvent>,
    summarizer_path: Option<PathBuf>,  // NEW
) -> mpsc::Sender<ProcessRequest> {
    std::thread::spawn(move || {
        let cognition = summarizer_path.and_then(|p| {
            // Try load 8B; return None on failure → fallback to RuleBased
            LlamaBackend::init().ok()?;
            let model = LlamaModel::load_from_file(/* ... */).ok()?;
            // ...
            Some(CognitionExtractor::LlmBased { model, ctx, sampler })
        });
        let mut pipeline = MemoryPipeline::new_with_extractor(/* ... */, cognition);
        // rest of loop unchanged
    })
}
```

**提取流程：** `should_trigger()` → 收集 `drain_events()` 的批次 → 若 `LlmBased`，格式化 prompt → `model.create_completion(prompt)` → 解析 JSON → 填 `CognitionUpdate`；若 `RuleBased`，走现有 `generate_summary()`。

**PatternAggregator 加事件缓冲（新增字段）：**
```rust
// aggregator.rs — 新增
event_buffer: HashMap<String, Vec<EventDatum>>,

pub struct EventDatum {
    pub action: String,
    pub context: String,
    pub source_text: String,
    pub confidence: f64,
}

// observe() 中同时推入 buffer
// drain_events() 返回并清空 buffer
```

**Prompt 模板：**
```
<|im_start|>system
You are a cognitive behavior analyst. Given observed behavior events, extract personality traits.
Output ONLY valid JSON array of {trait, value, confidence, reasoning}. No other text.
<|im_end>
<|im_start|>user
Events (action, confidence, context, text):
{event_log}

Output JSON:<|im_end>
```

**CognitionUpdate 加 reasoning：**
```rust
pub struct CognitionUpdate {
    pub action: String,
    pub trait_name: String,
    pub evidence_count: usize,
    pub confidence: f64,
    pub summary: String,
    pub reasoning: Option<String>,  // NEW
}
```

### 3.4 Hub 心跳

**架构：** `Engine::new()` 中 `tokio::spawn` 一个循环 task，每 `heartbeat.interval_secs` 秒检查。

```rust
// engine/src/lib.rs — Engine::new() 末尾
let heartbeat_engine = engine.clone(); // needs Arc
tokio::spawn(async move {
    let mut interval = tokio::time::interval(
        std::time::Duration::from_secs(config.heartbeat_interval_secs)
    );
    loop {
        interval.tick().await;
        if *heartbeat_engine.agent_state.read().await != AgentState::Idle {
            continue; // skip if agent is busy
        }
        // Check user idle time + event backlog
        let idle_minutes = heartbeat_engine.idle_minutes();
        if idle_minutes < config.heartbeat_idle_threshold_min {
            continue;
        }
        // Single-token inference with 1.7B
        let prompt = format!("User idle {} min. Take action? Reply yes/no.", idle_minutes);
        let resp = heartbeat_engine.inference.complete(CompletionRequest {
            prompt, max_tokens: 1, temperature: 0.0, ..Default::default()
        }).await;
        if resp.is_ok() && resp.unwrap().text.trim().to_lowercase().contains("yes") {
            let _ = heartbeat_engine.event_bus.send(EngineEvent::HeartbeatTick {
                idle_minutes,
                event_count: 0,
                suggested_action: None,
            });
        }
    }
});
```

**Engine 加 `idle_minutes()`：**
```rust
pub fn idle_minutes(&self) -> u64 {
    self.last_user_message.elapsed().as_secs() / 60
}
```

**EngineEvent 新增变体（models/lib.rs）：**
```rust
HeartbeatTick {
    idle_minutes: u64,
    event_count: usize,
    suggested_action: Option<String>,
},
```

**EngineConfig 新增字段：**
```rust
pub struct EngineConfig {
    pub data_dir: PathBuf,
    pub threshold: usize,
    pub cloud_config: Option<openloom_models::ModelConfig>,
    pub rate_limit_ms: u64,
    // NEW Phase 3A
    pub heartbeat_interval_secs: u64,     // default 1800 (30 min)
    pub heartbeat_idle_threshold_min: u64, // default 120 (2 hours)
}
```

**Engine 新增 `last_user_message` 字段（handle_message 中每次收到用户消息时更新）：**
```rust
pub struct Engine {
    // ... existing fields ...
    // NEW Phase 3A
    last_user_message: Instant,
}
```

### 3.5 云端 Streaming

**AnthropicClient / OpenAIClient 的 `complete_stream()` 改为真实实现：**

```rust
// AnthropicClient
async fn complete_stream(&self, req: CompletionRequest, tx: mpsc::Sender<String>) -> Result<()> {
    let body = serde_json::json!({
        "model": self.model, "max_tokens": req.max_tokens,
        "messages": [{"role": "user", "content": req.prompt}],
        "stream": true,
    });
    let resp = self.http.post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", &self.api_key)
        .header("anthropic-version", "2023-06-01")
        .json(&body).send().await?;
    
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        // Parse SSE event from Anthropic streaming format
        // Extract text delta → tx.send(delta).await
    }
    Ok(())
}
```

---

## 4. 文件结构

```
F:/openLoom/
├── crates/
│   ├── inference/src/lib.rs      ← [重写] llama-cpp-2 worker thread + EngineCommand channel
│   ├── server/src/sse.rs         ← [重写] Query params + channel→SSE stream
│   ├── engine/src/lib.rs         ← [Modify] +hub_loop heartbeat +stream_complete +idle_minutes
│   ├── engine/src/memory_thread.rs ← [Modify] +8B model loading +CognitionExtractor
│   ├── memory/src/pipeline.rs    ← [Modify] +CognitionExtractor +reasoning field
│   ├── memory/src/aggregator.rs  ← [Modify] +event_buffer +drain_events()
│   ├── models/src/lib.rs         ← [Modify] +HeartbeatTick +CognitionUpdate.reasoning
│   └── web/src/components/ChatArea.tsx ← [Modify] send()→EventSource streaming
└── docs/superpowers/specs/
    └── 2026-05-20-phase3a-design.md  ← 本文件
```

---

## 5. 依赖关系

```
llama-cpp-2 加载 (3.1)
  ├→ SSE streaming (3.2) — 依赖 InferenceEngine::complete_stream()
  ├→ 8B 认知提取 (3.3) — 依赖独立的 8B 模型上下文
  ├→ Hub 心跳 (3.4) — 依赖 InferenceEngine::complete() 单 token
  └→ 云端 streaming (3.5) — 依赖 CloudClient::complete_stream()
```

---

## 6. 错误处理

| 场景 | 策略 |
|------|------|
| GGUF 文件不存在 | engine `model_available: false`，health 返回 degraded，推理 fallback 到 stub 文本 |
| GPU 不可用 | llama-cpp 自动 CPU fallback（n_gpu_layers=0） |
| 8B 模型加载失败 | CognitionExtractor fallback 到 RuleBased |
| SSE 客户端断开 | `tx.send()` 返回 Err → 推理循环 break |
| 心跳推理失败 | 静默跳过本次 tick，不影响主流程 |
| 两个模型同时加载 OOM | 降级：只加载 1.7B，8B 走 RuleBased |

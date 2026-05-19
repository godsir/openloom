# Phase 2 Milestone B: Agent Loop + Persona + Message History — 设计规范

**版本:** 1.0
**日期:** 2026-05-19
**状态:** 设计完成，待实现
**前置:** Phase 2 Milestone A (已完成)

---

## 1. 目标

实现被动 ReAct Agent Loop、真正的认知画像生成、消息历史持久化。让引擎从"单轮问答"升级为"多步推理 + 认知记忆 + 会话延续"。

**核心交付:**
- Persona Projector — 从 cognitions 表生成真实画像（替换 NoopPersonaProvider stub）
- Agent Loop — 被动 ReAct 循环，最多 3 轮、120 秒超时
- Message History — 会话级消息持久化，get_working_memory() 返回最近 20 条
- Mid-turn Protection — Agent Loop 运行时排队新消息

**不做:**
- 定时心跳/自主触发（Phase 3）
- 8B LLM 认知提取（Phase 3，规则引擎继续用）
- KV Cache 磁盘持久化（Phase 3）
- 多 Agent 协作

---

## 2. Crate 变更

| Crate | 变更 |
|-------|------|
| `models` | PersonaProvider trait 加 `invalidate()`；ChatMessage 加 `timestamp` 字段；新增 `AgentState` enum |
| `memory` | 新增 `persona.rs` — CognitionsPersonaProvider；`store.rs` 新增 `MessageStore`；cognitions 表加 `source` 列 |
| `engine` | `lib.rs` 新增 `agent_loop()` 方法；`handle_message()` 集成 Agent Loop；新增 message_history 写入；新增 mid-turn protection；persona watcher 后台 task |
| `weaver` | 移除 `persona: Arc<dyn PersonaProvider>` 字段，`assemble()` 改为接收 `persona_summary: &str` 参数 |
| `server` | 新增 `agent.state` JSON-RPC 返回实际 AgentState；新增 `session.switch` 方法 |
| `cli` | 无需变更 |

---

## 文件结构

```
F:/openLoom/
├── migrations/
│   └── V3__add_message_history.sql          ← [Create] message_history 表 + cognitions.source 列
├── crates/
│   ├── models/src/lib.rs                    ← [Modify] PersonaProvider::invalidate(), ToolCall, AgentState, ChatMessage.timestamp
│   ├── memory/src/
│   │   ├── persona.rs                       ← [Create] CognitionsPersonaProvider
│   │   ├── store.rs                         ← [Modify] +MessageStore
│   │   └── lib.rs                           ← [Modify] pub mod persona
│   ├── weaver/src/lib.rs                    ← [Modify] 移除 persona 字段, assemble() 加 persona_summary 参数
│   ├── engine/src/
│   │   ├── lib.rs                           ← [Modify] +agent_loop(), +persona field, +agent_state, +interruptible, handle_message 重构
│   │   └── memory_thread.rs                 ← 不变
│   └── server/src/dispatch.rs              ← [Modify] agent.state 返回实际 AgentState
```

---

## 3. 详细设计

### 3.1 Persona Projector — `CognitionsPersonaProvider`

**位置:** `crates/memory/src/persona.rs`

```rust
pub struct CognitionsPersonaProvider {
    db_path: PathBuf,
    cache: Mutex<Option<String>>,
}

impl CognitionsPersonaProvider {
    pub fn new(db_path: PathBuf) -> Self;
    pub fn invalidate(&self) { *self.cache.lock().unwrap() = None; }
}

#[async_trait::async_trait]
impl PersonaProvider for CognitionsPersonaProvider {
    async fn summarize(&self) -> anyhow::Result<String> {
        // 1. Check cache
        if let Some(ref cached) = *self.cache.lock().unwrap() {
            return Ok(cached.clone());
        }
        // 2. Open read-only connection
        // 3. SELECT value, confidence, evidence_count, last_updated, source
        //    FROM cognitions WHERE subject='USER'
        // 4. weighted_score = confidence * evidence_count * exp(-days_since_update / 30.0)
        // 5. observed 优先于 inferred（同分时 observed 排前面）
        // 6. ORDER BY source_priority, weighted_score DESC LIMIT 5
        // 7. 拼接为中文画像：trait: value；trait: value；
        // 8. 写缓存，返回
    }
}
```

**画像格式示例:**
```
用户画像：偏好短线交易；有追高倾向；对止损有抗拒心理；偏好科技股；情绪易受市场波动影响。
```

**trait 变更:**
```rust
// models/lib.rs — PersonaProvider trait 加 invalidate
#[async_trait::async_trait]
pub trait PersonaProvider: Send + Sync {
    async fn summarize(&self) -> anyhow::Result<String>;
    fn invalidate(&self);  // NEW
}

// NoopPersonaProvider 空实现
impl PersonaProvider for NoopPersonaProvider {
    async fn summarize(&self) -> anyhow::Result<String> { Ok(String::new()) }
    fn invalidate(&self) {}  // NEW
}
```

**Weaver 变更:**
`assemble()` 当前在同步上下文中使用 persona（hardcoded `""`）。改为在 `Engine::handle_message()` 中先 await persona.summarize()，结果作为参数传给 weaver。

```rust
// weaver 签名简化
pub fn assemble(
    &self,
    system_instruction: &str,
    user_message: &str,
    persona_summary: &str,        // was: hardcoded ""
    skill_context: Option<&str>,
    working_memory: &[ChatMessage],
) -> AssembledPrompt;
```

**Engine 集成:**
1. `Engine::new()` 中 `persona: Arc<dyn PersonaProvider>` 改为 `Arc::new(CognitionsPersonaProvider::new(db_path.clone()))`
2. `Engine` struct 加 `persona: Arc<dyn PersonaProvider>` 字段（独立于 weaver 的 persona）
3. 新增 `invalidate_persona()` 方法
4. `subscribe_persona_watcher()` — 后台 task 监听 event_bus，`CognitionUpdated` → `persona.invalidate()`

### 3.2 Message History

**迁移 V3** (`migrations/V3__add_message_history.sql`):
```sql
-- Add source column to cognitions (default 'observed' for existing rows)
ALTER TABLE cognitions ADD COLUMN source TEXT NOT NULL DEFAULT 'observed';

-- Message history table
CREATE TABLE IF NOT EXISTS message_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL,
    seq INTEGER NOT NULL,
    role TEXT NOT NULL,
    content TEXT NOT NULL,
    timestamp TEXT NOT NULL,
    FOREIGN KEY (session_id) REFERENCES sessions(id)
);
CREATE INDEX IF NOT EXISTS idx_message_session_seq ON message_history(session_id, seq);
```

**MessageStore:**
```rust
// crates/memory/src/store.rs 新增
pub struct MessageStore { conn: Connection }

impl MessageStore {
    pub fn new(conn: &Connection) -> Self;

    /// 插入一条消息，返回 id
    pub fn insert(&self, session_id: &str, seq: usize, role: &str, content: &str) -> Result<i64>;

    /// 获取最近 N 条消息（按 seq DESC 查询后反转）
    pub fn recent(&self, session_id: &str, limit: usize) -> Result<Vec<ChatMessage>>;

    /// 获取当前最大 seq（用于自增插入）
    pub fn max_seq(&self, session_id: &str) -> Result<usize>;
}
```

**Engine 变更:**
1. `get_working_memory()` 改为打开 `message_history` 表连接，调用 `MessageStore::recent(session_id, 20)`
2. `handle_message()` 在返回 response 后：
   - 写入 user 消息（seq = max_seq + 1）
   - 写入 assistant 回复（seq = max_seq + 2）
   - 调用 `session_tx.send(SessionCommand::UpdateCount { id, count })`

### 3.3 Agent Loop

**AgentState:**
```rust
// models/lib.rs 新增
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AgentState {
    Idle,
    Thinking,
    Acting,
}
```

**Engine struct 变更:**
```rust
pub struct Engine {
    // ... existing fields ...
    persona: Arc<dyn PersonaProvider>,     // NEW
    agent_state: Arc<RwLock<AgentState>>,  // NEW (替代 atomic bool)
    interruptible: AtomicBool,             // NEW: mid-turn protection
}
```

**`agent_loop()` 实现:**

ToolCall 类型定义在 models:
```rust
// models/lib.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub tool: String,
    pub params: serde_json::Value,
}
```

invoke_model() 复用现有 handle_message() 中的 Cloud/Local/None dispatch 逻辑。save_messages() 为 Engine 私有 helper，打开 message_history 连接并写入 user + assistant 两条记录。

```rust
impl Engine {
    async fn agent_loop(&self, msg: &ChatMessage, session_id: &str) -> Result<ChatResponse> {
        // Set state
        *self.agent_state.write().await = AgentState::Thinking;
        self.interruptible.store(true, Ordering::SeqCst);

        let mut history: Vec<ChatMessage> = self.get_working_memory(session_id)?;
        history.push(msg.clone());

        let mut last_response = String::new();
        let total_result = tokio::time::timeout(
            Duration::from_secs(120),
            async {
                for iteration in 0..3 {
                    // Assemble prompt with current history
                    let persona_summary = self.persona.summarize().await?;
                    let assembled = self.weaver.assemble(
                        SYSTEM_INSTRUCTION,
                        "", // user message already in history
                        &persona_summary,
                        None, // skill context already determined
                        &history,
                    );

                    // Call LLM
                    let response = self.invoke_model(&assembled.prompt).await?;

                    // Parse response: does it contain a tool call?
                    if let Some(tool_call) = self.parse_tool_call(&response) {
                        // Execute tool
                        *self.agent_state.write().await = AgentState::Acting;
                        let result = self.execute_tool(&tool_call).await?;
                        history.push(ChatMessage { role: "assistant".into(), content: response });
                        history.push(ChatMessage { role: "tool".into(), content: result });
                    } else {
                        // No tool call — LLM says done
                        last_response = response;
                        break;
                    }
                }
                Ok::<_, anyhow::Error>(last_response)
            }
        ).await;

        // Reset state
        *self.agent_state.write().await = AgentState::Idle;
        self.interruptible.store(false, Ordering::SeqCst);

        match total_result {
            Ok(Ok(response)) => {
                // Write message history
                self.save_messages(session_id, msg, &response)?;
                Ok(ChatResponse { response, session_id: session_id.to_string(), token_usage: self.count_tokens(msg, &response) })
            }
            Ok(Err(e)) => Err(e),
            Err(_) => {
                // Timeout — return partial result or error
                Err(anyhow::anyhow!("Agent loop timed out after 120s"))
            }
        }
    }

    fn parse_tool_call(&self, response: &str) -> Option<ToolCall> {
        // Parse JSON block from response for tool calls
        // Format: {"tool": "skill_name", "params": {...}}
        // LLM is instructed to emit this format when it needs to act
    }

    async fn execute_tool(&self, call: &ToolCall) -> Result<String> {
        // Route to skill invocation: self.skills.invoke(&call.tool, call.params).await
        // Returns tool output as string
    }
}
```

**`handle_message()` 变更:**
```rust
pub async fn handle_message(&self, msg: ChatMessage, session_id: &str) -> Result<ChatResponse> {
    // Mid-turn protection
    if self.interruptible.load(Ordering::SeqCst) {
        return Err(anyhow::anyhow!("Agent is busy, please wait"));
    }

    let out = self.router.classify_sync(&msg.content);

    // Simple intent → direct response (no agent loop)
    if out.complexity < 0.5 && out.skill_match.is_none() {
        // ... existing simple path ...
    }

    // Complex intent or skill match → agent loop
    self.agent_loop(&msg, session_id).await
}
```

### 3.4 Weaver 签名调整

```rust
pub fn assemble(
    &self,
    system_instruction: &str,
    user_message: &str,
    persona_summary: &str,       // 外部传入（Engine 先 await persona.summarize()）
    skill_context: Option<&str>,
    working_memory: &[ChatMessage],
) -> AssembledPrompt;
```

persona_summary 不再由 weaver 内部获取（weaver 不再持有 `Arc<dyn PersonaProvider>`），由 Engine 在调用前 await 获取后传入。这样 Weaver 保持纯同步组装逻辑。

### 3.5 Agent Loop System Prompt 指令

LLM 需要知道如何输出工具调用格式：

```
You are openLoom, a private AI assistant running locally.
When you need to use a tool, respond with a JSON block:
{"tool": "skill_name", "params": {"key": "value"}}
Available tools: [dynamically injected skill list]
When you have the final answer, respond in natural language without JSON.
```

---

## 4. 数据流

```
用户消息
  ↓
Engine::handle_message()
  ├── mid-turn check (interruptible?)
  ├── Router::classify_sync()
  ├── simple? → 直接响应（保持不变）
  └── complex? → agent_loop()
        ├── get_working_memory() → MessageStore::recent(20)
        ├── persona.summarize() → CognitionsPersonaProvider（缓存或查询）
        ├── Weaver::assemble()
        ├── LLM invoke
        ├── parse_tool_call?
        │   ├── yes → execute_tool → 结果注入 history → 下一轮
        │   └── no  → 结束循环
        ├── save_messages() → MessageStore::insert(user + assistant)
        ├── event_bus.send(TokenUsage)
        └── return ChatResponse
```

---

## 5. 错误处理

| 场景 | 策略 |
|------|------|
| Agent Loop 超时 (120s) | 返回 `"Agent loop timed out"` 错误，不保存消息历史 |
| Tool 执行失败 | 错误信息注入 history，让 LLM 看到并重试或告知用户 |
| Persona 查询失败（DB 锁） | 降级返回空字符串（等效 NoopPersonaProvider） |
| MessageHistory 写入失败 | tracing::error! 记录，不影响主流程返回 |
| Mid-turn 新消息到达 | 返回错误 "Agent is busy"，不排队（Milestone B 简化处理） |

---

## 6. 测试策略

| 层级 | 内容 |
|------|------|
| 单元测试 | CognitionsPersonaProvider: 缓存命中/失效、加权排序、空表、时间衰减；MessageStore: CRUD、LIMIT、反转顺序；AgentLoop: 工具调用解析、无工具调用终止 |
| 集成测试 | Engine handle_message → Agent Loop → 多轮工具调用；Message History 写入后 get_working_memory 可读；Persona 缓存失效后重新查询 |
| Clippy | `-D warnings` 零警告 |

---

## 7. 依赖关系

```
engine → persona (cognitions query) + message_history (MessageStore)
weaver → models (no longer holds PersonaProvider)
memory/persona → models (PersonaProvider trait impl)
memory/store → models (MessageStore returns ChatMessage)
```

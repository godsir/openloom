# Orchestrator Type Fields Fix — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 修复 orchestrator.rs 的 12 个编译错误，补全类型定义中缺失的字段和变体。

**Architecture:** 在 loom-types 和 loom-core 的 6 个类型中添加缺失字段，更新所有构造点和消费方，让 agent_loop 产生对应的数据流。

**Tech Stack:** Rust, serde, tokio mpsc

---

## File Structure

**loom-types crate:**
- Modify: `backend/crates/loom-types/src/tool.rs` — ToolDefinition 加 tags 字段
- Modify: `backend/crates/loom-types/src/inference.rs` — StreamDelta 加 ToolResult 变体

**loom-core crate:**
- Modify: `backend/crates/loom-core/src/event_bus.rs` — AgentEvent::ToolStarted/ToolCompleted 加字段
- Modify: `backend/crates/loom-core/src/agent_loop.rs` — TurnResult 加 tool_messages，产生端逻辑
- Modify: `backend/crates/loom-core/src/tool_registry.rs` — SpawnContext 加 event_bus

**loom-server crate:**
- Modify: `backend/crates/loom-server/src/ws.rs` — AgentEvent match 分支补新字段

**构造点批量更新:**
- `backend/crates/loom-core/src/builtin_tools.rs` — 9 处 ToolDefinition 加 tags
- `backend/crates/loom-core/src/orchestrator.rs` — 3 处 ToolDefinition 加 tags

---

### Task 1: ToolDefinition 加 tags 字段

**Files:**
- Modify: `backend/crates/loom-types/src/tool.rs:11-15`

- [ ] **Step 1: 修改 ToolDefinition 结构体**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    #[serde(default)]
    pub tags: Vec<String>,
}
```

- [ ] **Step 2: 验证编译**

Run: `cargo check -p loom-types`
Expected: FAIL — 其他 crate 构造点缺 tags 字段

---

### Task 2: 批量更新 ToolDefinition 构造点

**Files:**
- Modify: `backend/crates/loom-core/src/builtin_tools.rs` (9 处)
- Modify: `backend/crates/loom-core/src/tool_registry.rs:173-174`
- Modify: `backend/crates/loom-core/src/agent_loop.rs:115-126`
- Modify: `backend/crates/loom-core/src/orchestrator.rs` (3 处缺 tags 的)
- Modify: `backend/crates/lume-mcp/src/lib.rs:338`

- [ ] **Step 1: builtin_tools.rs — 9 处 ToolDefinition 加 `tags: vec![]`**

每个 `ToolDefinition { ... }` 块末尾加：
```rust
    tags: vec![],
```

- [ ] **Step 2: tool_registry.rs:173 — SpawnAgentTool 加 tags**

```rust
    fn tool_definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "spawn_agent".into(),
            description: "...",
            input_schema: ...,
            tags: vec![],
        }
    }
```

- [ ] **Step 3: agent_loop.rs:115 — request_tools_definition 加 tags**

```rust
fn request_tools_definition() -> ToolDefinition {
    ToolDefinition {
        name: "request_tools".into(),
        description: "...",
        input_schema: ...,
        tags: vec![],
    }
}
```

- [ ] **Step 4: orchestrator.rs — 3 处 LSP/MCP meta tool 加 tags**

找到 `McpMetaTool` 和 `LspTool` 的 `tool_definition` 方法，加 `tags: vec![]`

- [ ] **Step 5: lume-mcp/src/lib.rs:338 — MCP tool 加 tags**

```rust
defs.push(ToolDefinition {
    name: ...,
    description: ...,
    input_schema: ...,
    tags: vec![],
});
```

- [ ] **Step 6: 验证编译**

Run: `cargo check --workspace`
Expected: FAIL — 还有其他错误待修

---

### Task 3: StreamDelta 加 ToolResult 变体

**Files:**
- Modify: `backend/crates/loom-types/src/inference.rs:85-108`

- [ ] **Step 1: 添加 ToolResult 变体**

```rust
pub enum StreamDelta {
    Text(String),
    Reasoning(String),
    Image {
        media_type: String,
        data: String,
    },
    ToolCallBegin {
        index: usize,
        id: String,
        name: String,
    },
    ToolCallArgsChunk {
        index: usize,
        chunk: String,
    },
    ToolResult {
        call_id: String,
        tool_name: String,
        success: bool,
        result: Option<String>,
    },
    Usage {
        prompt_tokens: u64,
        completion_tokens: u64,
        cache_read_tokens: u64,
        cache_write_tokens: u64,
    },
}
```

- [ ] **Step 2: 验证编译**

Run: `cargo check -p loom-types`
Expected: PASS

---

### Task 4: AgentEvent::ToolStarted 加 args 字段

**Files:**
- Modify: `backend/crates/loom-core/src/event_bus.rs:36-41`
- Modify: `backend/crates/loom-server/src/ws.rs:147-153`

- [ ] **Step 1: event_bus.rs — 加 args 字段**

```rust
ToolStarted {
    agent_id: AgentId,
    call_id: String,
    tool_name: String,
    args: serde_json::Value,
},
```

- [ ] **Step 2: ws.rs — match 分支补 args**

```rust
AgentEvent::ToolStarted {
    agent_id: _,
    call_id,
    tool_name,
    args,
} => {
    json!({ "id": call_id, "name": tool_name, "args": args })
}
```

- [ ] **Step 3: 验证编译**

Run: `cargo check -p loom-core -p loom-server`
Expected: FAIL — ToolCompleted 还缺 result

---

### Task 5: AgentEvent::ToolCompleted 加 result 字段

**Files:**
- Modify: `backend/crates/loom-core/src/event_bus.rs:42-48`
- Modify: `backend/crates/loom-server/src/ws.rs:154-161`

- [ ] **Step 1: event_bus.rs — 加 result 字段**

```rust
ToolCompleted {
    agent_id: AgentId,
    call_id: String,
    tool_name: String,
    success: bool,
    result: Option<String>,
},
```

- [ ] **Step 2: ws.rs — match 分支补 result**

```rust
AgentEvent::ToolCompleted {
    agent_id: _,
    call_id,
    tool_name,
    success,
    result,
} => {
    json!({ "id": call_id, "name": tool_name, "success": success, "result": result })
}
```

- [ ] **Step 3: 验证编译**

Run: `cargo check -p loom-core -p loom-server`
Expected: FAIL — TurnResult 还缺 tool_messages

---

### Task 6: TurnResult 加 tool_messages 字段

**Files:**
- Modify: `backend/crates/loom-core/src/agent_loop.rs:20-34`
- Modify: `backend/crates/loom-core/src/agent_loop.rs` (5 个构造点)

- [ ] **Step 1: TurnResult 结构体加字段**

```rust
pub struct TurnResult {
    pub response: String,
    pub thinking: String,
    pub content_parts: Vec<ContentPart>,
    pub tool_calls_made: usize,
    pub iterations: usize,
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub cached_tokens: usize,
    pub kv_cache_hit: Option<bool>,
    pub tool_messages: Vec<Message>,
}
```

- [ ] **Step 2: 5 个构造点加 `tool_messages: vec![]`**

- agent_loop.rs:362 (cancel 分支)
- agent_loop.rs:568 (正常返回)
- agent_loop.rs:581 (max iterations)
- agent_loop.rs:813 (streaming cancel)
- agent_loop.rs:1078 (streaming 正常返回)

每个 `TurnResult { ... }` 块末尾加：
```rust
    tool_messages: vec![],
```

- [ ] **Step 3: 验证编译**

Run: `cargo check -p loom-core`
Expected: FAIL — SpawnContext 还缺 event_bus

---

### Task 7: SpawnContext 加 event_bus 字段

**Files:**
- Modify: `backend/crates/loom-core/src/tool_registry.rs:153-158`

- [ ] **Step 1: SpawnContext 加 event_bus**

```rust
pub struct SpawnContext {
    pub cloud_client: Arc<RwLock<Option<Arc<dyn loom_inference::engine::CloudClient>>>>,
    pub tool_registry: Arc<RwLock<ToolRegistry>>,
    pub agent_pool: Arc<crate::agent_pool::AgentPool>,
    pub loop_config: Arc<RwLock<crate::agent_loop::AgentLoopConfig>>,
    pub event_bus: crate::event_bus::EventBus,
}
```

- [ ] **Step 2: 验证编译**

Run: `cargo check -p loom-core`
Expected: PASS

---

### Task 8: agent_loop 非流式路径收集 tool_messages

**Files:**
- Modify: `backend/crates/loom-core/src/agent_loop.rs:248-594`

- [ ] **Step 1: 在 run_agent_turn_inner 开头声明 tool_messages**

```rust
let mut tool_messages: Vec<Message> = Vec::new();
```

- [ ] **Step 2: 工具调用时收集 assistant message**

在 `messages.push(Message { role: Role::Assistant, ... })` 后加：
```rust
tool_messages.push(messages.last().unwrap().clone());
```

- [ ] **Step 3: 工具执行后收集 tool result message**

在 `messages.push(Message::tool(...))` 后加：
```rust
tool_messages.push(messages.last().unwrap().clone());
```

- [ ] **Step 4: TurnResult 返回时写入 tool_messages**

```rust
return Ok(TurnResult {
    response: response_text,
    thinking: thinking_text,
    content_parts,
    tool_calls_made,
    iterations: iteration + 1,
    prompt_tokens: total_prompt,
    completion_tokens: total_completion,
    cached_tokens: client.estimated_cache_tokens(),
    kv_cache_hit: client.last_cache_hit(),
    tool_messages,
});
```

- [ ] **Step 5: 验证编译**

Run: `cargo check -p loom-core`
Expected: PASS

---

### Task 9: agent_loop 流式路径收集 tool_messages + 发 StreamDelta::ToolResult

**Files:**
- Modify: `backend/crates/loom-core/src/agent_loop.rs:799-1089`

- [ ] **Step 1: 在 run_agent_turn_streaming_inner 开头声明 tool_messages**

```rust
let mut tool_messages: Vec<Message> = Vec::new();
```

- [ ] **Step 2: 工具调用时收集 assistant message**

在 `messages.push(Message { role: Role::Assistant, ... })` (line ~1001) 后加：
```rust
tool_messages.push(messages.last().unwrap().clone());
```

- [ ] **Step 3: 工具执行后发 StreamDelta::ToolResult 并收集**

在工具执行成功后（line ~1038）：
```rust
let content = if result.is_error {
    format!("Error: {}", result.content)
} else {
    result.content.clone()
};
let tool_msg = Message::tool(tc_id, tc_name, &content);
messages.push(tool_msg.clone());
tool_messages.push(tool_msg);
let _ = delta_tx.send(StreamDelta::ToolResult {
    call_id: tc_id.clone(),
    tool_name: tc_name.clone(),
    success: !result.is_error,
    result: Some(content),
}).await;
```

- [ ] **Step 4: TurnResult 返回时写入 tool_messages**

```rust
Ok(TurnResult {
    response: final_text,
    thinking: captured_thinking,
    content_parts,
    tool_calls_made,
    iterations: 1,
    prompt_tokens: total_prompt,
    completion_tokens: total_completion,
    cached_tokens: client.estimated_cache_tokens(),
    kv_cache_hit: client.last_cache_hit(),
    tool_messages,
})
```

- [ ] **Step 5: 全量验证**

Run: `cargo check --workspace`
Expected: PASS

Run: `cargo test -p loom-core -p loom-types -p loom-server`
Expected: PASS

---

### Task 10: 提交

- [ ] **Step 1: git add + commit**

```bash
git add backend/crates/loom-types/src/tool.rs
git add backend/crates/loom-types/src/inference.rs
git add backend/crates/loom-core/src/event_bus.rs
git add backend/crates/loom-core/src/agent_loop.rs
git add backend/crates/loom-core/src/tool_registry.rs
git add backend/crates/loom-core/src/builtin_tools.rs
git add backend/crates/loom-core/src/orchestrator.rs
git add backend/crates/loom-server/src/ws.rs
git add backend/crates/lume-mcp/src/lib.rs
git commit -m "feat: add missing fields to types for orchestrator compilation

- ToolDefinition: add tags field
- StreamDelta: add ToolResult variant
- AgentEvent: add args to ToolStarted, result to ToolCompleted
- TurnResult: add tool_messages field
- SpawnContext: add event_bus field
- Update agent_loop to populate tool_messages and emit ToolResult"
```

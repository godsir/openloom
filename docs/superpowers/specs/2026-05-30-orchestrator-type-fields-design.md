# 修复 Orchestrator 编译错误：类型字段补全

## 问题

`orchestrator.rs` 引用了 6 个类型中不存在的字段/变体，导致 12 个编译错误：

1. `SpawnContext.event_bus` — 字段不存在
2. `ToolDefinition.tags` — 字段不存在（6 处引用）
3. `StreamDelta::ToolResult` — 变体不存在
4. `AgentEvent::ToolStarted.args` — 字段不存在（2 处引用）
5. `AgentEvent::ToolCompleted.result` — 字段不存在
6. `TurnResult.tool_messages` — 字段不存在

这些字段代表 orchestrator 的设计意图——类型定义落后于使用方。

## 方案：补全类型定义

### loom-types crate（2 个类型）

#### 1. ToolDefinition 加 `tags`

文件：`backend/crates/loom-types/src/tool.rs`

```rust
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    #[serde(default)]
    pub tags: Vec<String>,  // 新增
}
```

影响：约 20 个构造点需补 `tags: vec![]`
- `builtin_tools.rs`：9 处
- `tool_registry.rs`：1 处
- `agent_loop.rs`：1 处（request_tools_definition）
- `orchestrator.rs`：6 处已有 tags（不改）+ 3 处需补
- `lume-mcp/src/lib.rs`：1 处

#### 2. StreamDelta 加 `ToolResult` 变体

文件：`backend/crates/loom-types/src/inference.rs`

```rust
pub enum StreamDelta {
    // ... 现有变体 ...
    ToolResult {
        call_id: String,
        tool_name: String,
        success: bool,
        result: Option<String>,
    },
}
```

### loom-core crate（4 个类型）

#### 3. AgentEvent::ToolStarted 加 `args`

文件：`backend/crates/loom-core/src/event_bus.rs`

```rust
ToolStarted {
    agent_id: AgentId,
    call_id: String,
    tool_name: String,
    args: serde_json::Value,  // 新增
},
```

影响：`loom-server/src/ws.rs` 的 match 分支需补 `args`

#### 4. AgentEvent::ToolCompleted 加 `result`

文件：`backend/crates/loom-core/src/event_bus.rs`

```rust
ToolCompleted {
    agent_id: AgentId,
    call_id: String,
    tool_name: String,
    success: bool,
    result: Option<String>,  // 新增
},
```

影响：`loom-server/src/ws.rs` 的 match 分支需补 `result`

#### 5. TurnResult 加 `tool_messages`

文件：`backend/crates/loom-core/src/agent_loop.rs`

```rust
pub struct TurnResult {
    // ... 现有字段 ...
    pub tool_messages: Vec<Message>,  // 新增：中间工具调用和结果消息
}
```

影响：5 个构造点需补 `tool_messages: vec![]` 或实际收集

#### 6. SpawnContext 加 `event_bus`

文件：`backend/crates/loom-core/src/tool_registry.rs`

```rust
pub struct SpawnContext {
    // ... 现有字段 ...
    pub event_bus: crate::event_bus::EventBus,  // 新增
}
```

影响：1 个构造点（orchestrator.rs:303）已有，不改

### agent_loop.rs 产生端改动

#### 非流式路径（run_agent_turn_inner）

在工具执行循环中收集 assistant tool_call message 和 tool result message 到 `tool_messages` vec，最终写入 TurnResult。

```rust
let mut tool_messages: Vec<Message> = Vec::new();

// 在工具调用前：
tool_messages.push(Message { role: Role::Assistant, content: assistant_content, ... });

// 在工具执行后：
let tool_msg = Message::tool(&tc.id, &tool_name, &content);
messages.push(tool_msg.clone());
tool_messages.push(tool_msg);
```

#### 流式路径（run_agent_turn_streaming_inner）

同上收集 `tool_messages`，并在工具执行完成后通过 `delta_tx` 发送 `StreamDelta::ToolResult`：

```rust
let _ = delta_tx.send(StreamDelta::ToolResult {
    call_id: tc_id.clone(),
    tool_name: tc_name.clone(),
    success: !result.is_error,
    result: Some(content.clone()),
}).await;
```

## 验证

```bash
cargo check --workspace
cargo test -p loom-core -p loom-types -p loom-server
```

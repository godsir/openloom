# Loom vs OpenClaw — Skill/Tool 调用机制深度对比

> 分析时间：2026-06-03  
> 分析范围：loom skill 无法调用、tool 无法执行两个问题的根因定位，以及与 OpenClaw（376K stars）的机制对比

---

## 一、架构对比总览

| 维度 | **Loom** | **OpenClaw** |
|------|----------|--------------|
| 语言 | Rust (backend) + TypeScript (frontend) | TypeScript (全栈 monorepo) |
| Tool 调度模型 | Lazy tools（`request_tools` 元工具 + 按需加载） | 完整工具列表直接注入，无元工具 |
| Skill 调用机制 | `use_skill(skill_name)` 工具调用 → 动态注入 body | 无专门 skill 工具；SKILL.md 通过 `config.transformContext` 在 system prompt 层直接注入 |
| 工具参数修复 | `parse_inline_tool_calls()` — 自研 XML/JSON 解析 | `tool-call-repair` 独立包 — 多格式 promote（XML、JSON、raw text 全兼容） |
| 工具执行模式 | 顺序执行（单线程迭代） | 支持 **parallel** 和 **sequential** 两种模式，`executionMode` per-tool 可配置 |
| 权限/拦截 | `check_permission()` + per-tool SkillPermissions，`ask`/`read_only`/`operate` 三模式 | `beforeToolCall` hook — 调用方决定是否 block，框架不内置权限逻辑 |
| 中止机制 | `CancellationToken` + 事件发布 | `AbortSignal` 原生 Web API |
| 流式事件粒度 | `StreamDelta`（粗粒度：text/tool/usage） | `AgentEvent`（细粒度：`message_start/update/end`、`tool_execution_start/update/end`、`turn_start/end`、`agent_start/end`） |

---

## 二、Skill 调用机制详细对比

### OpenClaw 的做法

OpenClaw **没有** `use_skill` 工具。Skill 完全在 system prompt 层处理：

```
config.transformContext(messages, signal)
    → AgentMessage[] 转换
    → skill 内容直接合并进 systemPrompt 或插入 user 消息
```

- Skill 不需要 LLM 主动"调用"，它们在 `prepareNextTurn` / `transformContext` 阶段被框架自动注入
- SKILL.md 格式由调用方（应用层）决定如何处理，框架只暴露钩子
- **完全没有"先调用 use_skill 再执行"的两跳问题**

### Loom 的做法

```
LLM → request_tools(reason) → 加载工具列表
LLM → use_skill(skill_name) → 注入 skill body 到 tool result
LLM → 执行实际工具（shell/web_search/...）
```

**3 步跳，每一跳都可能失败：**

1. `request_tools` 没匹配到 `use_skill`（reason 关键词不匹配）
2. LLM 调用了 `use_skill` 但参数名不完全匹配
3. Skill body 注入进 tool result，但后续 LLM 没有按照 skill 指令行事

### 关键代码位置

```rust
// backend/crates/loom-core/src/agent_loop.rs 第 856-872 行
// 非流式路径的 web_search 拦截
if tool_name == "web_search" || tool_name == "web_fetch" {
    let skill_used = tool_messages.iter().any(|msg| {
        msg.content.iter().any(|part| {
            matches!(part, ContentPart::ToolResult { name, .. } if name == "use_skill")
        })
    });
    if !skill_used {
        // 强行拦截并要求先 use_skill — 无 skill 的用户直接卡死
        continue;
    }
}
```

```rust
// backend/crates/loom-core/src/agent_loop.rs 第 1798-1812 行
// 流式路径的同等逻辑
let wants_web = matched.iter().any(|t| t.name == "web_search" || t.name == "web_fetch");
let skill_already_loaded = tools.iter().any(|t| t.name == "use_skill");
if wants_web && !skill_already_loaded {
    matched.retain(|t| t.name == "use_skill" || ...);
    let content = "内置搜索已禁用，因为技能更准确。...";
    // → 返回错误 message，LLM 陷入循环
}
```

---

## 三、Tool 调用机制详细对比

### OpenClaw — `tool-call-repair` 包

这是 OpenClaw 的精华：专门处理 LLM 输出格式不规范的工具调用。

```typescript
// packages/tool-call-repair/src/promote.ts
promoteStandalonePlainTextToolCallMessage({
  allowedToolNames: Set<string>,
  createToolCallBlock: (block, resolvedName) => {...},
  resolveToolName: (rawName, allowedNames) => string | null,
  // ...
})
```

**支持的修复场景：**
- `<tool_call>` XML 格式 → 结构化工具调用
- `{"name": "shell", "arguments": {...}}` 纯 JSON → 结构化调用
- 跨多个 text part 分割的工具调用 → 重组后解析
- 工具名大小写/前缀不匹配 → `resolveToolName` 自定义修复
- 多个工具调用块拼接在一条消息里 → 逐块 promote

**核心 promote 逻辑：**

```typescript
function createPromotedToolCallBlocks(text, options) {
  const parsedBlocks = parseStandalonePlainTextToolCallBlocks(text)
  if (!parsedBlocks) return undefined

  const resolveToolName = options.resolveToolName ?? resolveExactToolName
  const toolCalls = []
  for (const block of parsedBlocks) {
    const resolvedName = resolveToolName(block.name, options.allowedToolNames)
    if (!resolvedName) return undefined  // 有未知工具则放弃整条消息
    toolCalls.push(options.createToolCallBlock(block, resolvedName))
  }
  return toolCalls
}
```

### Loom 的做法

```rust
// backend/crates/loom-core/src/agent_loop.rs 第 740-748 行
if response.tool_calls.is_empty() && !response.text.is_empty() {
    let (cleaned, inline_tcs) = loom_inference::parse_inline_tool_calls(&response.text);
    if !inline_tcs.is_empty() {
        response.text = cleaned;
        if !tools.is_empty() {
            response.tool_calls = inline_tcs;
        }
    }
}
```

**仅支持：** `<tool_name>...</tool_name>` 简单 XML 格式

**不支持：**
- `<function_calls><invoke>` 嵌套格式（Anthropic XML）
- `{"name": "shell", "arguments": {...}}` 纯 JSON 对象
- `copy "D:\..." "D:\..."` 这类纯命令文本（永远识别不了）
- 跨 text chunk 分割的调用

---

## 四、事件粒度对比

### OpenClaw 事件流（精细）

```
agent_start
  turn_start
    message_start (user)
    message_end (user)
    message_start (assistant partial)
      message_update (text_start)
      message_update (text_delta × N)
      message_update (text_end)
      message_update (toolcall_start)
      message_update (toolcall_delta × N)
      message_update (toolcall_end)
    message_end (assistant final)
    tool_execution_start { toolCallId, toolName, args }
      tool_execution_update (partialResult)
    tool_execution_end { toolCallId, result, isError }
    tool_result_message { toolCallId, results[] }
  turn_end { message, toolResults[] }
agent_end { messages[] }
```

每次 tool 执行有独立的 `start/update/end` 三个事件，UI 层用 `toolCallId` 精确更新每个工具块。

### Loom 事件流（粗粒度）

```
chat.stream_delta (StreamDelta::Text)
chat.stream_delta (StreamDelta::ToolCallBegin { index, id, name })
chat.stream_delta (StreamDelta::ToolCallArgsChunk { index, chunk })
→ [工具执行中，无事件]
chat.stream_delta (StreamDelta::ToolResult { call_id, tool_name, success, result })
tool.started  ← AgentEvent (broadcast, 可能乱序)
tool.completed ← AgentEvent (broadcast, 可能乱序)
chat.stream_end
```

**核心问题：**

1. `tool.started` / `tool.completed` 通过 WebSocket broadcast `AgentEvent` 发出，**与 stream channel 是两套异步路径**，顺序无法保证
2. `stream-buffer.ts` 的 `flush()` 全量重建 blocks 数组，没有用 `toolId` 做精确定位

```typescript
// frontend/src/renderer/src/services/stream-buffer.ts 第 391-412 行
// 问题：每次 flush 整体重建，先到的 tool block 会被后来的 flush 覆盖
for (const sc of buf.shellCalls) {
  blocks.push({
    type: 'shell',
    toolName: sc.name,
    // ...
  })
}
```

---

## 五、根本问题定位

| # | 现象 | 根因 | OpenClaw 对应解法 |
|---|------|------|-----------------|
| 1 | **Skill 无法正常调用** | `request_tools` → `use_skill` 两跳机制，非 Claude 模型跳过第二跳；`web_search` 拦截逻辑在无 skill 场景下会让 LLM 陷入死循环 | 无两跳：skill body 在 `transformContext` 直接注入，LLM 无需主动调用 |
| 2 | **Tool 不执行（只输出文字）** | 非 Claude 模型在 lazy tools 模式下输出纯文本命令；`parse_inline_tool_calls` 只识别简单 XML，识别不了 `copy "..."` 这类格式 | `tool-call-repair` 包支持多格式 promote，把任意格式工具调用转为结构化调用 |
| 3 | **工具块在 UI 被覆盖/消失** | `stream-buffer.flush()` 全量重建 blocks，`shellCalls` 用数组按顺序追加，多工具场景下顺序不稳定 | 每个 tool 有独立 `toolCallId`，UI 用 Map 精确更新，不走全量重建 |

---

## 六、各层代码问题文件清单

### Backend（Rust）

| 文件 | 行号 | 问题 |
|------|------|------|
| `backend/crates/loom-core/src/agent_loop.rs` | 856-872 | `web_search`/`web_fetch` 拦截过激，无 skill 场景死循环 |
| `backend/crates/loom-core/src/agent_loop.rs` | 1798-1812 | 流式路径同等问题 |
| `backend/crates/loom-core/src/agent_loop.rs` | 740-748 | `parse_inline_tool_calls` 格式支持不足 |
| `backend/crates/loom-inference/src/` | — | inline tool call 解析只支持简单 XML |

### Frontend（TypeScript）

| 文件 | 行号 | 问题 |
|------|------|------|
| `frontend/src/renderer/src/services/stream-buffer.ts` | 391-429 | `flush()` 全量重建，工具块无 id 级精确更新 |
| `frontend/src/renderer/src/services/stream-buffer.ts` | 183-212 | `handleToolStarted` 依赖 broadcast 事件，可能与 stream 事件乱序 |

---

## 七、建议修复优先级

### 短期修复（直接解决用户反馈的两个问题）

**P0 — 移除 web_search 拦截逻辑**

```rust
// agent_loop.rs 第 856-872 行 — 删除此段
// 第 1798-1812 行 — 删除此段
// 原因：无 skill 安装时完全阻塞正常搜索，收益不大风险极高
```

**P0 — 增加 JSON 格式 inline tool call 识别**

在 `parse_inline_tool_calls` 中增加：
```rust
// 识别 {"name": "shell", "arguments": {"command": "..."}}
// 识别 [{"name": ..., "arguments": ...}]  (数组格式)
```

### 中期重构（对齐 OpenClaw 最佳实践）

**P1 — Skill 机制去两跳**

对用户在 UI 选中的 skill，已有 `selected_skills` → system prompt 直接注入路径，应该作为主路径推广。`use_skill` 工具改为辅助/发现工具，不再是执行路径的必经之路。

**P1 — stream-buffer 精确更新**

```typescript
// 改：用 Map<id, ShellCall> 而非数组
// flush() 保持顺序输出但按 id 做 upsert
// 参考 OpenClaw tool_execution_start/end 的 toolCallId 追踪模式
```

**P2 — 引入 tool-call-repair 等价逻辑**

参考 OpenClaw `packages/tool-call-repair` 的实现，在 `loom-inference` 中增加：
- XML `<function_calls>` 嵌套格式
- 纯 JSON 对象格式
- 跨 chunk 拼接后解析

---

## 八、参考资料

- OpenClaw GitHub: https://github.com/openclaw/openclaw
- OpenClaw agent-loop: `packages/agent-core/src/agent-loop.ts`
- OpenClaw tool-call-repair: `packages/tool-call-repair/src/promote.ts`
- Loom agent loop: `backend/crates/loom-core/src/agent_loop.rs`
- Loom stream buffer: `frontend/src/renderer/src/services/stream-buffer.ts`

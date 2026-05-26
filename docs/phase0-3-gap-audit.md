# openLoom v2 自检缺口清单

> 审计日期: 2026-05-26 | 范围: `backend/crates/` (10 crates, ~45 .rs files) | 方法: 3 个 subagent 并行审计
> 最后更新: 2026-05-26

---

## 修复记录

| 日期 | 缺口 | 状态 |
|------|------|:---:|
| 2026-05-26 | #1 WebSocket 空壳 | ✅ |
| 2026-05-26 | #2 dispatch 5→20 方法 | ✅ |
| 2026-05-26 | #3 Agent 状态机接入 | ✅ |
| 2026-05-26 | #4 spawn_agent 走 AgentPool | ✅ |
| 2026-05-26 | #5 流式输出 | ✅ |
| 2026-05-26 | #6 McpClient disconnect | ✅ |
| 2026-05-26 | #7 ContentSearch bug | ✅ |
| 2026-05-26 | #8 Skills 运行时门控 | ✅ |
| 2026-05-26 | #9 dead_code 清零 | ✅ |
| 2026-05-26 | #10 feed_knowledge_graph 去重 | ✅ |
| 2026-05-26 | #11 PersonaProvider 接线 | ✅ |
| 2026-05-26 | #12 EntityExtractor 实现 | ✅ |
| 2026-05-26 | #13 loom-security 接线 | ✅ |
| 2026-05-26 | #14 serve 端口显示实际值 | ✅ |
| 2026-05-26 | #15 ContentSearch Windows 修复 (findstr→递归遍历) | ✅ |
| 2026-05-26 | #16 Shell 工具成功消息明确化 | ✅ |
| 2026-05-26 | #17 系统提示增强 (非沙箱) | ✅ |
| - | #18 loom-context 未引用 | 📋 deferred |
| - | #19 InferenceEngine stub | 📋 deferred |
| - | #20 老 crates 未删 | 📋 deferred |
| - | #21 Node.js 第二后端 | 📋 deferred |
| - | #22 前端死 slices | 📋 deferred |
| - | #23 无集成测试 | 📋 deferred |

---

## 一、编译态

| 指标 | 结果 |
|------|:---:|
| `#[allow(dead_code)]` | **0 (56 文件)** |
| `TODO`/`FIXME`/`unimplemented` | 0 |
| `openloom_*` 老代码泄漏 | 0 |
| `cargo check` 全量 | 零警告零错误 |

---

## 二、严重缺口

### ~~1. WebSocket 完全空壳~~ ✅ FIXED
**文件:** `backend/crates/loom-server/src/ws.rs`
- JSON-RPC 解析 + dispatch 路由 + 响应回传
- EventBus → WS 通知推送（agent.state_changed, tool.started, chat.stream_delta 等）
- lag 自动重订阅

### ~~2. dispatch 仅 5 个方法~~ ✅ FIXED
**文件:** `backend/crates/loom-server/src/dispatch.rs`
- 扩展到 20 个方法: chat.send, session CRUD(6), agent(3), mcp(2), tools, skills, config(2), model(2), system
- 新增 SessionStore（内存会话管理）

### 2. dispatch 空壳方法已清除 ✅ FIXED
**文件:** `backend/crates/loom-server/src/dispatch.rs`
- session.list、session.create、config.get 之前返回空值，现已全部实现

### 3. Agent 状态机未接入 agent loop
**文件:** `backend/crates/loom-core/src/orchestrator.rs:190`
- `process_message()` 直接调 `run_agent_turn()` 函数，绕过 Agent struct、AgentPool、状态机
- `Agent.handle: Option<JoinHandle>` 永远是 `None` — 没有 Agent 作为 tokio task 运行
- 9 状态机存在于类型系统，运行时从未转换

### 4. Electron IPC 缺失 (老前端) — **暂不处理**
**文件:** `electron/main.cjs`
- `preload.js` 定义了 `getSplashInfo`, `onboardingComplete`, `reloadMainWindow`, `getEngineToken` 四个 IPC 通道
- `main.cjs` 没有对应的 `ipcMain.handle()` — 调用方 Promise 永久挂起

---

## 三、中等缺口 — 编译过但不干活

### 5. spawn_agent 绕过了 AgentPool
**文件:** `backend/crates/loom-core/src/tool_registry.rs:171-240`
- `SpawnAgentTool.execute()` 自己内联了一个 LLM 循环
- `SpawnContext.agent_pool` 从未使用
- 不是真正的子 Agent 委派

### 6. 流式变体不是真流式
**文件:** `backend/crates/loom-core/src/agent_loop.rs:187,225`
- `run_agent_turn_streaming()` 调 `client.complete()` (非流式)，等全文返回后发一个 `StreamDelta::Text`
- 无逐 token 输出

### 7. Skills 运行时门控全缺失
**文件:** `backend/crates/lume-skills/src/lib.rs`
- 解析了 21 个 YAML 字段，但运行时只用了 name + description
- `os_restriction`, `requires_bins`, `requires_env`, `allowed_tools`, `always_active` — 全部解析但不检查
- 无 skill 激活/停用机制，无 fork 执行模式

### 8. loom-context 完全未被引用
**文件:** `backend/crates/loom-context/src/lib.rs`
- `ContextAssembler` 从未被任何 crate 导入或实例化
- 不是 `loom-core` 的依赖
- `compact()` 返回空 `Vec` (stub)
- 系统提示用了 `Message::user()` 而非 `Role::System`

### 9. loom-security 完全未被引用
**文件:** `backend/crates/loom-security/src/lib.rs`
- `check_permission()` 从未被调用 — 工具执行无权限检查
- 检查了不存在的 `file_edit` 工具
- 不在任何 crate 的依赖中

### 10. PersonaProvider trait 被绕过
**文件:** `backend/crates/loom-memory/src/persona.rs`
- `CognitionsPersonaProvider::summarize()` 返回空字符串
- 实际的 persona 加载在 `lume-cli/src/memory.rs` 中用 `MemoryStore::get_persona()` 直接做，完全绕过 trait

### 11. EntityExtractor trait 无实现
**文件:** `backend/crates/loom-memory/src/extractor.rs:36`
- trait 定义了 `extract_entities()` 和 `extract_relationships()`
- 文档说 `RuleBasedExtractor` 和 `LlmBasedExtractor` 存在，但无任何 struct 实现此 trait

### 12. MemoryPipeline 配置被忽略
**文件:** `backend/crates/loom-memory/src/pipeline.rs:17`
- `PipelineConfig` (pattern_threshold, auto_extract_kg) 被存储但从未读取
- `with_config()` 接受自定义配置但静默丢弃

### 13. McpClient 无 disconnect
**文件:** `backend/crates/lume-mcp/src/lib.rs`
- 有 `connect` 无 `disconnect`/`remove_server`/`shutdown`
- stdio 子进程连接后永远无法清理

### 14. ContentSearch 命令 bug
**文件:** `backend/crates/loom-core/src/builtin_tools.rs:340`
- Windows: `2>nul` 应改为 `2>NUL`
- 路径含空格时 `findstr` 的引号嵌套有问题

### 15. feed_knowledge_graph 重复实现
- `MemoryPipeline::feed_knowledge_graph` (pipeline.rs:97)
- `LoomMemoryStore::feed_knowledge_graph` (lume-cli/src/memory.rs:200)
- 两套完全重复的图写入逻辑，应合并

### 16. InferenceEngine 是完整 stub
**文件:** `backend/crates/loom-inference/src/engine.rs:62`
- `stub_complete()` 返回硬编码错误消息
- 不加载模型，不执行推理
- `_model_path` 和 `_n_gpu_layers` 存储但从未读取

---

## 四、低优先级 — 清理/结构

### 17. `crates/` 老目录未清理
- 50+ 老 crate 仍在 workspace 中编译
- 30+ 种类型在新旧代码中重复定义
- 计划 Phase 4 删除，但目前仍是编译单元

### 18. `core/` + `lib/` Node.js 第二后端
- `core/` (~90 JS 文件): agent-manager, LLM client, plugin manager, skill manager
- `lib/` (~120 JS 文件): bridge/IM 适配器, browser, desk, tools
- 与 Rust 后端并行存在，功能重叠

### 19. 前端 8 个死 slices
- `bridge-slice`, `desk-slice`, `automation-slice`, `computer-overlay-slice`
- `browser-slice`, `channel-slice`, `screenshot-slice`, `plugin-ui-slice`
- 全部对应后端空壳 API

### 20. 无集成测试
- backend/crates/ 下有 0 个跨 crate 集成测试
- 各 crate 单元测试仅覆盖独立模块 (loom-memory 6, lume-skills 3, lume-mcp 2)

---

## 五、汇总

| 状态 | 数量 | 关键项 |
|------|:---:|--------|
| ✅ 已修复 | 17 | WS, dispatch, Agent 状态机, spawn_agent, 流式, McpClient, ContentSearch (×2), Skills 门控, dead_code, feed_knowledge_graph 去重, PersonaProvider, EntityExtractor, loom-security, serve 端口, Shell 工具, 系统提示 |
| 📋 deferred | 6 | loom-context, InferenceEngine, 老 crates, Node.js, 前端 slices, 集成测试 |
| **合计** | **23** | |

# Codex CLI 移植设计

**日期:** 2026-05-22
**状态:** 已确认
**目标:** 将 Codex CLI（F:/codex/codex-rs/）移植到 openLoom，替代现有 CLI，保留 openLoom Engine 作为后端。

## 1. 架构概览

```
┌──────────────────────────────────────┐
│  TUI + CLI（~30 crate，源码搬入）     │
│  不改代码，只改 crate 名和路径        │
├──────────────────────────────────────┤
│  loom-app-server（新 crate）          │
│  实现 AppServerClient 接口            │
│  Codex 协议 ↔ openLoom Engine 翻译    │
├──────────────────────────────────────┤
│  openLoom Engine（不动）              │
│  router / weaver / inference / memory │
└──────────────────────────────────────┘
```

## 2. 核心决策

| 决策 | 选择 | 理由 |
|------|------|------|
| CLI 形态 | 单二进制 + 模式切换 | 编码和陪伴两大场景，命令显式切换 |
| 集成方式 | 源码直接搬入 `crates/` | 自由修改，保留和上游的差异能力 |
| Engine 连接 | 默认嵌入 + `--remote` 可选 | 零配置开箱即用，预留 Electron 客户端 |
| 适配策略 | 实现 `AppServerClient` 接口 | TUI 不改一行，换掉整个后端 |
| 工具体系 | openLoom skill 体系执行 | 避免两套系统争抢状态 |
| 功能开关 | 保留 | 纯本地机制，config.toml + CLI |

## 3. 搬来的 crate（不改代码，只改名）

### 协议层
- `app-server-client` — `AppServerClient` 抽象接口（enum InProcess/Remote）
- `app-server-protocol` — JSON-RPC 请求/响应/通知类型
- `protocol` — 核心共享类型（ThreadId, ResponseItem, PermissionProfile）
- `config` — 配置加载、层级合并

### CLI/TUI 层
- `cli` — 入口、命令解析、子命令分发
- `tui` — 完整 ratatui 交互界面

### 功能模块
- `features` — 本地功能开关系统（Stable/Experimental/UnderDevelopment/Deprecated）
- `sandboxing` — 沙箱路径校验
- `exec-server` — EnvironmentManager
- `execpolicy` — 执行策略定义
- `shell-command` — Shell 命令解析
- `git-utils` — Git diff 展示
- `file-search` — 文件搜索
- `tools` — 工具定义（bash、文件操作）
- `core-skills` — 内置技能定义和执行
- `codex-mcp` / `mcp-server` / `rmcp-client` — MCP 服务器管理
- `hooks` — 文件级 Agent 钩子
- `network-proxy` — 网络代理类型
- `external-agent-sessions` / `external-agent-migration` — Claude Code 迁移工具（目标路径改为 `.loom/`）

### 工具库
- `utils-absolute-path` / `utils-path` / `utils-cli` / `utils-home-dir` / `utils-string` / `utils-elapsed` / `utils-fuzzy-match` / `utils-sandbox-summary` / `utils-sleep-inhibitor` / `utils-oss` / `utils-plugins` / `utils-approval-presets` / `utils-rustls-provider`
- `ansi-escape` / `arg0` / `install-context` / `terminal-detection`
- `uds` / `file-system` / `async-utils`
- `core-plugins`

## 4. 砍掉的 crate（~60 个）

登录/认证、云端同步、自更新、遥测（OpenTelemetry + Sentry）、App Server 实现、OpenAI 后端客户端（codex-api, codex-client, codex-backend-client, backend-openapi-models）、模型供应商（codex-model-provider, models-manager, model-provider-info）、状态存储（codex-state, codex-rollout）、云端任务（cloud-tasks）、扩展系统（extension-api, guardian, memories-extension）、WebRTC 实时语音、AWS Bedrock、LM Studio/Ollama 独立 provider、V8 JS 引擎、所有测试/sample/debug crate。

## 5. loom-app-server 适配层设计

### 接口实现

实现 `AppServerClient` 的四个方法：

- `request_typed<R>(ClientRequest) -> Result<R>` — RPC 请求映射
- `notification(ClientNotification) -> Result<()>` — 单向通知
- `events() -> Option<AppServerEvent>` — 服务端推送事件（流式 token、工具调用、状态变更）
- `shutdown() -> Result<()>` — 关闭

### 核心 RPC 映射

| Codex 协议 | → Engine 调用 |
|------------|---------------|
| `TurnStart { items, cwd, model }` | `handle_message_streaming(msg, sid, tx)` + router |
| `ThreadStart` | `session_tx.send(SessionCommand::Create)` |
| `ThreadList` | `session_tx.send(SessionCommand::List)` |
| `ThreadResume` | `session_tx.send(SessionCommand::Get)` |
| `ThreadFork` | `session_tx.send(SessionCommand::Fork)` |
| `ThreadSetName` | `session_tx.send(SessionCommand::SetName)` |
| `SkillsList` | `skill_registry.list()` |
| `ModelList` | 从 AppConfig 读取已配置模型 |
| `ConfigBatchWrite` | `config::set_config()` |

### 事件翻译

| Engine 事件 | → Codex 协议通知 |
|-------------|-----------------|
| 流式 token | `AgentMessageDelta { delta }` |
| 工具调用开始 | `ItemStarted(CommandExecution)` |
| 工具调用结束 | `ItemCompleted(CommandExecution)` |
| 文件变更 | `ItemStarted(FileChange)` + `ItemCompleted` |
| agent_loop 结束 | `TurnCompleted { status: Completed }` |
| 中断 | `TurnCompleted { status: Interrupted }` |
| 审批请求 | `ServerRequest::CommandExecutionRequestApproval` |
| 错误 | `Error { will_retry: false }` |

### 工具体系

- 工具定义沿用 Codex 的 JSON Schema 格式（经过大规模验证）
- 工具执行交给 openLoom 的 skill 系统
- 适配层只做事件格式翻译，不参与执行

## 6. 模式切换

两种模式通过命令显式切换：

- **陪伴模式（默认）**：全局记忆、人格交互、长期学习。用户在任何目录敲 `loom` 进入。
- **编码模式**：`loom code` 或 `/code` 斜杠命令切换。有文件操作、diff 预览、沙箱执行等完整编码工具。

两种模式共享同一份记忆库和人格，编码时的洞察可以写入全局记忆，陪伴时可用。

## 7. 测试策略

### 适配层单元测试
- `TurnStart` → `handle_message_streaming()` 参数映射
- 流式 token → `AgentMessageDelta` 逐个发出
- 工具调用 → `ItemStarted`/`ItemCompleted` 时序
- `TurnCompleted` 状态转换
- Session 操作映射正确性

### 集成测试
- `loom exec "修 bug"` 端到端：TUI → Engine → 工具 → 渲染
- 流式渲染：token 逐字出现
- 审批流：高风险操作 → 弹窗确认 → 执行 → 结果
- 模式切换：陪伴 ↔ 编码状态不串

### 不需要测
- Codex TUI 渲染逻辑（已有覆盖）
- openLoom Engine 核心逻辑（已有 180+ 测试覆盖）

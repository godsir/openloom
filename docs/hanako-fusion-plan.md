# Hanako → Loom 桌面壳融合计划

## 背景

openLoom 已有完整的 Rust 引擎（Memory Kernel、Smart Router、Skill Engine、Agent Loop）和 CLI 入口（`loom`），但 Electron 桌面壳仅 6 个组件，只是一个最低限度可用的验证壳。

OpenHanako 有 46+ 个打磨好的 React 组件 + 38 个 Zustand 状态切片 + 22 个 React hooks，是一个成熟的 Electron 桌面 UI 资产库。两边协议天然对齐（WebSocket + JSON-RPC 2.0）。

**目标**：将 Hanako 的桌面 UI 嫁接到 Loom 引擎上，保留 Loom 引擎不动，编写薄适配层完成对接。

Hanako文件路基 F:\openhanako
---

## 架构变更

```
                  改造前                              改造后
         ┌──────────────────┐              ┌──────────────────┐
         │  Hanako Electron │              │  Loom Electron   │
         │  (main.cjs)      │              │  (main.js)       │ ← 保留 Loom 的
         │  (preload.cjs)   │              │  (preload.js)    │ ← 合并 Hanako IPC
         ├──────────────────┤              ├──────────────────┤
         │  React 46 组件   │              │  Hanako React UI │ ← 搬过来（裁剪后）
         │  Zustand 38 切片 │              │  + adapter.ts    │ ← 新增
         ├──────────────────┤              ├──────────────────┤
         │  Node.js core/   │      →       │  JSON-RPC 2.0    │
         │  Node.js server/ │              │  WebSocket       │
         ├──────────────────┤              ├──────────────────┤
         │  云端 LLM 直调   │              │  Rust Engine     │ ← 不动
         └──────────────────┘              │  (crates/)       │
                                           └──────────────────┘
```

核心原则：
- **Loom 引擎不动**：Memory Kernel、Smart Router、Skill Engine、Agent Loop 全部保留
- **UI 层整体替换**：从 6 个组件 → Hanako 的 UI 体系（裁剪后）
- **只写胶水代码**：一个 adapter.ts + store 适配，不重写 UI 也不重写引擎

---

## 资产清单：保留 / 砍掉 / 替换

### 一、桌面 UI 组件

| 组件 | 决策 | 说明 |
|------|------|------|
| **chat/** — ChatArea, ChatTranscript, AssistantMessage, UserMessage, ThinkingBlock, ToolGroupBlock, MarkdownContent, StreamingMarkdownContent, block-renderers, MoodBlock, ChatTimelineNavigator, timeline-anchors, MessageActions, MessageFooterActions, FileOutputActions | **保留** | 核心聊天界面 + 完整消息渲染管线 |
| **chat/** — PluginCardBlock, SettingsConfirmCard, SettingsUpdateCard, SubagentCard, SubagentSessionPreview | **砍掉** | 插件卡片/子 Agent 卡片/设置确认卡片，Loom 无对应概念 |
| **app/** — AppTitlebar | **砍掉** | 使用 Loom Electron 原生标题栏，不搞定制 |
| **app/** — SidebarLayout, ChatSidebar, AppPages | **保留** | 侧栏布局 + 页面路由 |
| **input/** — InputArea, SendButton, InputControlBar, InputStatusBars, InputContextRow | **保留** | 核心输入交互 |
| **input/** — SlashCommandMenu, slash-commands.ts | **保留** | 斜杠命令，对接 Loom 命令体系 |
| **input/** — ModelSelector, ThinkingLevelButton, PlanModeButton | **保留** | 模型选择 + 思考等级 + Plan 模式 |
| **input/** — AttachedFilesBar, FileMentionMenu, FileBadgeView, SkillBadgeView, ContextRing, QuotedSelectionCard, TodoDisplay, SessionConfirmationPrompt | **保留** | 输入区辅助功能 |
| **input/extensions/** — input-editor-extensions.ts | **保留** | CodeMirror 编辑器扩展 |
| SessionList, ArchivedSessionsModal | **保留** | 会话列表 + 归档 |
| Agent 管理相关 UI（从 agent-slice 对应的面板） | **保留** | 创建/切换/删除 Agent |
| DeskSection, DeskDropZone, DeskTree, DeskToolbar, DeskEditor, DeskCwdSkills, DeskEmptyOverlay | **保留** | 书桌文件区 |
| MediaViewer（shared/MediaViewer/） | **保留** | 媒体全屏预览 |
| SkillViewerOverlay | **保留** | 技能浏览 |
| SettingsModalShell | **保留** | 设置面板 |
| PreviewPanel, PreviewEditor | **保留** | 文件/代码预览 |
| StatusBar, ToastContainer, ErrorBoundary, RegionalErrorBoundary | **保留** | 状态栏 + 通知 + 错误边界 |
| ActivityPanel | **保留** | Agent 活动日志 |
| AutomationPanel | **保留** | 自动化/定时任务管理 |
| FloatPreviewCard, FloatingPanels | **保留** | 浮动预览 |
| ChannelsPanel, ChannelCreateOverlay | **砍掉** | Agent 群聊，不需要 |
| BridgePanel | **砍掉** | 多平台桥接，不需要 |
| ComputerUseOverlay | **砍掉** | 远程桌面控制，不需要 |
| BrowserCard | **砍掉** | 内嵌浏览器，不需要 |
| LeavesOverlay | **砍掉** | 花瓣飘落特效 |
| plugin/ — 插件管理面板 | **砍掉** | Hanako 的插件体系不搬，只用技能 |
| right-workspace/ | **保留** | 右侧工作区（预览 + 活动 + 自动化面板容器） |

### 二、Zustand 状态切片

| 切片文件 | 决策 | 说明 |
|----------|------|------|
| chat-slice.ts + chat-types.ts | **保留** | 消息状态，adapter 改写网络部分 |
| session-slice.ts + session-actions.ts + session-selectors.ts | **保留** | 会话状态，adapter 改写 |
| agent-slice.ts + agent-actions.ts | **保留** | Agent 状态 |
| connection-slice.ts | **保留** | 连接状态，改为监听 Loom WS |
| streaming-slice.ts + stream-invalidator.ts | **保留** | SSE 流式，URL 改为 Loom 端口 |
| input-slice.ts | **保留** | 输入状态 |
| ui-slice.ts | **保留** | UI 面板/侧栏状态 |
| toast-slice.ts | **保留** | 通知 |
| desk-slice.ts + desk-actions.ts | **保留** | 书桌 |
| model-slice.ts | **保留** | 模型选择 |
| selection-slice.ts + selection-actions.ts | **保留** | 文本选择 |
| context-slice.ts | **保留** | 上下文感知 |
| activity-slice.ts | **保留** | Agent 活动日志 |
| automation-slice.ts | **保留** | 定时任务 |
| preview-slice.ts + preview-actions.ts | **保留** | 文件预览 |
| screenshot-slice.ts | **保留** | 截屏 |
| workspace-ui-state-actions.ts | **保留** | 工作区 UI 状态 |
| settings-modal-actions.ts | **保留** | 设置面板状态 |
| message-live-version.ts + message-turn-actions.ts | **保留** | 消息版本控制 |
| bridge-slice.ts | **砍掉** | 平台桥接 |
| channel-slice.ts + channel-actions.ts | **砍掉** | 群聊频道 |
| browser-slice.ts | **砍掉** | 内嵌浏览器 |
| computer-overlay-slice.ts | **砍掉** | 远程桌面 |
| plugin-ui-slice.ts + plugin-ui-actions.ts | **砍掉** | 插件 UI（非技能） |
| subagent-preview-slice.ts | **砍掉** | 子 Agent |
| create-keyed-slice.ts | **保留** | 工具函数 |
| index.ts | **保留** | store 入口 |
| selectors/ | **保留** | 派生选择器 |

### 三、Hooks

| Hook | 决策 | 说明 |
|------|------|------|
| use-hana-fetch.ts | **重写** | 替换为 adapter.ts 封装，不再直连 HTTP |
| use-stream-buffer.ts | **保留** | SSE 流缓冲，改 URL 指向 |
| use-config.ts | **保留** | 配置读取 |
| use-slash-items.ts | **保留** | 斜杠命令列表 |
| use-continuous-bottom-scroll.ts | **保留** | 对话自动滚底 |
| use-theme.ts | **保留** | 主题切换 |
| use-mermaid-diagrams.ts | **保留** | Mermaid 图表渲染 |
| use-platform.ts | **保留** | 平台检测 |
| use-sidebar-resize.ts | **保留** | 侧栏拖拽 |
| use-auto-update-state.ts | **保留** | 自动更新状态 |
| use-animate-presence.ts | **保留** | 动画 |
| use-panel.ts | **保留** | 面板状态 |
| use-plugin-iframe.ts | **砍掉** | 插件 iframe |
| use-i18n.ts | **砍掉** | 只保留中文，硬编码 t() |
| use-typewriter-text.ts | **保留** | 打字机效果 |

### 四、Electron 主进程

| 文件 | 决策 | 说明 |
|------|------|------|
| main.cjs（Hanako） | **替换** | 用 Loom 的 main.js，已有引擎生命周期管理 |
| preload.cjs（Hanako） | **合并** | 其 IPC 方法合并进 Loom 的 preload.js |
| bootstrap.cjs | **砍掉** | Loom main.js 已处理启动 |
| auto-updater.cjs | **保留** | Windows 自动更新 |
| file-text-io.cjs | **保留** | 文件读写（IPC handler） |
| file-watch-registry.cjs | **保留** | 文件变更监听（桌面 + Agent 都依赖） |
| workspace-watch-registry.cjs | **保留** | 书桌工作区监听 |
| ipc-wrapper.cjs | **保留** | IPC 封装工具 |

### 五、Hanako Node.js 后端（core/ + server/）

全部替换为 Loom Rust Engine，不保留 Node.js 后端代码。

| 关键模块 | Loom 对应 | 说明 |
|----------|-----------|------|
| engine.js + agent.js | crates/engine/ | Agent Loop + 请求派发 |
| session-coordinator.js | crates/engine/session.rs | 会话编排 |
| llm-client.js + llm-utils.js | crates/inference/ | LLM 调用 |
| model-manager.js + provider-registry.js | Config + inference | 模型/Provider 管理 |
| memory 相关（散布在 core/） | crates/memory/ | 认知图谱替代 |
| skill-manager.js | crates/skills/ | WASM 技能引擎替代 |
| compaction-utils.js | crates/weaver/ | Context Weaver 替代 |
| plugin-manager.js | **砍掉** | Loom 不需要完整插件体系 |
| slash-command-dispatcher.js | Loom CLI / 内置命令 | Loom 命令体系替代 |
| vision-bridge.js + vision-prepare.js | **砍掉** | 本地多模态未就绪，后续再考虑 |
| computer-use/ | **砍掉** | 不需要 |
| studio-cron-service.js | crates/engine/heartbeat.rs | 事件驱动替代轮询 |
| execution-boundary.js + sandbox | crates/sandbox/ | 声明式权限替代 |
| security-*.js + server-auth.js + local-user-account.js | **砍掉** | Loom 本地单用户，不需要认证体系 |
| device-registry.js | **砍掉** | 无多设备场景 |
| bridge-session-manager.js + channel-manager.js | **砍掉** | 无多平台桥接 |
| server/index.js + routes/ | crates/server/ | Axum 替代 Hono |
| ws-protocol.js + ws-scope.js | crates/server/ | WebSocket 替代 |
| session-stream-store.js | crates/server/ | SSE 替代 |
| cli.js | crates/cli/ | Rust CLI 替代 |
| block-extractors.js | **搬入前端** | 消息块解析逻辑移到 web/src/，属于 UI 渲染层 |
| message-sanitizer.js + message-utils.js | **搬入前端** | 消息处理逻辑移到 web/src/ |
| i18n.js | **砍掉** | 只保留中文 |
| migrations.js | refinery 迁移 | SQLite 迁移替代 |
| first-run.js | **新增** | Loom 需要自己的首次启动逻辑 |

---

## 实现计划

### Phase A — 聊天 + 会话 + Agent 切换

**目标**：在 Loom Electron 壳里跑起聊天界面，能跟 Loom 引擎对话。

**搬入的 React 层**：
- **消息渲染管线**：block-renderers.ts, AssistantMessage, UserMessage, ThinkingBlock, ToolGroupBlock, MarkdownContent, StreamingMarkdownContent, MoodBlock, ChatTimelineNavigator, timeline-anchors, MessageActions, MessageFooterActions, FileOutputActions, ChatTranscript, ChatArea
- **侧栏**：SidebarLayout, ChatSidebar, AppPages, SessionList, ArchivedSessionsModal
- **输入区**：InputArea, SendButton, InputControlBar, InputStatusBars, InputContextRow, SlashCommandMenu, slash-commands.ts, ModelSelector, ThinkingLevelButton, PlanModeButton, AttachedFilesBar, FileMentionMenu, FileBadgeView, SkillBadgeView, ContextRing, QuotedSelectionCard, TodoDisplay, SessionConfirmationPrompt, input-editor-extensions.ts
- **全局 UI**：StatusBar, ToastContainer, ErrorBoundary, RegionalErrorBoundary
- **入口**：App.tsx, main.tsx

**搬入的状态管理**（adapter 改写网络部分）：
- chat-slice.ts + chat-types.ts
- session-slice.ts + session-actions.ts + session-selectors.ts
- agent-slice.ts + agent-actions.ts（基础部分）
- connection-slice.ts → 监听 Loom WebSocket
- streaming-slice.ts + stream-invalidator.ts → SSE URL 改为 Loom 端口
- input-slice.ts
- ui-slice.ts
- toast-slice.ts
- message-live-version.ts + message-turn-actions.ts
- context-slice.ts
- selection-slice.ts + selection-actions.ts

**搬入的 Hooks**：
- use-hana-fetch.ts → 重写为 adapter 封装
- use-stream-buffer.ts, use-continuous-bottom-scroll.ts, use-theme.ts, use-mermaid-diagrams.ts, use-platform.ts, use-sidebar-resize.ts, use-slash-items.ts, use-animate-presence.ts, use-panel.ts, use-typewriter-text.ts

**从后端搬到前端的逻辑**：
- block-extractors.js → 移到 web/src/（消息块解析属于 UI 渲染）
- message-sanitizer.js + message-utils.js → 移到 web/src/

**Loom 侧改动**：
- `web/src/adapter.ts`：封装 `window.openloom.send()`，暴露 Hanako store 期望的 API
  - Session 列表 → `session.list`
  - 发送消息 → `agent.chat`（SSE 流式消费）
  - Agent 列表/切换 → `agent.list` / `agent.switch`
- `electron/preload.js`：合并 Hanako 的 `window.hana` IPC
- 新增 JSON-RPC 方法：`session.list`, `session.create`, `session.delete`, `session.archive`, `session.rename`, `agent.chat`, `agent.list`, `agent.switch`
- 斜杠命令列表挂载到 Loom 的命令注册表，通过 JSON-RPC `command.list` 暴露

**验证标准**：发送消息，收到 Loom 引擎流式回复；消息正确渲染（代码块、thinking 块、tool 调用块）；会话列表增删改正常；Agent 切换正常；斜杠命令可用。

---

### Phase B — 记忆可视化

**目标**：把 Loom 认知图谱的数据可视化到界面上。

**搬入组件**：PersonaPanel、TokenDashboard、CognitionAuditPanel（Loom 现有）

**Loom 侧改动**：
- 新增 JSON-RPC 方法：`memory.stats`、`memory.recent_events`、`memory.graph_snapshot`
- 对接 Loom Memory Kernel 的认知摘要数据

**验证标准**：PersonaPanel 展示当前 Agent 的演化人格摘要，TokenDashboard 显示实际 Token 消耗。

---

### Phase C — Agent 管理

**目标**：创建、配置、切换、删除 Agent，每个 Agent 独立记忆/人格/技能/Cron。

**搬入组件**：Agent 管理面板（从 agent-slice 对应的 UI）

**搬入的状态管理**：
- agent-slice.ts + agent-actions.ts（完整功能）
- automation-slice.ts（定时任务管理）
- activity-slice.ts（Agent 活动日志）

**搬入组件**：ActivityPanel, AutomationPanel, right-workspace/

**Loom 侧改动**：
- 新增 JSON-RPC 方法：`agent.create`, `agent.delete`, `agent.configure`, `agent.list`
- Agent 配置持久化到 Loom Config 体系
- 每个 Agent 绑定独立的 Persona + Memory 分片
- 新增 JSON-RPC 方法：`agent.activity_log`（活动日志）
- 新增 JSON-RPC 方法：`cron.list`, `cron.create`, `cron.delete`（定时任务管理）
- Tool 权限配置：`agent.tool_policy.get`, `agent.tool_policy.set`（每个 Agent 独立工具权限）

**验证标准**：创建两个 Agent，分别对话，记忆不交叉，各自独立工具权限。

---

### Phase D — 技能管理

**目标**：UI 管理 Loom 的 Skill Engine。

注意区分：Hanako 有插件系统 + 技能系统两个概念。Loom 只保留技能（WASM sandbox），不搬插件体系。

**搬入组件**：SkillViewerOverlay

**Loom 侧改动**：
- 新增 JSON-RPC 方法：`skill.list`, `skill.enable`, `skill.disable`, `skill.info`
- 对接 Loom Skill Engine 的 registry
- 技能懒加载状态上报

**验证标准**：技能列表显示 Loom 内置技能 + 外部安装技能，可启用/禁用。

---

### Phase E — 书桌

**目标**：文件拖拽 + 便签（笺）+ 文件预览 工作区。

**搬入组件**：DeskSection, DeskDropZone, DeskTree, DeskToolbar, DeskEditor, DeskCwdSkills, DeskEmptyOverlay, PreviewPanel, PreviewEditor

**搬入的状态管理**：
- desk-slice.ts + desk-actions.ts
- preview-slice.ts + preview-actions.ts
- screenshot-slice.ts
- workspace-ui-state-actions.ts

**Loom 侧改动**：
- 书桌数据持久化（文件列表 + 便签内容）
- 文件变更监听（file-watch-registry.cjs + workspace-watch-registry.cjs）
- 新增 JSON-RPC 方法：`desk.list`, `desk.create_note`, `desk.delete_item`, `desk.update_note`, `desk.watch`, `desk.unwatch`
- Agent 心跳改为监听文件变更事件（Loom Event Bus），替代轮询

**验证标准**：拖入文件，Agent 检测到并主动读取；写一张便签笺，Agent 按笺的指示执行；图片/视频点开全屏预览正常。

---

### Phase F — 设置 + 自动更新

**目标**：图形化配置 Loom 引擎，桌面端自动更新。

**搬入组件**：SettingsModalShell, AutoUpdateStatus

**搬入的状态管理**：
- model-slice.ts
- settings-modal-actions.ts

**Loom 侧改动**：
- 新增 JSON-RPC 方法：`config.get`, `config.set`, `config.schema`
- 对接 Loom Config 体系（模型选择、推理参数、沙盒级别、Provider 切换等）
- Session 功能开关：`session.thinking_level.set`, `session.permission_mode.set`
- 对接 `auto-updater.cjs`

**验证标准**：在设置面板修改模型，Agent 下次对话使用新模型；检查更新流程正常。

---

### Phase G — Onboarding 首次启动向导

**目标**：新用户首次启动 Loom 时的引导流程。

**搬入资产**：
- splash.html, splash-main.tsx（启动屏）
- onboarding.html, onboarding-main.tsx（引导向导）
- first-run.js → 重写为 Loom 版本

**Loom 侧改动**：
- 首次启动检测（无 config 文件 / 无 Agent）
- 向导步骤：选择本地模型路径 → 配置第一个 Agent → 设置工作目录 → 完成
- 对接 Loom 的 `loom doctor` 诊断结果

**验证标准**：删掉配置目录后启动，走完 Onboarding 流程，Agent 可正常对话。

---

## Adapter 设计

```
Hanako Zustand Store
        │
        ▼
   store action (e.g. sendMessage)
        │
        ▼
   adapter.ts                ← 你只写这一层
        │
        ▼
   window.openloom.send()    ← Loom 的 JSON-RPC 2.0
        │
        ▼
   Loom Rust Engine
```

### 核心改动点

1. **`use-hana-fetch.ts` → `adapter.ts`**：原来走 `fetch(/api/...)` 的地方改为走 `window.openloom.send()`
2. **流式消费**：原来的 SSE `/api/chat/stream` → Loom 的 `/sse/:sessionId`（协议相同，改 URL）
3. **连接状态**：`connection-slice.ts` 改为监听 `window.openloom` 的 WebSocket 状态
4. **消息格式映射**：Hanako 的消息结构（blocks: text/tool_call/thinking/...） → Loom 的消息结构，在 adapter 中做一次映射

### API 映射示例

| Hanako 原有调用 | Adapter 翻译为 |
|----------------|---------------|
| `POST /api/chat { message, sessionId }` | `agent.chat { message, session_id }` |
| `GET /api/sessions` | `session.list {}` |
| `POST /api/sessions` | `session.create {}` |
| `DELETE /api/sessions/:id` | `session.delete { id }` |
| `POST /api/sessions/:id/archive` | `session.archive { id }` |
| `POST /api/agents` | `agent.create { name, persona? }` |
| `GET /api/agents` | `agent.list {}` |
| `PATCH /api/agents/:id` | `agent.configure { id, ... }` |
| `DELETE /api/agents/:id` | `agent.delete { id }` |
| `GET /api/commands` | `command.list {}` |
| `POST /api/config` | `config.set { key, value }` |
| `GET /api/config` | `config.get { key }` |

### SSE 流式适配

Hanako 前端消费 HTTP SSE 流，Loom 暴露 SSE endpoint（`/sse/:sessionId`，已存在于 `crates/server/`）。协议相同，只需改连接 URL。

### 消息 Block 渲染管线

Hanako 的消息由多个 Block 组成，每个 Block 有不同的渲染器：

```
Server 返回 message.blocks[]
  ↓
block-extractors.js 解析
  ↓
block-renderers.ts 映射组件
  ↓
├── text        → MarkdownContent / StreamingMarkdownContent
├── tool_call   → ToolGroupBlock
├── thinking    → ThinkingBlock
├── mood        → MoodBlock
├── file_output → FileOutputActions
└── error       → ErrorBoundary
```

block-extractors.js 和 message-utils.js 从 server/ 搬到 web/src/，因为它们本质是 UI 渲染逻辑，不属于后端。

---

## 共享 IPC（window.hana）

Hanako preload 中的桌面操作 IPC 合并进 Loom 的 preload.js：

```
window.hana.getServerPort()     → 改为读 window.__enginePort__
window.hana.getServerToken()    → stub（Loom 本地无 token）
window.hana.selectFolder()      → 保留
window.hana.selectFiles()       → 保留
window.hana.readFile()          → 保留
window.hana.writeFile()         → 保留
window.hana.readFileBase64()    → 保留
window.hana.writeFileBinary()   → 保留
window.hana.copyFile()          → 保留
window.hana.openExternal()      → 保留
window.hana.openFolder()        → 保留
window.hana.openFile()          → 保留
window.hana.showInFinder()      → 保留
window.hana.trashItem()         → 保留
window.hana.screenshotRender()  → 保留
window.hana.watchFile()         → 保留（file-watch-registry.cjs）
window.hana.unwatchFile()       → 保留
window.hana.watchWorkspace()    → 保留（workspace-watch-registry.cjs）
window.hana.unwatchWorkspace()  → 保留
window.hana.readDocxHtml()      → 保留
window.hana.readXlsxHtml()      → 保留
window.hana.getFilePath()       → 保留（webUtils.getPathForFile）
window.hana.getFileUrl()        → 保留（路径 → file:// URL）
window.hana.getAppVersion()     → 保留
window.hana.checkUpdate()       → 保留
window.hana.autoUpdateCheck()   → 保留
window.hana.autoUpdateDownload()→ 保留
window.hana.autoUpdateInstall() → 保留
window.hana.autoUpdateState()   → 保留
window.hana.getAutoLaunchStatus()→ 保留
window.hana.setAutoLaunchEnabled()→ 保留
window.hana.onAutoUpdateState() → 保留
window.hana.appReady()          → 保留
window.hana.reloadMainWindow()  → 保留
window.hana.onboardingComplete()→ 保留（Phase G）
window.hana.getSplashInfo()     → 保留（Phase G）
window.hana.syncWindowTheme()   → 保留
window.hana.getAvatarPath()     → 保留（但去掉默认角色头像概念）
window.hana.onShowSkillViewer() → 保留
```

---

## 目录结构（变更后）

```
openLoom/
├── electron/
│   ├── main.js                 ← 不动（引擎生命周期管理）
│   ├── preload.js              ← 合并 Hanako IPC + 保留 JSON-RPC 桥
│   ├── auto-updater.cjs        ← 搬入
│   ├── file-text-io.cjs        ← 搬入
│   ├── file-watch-registry.cjs ← 搬入
│   ├── workspace-watch-registry.cjs ← 搬入
│   ├── ipc-wrapper.cjs         ← 搬入
│   └── package.json
├── web/
│   └── src/
│       ├── adapter.ts          ← 新增：API 翻译层
│       ├── block-extractors.ts ← 搬入（从 Hanako server/）
│       ├── message-utils.ts    ← 搬入（从 Hanako core/）
│       ├── components/
│       │   ├── chat/           ← ChatArea, ChatTranscript, block-renderers,
│       │   │                       AssistantMessage, UserMessage,
│       │   │                       ThinkingBlock, ToolGroupBlock,
│       │   │                       MarkdownContent, StreamingMarkdownContent,
│       │   │                       MoodBlock, ChatTimelineNavigator, ...
│       │   ├── app/            ← SidebarLayout, ChatSidebar, AppPages
│       │   ├── desk/           ← DeskSection, DeskDropZone, DeskTree, ...
│       │   ├── input/          ← InputArea, SlashCommandMenu, ModelSelector, ...
│       │   ├── shared/         ← MediaViewer, Toast, ErrorBoundary, ...
│       │   ├── plugin/         ← SkillViewerOverlay
│       │   ├── right-workspace/← ActivityPanel, AutomationPanel
│       │   ├── SessionList.tsx
│       │   ├── ArchivedSessionsModal.tsx
│       │   ├── SettingsModalShell.tsx
│       │   ├── FloatPreviewCard.tsx
│       │   ├── FloatingPanels.tsx
│       │   ├── ActivityPanel.tsx
│       │   ├── AutomationPanel.tsx
│       │   ├── PreviewPanel.tsx
│       │   ├── PreviewEditor.tsx
│       │   └── StatusBar.tsx
│       ├── stores/             ← 搬入（裁掉 bridge/channel/browser/computer/plugin/subagent）
│       ├── hooks/              ← 搬入（裁掉 use-plugin-iframe, use-i18n）
│       ├── splash-main.tsx     ← 搬入
│       ├── splash.html         ← 搬入
│       ├── onboarding-main.tsx ← 搬入
│       ├── onboarding.html     ← 搬入
│       ├── App.tsx             ← 搬入（裁掉 ChannelsPanel/BridgePanel/ComputerUse 引用）
│       ├── main.tsx            ← 替换（与 App.tsx 配套）
│       ├── styles.css          ← 搬入
│       ├── animations.css      ← 搬入
│       └── themes/             ← 搬入
├── crates/                     ← 不动
├── migrations/                 ← 不动
└── docs/
    └── hanako-fusion-plan.md   ← 本文档
```

# openLoom 前端重构方案

> 2026-05-27 | Phase 4 前置设计 | 替代老 electron/ + web/ + shared/ | 仅 Windows

---

## 一、背景与目标

### 现状问题

老前端从 openhanako 移植，存在以下结构性问题：

1. **web + Electron 分离模式** — 两套 package、两个 build step、端口注入逻辑重复
2. **22 个 Zustand slice，8 个对应已删除的后端空壳 API**（bridge、desk、automation、channels、browser、screenshot、computer-overlay、plugin-ui）
3. **两套并行流式处理** — `ws-message-handler.ts`（589行）+ `StreamBufferManager` 并存，需要 `stream-invalidator.ts` 打破循环依赖
4. **多入口页面** — splash、onboarding、settings、browser-viewer 各为独立 HTML，不共享状态
5. **`shared/` + `core/` + `lib/`** — 三套 Node.js 代码与 Rust 后端功能重叠

### 目标

- **仅 Windows 平台** — 不处理 macOS/Linux 适配
- **单 Electron app**，不再拆 web + electron 两个 package
- **薄主进程** — Main 只做窗口/托盘/文件对话框/启动 loom-server
- **渲染进程直连 Rust 后端** — WebSocket JSON-RPC 2.0，不走 preload 代理
- **Rust 类型自动生成 TypeScript 类型** — specta 从 loom-types 导出
- **老代码做参考，新写不复制**

---

## 二、竞品参考

| | Claude Desktop | Jan / LM Studio | OpenClaw | ChatGPT |
|---|---|---|---|---|
| 壳 | Electron | Electron → Tauri | daemon | 原生 Cocoa |
| 通信 | IPC (invoke+push) | REST+SSE | WS JSON-RPC | SSE |
| 页面 | 单窗口 SPA | 单窗口 SPA | 多客户端 | 多窗口 |
| 状态管理 | Zustand | Zustand | - | Core Data |

**关键结论：**
- 单窗口 SPA 是主流，settings/onboarding 集成为面板而非独立窗口
- Zustand 是 AI 桌面项目的共识选择
- API Key 必须留在主进程/后端，不进渲染进程（openLoom 已在 Rust 端）
- 流式推送不用轮询

---

## 三、后端 API 对照

以下基于 `loom-server/src/dispatch.rs` 实际暴露的 38 个 JSON-RPC 方法和 `event_bus.rs` 的 9 个事件类型。

### RPC 方法覆盖

| 方法 | 前端覆盖 | 说明 |
|------|:--:|------|
| `system.health` | ✅ | 启动时调用，StatusBar 展示版本/Agent 数 |
| `system.shutdown` | — | 不是独立 RPC。退出流程：主进程发 WS close → loom-server 检测断连后自行退出 |
| `chat.send` | ✅ | 核心路径，见 7.2 |
| `session.list` | ✅ | 启动加载 + 定时刷新 |
| `session.create` | ✅ | Sidebar 新建按钮 |
| `session.switch` | ✅ | 点击会话切换 |
| `session.messages` | ✅ | 加载历史消息 |
| `session.rename` | ✅ | Sidebar 右键→重命名（行内编辑） |
| `session.delete` | ✅ | Sidebar 右键→删除（确认弹窗） |
| `session.bind_agent` | ✅ | SettingsModal Agent 绑定 |
| `agent.list` | ✅ | 启动加载 Agent 池 |
| `agent.status` | ✅ | Agent 面板展示状态 |
| `agent.kill` | ✅ | Agent 面板终止按钮 |
| `agent.config.list` | ✅ | SettingsModal Agent 配置列表 |
| `agent.config.get` | ✅ | SettingsModal 查看配置详情 |
| `agent.config.create` | ✅ | SettingsModal 新建配置表单 |
| `agent.config.update` | ✅ | SettingsModal 编辑配置表单 |
| `agent.config.delete` | ✅ | SettingsModal 删除配置 |
| `model.list` | ⚠️ | 后端当前硬编码返回 `{ models: [], activeModel: null }`。首版前端 ModelSelector 使用硬编码模型列表，等后端补齐 |
| `model.switch` | ⚠️ | 后端当前 no-op（返回 `{ ok: true }` 但不实际切换） |
| `config.get` | ⚠️ | 后端当前为 no-op（返回硬编码 `{ key }`），首版 SettingsModal 的配置项暂用 localStorage，等后端补齐 |
| `config.set` | ⚠️ | 同上，后端 no-op（返回 `{ ok: true }` 但不持久化） |
| `tools.list` | ✅ | SettingsModal 工具列表展示 |
| `skills.list` | ⚠️ | 后端当前硬编码返回 `{ skills: [] }`。首版 SettingsModal Skills Tab 展示空列表，等后端补齐 |
| `mcp.list_servers` | ✅ | SettingsModal MCP 面板 |
| `mcp.list_tools` | ✅ | SettingsModal MCP 工具列表 |
| `mcp.list_resources` | 📋 | P2 — MCP 资源浏览器 |
| `mcp.read_resource` | 📋 | P2 — 同上 |
| `mcp.list_resource_templates` | 📋 | P2 — 同上 |
| `mcp.list_prompts` | 📋 | P2 — MCP 提示词面板 |
| `mcp.get_prompt` | 📋 | P2 — 同上 |
| `lsp.list_servers` | 📋 | P2 — LSP 状态面板 |
| `lsp.diagnostics` | 📋 | P2 — 行内诊断展示（CodeMirror） |
| `lsp.completion` | 📋 | P2 — 编辑器补全集成 |
| `lsp.hover` | 📋 | P2 — 编辑器悬停提示 |
| `lsp.definition` | 📋 | P2 — 编辑器跳转定义 |
| `lsp.references` | 📋 | P2 — 编辑器查找引用 |
| `lsp.symbols` | 📋 | P2 — 编辑器符号列表 |
| `lsp.shutdown` | 📋 | P2 — LSP 生命周期 |

> **注：** `mcp.connect` / `mcp.disconnect` 当前在 dispatch 中不存在对应的 RPC，需要后端补齐。KG 查询（`query_kg_context` / `search_knowledge`）也未暴露为 RPC。这些留到 P2。

### 后端推送事件（AgentEvent 9 种）

| 事件 | 前端处理 |
|------|----------|
| `chat.stream_delta` | StreamBufferManager 累积 text → 200ms flush |
| `chat.stream_end` | finalize buffer → 写入 chat store |
| `chat.token_usage` | 更新 ContextRing 数据 |
| `tool.started` | StreamBuffer 累积 → flush tool_group block |
| `tool.completed` | 更新 tool_group block 状态 + 耗时 |
| `agent.state_changed` | 更新 agent store 状态 + toast 通知 |
| `agent.subagent_spawned` | 渲染子 Agent 卡片（SubagentCard） |
| `agent.subagent_completed` | 更新子 Agent 卡片为完成态 |
| `agent.subagent_errored` | 显示子 Agent 错误状态 |

> **注：** 后端不存在 `chat.stream_start`、`thinking.*`、`content_block` 事件。流式开始由第一条 `chat.stream_delta` 隐式标记。Thinking/推理内容当前后端未推送独立事件。`content_block` 的 file/screenshot 等信息包含在 `chat.stream_end` 或 `tool.completed` 的 payload 中。

---

## 四、目录结构

```
frontend/                       ← 新，替代老 electron/ + web/
├── package.json                ← 单 package
├── electron-builder.yml
├── electron.vite.config.ts
├── tsconfig.json
├── src/
│   ├── main/                   ← 主进程（薄）
│   │   ├── index.ts            ← 入口：单实例锁、创建窗口、注册 IPC、启动 loom-server
│   │   ├── window.ts           ← BrowserWindow 工厂（frameless、状态持久化）
│   │   ├── tray.ts             ← 系统托盘 + 右键菜单
│   │   ├── engine.ts           ← spawn loom-server、读 stdout 拿 port、崩溃重启
│   │   ├── updater.ts          ← electron-updater 集成
│   │   └── ipc/
│   │       ├── files.ts        ← 文件对话框、读文件
│   │       ├── shell.ts        ← openExternal、openFolder
│   │       └── app.ts          ← getVersion、getPlatform、window 控制
│   │
│   ├── preload/                ← 极薄，50-200 行
│   │   └── index.ts            ← contextBridge 只暴露 OS 能力 (window.hana)
│   │
│   └── renderer/               ← React SPA（厚）
│       ├── index.html
│       └── src/
│           ├── App.tsx
│           ├── components/
│           │   ├── app/        ← AppShell、Sidebar、StatusBar、WindowControls
│           │   ├── chat/       ← ChatArea、MessageList、AssistantMessage、
│           │   │                  UserMessage、ThinkingBlock、ToolGroupBlock、
│           │   │                  TextBlock、FileBlock、SubagentCard、block-renderers
│           │   ├── input/      ← InputArea、TipTapEditor、SlashCommandMenu、
│           │   │                  FileMentionMenu、ContextRing、ModelSelector、
│           │   │                  PermissionModeButton、ThinkingLevelButton、
│           │   │                  SendButton、AttachedFiles、QuotedSelectionCard
│           │   └── shared/     ← Button、ContextMenu、Overlay、Select、Toggle、
│           │                      Toast、ErrorBoundary、WelcomeScreen、
│           │                      Onboarding、SettingsModal、MediaViewer
│           ├── stores/         ← Zustand（9 slices）
│           ├── hooks/          ← useStreamBuffer、useTypewriterText、
│           │                      useContinuousScroll、useAnimatePresence 等
│           ├── services/       ← WebSocket 单例、StreamBufferManager
│           ├── utils/          ← markdown、grapheme、file-kind、icons 等
│           ├── editor/         ← CodeMirror WYSIWYG + TipTap
│           ├── types/          ← specta 生成的 TS 类型 + Zod schema
│           └── themes/         ← CSS 主题文件
└── resources/                  ← 图标 (.ico)
```

老目录在 Phase 4 切换完成后删除：`electron/`、`web/`、`shared/`、`core/`、`lib/`、`crates/`

---

## 五、架构模式

### 方案：薄壳直连（Thin Shell + Direct WebSocket）

```
┌─ Renderer Process ─────────────────────────────┐
│  React App                                       │
│  Zustand stores (9 slices)                      │
│  WebSocket ──────── ws://127.0.0.1:PORT/ws ────→ loom-server (Rust) │
│  StreamBufferManager (non-React singleton)       │
└──────────────────────────────────────────────────┘
┌─ Main Process ─────────────────────────────────┐
│  BrowserWindow (frameless, 单窗口)               │
│  单实例锁 (app.requestSingleInstanceLock)        │
│  Tray + 右键菜单                                  │
│  文件对话框 (selectFolder/selectFiles)            │
│  窗口控制 (min/max/close/drag)                    │
│  自动更新 (electron-updater)                      │
│  loom-server 生命周期 (spawn / crash-restart /   │
│    graceful-shutdown)                            │
└──────────────────────────────────────────────────┘

preload: contextBridge 只暴露 OS 能力 (window.hana)
         WS 不经过 preload，渲染进程直连
```

**为什么选这个而不是 Electron IPC 桥：**
- API Key 和敏感逻辑全在 Rust 后端，Renderer 只是展示层
- 少一层 IPC 转发，流式延迟最低
- 老代码的 WebSocket 层逻辑成熟，但需要合并两条并行路径

---

## 六、Electron 平台层

### 6.1 安全配置

```typescript
// BrowserWindow webPreferences
{
  contextIsolation: true,       // 必须，渲染进程隔离
  nodeIntegration: false,       // 必须，禁止 Node.js 入渲染进程
  sandbox: true,                // OS 级沙箱
  webSecurity: true,            // 禁止跨域
  allowRunningInsecureContent: false,
}

// CSP — 在 renderer/index.html 的 <meta> 标签或 main 进程 session.webRequest 设置
// connect-src: ws://127.0.0.1:*  (仅允许本地 WS)
// script-src: 'self'             (禁止 inline script)
// img-src: 'self' data: file:    (允许本地图片 + data URL + 附件)
// style-src: 'self' 'unsafe-inline' (Tailwind 需要)
```

- **safeStorage**: 如主进程需持久化凭据（OAuth token 等），用 `safeStorage.encryptString()` 加密后存 `electron-store`
- **markdown 清洗**: `markdown-html-sanitizer.ts` 必须剥离 `<img src="http(s)://...">` 等远程资源，防止隐私泄露（IP 暴露给外部服务器）

### 6.2 生命周期

```
app.whenReady()
  ├─ app.requestSingleInstanceLock()  ← 禁止双开
  ├─ spawn loom-server (engine.ts)
  │     ├─ 读 stdout 逐行 JSON 解析
  │     ├─ { type: "ready", port: N } → 记录 port
  │     ├─ { type: "error", ... }     → 弹错误对话框 + 退出
  │     ├─ 超时 30s 无 ready           → 弹错误对话框 + 退出
  │     └─ child.on('exit')           → 自动重启（指数退避 1s/2s/4s/8s，最大 5 次）
  ├─ 创建 BrowserWindow（show: false）
  ├─ 加载 renderer/index.html
  └─ win.on('ready-to-show') → win.show()

app.on('window-all-closed')
  └─ 关闭 WS 连接 → loom-server 检测断连自行退出 → 等待 5s → child.kill() → app.quit()

app.on('before-quit')
  └─ 保存 window state（position、size、maximized）到 electron-store
```

### 6.3 崩溃恢复

| 场景 | 处理 |
|------|------|
| loom-server 启动失败 | 弹原生错误对话框（"引擎启动失败"），提供重试/退出按钮 |
| loom-server 中途崩溃 | `child.on('exit')` → 指数退避重启（最大 5 次）→ 重启成功后 Renderer 自动重连 WS |
| Renderer 崩溃 | `webContents.on('crashed')` → 重载窗口 |
| Renderer 无响应 | `webContents.on('unresponsive')` → 提示用户强制重载 |
| WebSocket 断连 | wsState → reconnecting（指数退避 1s/2s/4s/.../30s，最大 20 次）→ 恢复后重新调用 session.list + agent.list + model.list |
| 磁盘满 | 后端返回错误 → 前端 toast 告警（不静默） |

### 6.4 窗口状态持久化

- 窗口位置/大小/最大化状态 → `electron-store`（JSON 文件 in `app.getPath('userData')`）
- 启动时恢复，验证存储位置是否仍在有效显示器范围内（外接显示器可能已移除）
- 侧栏宽度 → localStorage（`useSidebarResize` hook 管理）

### 6.5 构建与打包

```yaml
# electron-builder.yml
appId: com.openloom.app
productName: openLoom
directories:
  output: dist
  buildResources: resources
extraResources:
  - from: ../target/release/loom-server.exe
    to: engine/loom-server.exe
win:
  target: nsis
  icon: resources/icon.ico
nsis:
  oneClick: false
  allowToChangeInstallationDirectory: true
```

- Dev 模式：`electron-vite dev` — Main/Preload 用 Node target，Renderer 用 Vite HMR
- Prod 模式：`electron-vite build` → `electron-builder` 打包为 NSIS 安装包
- loom-server 通过 `extraResources` 打包到安装目录下的 `engine/` 文件夹

---

## 七、Zustand Store 拆分（9 slices）

核心原则：流式数据不进 Zustand，持久数据进。稳定引用与易变状态分离。

| Slice | 职责 | 关键字段 |
|---|---|---|
| **connection** | WS 连接状态 | `wsState: 'connected'\|'reconnecting'\|'disconnected'`, `port`, `reconnectAttempt` |
| **session** | 会话列表 + CRUD | `sessions[]`, `currentSessionId`, `pinnedSessions[]`, `switchSession()`, `createSession()`, `renameSession()`, `deleteSession()`, `pinSession()`, `searchSessions()` |
| **chat** | 消息 + ContentBlock | `messagesBySession: Map<SessionId, Message[]>`, `appendBlocks()`, `upsertBlock()`, `patchBlockByTaskId()`, `deleteMessage()`, `regenerateMessage()`, `copyMessage()`, LRU 8 会话驱逐 |
| **streaming** | 流式状态 | `streamingSessionIds: Set<SessionId>`, inline error 自动消失计时器（5s TTL + 竞态守卫） |
| **agent** | Agent 列表 + 状态 | `agents[]`, `currentAgentId`, `agentStatuses: Map<AgentId, AgentStatus>` |
| **model** | 模型 + thinking level | `currentModel`, `thinkingLevel`, `models[]`, `tokenUsage: { prompt, completion }` |
| **ui** | 主题 + 布局 | `theme`, `sidebarWidth`, `activePanel`, `settingsOpen`, `osThemeFollow: boolean` |
| **input** | 输入草稿 + 附件 | `draftBySession: Map<SessionId, Draft>`, `attachedFiles[]`, `quotedSelection`, `permissionMode: 'operate'\|'ask'\|'read_only'` |
| **selection** | 消息多选 | `selectedMessageIds: Set<MessageId>`, `selectMode: boolean`（用于截图多选） |

**不进 Zustand 的：**
- `StreamBufferManager` — 纯 JS 单例，按 messageId 管理 buffer，200ms 节流 flush 到 Zustand
- `WebSocket` 实例 — 模块级单例，存活期独立于组件树

**从老代码保留的模式：**
- `create-keyed-slice` 工厂 — 用于 `messagesBySession`、`draftBySession` 等 per-session keyed store
- `message-live-version` — 并发守卫，防止历史加载覆盖正在流式写入的新消息
- LRU 会话驱逐 — 最大缓存 8 个会话（按会话数驱逐，单个会话消息数不做硬限制，靠虚拟化滚动保证性能）

---

## 八、组件树

```
components/
├── app/
│   ├── AppShell              ← frameless 窗口 + 标题栏拖拽区
│   ├── Sidebar               ← 会话列表（搜索、pin、右键菜单）+ Agent 选择 + 新建会话
│   ├── SessionItem           ← 单个会话行（pin 按钮、右键菜单、行内重命名）
│   ├── SessionSearch         ← 会话搜索框（180ms 防抖 + 两阶段搜索）
│   ├── ArchivedSessionsModal ← 已归档会话管理（恢复、永久删除、按时间清理）
│   ├── WindowControls        ← 最小化/最大化/关闭
│   └── StatusBar             ← WS 连接状态指示灯 + 当前模型名 + token 用量
│
├── chat/
│   ├── ChatArea              ← 消息列表容器 + 自动滚底 + 时间线导航
│   ├── MessageList           ← 虚拟化消息列表（@tanstack/react-virtual）
│   ├── TimelineNavigator     ← 右侧时间线锚点（对话轮次标记、点击跳转）
│   ├── AssistantMessage      ← ContentBlock 分发（switch type）+ 操作按钮（重新生成、删除、复制）
│   ├── UserMessage           ← 用户气泡 + 附件展示 + 行内编辑（Ctrl+Enter 确认）+ 操作按钮
│   ├── MessageFooterActions  ← 消息底部操作栏（复制/重新生成/编辑/删除 + 时间戳）
│   ├── ThinkingBlock         ← 折叠思考块 + 流式动画点 + sealed 标记
│   ├── ToolGroupBlock        ← 工具调用组（中文标签 + 耗时 + 进度条 + 展开参数 JSON + 成功/失败徽章）
│   ├── TextBlock             ← Markdown 渲染 + tail-fade 动画 + 代码块复制按钮
│   ├── FileBlock             ← 文件输出卡片（打开/下载/复制路径/在文件夹中显示 下拉菜单）
│   ├── SubagentCard          ← 子 Agent 任务卡片（流式状态 + 摘要 + 跳转子会话）
│   └── block-renderers       ← 可插拔 ContentBlock 渲染器注册表
│
├── input/
│   ├── InputArea             ← 编辑器 + 控制栏容器 + InputStatusBars
│   ├── TipTapEditor          ← 主编辑器（StarterKit + SkillBadge + FileBadge 扩展）
│   ├── SlashCommandMenu      ← 斜杠命令菜单（/new /compact /stop /clear + 动态 skill 命令）
│   ├── FileMentionMenu       ← @文件 提及菜单（跨源聚合：附件 + 会话 + 工作区文件）
│   ├── ContextRing           ← SVG 环形上下文窗口指示器（hover 详情 + 压缩中动画）
│   ├── ModelSelector         ← 模型切换下拉
│   ├── PermissionModeButton  ← 权限模式三态切换（operate / ask / read_only）
│   ├── ThinkingLevelButton   ← 思考等级六态切换（off/auto/low/medium/high/xhigh，模型感知过滤）
│   ├── SendButton            ← 发送/停止（流式中切换为停止按钮）
│   ├── AttachedFiles         ← 附件 chips 栏（图片缩略图 + 文件类型图标）
│   ├── QuotedSelectionCard   ← 引用选中文本卡片
│   └── InputStatusBars       ← 输入区状态条（slash busy / 压缩中 / 截图进度 / inline error）
│
└── shared/
    ├── ui/                   ← Button、ContextMenu、Overlay、Select、Toggle、Toast
    ├── ErrorBoundary         ← 顶级错误边界（区分渲染错误 vs 网络错误，auto-recovery resetKeys）
    ├── RegionalErrorBoundary ← 区域级错误边界
    ├── WelcomeScreen         ← 空状态欢迎页
    ├── Onboarding            ← 首次启动引导（集成在主窗口，分步骤）
    ├── SettingsModal         ← 设置面板（模态，不走独立窗口）
    ├── MediaViewer           ← 图片/视频全屏查看器（前后导航、缩放、自动隐藏工具栏）
    ├── ActivityPanel         ← 后台活动面板（Agent 心跳、子 Agent 运行、对话摘要卡片）
    └── ToastContainer        ← 全局 Toast 通知（4 类型、去重、持久模式最大 3 条）
```

---

## 九、数据流

### 9.1 启动流程

```
Main: app.requestSingleInstanceLock()
Main: spawn loom-server.exe
  → 读 stdout 逐行 JSON（ready/error）
  → 30s 超时 → 弹原生错误对话框（重试/退出）
  → child.on('exit') → 指数退避重启（最大 5 次）
Main: 创建 BrowserWindow (show: false)
  → 恢复窗口状态（electron-store）
  → 加载 renderer/index.html
  → CSP meta 注入

Renderer: React mount
  → 读 window.__enginePort__
  → 显示加载 spinner
  → new WebSocket(ws://127.0.0.1:{port}/ws)
  → WS open:
      loomRpc('system.health')  → 展示版本/状态
      loomRpc('session.list')   → 恢复会话列表
      loomRpc('agent.list')     → 加载 Agent 池
      loomRpc('model.list')     → 加载模型列表
  → Main: win.show()
  → 首次启动? → 显示 Onboarding 覆盖层
    否则 有历史会话? → 恢复上次会话
    否则 → WelcomeScreen
```

### 9.2 对话消息流（核心路径）

```
用户输入 → 采集 { text, attachedFiles, quotedSelection, permissionMode }
  → loomRpc('chat.send', { session_id, message })

服务端推送 (AgentEvent notifications):

chat.stream_delta ──→ 第一条 delta 隐式标记 stream "开始"
                        streamingSlice.add(sessionId)
                        chatSlice 创建 placeholder assistant message (messageId)
                        StreamBuffer 累积 text → 200ms flush
                          → chatSlice.upsertBlock(msgId, text)

chat.token_usage  ──→ 更新 modelSlice.tokenUsage → ContextRing 重绘

tool.started      ──→ StreamBuffer → flush
                        → chatSlice.upsertBlock(msgId, tool_group: { status: 'running' })

tool.completed    ──→ StreamBuffer → flush
                        → chatSlice.upsertBlock(msgId, tool_group: { status: 'done', elapsed })

chat.stream_end   ──→ StreamBuffer.finalize() → 最后 flush
                        streamingSlice.remove(sessionId)
                        后端异步触发 LLM 实体提取
```

> **注：** 当前后端不推送独立的 `thinking.*` 事件。如果模型返回 reasoning_content（通过 `\x02REASONING\x02` 控制信号嵌入 stream_delta），前端的 StreamBuffer 解析该信号并创建 thinking block。这是从老代码保留的逻辑。

### 9.3 StreamBufferManager

```
WebSocket onmessage → StreamBufferManager.handle(event)
  (纯 JS Map，零 React 重渲染)
  │
  ├─ 按 messageId 定位或创建 Buffer
  ├─ text 追加到 buffer.textAcc
  │     ├─ 检测 \x02REASONING\x02 控制信号 → 分离到 thinkingAcc
  │     └─ 检测 \x00USAGE:... 控制信号 → 提取 token 数据
  ├─ tool.started → 追加到 buffer.toolCalls[]
  ├─ tool.completed → 更新对应 toolCall 的状态
  │
  └─ 200ms 定时器到期 → flush(buffer)
       │
       ├─ 渲染 text → markdown HTML（只在完整行边界提交）
       ├─ 渲染 thinking → 折叠块内容
       ├─ 按 display order 组装 ContentBlock[]:
       │   thinking → tool_group → text
       └─ chatSlice.upsertBlocks(msgId, blocks)  ← 唯一一次 Zustand 写入
```

**与老代码的区别：** 合并 `ws-message-handler.ts` 和 `StreamBufferManager` 为一条路径。

**ContentBlock 两物种设计（从老代码保留）：**
- **TextDecorator** (species A) — 流式 upsert：thinking、tool_group、text。同一类型只保留最新
- **RichBlock** (species B) — 原子 push：file、subagent。通过 `taskId` 桥接延迟结果

### 9.4 会话切换

```
用户点击 Sidebar 会话 → sessionSlice.switchSession(newId)
  │
  ├─ inputSlice.saveDraft(oldSessionId)          ← 保存草稿
  ├─ 快照 StreamBuffer（如正在流式）              ← 防止数据丢失
  ├─ 更新 currentSessionId
  ├─ 检查 chatSlice.messagesBySession 缓存
  │     ├─ 命中 → 直接渲染（虚拟化列表滚动到上次位置）
  │     └─ 未命中 → loomRpc('session.messages')
  │                   → chatSlice.hydrate(sessionId, messages)
  ├─ inputSlice.restoreDraft(newSessionId)       ← 恢复草稿
  └─ 从 session.list 缓存中读取 agent_config_name → 恢复 agent 绑定显示
```

**消息版本守卫（从老代码保留）：**
```
switchSession 时递增 _switchVersion
loadMessages 前记录 messageLiveVersion
loadMessages 后检查:
  - messageLiveVersion 变了? → 跳过 hydrate（实时消息已写入）
  - _switchVersion 变了?   → 跳过 hydrate（用户已切走）
```

### 9.5 文件操作

```
用户拖拽文件 / 点击附件按钮
  → ipcRenderer.invoke('select-files')  → Main 打开原生对话框
  → 返回 FileRef[]（路径、大小、MIME）
  → inputSlice.addFiles(files)
      ├─ 图片 → Main 进程 sharp 生成缩略图 dataURL
      ├─ 代码文件 → 读取前 200 行预览
      ├─ 文件大小检查 → 超过 50MB 弹警告
      └─ AttachedFilesBar 展示

发送时:
  → 文件内容序列化为 base64 data URL 或文件路径引用，嵌入消息 content 文本
  → 后端处理消息时通过文件路径读取本地文件
  → 注：当前后端无独立 file.upload RPC，文件传输机制待实现时确定（base64 嵌入 vs 新增 HTTP endpoint）
  → 后端返回 file_id → 嵌入消息 ContentBlock

错误处理:
  → 发送失败 → inline error toast + 文件保留在 AttachedFilesBar 可重试
  → 文件被外部删除 → 发送前检测路径有效性 → toast "文件不存在"
  → 磁盘满 → 后端返回错误码 → 前端 toast "存储空间不足"
```

### 9.6 Slash Commands

```
用户输入 "/" → SlashCommandMenu 弹出
  │
  ├─ 内置命令:
  │   /new      → createSession() + switchSession()
  │   /compact  → 触发对话压缩
  │   /stop     → 停止当前流式
  │   /clear    → 清空当前输入
  │
  └─ 动态 Skill 命令:
      从 skills.list 获取已启用 skills
      → 映射为 SlashItem（name + description + icon）
      → 用户选择后插入 skill badge 到编辑器
```

### 9.7 设置面板（SettingsModal）

SettingsModal 以模态面板形式集成在主窗口内，按 Tab 组织：

| Tab | 内容 | 后端对接 |
|-----|------|----------|
| **Agent** | Agent 配置列表、新建/编辑/删除、绑定到会话 | `agent.config.*` + `session.bind_agent` |
| **Model** | 模型列表、切换活跃模型、Thinking Level 默认值 | `model.list` + `model.switch` |
| **Appearance** | 主题选择（9 套）、跟随 OS 主题开关、字体大小 | `config.get/set` |
| **MCP** | MCP 服务器列表、工具列表、连接状态 | `mcp.list_servers` + `mcp.list_tools` |
| **Skills** | 已安装技能列表、启用/禁用 | `skills.list` |
| **Tools** | 可用工具列表、工具描述 | `tools.list` |
| **About** | 版本号、检查更新、开源许可 | `system.health` + electron-updater |

---

## 十、类型安全

用 **specta** 从 `loom-types` 自动生成 TypeScript 类型：

```
loom-types (Rust)
  #[derive(Type)] 标注所有公开类型
  build.rs 调用 specta::export::ts_with_bigint()
  │
  ├── frontend/src/types/bindings.ts   ← 所有类型定义
  └── frontend/src/types/schemas.ts    ← Zod 运行时校验 schema
```

specta 的关键优势：自动递归解析类型依赖。标注 `EngineEvent` 一个入口，所有引用类型自动导出。不需要手写 `types.ts`。

WebSocket JSON-RPC 层用 Zod 校验入站消息，类型不匹配直接丢弃 + error toast。

---

## 十一、从老代码保留的资产清单

### 架构模式
- ContentBlock 两物种设计 + block-renderers 可插拔注册表
- StreamBufferManager 节流 flush 模式
- messageId 稳定引用贯穿流式生命周期
- messageLiveVersion 并发守卫
- create-keyed-slice per-session 工厂
- LRU 会话驱逐（最大 8 个）
- REASONING/USAGE 控制信号解析

### 组件逻辑（重新实现，参考老代码）
- StreamBufferManager（合并 ws-message-handler + stream-buffer）
- useTypewriterText（grapheme 感知批处理 + reduced-motion）
- useContinuousBottomScroll（物理缓动 + sticky 检测 + ResizeObserver）
- useAnimatePresence（进出动画生命周期）
- useSidebarResize（多面板拖拽 + localStorage 持久）
- usePanel（通用浮动面板控制器）
- useConfig（stale-while-revalidate 缓存）
- session-refresh-scheduler（250ms 防抖 + 去重）
- app-event-actions（7 种事件类型 → 协调 store 更新）

### 编辑器（CodeMirror WYSIWYG 重新实现）
- md-decorations — 实时预览（标题折叠、任务复选框、代码块、Obsidian 图片嵌入、KaTeX、高亮）
- mermaid-field / csv-field / table-field — 代码块替换为可交互组件
- TipTap 扩展（SkillBadge + FileBadge NodeView）+ input-editor-extensions

### 工具层（重新实现，纯逻辑，稳定）
- markdown.ts — markdown-it 配置（Obsidian callout 13 种类型、图片嵌入、高亮、KaTeX、mermaid、任务列表）
- markdown-html-sanitizer.ts — 自定义 DOM 清洗器 480 行（MathML、SVG、KaTeX、mermaid、**远程 img src 剥离**）
- grapheme.ts — Unicode 安全分词（Intl.Segmenter + Array.from fallback）
- message-parser.ts — 内容块解析（附件、mood、引用、卡片、25+ 工具类型详情提取）
- editor-serializer.ts — TipTap → 后端消息格式桥接
- history-builder.ts — 后端消息格式 → UI 数据模型（含 COMPAT 逻辑）
- file-kind.ts — 扩展名分类 + 结构化 FileRef ID 防碰撞
- model-metadata.ts — 多级模型元数据查找（provider 专属 → 全局 fallback）
- icons.ts — SVG 图标注册表 + 文件类型→图标映射
- format.ts — cronToHuman、formatSessionDate、escapeHtml、parseCSV
- mermaid-renderer.ts — 懒加载 + 幂等守卫 + source 去重 + 可测试
- screenshot.ts + screenshot-extract.ts + screenshot-segments.ts — 多段截图管线（对话模式 + 文章模式）
- file-mention-items.ts — @文件 跨源聚合（附件 + 会话 + 工作区）
- quoted-selection.ts — 引用排版格式化
- slash-commands.ts — 内置命令注册架构
- timeline-anchors.ts — 时间线锚点纯逻辑（对数缩放 + i18n 日期）
- agent-display.tsx — Agent 身份/头像解析（含 fallback 错误状态）
- model-thinking.ts — 模型思考能力检测

### UI 设计资产
- 9 套 CSS 主题文件 + 主题切换机制
- WindowControls 无框窗口自定义标题栏
- i18n 架构（`zh.json` + 扩展点）
- 基础 UI 组件设计（Button、ContextMenu、Overlay、Toast、Select、Toggle）
- 可访问性参考模式（roving tabindex、focus restore、menu roles、aria-expanded）

---

## 十二、不保留

- `ws-message-handler.ts` — 与 StreamBufferManager 合并
- `use-hana-fetch.ts` — REST→RPC 路由映射，直接用 JSON-RPC
- `adapter.ts` — 双通道 WS 模式，简化成单通道
- `PluginCardBlock / SettingsConfirmCard` — 空壳（返回 null）
- `SubagentCard` (旧) — 空壳，但新 SubagentCard 需从零实现
- `stream-resume.ts` — 全 no-op，重写断连恢复逻辑
- `usePluginIframe` — 占位 hook
- `provider-presets.ts` — provider 配置在后端
- Bridge/Desk/Automation/Channels/Browser/ComputerOverlay slices — 对应已删除的后端 API
- splash/onboarding/browser-viewer/settings 独立入口 — 集成到主窗口
- `shared/` Node.js 代码 — Rust 后端已替代
- `mobile/` — 本方案仅桌面端
- `core/` + `lib/` Node.js 后端 — Rust 后端已替代

---

## 十三、测试策略

| 层级 | 工具 | 覆盖 |
|------|------|------|
| **单元测试** | Vitest | 所有纯函数（utils、markdown 解析、message-parser、StreamBufferManager flush 逻辑、grapheme、slash command 匹配） |
| **组件测试** | Vitest + React Testing Library | 每个 ContentBlock 渲染器（ThinkingBlock 折叠/展开、ToolGroupBlock 状态切换、TextBlock tail-fade、UserMessage 编辑态、空状态/错误状态） |
| **集成测试** | Vitest + MockWebSocket | 模拟 WS 事件流 → 验证 Zustand store 最终状态、会话切换竞态、StreamBufferManager 节流 |
| **E2E** | Playwright + Electron | 启动 loom-server → 发送消息 → 验证流式渲染 → 会话切换 → 设置变更 |

关键测试场景：
- 流式消息写入中途切会话（messageLiveVersion 守卫 + _switchVersion 守卫）
- 流式消息写入中途断连重连（stream resume，WS 重连后重新加载状态）
- 延迟 content_block 通过 taskId 替换占位 block（pending → resolved）
- 大段 markdown 流式换行门控渲染（含代码块/表格，验证不闪烁）
- LRU 会话驱逐（第 9 个会话驱逐第一个，被驱逐会话切换回来时重新加载）
- 虚拟化列表：1000 条消息 + 每个 5 个 ContentBlock，滚动不卡顿
- 两个实例同时启动 → 第二个弹提示并退出（单实例锁）
- loom-server 启动超时 → 错误对话框展示
- WS 30s 无法连接 → StatusBar 展示 "未连接" 状态 + 手动重试按钮
- 文件大小超 50MB → 弹警告阻止发送

---

## 十四、Phase 4 更新计划

原 Phase 4 计划：

| 原计划 | 调整 |
|--------|------|
| `frontend/` monorepo + `@loom/shared` API 客户端 | → 单 package 布局，类型从 Rust specta 生成 |
| 核心页面迁移（ChatPage, AgentsPage, SettingsPage） | → 保留原范围，新增 Onboarding 集成 + Settings 模态化 |
| Electron 壳 TypeScript 化 | → 全栈 TypeScript（main + preload + renderer） |
| 前后端集成测试 | → 保留，新增 Electron 平台层测试（崩溃恢复、双开锁） |
| 删除 legacy 目录 + 旧 crate | → 保留 |
| — | 新增：仅 Windows 平台，不处理 macOS/Linux |
| — | MCP/LSP/KG 前端面板标记 P2，首版不做 |

---

## 十五、P2 后续规划（不纳入首版）

以下功能后端已有 API，前端首版不实现，留到 P2：

- MCP 资源浏览器（`mcp.list_resources` / `read_resource` / `resource_templates`）
- MCP 提示词面板（`mcp.list_prompts` / `get_prompt`）
- LSP 诊断行内展示、补全、悬停、跳转、符号列表（需与 CodeMirror 深度集成）
- KG 搜索/统计面板（需要后端先暴露 KG RPC 方法）
- MCP 连接/断开 RPC（需要后端补齐 dispatch 路由）
- Persona 动态演化触发按钮
- Agent 暂停/恢复 UI

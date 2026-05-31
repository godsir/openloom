# openLoom

本地优先的私人 AI 助理内核。认知图谱记忆、多 Agent 编排、Skills/Plugins/MCP/LSP 工具链、桌面宠物，支持云端和本地模型。

## 架构

```
backend/crates/                   12 个 crate，Rust 2024 + Tokio
├── loom-types        ← 统一类型系统（Agent / Message / Tool / MCP / KG / Session）
├── loom-inference    ← 推理引擎分发（Anthropic / OpenAI / DeepSeek / LM Studio / Ollama）
├── loom-memory       ← 记忆内核（SQLite + FTS5，三库拆分，认知图谱，人格演化）
├── loom-core         ← 编排引擎（AgentPool + agent_loop + ToolRegistry + builtin_tools）
├── loom-context      ← 上下文组装（稳定前缀 + 动态后缀，Token 感知截断）
├── loom-security     ← 权限检查（风险等级 + 沙箱策略）
├── loom-server       ← HTTP/WebSocket 服务（Axum 0.7 + JSON-RPC 2.0）
├── lume-cli          ← CLI 入口（serve / chat / mcp / kg / doctor）
├── lume-mcp          ← MCP 客户端（stdio + HTTP/SSE，resources/prompts 协议）
├── lume-lsp          ← LSP 客户端（30+ 语言，diagnostics/hover/completion/definition/references）
├── lume-skills       ← Skills 解析（Claude Code + OpenClaw SKILL.md 兼容）
└── lume-bridge       ← 外部消息平台接入（Telegram + WeChat iLink）

frontend/                        Electron 38 + React 19
├── src/main/         ← 主进程（窗口管理、引擎生命周期、自动更新、系统托盘、桌宠）
├── src/preload/      ← IPC 桥接（27 个 contextBridge API）
└── src/renderer/     ← 渲染进程（TypeScript + Tailwind CSS 4 + Vite 6 + Zustand 5）
```

## 快速开始

### 前置

- Rust 1.85+
- Node.js 20+（仅桌面客户端开发需要）
- 云端推理：DeepSeek / Anthropic / OpenAI 任一 API Key
- 本地推理：LM Studio (localhost:1234) 或 Ollama (localhost:11434)

### 构建

```powershell
# 后端 CLI
cargo build -p lume-cli --release

# 桌面客户端（需要先启动后端服务）
cd frontend
npm install
npm run dev
```

### 运行

```powershell
# 环境诊断
.\target\release\lume.exe doctor

# 启动服务（前端通过 WebSocket 连接）
.\target\release\lume.exe serve --port 8080

# 终端聊天
$env:DEEPSEEK_API_KEY = "sk-xxx"
.\target\release\lume.exe chat
```

## CLI 参考

### lume chat

| 参数 | 默认值 | 说明 |
|------|--------|------|
| `--model` | `deepseek-v4-flash` | 模型名或 ID |
| `--provider` | `auto` | auto / openai / anthropic / deepseek / lmstudio / ollama |
| `--api-key` | — | API Key 明文 |
| `--api-key-env` | — | 从环境变量读取 |
| `--base-url` | — | 自定义 API 端点 |
| `--mcp-args` | — | 快速连接 MCP |
| `--no-mcp-config` | — | 跳过 mcp.json |
| `--resume` | — | 恢复指定会话 |
| `-c` / `--continue` | — | 继续上次会话 |

### lume serve

| 参数 | 默认值 | 说明 |
|------|--------|------|
| `--host` | `127.0.0.1` | 绑定地址 |
| `--port` | `0` | 端口（0 = 随机） |

端点：

| 路径 | 协议 | 说明 |
|------|------|------|
| `/ws` | WebSocket | JSON-RPC 2.0 双向 + 服务端推送 |
| `/api` | HTTP POST | JSON-RPC 2.0（无推送） |
| `/health` | HTTP GET | 健康检查 |

### lume mcp

```powershell
lume mcp add --name my-server --transport http --url http://localhost:3000
lume mcp list
```

### lume kg

```powershell
lume kg search "trading" --limit 20
lume kg search "偏好" --expand
lume kg stats
```

### lume doctor

诊断运行环境：检查 Rust 工具链、数据库、推理端点可达性。

## 核心功能

### 多 Agent 编排

- `AgentConfig` 16 个字段：persona、system_prompt_override、model、thinking_level、temperature、tool_scope、allowed_tools/disallowed_tools、max_iterations、timeout_secs、max_concurrent_subagents 等
- AgentPool 并发执行子 Agent，WebSocket 实时推送 subagent_spawned/completed/errored 事件
- 会话绑定 Agent 配置，切换会话自动切换 Agent
- 前端 Agent 配置面板完整 CRUD

### 知识图谱长期记忆

- 四表结构：kg_nodes / kg_edges / kg_aliases / kg_evidence
- GraphStore：全文搜索 (FTS5)、邻居查询、BFS 游走、最短路径、兴趣评分（时间衰减加权）
- 每轮对话自动注入相关 KG 上下文到系统提示
- 对话后自动提取实体和关系写入图谱（规则 + LLM）
- 会话删除时自动提升高置信度记忆到全局

### 认知演化

- 23 个技术关键词 + 9 个中文偏好模式自动识别
- 版本化认知快照 (cognition_snapshots)，追踪认知变化历史
- 人格摘要 (PersonaProvider)，从累计认知生成用户画像
- 会话隔离 scope，支持 session/global 两级作用域

### Skills 兼容

- 兼容 Claude Code + OpenClaw SKILL.md 格式，21 个 YAML 字段解析
- 运行时门控：requires_env、requires_bins、requires_config、os_restriction、always_active
- `use_skill(name)` 工具：LLM 可动态获取完整 skill 指令体
- 扫描路径：`~/.loom/skills/`、`~/.claude/skills/`、`~/.openclaw/skills/`
- 前端支持文件夹和 ZIP 导入

### Plugin 系统

- 兼容 Claude Code (manifest.json) + openLoom (plugin.toml) 格式
- 递归扫描 4 层，自动发现 SKILL.md 目录
- 支持捆绑 Skills + MCP 配置
- 前端插件列表和重载

### MCP 调用

- 双传输：stdio 子进程 + HTTP/SSE
- 完整 resources/list、resources/read、prompts/list、prompts/get 协议
- 工具超时保护、连接健康检查、自动重连
- 前端 MCP 服务器 CRUD 面板、工具列表预览

### LSP 集成

- 30+ 语言支持：Rust、TypeScript、Python、Go、C/C++、Java、C#、Swift、Kotlin、Scala、Ruby、Lua、Zig、Haskell、Dart、Vue、Svelte 等
- diagnostics、hover、completion、definition、references、document_symbols
- 前端 LSP 面板：启动/停止服务器、查看诊断、快速启动预设语言

### 桌面宠物

- 基于 Petdex 精灵图格式，兼容 Codex 宠物生态
- 根据 AI 状态自动切换动画：wave / jump / dash / inspect / failed / idle
- 拖拽移动、右键菜单切换大小（小/中/大）、前端 PetTab 管理面板
- 支持多宠物切换，从 `~/.loom/pets/` 自动发现
- 默认关闭，设置 > 桌宠中启用

### Bridge 外部接入

- `ChannelAdapter` trait 统一抽象
- Telegram Bot API（环境变量 TELEGRAM_BOT_TOKEN）
- WeChat iLink 桥接（环境变量 ILINK_API_KEY）
- SQLite 持久化桥接会话和消息

### 自动更新

- electron-updater，GitHub Releases 分发
- 下载进度百分比显示，启动时检查更新提示
- blockmap 差分更新支持

### Token 用量统计

- 按会话/模型/时间段统计 token 消耗
- 缓存命中率、平均延迟、上下文窗口利用率
- 前端 Token 用量仪表盘，支持日/周/月粒度历史趋势图

## JSON-RPC API

55 个方法 + 9 个推送事件，完整定义见 [API 文档](docs/api.md)。

| 分类 | 方法数 | 说明 |
|------|--------|------|
| System | 1 | health |
| Chat | 2 | send、stop |
| Agent | 8 | list、status、kill、config CRUD |
| Session | 9 | list、create、switch、messages、rename、delete、bind_agent 等 |
| Workspace | 3 | get、set_session、set_default |
| Model | 10 | list、switch、config CRUD、save_key、discover |
| Config | 4 | get/set、vision、auxiliary |
| MCP | 13 | list_tools、connect、disconnect、resources、prompts、health、config CRUD |
| LSP | 10 | diagnostics、completion、hover、definition、references、symbols 等 |
| Skills | 4 | list、get、import、delete |
| Plugins | 2 | list、reload |
| KG | 8 | search、stats、neighbors、walk、list、edges_between、node/edge delete、prune |
| Cognitions | 3 | list、snapshots、subjects |
| Token | 2 | summary、history |

推送事件：`chat.stream_delta`、`chat.stream_end`、`chat.token_usage`、`tool.started`、`tool.completed`、`agent.state_changed`、`agent.subagent_spawned`、`agent.subagent_completed`、`agent.subagent_errored`

## 数据目录

| 平台 | 路径 |
|------|------|
| Windows | `%USERPROFILE%\.loom\` |
| macOS / Linux | `~/.loom/` |

```
~/.loom/
├── loom.db          ← 配置库（model_configs / agent_configs / mcp_servers）
├── memory.db        ← 记忆库（events / cognitions / kg_* / cognition_snapshots）
├── session.db       ← 会话库（sessions / message_history / token_usage / bridge_*）
├── skills/          ← 全局技能定义
├── plugins/         ← 插件目录
├── pets/            ← 桌宠资源（Petdex sprite sheet）
├── mcp.json         ← MCP 服务器配置
└── workspace.json   ← 默认工作空间
```

## 开发

```powershell
# 全量类型检查
cargo check --workspace

# 运行测试
cargo test -p loom-inference -p loom-memory -p lume-skills -p lume-mcp \
           -p loom-core -p loom-context -p loom-security -p lume-bridge

# 代码风格
cargo fmt --all
cargo clippy --workspace -- -D warnings

# 前端
cd frontend
npm run typecheck
npm run build
```

## 技术栈

| 层 | 选型 |
|----|------|
| 核心引擎 | Rust 2024 + Tokio async |
| 数据库 | SQLite + FTS5 + refinery 迁移 |
| 推理 | Anthropic / OpenAI / DeepSeek / LM Studio / Ollama |
| 服务 | Axum 0.7 + WebSocket + JSON-RPC 2.0 |
| CLI | clap + tracing-subscriber |
| 前端 | React 19 + TypeScript + Tailwind CSS 4 + Vite 6 + Zustand 5 |
| 桌面 | Electron 38 + electron-updater + electron-store |

## 许可证

Apache 2.0

# openLoom

本地优先的私人 AI 助理内核。多 Agent 编排、认知图谱记忆、Skills/MCP/LSP 工具链、桌面宠物，支持云端和本地模型。

## 架构

```
backend/crates/                   13 个 crate，Rust 2024 + Tokio
├── loom-types        ← 统一类型系统（Agent / Message / Tool / MCP / KG / Session）
├── loom-inference    ← 推理引擎分发（Anthropic / OpenAI / DeepSeek / LM Studio / Ollama）
├── loom-memory       ← 记忆内核（SQLite + FTS5，三库拆分，认知图谱，人格演化）
├── loom-core         ← 编排引擎（AgentPool + agent_loop + ToolRegistry + builtin_tools）
├── loom-context      ← 上下文组装（稳定前缀 + 动态后缀，Token 感知截断）
├── loom-security     ← 权限检查（风险等级 + 沙箱策略）
├── loom-server       ← HTTP/WebSocket 服务（Axum 0.7 + JSON-RPC 2.0）
├── loom-cli          ← CLI 入口（serve / chat / mcp / kg / doctor）
├── loom-cron         ← 定时任务调度（SQLite 持久化，AI 提示词驱动，cron 表达式）
├── loom-mcp          ← MCP 客户端（stdio + HTTP/SSE，resources/prompts 协议）
├── loom-lsp          ← LSP 客户端（30+ 语言，二进制检测、一键安装/卸载、安装进度、诊断面板）
├── loom-skills       ← Skills 解析（Claude Code + OpenClaw SKILL.md 兼容）
└── loom-bridge       ← 外部消息平台接入（ChannelAdapter 抽象，Telegram / 飞书 / 微信适配器）

frontend/                        Electron 38 + React 19
├── src/main/         ← 主进程（窗口管理、引擎生命周期、自动更新、系统托盘、桌宠、IM 接入）
├── src/preload/      ← IPC 桥接（27 个 contextBridge API）
└── src/renderer/     ← 渲染进程（TypeScript + Tailwind CSS 4 + Vite 6 + Zustand 5）
     ├── components/app/      ← 应用壳（顶栏、侧边栏、状态栏）
     ├── components/chat/     ← 聊天工作区（消息流、流式渲染、工具调用卡片）
     ├── components/input/    ← 输入区（多行输入、附件、模型选择、Agent 切换）
     ├── components/write/    ← 写作工作区（CodeMirror 6 编辑器 + AI 助手面板）
     ├── components/kg/       ← 知识图谱（2D 星图可视化、节点详情、维护面板）
     ├── components/plan/     ← 计划面板（任务分解、进度追踪）
     ├── components/todo/     ← 待办面板（任务管理、状态流转）
     ├── components/settings/ ← 全页设置（模型、Agent、MCP、LSP、Skills、定时任务、KG、主题、关于）
     ├── components/shared/   ← 通用组件（按钮、模态框、Toggle 等）
     ├── i18n/                ← 国际化（zh-CN / zh-TW / en-US）
     └── services/            ← 前端服务层（JSON-RPC、WebSocket、FIM、流缓冲）
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
cargo build -p loom-cli --release

# 桌面客户端（需要先启动后端服务）
cd frontend
npm install
npm run dev
```

### 运行

```powershell
# 环境诊断
.\target\release\loom.exe doctor

# 启动服务（前端通过 WebSocket 连接）
.\target\release\loom.exe serve --port 8080

# 终端聊天
$env:DEEPSEEK_API_KEY = "sk-xxx"
.\target\release\loom.exe chat
```

## CLI 参考

### loom chat

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

### loom serve

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

### loom mcp

```powershell
loom mcp add my-server --transport http --url http://localhost:3000
loom mcp list
```

### loom kg

```powershell
loom kg search "trading" --limit 20
loom kg search "偏好" --expand
loom kg stats
```

### loom doctor

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
- 前端星图可视化面板，支持 2D 力导向图浏览与交互

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

### 技能市场（Clawhub）

- Clawhub 社区技能注册表集成 + 自定义源支持
- 支持修改 API 基地址（默认 clawhub.ai，可替换为镜像或私有源）
- 一键安装 / 卸载技能
- 前端技能市场与本地技能合并为统一 Tab（「已安装」/「市场」切换）

### MCP 调用

- 双传输：stdio 子进程 + HTTP/SSE
- 完整 resources/list、resources/read、prompts/list、prompts/get 协议
- 默认内置 playwright + context7，开箱即用
- 工具超时保护、连接健康检查
- DB 持久化配置，断开/删除状态跨重启保留
- 前端 MCP 服务器 CRUD 面板、工具列表预览

### LSP 集成

- 30+ 语言支持：Rust、TypeScript、Python、Go、C/C++、Java、C#、Swift、Kotlin、Scala、Ruby、Lua、Zig、Haskell、Dart、Vue、Svelte 等
- diagnostics、hover、completion、definition、references、document_symbols
- 一键安装/卸载语言服务器，实时安装进度日志
- 前端 LSP 面板：语言卡片网格、状态指示灯（运行中/就绪/未安装）、诊断汇总

### 桌面宠物

- 基于 Petdex 精灵图格式，兼容 Codex 宠物生态
- 根据 AI 状态自动切换动画：wave / jump / dash / inspect / failed / idle
- 拖拽移动、右键菜单切换大小（小/中/大）、前端 PetTab 管理面板
- 支持多宠物切换，从 `~/.loom/pets/` 自动发现
- 默认关闭，设置 > 桌宠中启用

### 主题

- 9 套内置主题：暗色、亮色、星夜、素笺、紫夜、熔岩、鎏金、摩卡、自定义
- 自定义主题支持自主配色，CSS 变量驱动即时切换

### Bridge 外部接入

- `ChannelAdapter` trait 统一抽象，后端 loom-bridge 实现 Telegram / 飞书 / 微信适配器
- 桌面客户端 IM（`frontend/src/main/im/`）完整实现 8 个平台：微信、Telegram、POPO、Discord、QQ、飞书、企业微信、钉钉
- 微信 / POPO 扫码连接，其余平台填凭据（Token / AppID+Secret 等）一键连接，设置页内置接入教程引导
- SQLite 持久化实例配置与收发消息时间（`im_instances` 表，凭据存 configJson）
- 会话级 Agent 绑定，IM 消息路由到对应 Agent

### 自动更新

- electron-updater，GitHub Releases 分发
- 下载进度百分比显示，启动时检查更新提示
- blockmap 差分更新支持

### Token 用量统计

- 按会话/模型/时间段统计 token 消耗
- 缓存命中率、平均延迟、上下文窗口利用率
- 前端 Token 用量仪表盘，支持日/周/月粒度历史趋势图

### 写作助手

- CodeMirror 6 编辑器，支持语法高亮、代码补全、多语言切换
- 侧边 AI 助手面板，选中文本即时提问、改写、翻译、总结
- FIM（Fill-in-the-Middle，中间填充）支持，光标位置上下文感知补全
- VFS 虚拟文件系统，工作区文件侧边栏浏览与切换

### 定时任务调度

- SQLite 持久化 cron 任务，支持标准 cron 表达式
- AI 提示词驱动执行，替代传统 Shell 命令
- 前端 CronTab 完整 CRUD 管理面板
- 任务执行历史追踪

### 计划与待办

- PlanPanel：任务分解、步骤规划、进度追踪
- TodoPanel：待办事项管理、状态流转（待处理 / 进行中 / 已完成）
- 与 Agent 执行流程联动，任务完成后自动通知

### 国际化

- 三语言支持：简体中文 (zh-CN)、繁體中文 (zh-TW)、English (en-US)
- 前端 UI 全覆盖，运行时动态切换无需重启
- 后端 API 错误消息多语言适配

### 全页设置

- 设置页改为全页面布局，左侧分类导航
- 涵盖：模型配置、Agent 管理、MCP 服务器、LSP 服务器、Skills、插件市场、定时任务、知识图谱维护、主题、关于
- CronTab、DevTestTab 等开发调试面板集成
- 自定义市场源：支持添加远程 JSON 目录 URL，持久化存储

## JSON-RPC API

136 个方法 + 9 个推送事件，完整定义见 [API 文档](docs/api.md)。

| 分类 | 方法数 | 说明 |
|------|--------|------|
| System | 1 | health |
| Agent | 3 | list、status、kill |
| Chat | 4 | send、stop、compact、session.last_stop_reason |
| Session | 10 | list、create、switch、messages、delete_message、rename、auto_title、delete、bind_agent、set_memory_enabled |
| Workspace | 3 | get、set_session、set_default |
| Model | 5 | list、switch、save_key、check_key、discover |
| Skills | 5 | list、get、import、delete、reload |
| MCP | 10 | list_servers/tools/resources、read_resource、list_resource_templates、list_prompts、get_prompt、connect、disconnect、server_health |
| Bridge | 9 | list_configs、set_config、delete_config、start/stop_channel、start/stop_all、get_status、test_connectivity |
| LSP | 16 | list_servers、diagnostics、completion、hover、definition、references、symbols、shutdown、shutdown_all、supported_languages、check、install、uninstall、install_status、all_diagnostics、start |
| Tools | 2 | list、respond |
| Config | 10 | get/set · vision、auxiliary、fim、sandbox、defaults |
| LoomMd | 2 | read、save |
| Cognitions | 4 | list、snapshots、subjects、delete |
| Stats | 3 | token_summary、token_history、reset |
| Memory | 11 | promote、quality、health、persona、patterns、consolidate、forget、promote_to_layer、pipeline_status、layer_stats、vector_search |
| KG | 7 | search、stats、neighbors、walk、list、edges_between、prune |
| Completion | 1 | FIM 代码补全 |
| Cron | 9 | detect、list、create、update、delete、pause、resume、history、run_now |
| Plan | 5 | create、get、list、update、delete |
| Todo | 3 | list、update_status、clear |
| Goal | 2 | set、status |
| VFS | 8 | read_file、write_file、list_directory、create_directory、rename、delete、watch_file、unwatch_file |
| Write | 3 | index_workspace、search_workspace、reindex_file |

推送事件：`chat.stream_delta`、`chat.stream_end`、`chat.token_usage`、`tool.started`、`tool.completed`、`agent.state_changed`、`agent.subagent_spawned`、`agent.subagent_completed`、`agent.subagent_errored`

## 数据目录

| 平台 | 路径 |
|------|------|
| Windows | `%USERPROFILE%\.loom\` |
| macOS / Linux | `~/.loom/` |

```
~/.loom/
├── data/
│   ├── session.db       ← 会话库（sessions / message_history / token_usage / bridge_*）
│   ├── memory.db        ← 记忆库（events / cognitions / kg_* / cognition_snapshots）
│   └── cron.db          ← 定时任务库
├── skills/          ← 全局技能定义
├── pets/            ← 桌宠资源（Petdex sprite sheet）
├── mcp.json         ← MCP 服务器配置
└── Loom.md          ← 全局 Agent 纪律文件
```

## 开发

### 推送与版本号

每次推送 `main` 分支时，使用 `scripts/push.sh` 替代 `git push`，脚本会自动统一 `Cargo.toml` 和 `frontend/package.json` 的版本号，并将 patch 版本 +1（如 `0.4.2` → `0.4.3`），然后提交并推送。

```powershell
# 推送 main 分支（自动 bump 版本号）
bash scripts/push.sh

# 带额外参数
bash scripts/push.sh --force
bash scripts/push.sh -u origin main
```

### 编译与检查

```powershell
# 全量类型检查
cargo check --workspace

# 运行测试
cargo test -p loom-inference -p loom-memory -p loom-skills -p loom-mcp \
           -p loom-core -p loom-context -p loom-security -p loom-bridge

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
| 桌面 | Electron 38 + electron-updater |
| 图表 | react-force-graph-2d (KG 星图) |
| 编辑器 | CodeMirror 6 |
| 渲染 | markdown-it + KaTeX + Mermaid + highlight.js |

## 许可证

Apache 2.0

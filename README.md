# openLoom

本地优先的个人 AI 助手。openLoom 将桌面聊天、多 Agent 协作、长期记忆、工具调用和写作工作区整合在一个应用中；既可连接云端模型，也可使用本地运行的 LM Studio 或 Ollama。

> 当前版本：`0.4.32` · 许可证：Apache-2.0

## 能做什么

- **对话与协作**：支持多会话、流式回复、Agent 配置与子 Agent 协作。
- **长期记忆**：通过 SQLite、全文检索和知识图谱保存、检索并关联重要信息。
- **模型选择**：支持 OpenAI、Anthropic、DeepSeek、LM Studio 与 Ollama；可在桌面端管理模型与密钥。
- **工具生态**：支持 MCP 的 stdio、HTTP/SSE 传输，以及 Claude Code / OpenClaw 格式的 Skills。
- **写作与代码**：内置编辑器、AI 选区操作、文件工作区、FIM 补全与 LSP 语言服务。
- **工作流**：提供计划、待办、定时任务、知识图谱可视化和 Token 用量统计。
- **桌面体验**：Electron 客户端支持主题、多语言、自动更新和可选桌面宠物。
- **外部消息接入**：可配置 Telegram、飞书、微信及其他消息渠道，将消息路由至对应 Agent。

## 技术架构

```text
frontend/                  Electron 38 + React 19 桌面客户端
├── src/main/              窗口、引擎生命周期、自动更新、系统集成与 IM
├── src/preload/           安全的 IPC 桥接
└── src/renderer/          React UI、状态管理、聊天、写作与设置界面

backend/crates/            Rust 2024 后端工作区
├── loom-cli               命令行入口：serve / chat / mcp / kg / doctor
├── loom-server            Axum HTTP/WebSocket 与 JSON-RPC 服务
├── loom-core              Agent 编排、工具注册和执行循环
├── loom-inference         云端与本地模型适配
├── loom-memory            会话、认知记忆和知识图谱
├── loom-mcp               MCP 客户端
├── loom-lsp               LSP 客户端与语言服务管理
├── loom-skills            SKILL.md 解析与管理
├── loom-cron              定时任务调度
├── loom-context           上下文组装与裁剪
├── loom-security          工具权限与沙箱策略
└── loom-bridge            外部消息渠道适配
```

前端使用 TypeScript、Tailwind CSS、Vite 与 Zustand；后端基于 Tokio、SQLite、Axum、WebSocket 和 JSON-RPC 2.0。

## 快速开始

### 前置条件

- Rust `1.85+`
- Node.js `20+`
- 至少一种模型服务：云端 API Key，或已启动的 LM Studio / Ollama

### 本地开发

```powershell
# 构建后端 CLI
cargo build -p loom-cli --release

# 启动后端服务（端口为 0 时自动分配）
.\target\release\loom.exe serve --port 8080
```

另开一个终端启动桌面客户端：

```powershell
cd frontend
npm install
npm run dev
```

首次使用可在桌面端的“设置 → 模型”添加服务商和密钥；也可通过环境变量为 CLI 提供密钥。

```powershell
$env:DEEPSEEK_API_KEY = "sk-..."
.\target\release\loom.exe chat --provider deepseek
```

## 打包桌面应用

打包前需要先编译与当前系统匹配的 Rust 引擎。随后在 `frontend` 目录运行打包命令；该命令会先构建前端，再由 electron-builder 生成**当前平台**的安装包，并且不会发布到远程仓库。

```powershell
# 在仓库根目录构建后端引擎
cargo build -p loom-cli --release

# 安装前端依赖并打包
cd frontend
npm ci
npm run package
```

构建产物位于 `frontend/dist/`：Windows 为 NSIS `.exe` 安装程序，macOS 为 `.dmg` 与 `.zip`，Linux 为 `.AppImage`。打包时会将 Rust 引擎和 `resources/builtin` 中的内置资源一并放入应用包中。

如果需要生成自动更新所需的元数据，使用：

```powershell
npm run package:updater
```

## CLI

```text
loom serve                 启动 HTTP / WebSocket 服务
loom chat                  打开终端交互式对话
loom mcp add|list          管理 MCP 服务
loom kg search|stats       搜索或查看知识图谱
loom doctor                检查运行环境
```

常用示例：

```powershell
# 指定模型或兼容 OpenAI 的服务地址
loom chat --model deepseek-chat --provider deepseek
loom chat --provider openai --base-url http://localhost:1234/v1

# 继续上一轮对话或恢复指定会话
loom chat --continue
loom chat --resume "research-notes"

# 添加 MCP 服务
loom mcp add local-tools --transport stdio --command npx --args "-y @example/mcp-server"
loom mcp add remote-tools --transport http --url http://localhost:3000

# 查询知识图谱
loom kg search "项目偏好" --limit 20
loom kg stats
```

服务默认提供以下端点：

| 端点 | 用途 |
| --- | --- |
| `/ws` | WebSocket 双向 JSON-RPC 与服务端事件推送 |
| `/api` | HTTP POST JSON-RPC 接口 |
| `/health` | 健康检查 |

完整接口说明见 [API 文档](docs/api.md)。

## 配置与数据

可复制 [config.example.toml](config.example.toml) 作为模型、路由、存储和日志配置的参考。桌面端也可直接管理大部分设置。

默认数据目录为：

| 平台 | 目录 |
| --- | --- |
| Windows | `%USERPROFILE%\.loom\` |
| macOS / Linux | `~/.loom/` |

其中包含会话、SQLite 数据库、Skills、MCP 配置、插件资源与运行日志。请勿将含有 API Key 或个人数据的目录提交到版本库。

## 开发与验证

```powershell
# Rust
cargo check --workspace
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace -- -D warnings

# 前端
cd frontend
npm run typecheck
npm run build
npm test
```

项目设计记录位于 [docs/design](docs/design/README.md)。

## 发布

项目提供版本同步与发布辅助脚本；在推送 `main` 前可使用：

```powershell
.\scripts\push.ps1
```

脚本会同步 `Cargo.toml` 与 `frontend/package.json` 的版本号，并创建提交后推送。请在确认工作区改动无误后再执行。

## 许可证

openLoom 采用 Apache-2.0 许可证。

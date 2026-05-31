# openLoom v0.2.17

可配置多 Agent、多对话的私人 AI 助理内核。认知图谱记忆 + Skills/Plugins/MCP 调用 + 外部消息平台接入。

## 架构

```
backend/crates/
├── loom-types        ← 统一类型系统 (14 模块)
├── loom-inference    ← 推理引擎 (Anthropic/OpenAI/DeepSeek/LM Studio/Ollama)
├── loom-memory       ← 记忆内核 (SQLite + FTS5 + 知识图谱 kg_nodes/edges/aliases/evidence)
├── loom-core         ← 编排引擎 (Agent + AgentPool + Orchestrator + ToolRegistry)
├── loom-context      ← 上下文组装 (ContextAssembler)
├── loom-security     ← 权限检查
├── loom-server       ← Axum HTTP + WebSocket + JSON-RPC 2.0
├── lume-cli          ← CLI (lume serve/chat/mcp/kg/doctor)
├── lume-mcp          ← MCP 客户端 (stdio + HTTP/SSE + resources + timeout)
├── lume-skills       ← Skills 解析 (Claude Code + OpenClaw SKILL.md)
├── lume-lsp          ← LSP 客户端 (40+ 语言)
└── lume-bridge       ← Bridge 外部接入 (ChannelAdapter + Telegram + WeChat iLink)

frontend/
├── src/main/         ← Electron 主进程
├── src/preload/      ← 预加载脚本 (IPC 桥接)
└── src/renderer/     ← React 19 + Tailwind CSS 4 + Vite 6
```



## 快速开始

### 前置

- Rust 1.85+
- Node.js 20+（桌面客户端开发）
- 云端推理：任一 API Key（DeepSeek / Anthropic / OpenAI）
- 本地推理：LM Studio (localhost:1234) 或 Ollama (localhost:11434)

### 构建 & 运行

```powershell
cargo build -p lume-cli --release

# 聊天（DeepSeek）
$env:DEEPSEEK_API_KEY = "sk-xxx"
.\target\release\lume.exe chat

# 聊天（本地 LM Studio）
.\target\release\lume.exe chat --model qwen3-8b --provider lmstudio --base-url http://127.0.0.1:1234/v1

# 启动 HTTP/WS 服务（自动加载 MCP 配置）
.\target\release\lume.exe serve --port 8080

# 环境诊断
.\target\release\lume.exe doctor
```

### lume chat 参数

| 参数 | 默认值 | 说明 |
|------|--------|------|
| `--model` | `deepseek-v4-flash` | 模型名或 ID |
| `--provider` | `auto` | `auto` / `openai` / `anthropic` / `deepseek` / `lmstudio` / `ollama` |
| `--api-key` | — | API Key（明文） |
| `--api-key-env` | — | 从环境变量读取 API Key |
| `--base-url` | — | 自定义 API 端点 |
| `--mcp-args` | — | 快速连接 MCP：`'http://url'` 或 `'command args...'` |
| `--no-mcp-config` | — | 跳过加载 mcp.json |
| `--resume` | — | 恢复指定会话 |
| `-c` / `--continue` | — | 继续上次默认会话 |

### 会话管理

```
lume chat                    新会话（自动生成唯一 ID）
lume chat -c                 继续 "default" 会话
lume chat --resume my-sess   恢复指定会话
```

### lume serve

| 参数 | 默认值 | 说明 |
|------|--------|------|
| `--host` | `127.0.0.1` | 绑定地址 |
| `--port` | `0` | 绑定端口（0 = 随机端口） |

启动时自动加载 `~/.loom/mcp.json` 和 `.lume/mcp.json` 并连接所有 MCP 服务器。

服务端点：

| 端点 | 协议 | 说明 |
|------|------|------|
| `/ws` | WebSocket | JSON-RPC 2.0 双向 + 服务端推送 |
| `/api` | HTTP POST | JSON-RPC 2.0（无推送） |
| `/health` | HTTP GET | 健康检查 |

### MCP 管理

```powershell
# 添加 HTTP 服务器
.\target\release\lume.exe mcp add --name my-server --transport http --url http://localhost:3000

# 添加 stdio 服务器
.\target\release\lume.exe mcp add --name my-tool --transport stdio --command node --args "server.js"

# 列出已配置的服务器
.\target\release\lume.exe mcp list
```

| 参数 | 默认值 | 说明 |
|------|--------|------|
| `--name` | *必需* | 服务器名 |
| `--transport` | `http` | `http` / `stdio` |
| `--url` | — | HTTP 端点 |
| `--command` | — | stdio 启动命令 |
| `--args` | — | 命令行参数（空格分隔） |
| `--header` | — | HTTP 请求头 `key=value`（可重复） |

配置格式（`~/.loom/mcp.json`）：

```json
{
  "mcpServers": {
    "my-server": {
      "type": "http",
      "url": "https://mcp.example.com/sse",
      "headers": { "Authorization": "Bearer xxx" }
    },
    "local-tool": {
      "type": "stdio",
      "command": "node",
      "args": ["server.js"]
    }
  }
}
```

### 知识图谱

```powershell
# 全文搜索
.\target\release\lume.exe kg search "trading"

# LLM 查询扩展
.\target\release\lume.exe kg search "偏好" --expand

# 统计
.\target\release\lume.exe kg stats
```

| 参数 | 默认值 | 说明 |
|------|--------|------|
| `query` | *必需* | 搜索词 |
| `--limit` | `20` | 最大结果数 |
| `--expand` | — | LLM 查询扩展 |
| `--model` | `gemma-4-e4b` | 扩展用模型 |
| `--expand-url` | `http://localhost:1234/v1` | 扩展用推理端点 |

### 桌面客户端

```powershell
cd frontend

# 安装依赖（首次）
npm install

# 开发模式（需要先启动后端服务）
npm run dev

# 类型检查
npm run typecheck

# 打包
npm run build
npm run package
```

前端通过 WebSocket 连接后端（`ws://127.0.0.1:{port}/ws`），支持热重载。

## 核心功能

### 多 Agent 可配置

- `AgentConfig` 16 个字段（system_prompt_override / model / temperature / tool_scope / allowed_tools 等）
- 5 个 `agent.config.*` RPC 方法
- 会话绑定 agent（`session.bind_agent`）
- Agent 作为 tokio task 独立运行

### 知识图谱长期记忆

- 四表结构：kg_nodes / kg_edges / kg_aliases / kg_evidence
- GraphStore 10 种图查询（search_entities / neighbors / walk / path_between / top_interests）
- 每轮对话自动注入相关 KG 上下文
- LLM 对话后实体自动提取写入

### Skills 兼容

- 兼容 Claude Code + OpenClaw SKILL.md 格式
- 21 个 YAML 字段解析 + 运行时门控
- `use_skill(name)` 工具：LLM 可获取完整 skill 指令体
- 扫描 `~/.claude/skills/` / `~/.openclaw/skills/` / `~/.loom/skills/`

### Plugin 系统

- 兼容 Claude Code (manifest.json) + openLoom (plugin.toml) 格式
- 递归扫描 4 层，自动发现 SKILL.md 目录
- 扫描 `~/.claude/plugins/` / `~/.openclaw/plugins/` / `~/.loom/plugins/`
- 支持捆绑 Skills + MCP 配置

### MCP 调用

- 双传输：stdio 子进程 + HTTP/SSE
- `resources/list` / `resources/read` 协议支持
- 工具调用超时保护（默认 60s）
- 连接健康检查

### 桌面宠物

- 基于 Petdex 精灵图格式的桌面伙伴，兼容 Codex 宠物生态
- 根据 AI 状态自动切换动画：思考/工作/完成/错误等
- 支持拖拽移动、右键菜单切换大小、设置面板管理宠物
- 默认关闭，可在设置 > 桌宠中启用

### Bridge 外部接入

- `ChannelAdapter` trait 统一抽象
- Telegram Bot API (long polling)
- WeChat iLink 桥接
- 飞书 / QQ 按需扩展

### E2E 测试

```bash
cargo test -p loom-inference -p loom-memory -p lume-skills -p lume-mcp -p loom-core -p loom-context -p loom-security -p lume-bridge
```

## 数据目录

| 平台 | 路径 |
|------|------|
| Windows | `%USERPROFILE%/.loom/` |
| macOS | `~/.loom/` |
| Linux | `~/.loom/` |

```
~/.loom/
├── loom.db          ← 配置库 (model_configs / agent_configs / mcp_servers)
├── memory.db        ← 记忆库 (events / cognitions / kg_nodes / kg_edges / kg_aliases)
├── session.db       ← 会话库 (sessions / message_history / token_usage / bridge_*)
├── skills/          ← 全局技能 (SKILL.md)
├── plugins/         ← 插件目录
├── pets/            ← 桌宠资源 (Petdex sprite sheet 格式)
├── mcp.json         ← MCP 服务配置
└── workspace.json   ← 默认工作空间路径
```

## 技术栈

| 层 | 选型 |
|----|------|
| 核心引擎 | Rust 2024 + Tokio |
| 数据库 | SQLite + FTS5 + refinery 迁移 |
| 推理 | Anthropic / OpenAI / DeepSeek / LM Studio / Ollama |
| 服务 | Axum 0.7 + WebSocket + JSON-RPC 2.0 |
| CLI | clap + tracing-subscriber |
| 前端 | React 19 + Tailwind CSS 4 + Vite 6 + Electron 38 |

## 许可证

Apache 2.0

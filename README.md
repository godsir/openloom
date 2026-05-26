# openLoom v2

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
├── lume-cli          ← CLI (lume serve/chat/mcp/doctor)
├── lume-mcp          ← MCP 客户端 (stdio + HTTP/SSE + resources + timeout)
├── lume-skills       ← Skills 解析 (Claude Code + OpenClaw SKILL.md)
├── lume-bridge       ← Bridge 外部接入 (ChannelAdapter + Telegram + WeChat iLink)
└── lume-plugins      ← 内置于 lume-cli，兼容 Claude Code/OpenClaw 插件格式
```

## 开发状态

| Phase | 内容 | 状态 |
|-------|------|:--:|
| Phase 0 | 基础设施 (10 crate + 类型 + inference + V8 迁移) | ✅ |
| Phase 1 | Agent 核心 (Agent + AgentPool + Orchestrator + Server) | ✅ |
| Phase 2 | 工具 + 子 Agent (ToolRegistry + MCP + WS + 流式 + 安全) | ✅ |
| Phase 3 | 记忆 + 技能 (KG + GraphStore + Skills + LLM 提取 + 对话持久) | ✅ |
| Phase 4 | 前端 + 切换 + 删除 legacy | ⏳ |

**愿景差距：** 10/11 已修复（只剩 LSP deferred）。详见 [docs/v2-vision-gap-analysis.md](docs/v2-vision-gap-analysis.md)

**质量：** 20+ tests pass, clippy 0 warnings, fmt clean

## 快速开始

### 前置

- Rust 1.85+
- 云端推理：任一 API Key（DeepSeek / Anthropic / OpenAI）
- 本地推理：LM Studio (localhost:1234) 或 Ollama (localhost:11434)

### 构建 & 运行

```powershell
# 构建
cargo build -p lume-cli --release

# 聊天（DeepSeek）
$env:DEEPSEEK_API_KEY = "sk-xxx"
.\target\release\lume.exe chat

# 聊天（本地 LM Studio）
$env:LMSTUDIO_API_KEY = "lm-studio"
.\target\release\lume.exe chat --model llama-3-8b --api-key-env LMSTUDIO_API_KEY --base-url http://127.0.0.1:1234

# 继续上次对话
.\target\release\lume.exe chat --c

# 启动 HTTP/WS 服务
.\target\release\lume.exe serve --port 8080

# 环境诊断
.\target\release\lume.exe doctor
```

### 会话管理

```
lume chat                    新会话（自动生成唯一 ID）
lume chat --c                继续 "default" 会话
lume chat --resume my-sess   恢复指定会话
```

### MCP 管理

```powershell
.\target\release\lume.exe mcp add --name my-server --transport stdio --command node --args server.js
.\target\release\lume.exe mcp list
```

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
├── data/memory.db   ← SQLite (26 表, V1~V9 迁移)
├── skills/          ← 全局技能 (SKILL.md)
├── plugins/         ← 插件目录
├── mcp.json         ← MCP 服务配置
└── config.toml      ← 应用配置
```

## 技术栈

| 层 | 选型 |
|----|------|
| 核心引擎 | Rust 2024 + Tokio |
| 数据库 | SQLite + FTS5 + refinery 迁移 |
| 推理 | Anthropic / OpenAI / DeepSeek / LM Studio / Ollama |
| 服务 | Axum 0.7 + WebSocket + JSON-RPC 2.0 |
| CLI | clap + tracing-subscriber |
| 前端 | React 19 + Tailwind CSS 4 + Vite 6 (Phase 4 迁移中) |

## 许可证

Apache 2.0

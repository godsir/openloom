# CLAUDE.md — openLoom 项目规范

## 项目定位

openLoom 是一个本地优先的私人 AI 助理内核。核心差异化：
- **认知图谱** 替代聊天记录存储记忆（事件→模式→认知→人格演化）
- **分层路由** 实现 80% 请求不触及大模型（关键词快速路径 + 本地模型兜底）
- **事件驱动** 替代轮询，空闲零 Token 消耗

## 技术栈

- **核心引擎:** Rust 2024 edition, Tokio async runtime
- **数据库:** SQLite + FTS5 (rusqlite bundled), refinery 迁移
- **推理:** LM Studio / Ollama (本地), reqwest (云端 Anthropic/OpenAI/DeepSeek)
- **服务:** Axum 0.7 + WebSocket + JSON-RPC 2.0
- **桌面壳:** Electron 38
- **前端:** React 19 + Tailwind CSS 4 + Vite 6
- **CLI:** clap + tracing-subscriber
- **测试:** cargo test, tempfile

## 工程目录

```
F:/openLoom/
├── backend/crates/          ← ★ v2 新后端（唯一开发目标）
│   ├── loom-types/          ← 统一类型系统 (14 模块，唯一 canonical)
│   ├── loom-inference/      ← 推理引擎 (Anthropic/OpenAI/DeepSeek + InferenceEngine HTTP 本地)
│   ├── loom-memory/         ← 记忆内核 (SQLite + FTS5 + 知识图谱 + AgentConfigStore)
│   ├── loom-core/           ← 编排引擎 (Agent + AgentPool + Orchestrator + ToolRegistry)
│   ├── loom-context/        ← 上下文组装 (ContextAssembler, 已接入 agent_loop)
│   ├── loom-security/       ← 权限检查 (已接入 agent_loop)
│   ├── loom-server/         ← Axum HTTP + WebSocket + JSON-RPC dispatch
│   ├── lume-cli/            ← CLI (lume serve/chat/mcp/doctor)
│   ├── lume-mcp/            ← MCP 客户端 (stdio + HTTP/SSE)
│   ├── lume-lsp/            ← LSP 客户端 (40+ 语言, diagnostics/hover/completion/definition/references/symbols)
│   ├── lume-skills/         ← Skills 解析 (Claude Code + OpenClaw SKILL.md)
│   └── lume-bridge/         ← Bridge 外部接入 (ChannelAdapter + Telegram + BridgeManager)
├── crates/                  ← 老代码 (50+ crate, 参考+并行编译, Phase 4 删除)
│   ├── engine/memory/skills/models/router/weaver/cache/server/inference/sandbox/
│   ├── loom-protocol/       ← Codex 移植 (MCP 客户端/服务端/插件框架/Skills/Bridge, 可复用资产)
│   └── loom-utils/          ← Codex 移植工具库
├── core/ + lib/ + shared/   ← Node.js 第二后端 (Phase 4 删除)
├── web/ + electron/         ← 前端 + Electron 壳 (Phase 4 迁移)
├── migrations/              ← refinery SQL 迁移 (V1~V9)
├── docs/                    ← 设计文档 + 差距分析
└── tests/                   ← 集成测试
```

## 核心开发原则

1. **所有后端开发在 `backend/crates/` 进行**，不修改老 `crates/` / `core/` / `lib/`
2. **复制再修改** — 老 `crates/loom-protocol/` 有大量可复用代码（MCP、Skills、Bridge、Plugin），先复制到 `backend/crates/` 再适配新类型系统，不从零造轮子
3. **老代码只做参考和并行编译**，Phase 4 统一删除
4. 禁止自己使用 `npm run dev` 启动 Electron 和 server，用户会自己手动启动

## 开发约定

1. **TDD 强制:** 先写测试，验证失败，再写实现，验证通过，提交
2. **提交粒度:** 每个 Task 一个 commit，commit message 遵循 `feat:` / `test:` / `fix:` / `chore:` 前缀
3. **代码风格:** `cargo fmt` + `cargo clippy -- -D warnings` 零警告
4. **测试覆盖:** 每个公开函数必须有单元测试，每个管线阶段必须有集成测试
5. **禁止:** 不写 docstring（代码即文档），不引入不必要的抽象层。功能实现以当前设计文档为准
6. **错误处理:** 使用 `anyhow::Result` 作为公开 API 返回类型，内部用 `thiserror`
7. **日志:** `tracing` crate，默认 INFO 级别，不记录用户对话内容

## 路径约定

| 平台 | 数据目录 |
|------|---------|
| Windows | `%USERPROFILE%/.loom/` |
| macOS | `~/.loom/` |
| Linux | `~/.loom/` |

子目录: `skills/` (SKILL.md), `data/` (memory.db), `mcp.json` (MCP 配置)

## 测试命令

```bash
# 全量 workspace 检查
cargo check --workspace

# 只测 v2 backend crates
cargo test -p loom-inference -p loom-memory -p lume-skills -p lume-mcp -p loom-core -p loom-context -p loom-security

# 全量测试（排除有问题的老 crate）
cargo test --workspace --exclude loom-hooks --exclude loom-exec-server --exclude loom-exec

# 构建 lume CLI
cargo build -p lume-cli --release

# 运行
./target/release/lume.exe doctor
./target/release/lume.exe chat --model deepseek-v4-flash
./target/release/lume.exe serve --port 8080
```

## Subagent 使用规范

### 适用场景
- **探索老代码找可复用资产** — 派 Explore agent 搜索 `crates/` 目录，不手动翻找
- **并行审计** — 多个独立模块同时检查时，每模块派一个 agent
- **设计方案** — 复杂实现前派 Plan agent 出方案，主 agent 审核后执行
- **代码审查** — 改动超过 3 个文件时派 review agent 做独立复查

### 不适用
- 单文件小改、已知路径的简单查询 — 直接 Glob/Grep/Read

### 纪律
1. 每次派发前交代清楚：查什么、范围在哪、期望什么产出
2. agent 返回值是参考，关键路径自己读一遍确认
3. 多个独立探索任务并行派发，不串行等待
4. 失败/超时重新派发，不跳过
5.禁止使用内联样式，必须用module.css做样式

## 参考文档

| 文档 | 说明 |
|------|------|
| [API 文档](docs/api.md) | JSON-RPC 2.0 接口（~55 方法 + 9 推送事件） |


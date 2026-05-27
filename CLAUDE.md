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

## 参考文档

| 文档 | 说明 |
|------|------|
| [v2 重建计划](docs/v2-rebuild-plan.md) | Phase 0-4 分阶段执行计划 |
| [Phase 0-3 缺口审计](docs/phase0-3-gap-audit.md) | 编译/逻辑缺口 (23 项, 19 修 4 deferred) |
| [愿景差距分析](docs/v2-vision-gap-analysis.md) | 5 维度逐项差距 + 可复用老代码资产 + 优先级 |
| [全仓库审计报告](docs/audit-report-2026-05-26.md) | 三套代码并存现状 |
| [架构决策 / 已知限制](docs/architecture.md) | 技术决策记录 |
| [记忆系统优化 P0/P1](docs/memory-optimization-plan.md) | 摘要引擎 + 稳定前缀 + KG 增强 |
| [记忆系统优化 P2](docs/p2-memory-optimization-plan.md) | Evidence + 修剪 + 搜索 + 中文修复 |

## 当前进度 (v2)

| 阶段 | 状态 | 说明 |
|------|:--:|------|
| Phase 0 — 基础设施 | ✅ | 10 crate + loom-types + inference + V8 迁移 |
| Phase 1 — Agent 核心 | ✅ | Agent struct + 9 状态机 + AgentPool + Orchestrator + Server 骨架 |
| Phase 2 — 工具 + 子 Agent | ✅ | ToolRegistry + MCP 分发 + spawn_agent + WS + 流式 + 安全 + 防循环 |
| Phase 3 — 记忆 + 技能 | ✅ | KG 四表 + GraphStore + Skills 解析 + LLM 实体提取 + 对话持久 + Persona + EntityExtractor |
| Phase 4 — 前端 + 切换 | ⏳ | frontend monorepo + 页面迁移 + Electron TS + 删除 legacy |

### 2026-05-26~27 新增

| 改动 | 说明 |
|------|------|
| loom-context 接入 agent_loop | ContextAssembler 替代手写消息拼接 |
| InferenceEngine 真实实现 | stub→HTTP 调用 LM Studio/Ollama，实现 CloudClient trait |
| Agent 配置系统 | V9 迁移 + AgentConfigStore + 5 CRUD RPC + 会话绑定 + 工具过滤 |
| KG 读取接入 | MemoryStore.query_kg_context + 每轮自动注入 USER 实体 + 邻近关系 |
| use_skill 工具 | LLM 可调用 use_skill(name) 获取完整 SKILL.md 指令体 |
| Agent tokio task 化 | process_message 内 agent 循环通过 tokio::spawn 运行 |
| 会话历史隔离 | session_histories HashMap 替代单 Vec，按 session_id 隔离 |
| --resume / -c 会话延续 | 启动加载历史 + 完整回显 + 新会话自动生成唯一 ID |
| 会话持久化 | SessionStore → SQLite 同步，lume serve 启动恢复 |
| MCP 增强 | tool call timeout + resources/list/read + resourceTemplates + prompts/list/get + server_health + RPC |
| Plugin 系统 | 兼容 Claude Code/OpenClaw，递归扫描 4 层，TOML+JSON manifest |
| Bridge 外部接入 | lume-bridge crate：ChannelAdapter + Telegram + WeChat(iLink) + BridgeManager + BridgeStore |
| 默认模型 | deepseek-v4-flash (autodetect deepseek provider) |
| MCP resources/prompts | McpPrompt/McpPromptMessage 类型 + list_prompts/get_prompt + resourceTemplates + LLM 工具注册 |
| MCP bug fixes | stdio 连接修复 + SSE \r\n 支持 + stderr drain + reqwest timeout + HTTP body drain |
| LSP 客户端 | lume-lsp crate：LspClient + 40+ 语言 LS 自动检测 + Content-Length 帧协议 + 6 LLM 工具 + 8 RPC |
| LSP bug fixes | file_uri 中文路径 percent-encoding + Content-Length 10MB 上限 + didChange 支持 + timeout + stderr drain |

### 2026-05-27 新增 (记忆系统 + Token 优化)

| 改动 | 说明 |
|------|------|
| **KV Cache 追踪** | PrefixCache + 每轮显示 `kv hit/miss`，CLI 输出 cache 命中状态 |
| **本地端点路由** | InferenceEngine vs AnthropicClient 按 provider 分流，URL 自动补 `/v1` |
| **Lazy Tools** | `request_tools` 元工具：纯对话只加载 1 个工具，LLM 需时才按关键字匹配注入 |
| **Skill 名精简** | 系统提示词只列 skill 名称（`- name`），完整描述通过 `use_skill()` 按需获取 |
| **系统提示词缩短** | 388→190 字符 |
| **对话摘要引擎** | `SummaryEngine`：长对话(≥12条)自动触发 LLM 摘要，增量更新，语言自适应 |
| **稳定前缀上下文** | `ContextAssembler` 重写：稳定前缀 + 动态后缀，最大化 KV cache 复用 |
| **中文 token 估算修正** | `ascii/4 + (non_ascii+1)/2` 替代 `chars/4`，修正 4-8 倍低估 |
| **中文实体提取** | CJK n-gram 滑动窗口提取候选实体名，替代仅英文 whitespace+大写方案 |
| **KG 访问计数** | `touch_node`/`touch_rows`：查询自动更新 `access_count`/`last_accessed` |
| **KG 时间衰减** | `top_interests` 公式加入 `access_count` 权重 + 7 天近因衰减 |
| **KG evidence 接线** | `save_turn` 返回 event_id → `feed_knowledge_graph` 写入 `kg_evidence` |
| **Persona 增强** | 按 `confidence × evidence_count` 排序 + `top_n` 限制 |
| **Web 搜索** | `web_search` (DuckDuckGo) + `web_fetch` (HTML→text) 内置工具，按需加载 |
| **记忆自动修剪** | 启动时清理 >500 实体中 30 天未访问的低置信度实体 |
| **跨会话 KG 搜索** | `lume kg search` / `lume kg stats` CLI + FTS5 前缀自动扩展 |
| **LLM 查询扩展** | `lume kg search --expand "性能优化"` → LLM 多语言关键词扩展 |
| **KG 污染修复** | `query_kg_context` 加 `MIN_CONFIDENCE = 0.5` 过滤低质量实体 |
| **V10 迁移** | `sessions.summary` + `kg_nodes.access_count/last_accessed` |

### 参考文档

| 文档 | 说明 |
|------|------|
| [记忆系统优化 P0/P1](docs/memory-optimization-plan.md) | 摘要引擎 + 稳定前缀 + KG 增强 实施记录 |
| [记忆系统优化 P2](docs/p2-memory-optimization-plan.md) | Evidence + 修剪 + 搜索 + 中文修复 实施记录 |

### 测试状态

新 backend: 34+ tests pass (含 8 summary + 3 graph + 3 extractor) | Clippy: 0 warnings | fmt: clean

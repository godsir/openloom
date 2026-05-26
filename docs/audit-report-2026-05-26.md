# openLoom 代码库审计报告

> 生成日期: 2026-05-26 | 范围: 全仓库 | 目的: 决定重构 vs 重写

---

## 一、仓库真实结构

仓库包含三套互不配套的代码，共存在同一个 workspace 里：

| 层 | 来源 | 文件量 | 状态 |
|---|---|---|---|
| `crates/loom-protocol/` + `loom-utils/` | OpenAI Codex 移植 | 687 .rs 文件 (26 个 crate) | 完整的多 Agent 协议、MCP 客户端/服务端、配置系统。**引擎完全不引用** |
| `crates/engine/` + `memory/` + `skills/` 等 | 自己写的 | ~100 .rs 文件 (10 个 crate) | 实际运行的引擎/服务/记忆管线。**对多 Agent、MCP 一无所知** |
| `web/` + `electron/` | 前端 + 壳 | React 19 + Electron 38 | 多会话 UI、设置面板。**和后端 API 大量不匹配** |

两套 Rust 生态之间只有 2 个 bridge crate（`app-server-client`、`cli`）连接。核心 `loom-protocol` crate 对 `openloom-*` 是零依赖。

**Codex 残留：** `CODEX_` 常量、`.codex-system-skills.marker`、用户文案直接写"Codex 可以读取文件"、`openai-oss-forks` git patches、`F:/codex/codex-rs` 路径硬编码在 Cargo.toml 注释里。

---

## 二、用户愿景 vs 现状

用户需求: 可配置多 Agent、多对话、MCP/LSP 调用、Claude Code/OpenClaw skills 兼容、知识图谱长期记忆。

| 需求 | 状态 | 完成度 |
|------|------|:------:|
| 多 Agent 可配置 | Engine 是单例，无 Agent 实体。loom-protocol 有 AgentPath/AgentRole 协议但引擎没接 | 5% |
| 多对话 | Session CRUD 完整（85 个 JSON-RPC 方法中 20+ 是 session 相关的），但单线程执行 | 75% |
| MCP 调用 | loom-protocol 有完整 MCP 客户端（连接管理、OAuth、stdio/HTTP 传输），引擎零集成 | 15% |
| LSP 调用 | 零代码 | 0% |
| Skills 兼容 | 能读 SKILL.md 的 YAML frontmatter，扫描 ~/.claude/skills 目录，但 invoke() 只返回 markdown 文本不执行 | 30% |
| 知识图谱 | cognitions 表是 `(subject, trait, value)` 键值对，不是图。无实体提取、无关系边、无图查询 | 35% |

---

## 三、具体问题清单

### A. 关键功能不工作

| 问题 | 位置 | 严重度 |
|------|------|:------:|
| Onboarding 流程：4 个 IPC 通道在 preload.js 定义但 main.cjs 无 handler，Promise 会挂起直到超时 | `electron/preload.js:162-175` ←→ `electron/main.cjs` | CRITICAL |
| Desk 文件操作（create/rename/move/delete/search）全部返回 `{ok: true}` 但不执行任何操作 | `dispatch.rs:1465-1474` | HIGH |
| Bridge 消息发送是空壳，有 TODO 注释 | `dispatch.rs:2203` | HIGH |
| `thinking_level.set` 接受参数但不持久化 | `dispatch.rs:2006` | HIGH |
| `chat.replay` 返回 `{ok: true}` 但无实际实现 | `dispatch.rs:1454` | MEDIUM |

### B. 前后端不对齐

| 不对齐 | 详情 |
|--------|------|
| 后端实现但前端不调用 | 26 个 JSON-RPC 方法（agent.status, cache.stats, memory.query, command.list 等） |
| 前端监听但后端不发 | 4 种 WS 通知（models-changed, bridge.status_changed, compaction_end, thinking.start） |
| 后端空壳 | 15+ 个方法返回空值/固定值（memory.stats, cron.*, config.schema, agent.tool_policy.* 等） |

### C. 代码组织问题

| 问题 | 证据 |
|------|------|
| `ToolCall` 重复定义 | `models/lib.rs:398` vs `loom-protocol/tools/tool_call.rs:28` — 同名字段完全不同 |
| 5 种 Manifest 结构 | PluginManifest (2处), SkillManifest, LoomManifest, UniversalManifest |
| 两套 Config 系统 | `openloom_models::AppConfig` vs `loom-protocol::config_types::Settings` — 零类型共享 |
| 两套消息类型 | `models/lib.rs` 里 ChatMessage 和 Message 是两套体系，有 `from_legacy()` 转换方法 |
| `models/lib.rs` | 1460 行单文件，混合了 model/router/engine/jsonrpc/config/util 七不相关子系统 |
| `engine/lib.rs` | 3720 行，20 个模块，涵盖 agent loop/bridge/skills/plugins/vision/computer-use/cron/checkpoint 等 |
| 死代码 | 34 个 `.bak` 文件、45 个 `#[allow(dead_code)]`、4 个 IM bridge 全标注 dead_code、3 个 stub crate |
| 两套 skills | `openloom-skills` (运行时注册表) vs `loom-skills` (Codex 系统 skill 安装) — 互不相关 |

---

## 四、可保留资产

| 资产 | 理由 |
|------|------|
| `crates/inference/` | 5 个 provider (Anthropic/OpenAI/DeepSeek/LM Studio/Ollama) 完整实现，流式+重试+工具调用 |
| `crates/memory/src/store.rs` | SQLite schema 设计成熟，FTS5，migration pipeline (7 个 migration) |
| `crates/server/` | 85 个 JSON-RPC 方法骨架，WebSocket + SSE 协议层稳定 |
| `web/` 前端 | 多会话 UI、设置面板、onboarding 流程，React 19 + Tailwind 4 |
| `loom-protocol/` MCP 实现 | 完整的 MCP 客户端/服务端，连接生命周期管理，OAuth，可作为新架构的基础 |

---

## 五、建议方向

**深度重构（3-4 周）优于完全重写（2-3 月）。**

核心思路：保留推理层/服务层/前端/存储层，用 loom-protocol 的类型系统统一替换 engine 中的重复类型，把 Agent 从 Engine 单例中拆出来，将 MCP 连接管理接入引擎工具分发。

### 优先级排序

1. 统一类型系统 — 以 loom-protocol 为 canonical
2. Agent 实体化 — 拆出独立 Agent struct（id、system prompt、工具集、状态）
3. MCP 集成 — McpConnectionManager 接入引擎工具分发
4. 知识图谱 — 将 cognitions 从键值对升级为 nodes + edges 图结构
5. ExternalSkill 改造 — invoke() 从返回文本改为真实工具执行
6. LLM 记忆提取 — 替换中文正则提取器
7. 前后端对齐 — 清理 IPC 缺口，移除空壳 API
8. LSP — 最后，MCP 和 Agent 基础设施稳定后

### 清理项

- 删除 34 个 .bak 文件
- 删除 4 个 IM bridge dead code
- 合并 5 种 Manifest/Hub 类型
- 拆分 models/lib.rs（1460 行单文件）
- 统一两套 skills 概念
- 重写 Approvals 文案（还在说"Codex 可以..."）

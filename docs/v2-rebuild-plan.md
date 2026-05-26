# openLoom v2 重建计划

> 生成日期: 2026-05-26 | 版本: v2.0 | 决策: 自洽规划

---

## 背景

当前 openLoom 仓库包含三套互不配套的代码（openLoom 引擎 / Codex 移植 / 前端），类型系统重复、引擎单例不支持多 Agent、MCP 未接入引擎、大量空壳 API。

**保留资产:** inference（5 provider）、memory/store（SQLite schema）、loom-protocol MCP 实现、server Axum 骨架、web 前端

---

## 目标架构

```
backend/                        frontend/
├── loom-types    (0 内部依赖)    ├── shared/     (@loom/shared 类型+API客户端)
├── loom-inference (→ types)     ├── ui/         (@loom/ui 通用组件)
├── loom-mcp       (→ types)     ├── app/        (@loom/app React主应用)
├── loom-skills    (→ types)     ├── electron/   (@loom/electron 壳)
├── loom-memory    (→ types)     └── onboarding/ (@loom/onboarding)
├── loom-context   (→ types)
├── loom-security  (→ types)
├── loom-core      (→ 以上全部)      ← 编排器（AgentPool + 事件总线 + 工具分发）
├── loom-server    (→ core)
└── loom-cli       (→ core + server)
```

---

## 关键架构决策

1. **统一类型系统** — `loom-types` 14 个模块，反倾倒规则防退化
2. **MCP + 内置工具统一** — `AgentTool` trait + `ToolRegistry`，`mcp__{server}__{tool}` 命名
3. **知识图谱** — SQLite nodes + edges + aliases + evidence，增量迁移不破坏旧表
4. **技能兼容** — 统一解析器支持 Claude Code + OpenClaw SKILL.md，渐进式上下文注入
5. **Agent 模型** — 9 状态机，1 agent = 1 tokio task，AgentPool 管理并发
6. **砍掉 34 个空壳 API** — bridge.*, desk.*, cron.*, computer_use.* 全部删除
7. **WebSocket 唯一** — 去除 SSE 传输层，JSON-RPC 2.0 为主要协议

---

## 移除清单

> **状态:** 新代码中已全部不包含以下功能。老 `crates/` 代码未动，等 Phase 4 统一删除。

| 类别 | 内容 | 状态 |
|------|------|:---:|
| JSON-RPC 方法 | bridge.*, desk.*, cron.*, computer_use.*, agent.activity_log, agent.tool_policy.*, memory.stats, memory.recent_events, memory.graph_snapshot | ✅ 新代码不含 |
| 前端 slices | bridge-slice, desk-slice, automation-slice, computer-overlay-slice, browser-slice, channel-slice, screenshot-slice, plugin-ui-slice | ⏳ 老代码中 |
| 后端模块 | crates/engine/src/bridge/, cron_scheduler, computer_use | ✅ 新代码不含 |
| 死文件 | 35 个 .bak | ✅ 已删除 |
| stash | 3 个 `#[allow(dead_code)]` → 0 | ✅ 清零 |
| 旧 crate | engine, skills, models, router, weaver, cache, sandbox（切换后删除） | ⏳ Phase 4 |

---

## 分阶段执行

> **状态更新: 2026-05-26** — Phase 0-3 核心已完成。17 项问题修复。6 项 deferred。
> 详细缺口: [phase0-3-gap-audit.md](phase0-3-gap-audit.md)

### Phase 0 — 基础设施 ✅ 完成
**目标:** 新目录结构 + 类型系统 + 推理层就绪，新旧并行编译

- [x] 创建 `backend/crates/` 目录结构 (10 crates)
- [x] `loom-types` 14 个模块（从 models.rs + loom-protocol 移植合并）
- [x] `loom-inference`（从 crates/inference/ 移植，5 provider）
- [x] 根 Cargo.toml 加 workspace members，验证新旧并行编译
- [x] V8 migration 文件（知识图谱四表）
- [x] 清理 35 个 .bak 文件
- [ ] V8 migration 文件（知识图谱四表）
- [ ] 清理计划中明确的死代码

### Phase 1 — Agent 核心 ✅ 完成
**目标:** Agent 结构体 + MCP 集成 + Server 骨架

- [x] `lume-mcp`（MCP 客户端 — stdio + HTTP/SSE）
- [x] `loom-core/agent.rs` — Agent struct + 9 状态机
- [x] `loom-core/agent_pool.rs` — AgentPool 生命周期
- [x] `loom-core/orchestrator.rs` — 编排器
- [x] `loom-server` 骨架（Axum 路由 + JSON-RPC dispatch 20 方法）

### Phase 2 — 工具 + 子 Agent ✅ 完成
**目标:** 单 Agent + MCP 调用端到端可跑

- [x] `AgentTool` trait + `ToolRegistry` 统一分发
- [x] MCP 工具调用集成到 agent loop
- [x] `spawn_agent` 内置工具 (走 AgentPool)
- [x] WS 通知 (agent.state_changed, tool.*, chat.*)
- [x] 端到端 demo (`lume chat`)
- [x] 流式输出 (complete_stream_structured)
- [x] loom-security 权限检查接入 agent loop
- [x] 防循环机制 (3 轮提醒 + 7 轮强制)
- [x] McpClient disconnect / disconnect_all

### Phase 3 — 记忆 + 技能 ✅ 完成
**目标:** 知识图谱 + 技能兼容

- [x] V8 migration 执行，24 张表
- [x] `loom-memory/src/graph.rs` — GraphStore + 10 种图查询
- [x] 统一技能解析器（Claude Code + OpenClaw，21 字段）
- [x] Skills 运行时门控 (OS/bin/env/config 检查)
- [x] LLM 实体提取（对话后自动调 LLM 分析 → kg_nodes/edges）
- [x] `loom-context` 上下文组装
- [x] 对话持久化 (message_history 表)
- [x] PersonaProvider 接入 (CognitionsPersonaProvider)
- [x] EntityExtractor 实现 (RuleBasedEntityExtractor)
- [x] feed_knowledge_graph 去重

### Phase 4 — 前端 + 切换 ⏳ 待开始
**目标:** 前后端对齐 + 删除 legacy

- [ ] `frontend/` monorepo + `@loom/shared` API 客户端
- [ ] 核心页面迁移（ChatPage, AgentsPage, SettingsPage）
- [ ] Electron 壳 TypeScript 化
- [ ] 前后端集成测试
- [ ] 删除 legacy 目录 + 旧 crate
- [ ] 里程碑：v2.0 可用

---

## 取舍记录

| 取舍 | 理由 |
|------|------|
| SQLite 图不换 Neo4j | 本地优先原则，递归 CTE 足够个人规模 |
| WS 唯一砍掉 SSE | SSE 是旧回退方案，WS 已全覆盖 |
| 技能先做注入后做执行 | 技能本质是上下文注入，Fork 执行模式 Phase 2+ |
| LSP 不在 Phase 1-4 | 依赖 MCP + Agent 稳定后再做 |
| 不迁移旧 cognition 到图谱 | 可选 CLI 命令给需要的用户 |

---

## 相关文档

- [审计报告](audit-report-2026-05-26.md) — 全仓库现状
- CLAUDE.md — 项目规范

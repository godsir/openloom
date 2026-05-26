# openLoom v2 愿景差距分析

> 生成: 2026-05-26 | 最后更新: 2026-05-27 | 剩余: LSP(deferred) + Phase 4 + Phase 4 前端

## 核心开发原则

1. **所有新功能在 `backend/crates/` 实现**，不修改老 `crates/` / `core/` / `lib/`
2. **复制再修改** — 老 `crates/loom-protocol/` 有大量可复用代码（MCP 客户端/服务端、插件框架、Skills 执行、LSP stubs、Bridge 适配器等），先复制到 `backend/crates/` 再适配新类型系统，不从头造轮子
3. 老代码只做参考和并行编译，Phase 4 统一删除

---

## 一、用户愿景

一个**可配置多 Agent、多对话**的工具，可调用 **MCP / LSP** 服务器/工具做搜集计算，兼容 **Claude Code / OpenClaw** 的 Skills/Plugins 系统，拥有**长期记忆知识图谱**。

---

## 二、整体进度

| 阶段 | 状态 | 说明 |
|------|:--:|------|
| Phase 0 — 基础设施 | ✅ | 10 crate 目录 + loom-types 14 模块 + inference + V8 迁移 |
| Phase 1 — Agent 核心 | ✅ | Agent struct + 9 状态机 + AgentPool + Orchestrator + Server 骨架 |
| Phase 2 — 工具 + 子 Agent | ✅ | ToolRegistry + MCP 工具分发 + spawn_agent + WS + 流式 + 安全 + 防循环 |
| Phase 3 — 记忆 + 技能 | ✅ | KG 四表 + GraphStore + Skills 解析 + LLM 实体提取 + 对话持久 + Persona + EntityExtractor |
| Phase 4 — 前端 + 切换 | ⏳ | frontend monorepo + 页面迁移 + Electron TS 化 + 删除 legacy + v2.0 |

### 本次会话新增

| 日期 | 改动 | 说明 |
|------|------|------|
| 2026-05-26 | loom-context 接入 | #18 deferred→done，ContextAssembler 接入 agent_loop |
| 2026-05-26 | InferenceEngine 实现 | #19 deferred→done，stub→真实 HTTP LLM 调用 |
| 2026-05-26 | Agent 配置系统 | V9 迁移 + AgentConfigStore + 7 新 RPC + 会话绑定 + 工具过滤 |

---

## 三、逐项差距分析

### 1. 可配置多 Agent

#### 已有
| 能力 | 位置 |
|------|------|
| AgentConfig 16 字段 | `loom-types/src/config/mod.rs:173` |
| 9 状态机 + AgentPool | `loom-core/src/agent.rs` + `agent_pool.rs` |
| agent.config.* CRUD（5 个 RPC） | `loom-server/src/dispatch.rs` |
| 会话绑定 agent（session.bind_agent） | dispatch.rs |
| chat.send 自动解析绑定配置 | dispatch.rs |
| 子 agent（spawn_agent tool） | `loom-core/src/tool_registry.rs` |
| 工具过滤（allowed/disallowed_tools） | `tool_registry.rs` + `agent_loop.rs` |
| AgentConfig SQLite 持久化 | `loom-memory/src/store.rs` (AgentConfigStore) |
| 启动加载 agent configs | `lume-cli/src/main.rs` |

#### 缺失
| 缺口 | 严重度 | 说明 |
|------|:--:|------|
| Agent 不并发运行 | **P1** | `process_message_with_config` 创建→跑→删除，AgentPool 的 tokio task 未启用，`handle` 永远是 None |
| 无 per-agent 模型切换 | **P1** | Orchestrator 只有一个 CloudClient，所有 agent 共享 |
| 不能同时跑多个 agent | **P1** | 多会话并行不可用 |
| 无 agent 暂停/恢复 | **P2** | 状态机定义存在，但运行时未实现 |

**结论：配置层完整，运行时层 Agent 没有真正作为独立 task 存活。**

---

### 2. 多对话

#### 已有
| 能力 | 位置 |
|------|------|
| SessionStore 6 个 RPC | `loom-server/src/dispatch.rs` |
| session.bind_agent | dispatch.rs（2026-05-26 新增） |
| message_history SQLite 表 | V3 migration |
| chat.send 带 session_id | dispatch.rs |

#### 缺失
| 缺口 | 严重度 | 说明 |
|------|:--:|------|
| 会话历史不隔离 | **P1** | `orchestrator.history` 是单 Vec，所有会话共享同一段历史 |
| SessionStore 纯内存 | **P2** | 重启丢失所有会话，虽有 SQLite 表但未接入 |
| 不能并发多会话 | **P1** | 同"Agent 不并发" |

**结论：API 层完整，但底层历史不隔离 + 不持久。**

---

### 3. MCP / LSP 调用

#### 已有
| 能力 | 位置 |
|------|------|
| MCP 客户端 stdio + HTTP/SSE | `lume-mcp/src/lib.rs` |
| McpAgentTool 统一分发 | `loom-core/src/orchestrator.rs` |
| mcp.list_servers / mcp.list_tools | dispatch.rs |
| MCP 工具注册到 ToolRegistry | orchestrator.rs `connect_mcp_server()` |
| mcp.json 配置文件支持 | `lume-cli/src/mcp_config.rs` |

#### 缺失
| 缺口 | 严重度 | 说明 |
|------|:--:|------|
| **LSP 零代码** | **P1** | 没有 LSP 客户端、协议实现、工具封装 |
| MCP 连接管理简陋 | **P3** | 无重连、无心跳、无健康检查 |
| 不支持 MCP resources/prompts | **P3** | 当前仅 tools |

**结论：MCP 工具级可用但不健壮；LSP 完全空白。**

---

### 4. Skills / Plugins 兼容

#### 已有
| 能力 | 位置 |
|------|------|
| SKILL.md YAML 解析 21 字段 | `lume-skills/src/lib.rs` |
| Claude Code + OpenClaw 格式兼容 | lume-skills |
| ~/.claude/skills 等 3 目录扫描 | `lume-cli/src/main.rs` |
| 运行时门控（OS/bin/env/config） | lume-skills（2026-05-26 修复） |
| Skills 上下文注入到 system prompt | orchestrator `build_system_prompt()` |

#### 缺失
| 缺口 | 严重度 | 说明 |
|------|:--:|------|
| **Skills 不执行** | **P0** | `invoke()` 只返回 markdown 文本，不真正跑 skill |
| Skills 不暴露为 LLM tools | **P1** | LLM 只能看到文本描述，不能调用 |
| **无 Plugin 系统** | **P2** | 老 loom-protocol 有完整插件框架（安装/市场/远程），引擎零引用 |
| 无 fork 执行模式 | **P2** | Skills 计划中有 fork 模式但未实现 |

**结论：Skills 能解析能注入，但核心"执行"能力缺失。这是差异化最大的未完成项。**

---

### 5. 长期记忆 / 知识图谱

#### 已有
| 能力 | 位置 |
|------|------|
| kg_nodes/edges/aliases/evidence 四表 | V8 migration |
| GraphStore 10 种图查询 | `loom-memory/src/graph.rs` |
| LLM 对话后实体提取 | `loom-core/src/orchestrator.rs` `llm_extract_entities()` |
| RuleBasedEntityExtractor | `loom-memory/src/extractor.rs`（2026-05-26 修复） |
| Cognitions + FTS5 全文搜索 | `loom-memory/src/store.rs` |
| PersonaProvider | `loom-memory/src/persona.rs`（2026-05-26 接入） |

#### 缺失
| 缺口 | 严重度 | 说明 |
|------|:--:|------|
| **KG 只写不读** | **P0** | agent loop 不查询图谱，LLM 不知道图谱存在 |
| ContextAssembler 不注入 KG | **P0** | 组装上下文时不查图谱 |
| 无图谱查询工具 | **P0** | GraphStore 有 10 种查询，但 LLM 无法调用 |
| Persona 启动加载后不更新 | **P2** | 对话后 persona 应动态演化 |

**结论：图谱存取完整，但"记忆回路"断裂——存了用不上。**

---

## 四、其他计划项（来自 superpowers/docs 设计文档）

### OutputSink 统一（2026-05-25 方案）
- **范围：** 老 `crates/engine/src/agent_loop.rs`，统一 TUI/Electron 输出路径
- **状态：** 方案存在，编写给老代码的。**新 backend 的 agent_loop 已有自己的流式路径，此方案不需要在新 backend 重做**
- **关联：** 新 backend 缺少 Electron 路径的 OutputSink 等价物，目前只有 `process_message_streaming`（mpsc channel）

### Bridge 系统（2026-05-25 设计文档）
- **范围：** Telegram/飞书/微信/QQ 外部平台接入，ChannelAdapter trait，BridgeManager
- **状态：** 完整设计文档，V7 migration 已执行（bridge_sessions/bridge_messages/bridge_known_users 表已建），但 **Adapter 代码零实现**
- **优先级：** P2（扩展性需求，非核心路径）

---

## 五、优先级总览

| 优先级 | 缺口 | 类别 | 影响 |
|:--:|------|------|------|
| **P0** | Skills 执行 | Skills | ✅ use_skill tool + skill bodies |
| **P0** | KG 只写不读 | 记忆 | ✅ query_kg_context + 每轮自动注入 |
| **P1** | Agent 不并发运行 | 多 Agent | ✅ tokio::spawn agent task |
| **P1** | 会话历史不隔离 | 多对话 | ✅ session_histories HashMap |
| **P2** | 会话不持久 | 多对话 | ✅ SQLite 同步 + 启动恢复 + -c 延续 |
| **P2** | 无 Plugin 系统 | Plugins | ✅ Claude Code/OpenClaw 兼容 + 递归扫描 |
| **P3** | MCP 连接管理 | MCP | ✅ timeout + resources + health check |
| **P3** | agent.config RPC | Agent | ✅ 5 CRUD RPC + 会话绑定 |
| **P2** | Bridge 系统 | 外部接入 | ✅ lume-bridge crate + Telegram adapter + BridgeManager |
| **P1** | LSP 空白 | LSP | 📋 deferred |
| **P3** | MCP resources/prompts | MCP | 协议不完整 |

---

## 六、可复用老代码资产（复制→适配，不修改原文件）

### Skills 执行 → `backend/crates/lume-skills/`

| 源文件 | 行数 | 内容 |
|------|:--:|------|
| `crates/skills/src/lib.rs` | 315 | `Skill` trait + `SkillRegistry`（register/find_by_trigger/invoke/invoke_tracked） |
| `crates/skills/src/builtins/` | 1,800 | 14 个内置 skill（shell/file_read/file_write/file_search/content_search/web_browser 等） |
| `crates/skills/src/plugin_loader.rs` | 285 | 从 .toml 插件清单加载 skills |
| `crates/loom-protocol/core-skills/src/model.rs` | 213 | SkillMetadata/SkillPolicy/SkillInterface（更丰富的 Codex 模型） |

### MCP 增强 → `backend/crates/lume-mcp/`

| 源文件 | 行数 | 内容 |
|------|:--:|------|
| `crates/loom-protocol/mcp/src/connection_manager.rs` | 785 | 连接聚合、工具索引、启动状态 |
| `crates/loom-protocol/rmcp-client/src/rmcp_client.rs` | 1,094 | 完整客户端生命周期（connect/initialize/list_tools/call_tool/list_resources/notifications/shutdown） |
| `crates/loom-protocol/mcp/src/tools.rs` | 375 | 工具规范化/过滤 |
| `crates/loom-protocol/rmcp-client/src/stdio_server_launcher.rs` | 652 | stdio 子进程启动 + 传输 |
| `crates/loom-protocol/rmcp-client/src/oauth.rs` | 913 | OAuth 2.0 流程 |

### Plugin 系统 → `backend/crates/` 新建

| 源文件 | 行数 | 内容 |
|------|:--:|------|
| `crates/loom-protocol/core-plugins/src/marketplace_add/install.rs` | 137 | 插件下载/验证/解压 |
| `crates/loom-protocol/core-plugins/src/marketplace_add/metadata.rs` | 315 | 市场元数据解析 |
| `crates/loom-protocol/core-plugins/src/marketplace_add/source.rs` | 392 | 插件源解析（git/URL/local） |
| `crates/loom-protocol/core-plugins/src/marketplace_upgrade/` | ~450 | 升级激活 + git 工作流 |

### Bridge 外部接入 → `backend/crates/` 新建

| 源文件 | 行数 | 内容 |
|------|:--:|------|
| `crates/engine/src/bridge/adapter.rs` | 31 | `ChannelAdapter` trait（5 个方法） |
| `crates/engine/src/bridge/types.rs` | 247 | Platform/BridgeMessage/MessageContent 类型 |
| `crates/engine/src/bridge/manager.rs` | 232 | BridgeManager 生命周期管理 |
| `crates/engine/src/bridge/security.rs` | 189 | 频率限制/去重/循环检测 |
| `crates/engine/src/bridge/telegram.rs` | 303 | Telegram Adapter（long polling 参考实现） |
| `crates/engine/src/bridge/feishu.rs` | 212 | 飞书 Adapter |
| `crates/engine/src/bridge/wechat.rs` | 190 | 微信 Adapter |
| `crates/engine/src/bridge/qq.rs` | 194 | QQ Adapter |

### KG 查询模式 → `backend/crates/loom-memory/`

| 源文件 | 行数 | 内容 |
|------|:--:|------|
| `crates/memory/src/persona.rs` | 335 | `summarize()` 多因子排序算法（confidence × ln(evidence) × recency_decay × source_priority） |

### LSP

**零可复用代码。** 老 `crates/` 中无任何 LSP 实现。纯绿场。

### 总计

| 区域 | 可复用行数 | 难度 |
|------|:--:|:--:|
| Skills 执行 | ~3,800 | 低 — trait 干净，类型耦合少 |
| MCP 增强 | ~3,800 | 中 — 依赖 RMCP，需要适配 loom-types |
| Bridge 系统 | ~1,600 | 中 — 需适配新存储层 |
| Plugin 系统 | ~2,500 | 高 — 重度依赖 Codex 内部类型 |
| KG 查询 | ~100 (算法) | 低 — 仅算法逻辑 |
| **合计** | **~11,800** | |

---

## 七、建议执行顺序

```
P0 先导: Skills 执行 + KG 读取接入  (让核心差异化立起来)
  ↓
P1: Agent 并发 + 会话隔离            (让多 Agent 多对话真正可用)
  ↓
P1: LSP 客户端 + 工具封装            (代码智能)
  ↓
P2: 会话持久化 + Plugin 框架         (完善体验)
  ↓
P2: Bridge Adapter 实现              (按需逐个做)
  ↓
Phase 4: 前端 + 切换 + 删除 legacy   (收尾)
```

---

## 七、相关文档

| 文档 | 说明 |
|------|------|
| [v2-rebuild-plan.md](v2-rebuild-plan.md) | Phase 0-4 分阶段执行计划 |
| [phase0-3-gap-audit.md](phase0-3-gap-audit.md) | 编译/逻辑缺口审计（23 项，19 修 4 deferred） |
| [audit-report-2026-05-26.md](audit-report-2026-05-26.md) | 全仓库三套代码并存审计 |
| [superpowers/plans/2026-05-25-agent-loop-outputsink-unification.md](superpowers/plans/2026-05-25-agent-loop-outputsink-unification.md) | OutputSink 方案（老代码） |
| [superpowers/specs/2026-05-25-bridge-system-design.md](superpowers/specs/2026-05-25-bridge-system-design.md) | Bridge 外部平台接入设计 |

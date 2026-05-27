# openLoom v2 记忆系统优化 — 实施记录

> 日期: 2026-05-27 | 状态: 已完成

---

## 1. 已实现

### 1.1 对话摘要引擎 (SummaryEngine)

**文件**: `backend/crates/loom-memory/src/summary.rs`

- LLM 驱动的对话摘要，支持增量更新（已有摘要 + 新消息 → 更新摘要）
- 语言自适应（"Match the language of the conversation"）
- 单元测试覆盖：初始/增量 prompt、触发条件、空摘要处理

**触发机制** (在 `process_message_streaming` 中自动执行):
```
history.len() >= 12 && (history.len() - last_summary_at) >= 6
```
- 约 6 轮对话后首次触发，之后每 3 轮增量更新
- 摘要缓存到 `sessions.summary`，触发间隔内复用缓存不调 LLM
- 短对话（< 12 条消息）完全不触发

### 1.2 稳定前缀上下文 (ContextAssembler 重写)

**文件**: `backend/crates/loom-context/src/lib.rs`

```
┌─ 稳定前缀 (单条 system message) ─────────┐
│ [base instructions]                       │
│ [User Profile]           ← persona       │
│ [Conversation Summary]   ← summary       │  ← 跨轮不变 → KV cache 复用
│ [Knowledge Graph]        ← kg_context    │
│ [Available Tools]        ← tool_catalog  │
├─ 动态后缀 (多条独立 message) ─────────────┤
│ [user] 最近消息1                          │
│ [assistant] 最近消息2                     │  ← 每轮变化
│ ...                                       │
└──────────────────────────────────────────┘
```

- `AssembleOptions` 结构体替代旧的手写参数
- 中文 token 估算修正：`ascii/4 + non_ascii.max(1)/2`（旧版 `chars/4` 对中文低估 4-8 倍）

### 1.3 KG 访问计数与时间衰减

**文件**: `backend/crates/loom-memory/src/graph.rs`

- 新增 `touch_node()` / `touch_rows()` — `search_entities`/`neighbors`/`walk`/`resolve_node` 查询后自动更新 `access_count`/`last_accessed`
- `top_interests` 评分公式新增访问频次和近因衰减因子
- V10 迁移新增 `kg_nodes.access_count` / `kg_nodes.last_accessed` 列

### 1.4 KG Evidence 接线

**文件**: `backend/crates/loom-memory/src/graph.rs`

- 新增 `link_evidence_node()` / `link_evidence_edge()` / `latest_event_id()`
- `feed_knowledge_graph()` 中 upsert 节点/边后写入 evidence 记录

### 1.5 Persona 增强

**文件**: `backend/crates/loom-memory/src/persona.rs`

- 按 `confidence × evidence_count` 降序排列
- 新增 `with_top_n()` 限制输出条目数

### 1.6 数据库迁移 V10

**文件**: `migrations/V10__memory_optimizations.sql`

```sql
ALTER TABLE sessions ADD COLUMN summary TEXT DEFAULT '';
ALTER TABLE sessions ADD COLUMN summary_at_count INTEGER DEFAULT 0;
ALTER TABLE kg_nodes ADD COLUMN access_count INTEGER DEFAULT 0;
ALTER TABLE kg_nodes ADD COLUMN last_accessed INTEGER;
```

---

## 2. 架构决策

| # | 决策 | 理由 |
|:--:|------|------|
| 1 | 摘要用本地 LLM（同步调用） | 已有 CloudClient，无需额外依赖 |
| 2 | 摘要存在 sessions 表而非新表 | 与 session 生命周期一致，简单 |
| 3 | 摘要增量更新 | 避免每次全量重摘要，省 token |
| 4 | 稳定前缀放 KG 上下文 | KG 变化小，不破坏 cache 稳定性 |
| 5 | 中文 token 估算 ascii/4 + non_ascii/2 | 比 chars/4 准确 4-8 倍 |
| 6 | access_count 用 LN 加权 | 对数衰减避免热门实体主导 |

---

## 3. 改动文件汇总

| 文件 | 操作 |
|------|:--:|
| `migrations/V10__memory_optimizations.sql` | 新增 |
| `backend/crates/loom-memory/src/summary.rs` | 新增 |
| `backend/crates/loom-memory/src/lib.rs` | 修改 |
| `backend/crates/loom-memory/src/graph.rs` | 修改 |
| `backend/crates/loom-memory/src/persona.rs` | 修改 |
| `backend/crates/loom-context/src/lib.rs` | 重写 |
| `backend/crates/loom-core/src/orchestrator.rs` | 修改 |
| `backend/crates/loom-core/src/agent_loop.rs` | 修改 |
| `backend/crates/loom-core/src/tool_registry.rs` | 修改 |
| `backend/crates/lume-cli/src/memory.rs` | 修改 |

---

## 4. 后续 (P2)

- 向量语义搜索（需 embedding 端点 + sqlite-vec）
- 记忆自动修剪（定期清理低访问实体）
- 跨会话知识搜索 API
- KG evidence 接入真实 event ID（当前用占位符 0）

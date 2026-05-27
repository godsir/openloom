# openLoom v2 记忆系统 P2 优化计划

> 日期: 2026-05-27 | 状态: 实施中

## 进度

| # | 项目 | 状态 |
|:--:|------|:--:|
| P2-1 | KG evidence 接入真实 event ID | ✅ 已完成 |
| P2-2 | 记忆自动修剪 | ✅ 已完成 |
| P2-3 | 跨会话知识搜索 | ✅ 已完成 |
| P2-4 | 中文实体提取修复 | ✅ 已完成 |
| P2-5 | LLM 查询扩展 | ✅ 已完成 |
| P2-6 | FTS5 查询增强 (前缀匹配) | ✅ 已完成 |
| — | KG 跨会话污染修复 | ✅ 已完成 |
| — | `neighbors` 起始节点 touch | ✅ 已完成 |

> ~~向量语义搜索~~ 已砍。P2-4/5/6 替代，总投入 ~120 行 vs 原计划 200 行，解决实际中文问题。

---

## P2-4: 中文实体提取修复

### 问题

`orchestrator.rs` 中实体提取逻辑：
```rust
for w in user_message.split_whitespace() {  // 中文无空格 → 整句一个 token
    if w.len() > 3 && w.chars().next().is_some_and(|c| c.is_uppercase())  // 中文无大写
```

**中文消息"我聊过的关于性能优化的内容"提取到 0 个实体，整轮 KG 上下文空白。**

### 方案

用 LLM 轻量提取替代规则。在 `process_message_streaming` 中，调用 LLM 做一次小型实体提取（不阻塞主流程，跟摘要调用共用 client）：

```
Prompt: "Extract up to 3 key entity names from this message.
        Return only the entity names, one per line. No explanation."
```

延迟 ~0.3-0.5s，跟摘要检查一起做。

### 改动

| 文件 | 改动 |
|------|------|
| `loom-memory/src/extractor.rs` | 新增 `QUICK_EXTRACTION_PROMPT` 常量 + `parse_quick_extraction()` |
| `loom-core/src/orchestrator.rs` | 在 KG 上下文注入前，先调 LLM 提取实体名 |

---

## P2-5: LLM 查询扩展

### 问题

用户搜"性能优化"，KG 存的是英文 "Performance optimization"，FTS5 匹配不上。

### 方案

`search_knowledge()` 增加 LLM 扩展模式。用户查询先经 LLM 扩展为多语言关键词，再 FTS5 搜索：

```
Prompt: "Expand this search query into space-separated keywords in
        English and Chinese: 性能优化"
Response: "performance optimization speed latency profiling 性能 优化 加速"
```

成本 ~100 tokens，延迟 ~0.3s，纯用户触发（`lume kg search`），不影响对话性能。

### 改动

| 文件 | 改动 |
|------|------|
| `lume-cli/src/memory.rs` | 新增 `search_knowledge_expanded()` |
| `loom-core/src/orchestrator.rs` | MemoryStore trait 新增方法 |
| `lume-cli/src/main.rs` | `lume kg search --expand` flag |

---

## P2-6: FTS5 中文分词 + 列权重

### 问题

FTS5 默认逐字拆中文 → "性能优化" 匹配到无关的 "化学性能"。

### 方案

**分词**: 引入 `jieba-rs`（纯 Rust，零 C 依赖），在写入 KG 实体时对中文描述做分词，存入 FTS5 content 列。

**列权重**: FTS5 建表时设定 `name` 列权重高于 `description`：
```sql
CREATE VIRTUAL TABLE kg_nodes_fts USING fts5(name, description, tokenize='unicode61');
-- 查询时: SELECT ... WHERE kg_nodes_fts MATCH ?1 ORDER BY rank
-- rank 已按列权重加权
```

### 改动

| 文件 | 改动 |
|------|------|
| `migrations/V8__add_knowledge_graph.sql` | 无需改动（FTS5 列权重通过查询控制） |
| `loom-memory/src/graph.rs` | `search_entities` 查询优化 + 前缀/布尔支持 |
| `loom-memory/Cargo.toml` | 添加 `jieba-rs` 依赖 |
| `loom-memory/src/extractor.rs` | `RuleBasedEntityExtractor` 中对中文做 jieba 分词 |

---

## 关键决策

| # | 决策 | 理由 |
|:--:|------|------|
| 1 | 砍掉向量搜索 | 个人 KG 规模太小（<1000 实体），FTS5 召回率已够，真正瓶颈是中文 |
| 2 | 实体提取用 LLM 而非规则 | 规则无法处理中文，LLM 0.3s 延迟可接受 |
| 3 | 查询扩展只用在 CLI 搜索 | 对话中 KG 自动注入不需要扩展（实体名已一致） |
| 4 | jieba-rs 而非其他分词器 | 纯 Rust，最成熟的中文分词库 |
| 5 | FTS5 权重通过查询优化 | 不重建 FTS5 表，向后兼容 |

## 文件变更汇总

| 文件 | 操作 | Step |
|------|:--:|:--:|
| `loom-memory/src/extractor.rs` | 修改 | P2-4, P2-6 |
| `loom-core/src/orchestrator.rs` | 修改 | P2-4 |
| `lume-cli/src/memory.rs` | 修改 | P2-5 |
| `lume-cli/src/main.rs` | 修改 | P2-5 |
| `loom-memory/src/graph.rs` | 修改 | P2-6 |
| `loom-memory/Cargo.toml` | 修改 | P2-6 |

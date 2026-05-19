# Phase 2 Milestone B: Agent Loop + Persona + Message History — 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 实现被动 ReAct Agent Loop、CognitionsPersonaProvider 真实画像、消息历史持久化

**Architecture:** 基础类型 → 数据层 → 业务层 → 集成。先扩展 models 类型和 trait，再写 MessageStore + CognitionsPersonaProvider，然后重构 Weaver 签名，最后在 Engine 中集成 Agent Loop + persona watcher + message history + mid-turn protection

**Tech Stack:** Rust 2024, tokio, rusqlite, async-trait, serde_json, chrono

**已知可接受简化:**
- `save_messages`/`get_working_memory` 在 async 上下文中同步打开 SQLite（本地 DB <1ms，不阻塞）
- Persona watcher 监听所有 EngineEvent 而非仅 CognitionUpdated（效果等价，cognition 变更必定产生事件）
- `CognitionStore` 未加 `insert_inferred()` 方法 — inferred 认知暂由 Phase 3 的 LLM 提取管线处理
- Persona 缓存竞态：invalidate 在 summarize() DB 查询期间触发会导致旧值重缓存（极低概率，Phase 3 用 generation counter 修复）

---

## 文件结构

```
F:/openLoom/
├── migrations/
│   └── V3__add_message_history.sql          ← [Create]
├── crates/
│   ├── models/src/lib.rs                    ← [Modify]
│   ├── memory/
│   │   ├── Cargo.toml                       ← [Modify] +async-trait, +tokio(dev)
│   │   ├── src/persona.rs                   ← [Create]
│   │   ├── src/store.rs                     ← [Modify] +MessageStore
│   │   └── src/lib.rs                       ← [Modify] +pub mod persona
│   ├── weaver/src/lib.rs                    ← [Modify]
│   ├── engine/src/lib.rs                    ← [Modify]
│   └── server/src/dispatch.rs              ← [Modify]
```

---

### Task 1: Models 基础类型 + Migration

**Files:**
- Modify: `F:/openLoom/crates/models/src/lib.rs`
- Create: `F:/openLoom/migrations/V3__add_message_history.sql`

- [ ] **Step 1: 在 models/lib.rs 中新增类型和 trait 变更**

`F:/openLoom/crates/models/src/lib.rs` — 确认文件顶部已有 `use chrono::{DateTime, Utc};`

1. PersonaProvider trait 加 `invalidate()` 方法（约第65行）：

```rust
#[async_trait::async_trait]
pub trait PersonaProvider: Send + Sync {
    async fn summarize(&self) -> anyhow::Result<String>;
    fn invalidate(&self);
}
```

2. NoopPersonaProvider 加空实现（约第71行）：

```rust
#[async_trait::async_trait]
impl PersonaProvider for NoopPersonaProvider {
    async fn summarize(&self) -> anyhow::Result<String> {
        Ok(String::new())
    }
    fn invalidate(&self) {}
}
```

3. ChatMessage 加 timestamp 字段，用 `serde(default)` 保持向后兼容（约第125行）：

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    #[serde(default = "Utc::now")]
    pub timestamp: DateTime<Utc>,
}
```

关键：使用 `#[serde(default = "Utc::now")]` 而非 `chrono::Utc::now` 路径 — serde 要求函数路径可调用。

4. 新增 ToolCall 类型（加在 ChatResponse 附近）：

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub tool: String,
    pub params: serde_json::Value,
}
```

5. 新增 AgentState enum（加在 HealthStatus 附近）：

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AgentState {
    Idle,
    Thinking,
    Acting,
}
```

- [ ] **Step 2: 创建 V3 migration**

`F:/openLoom/migrations/V3__add_message_history.sql`：

```sql
ALTER TABLE cognitions ADD COLUMN source TEXT NOT NULL DEFAULT 'observed';

CREATE TABLE IF NOT EXISTS message_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL,
    seq INTEGER NOT NULL,
    role TEXT NOT NULL,
    content TEXT NOT NULL,
    timestamp TEXT NOT NULL,
    FOREIGN KEY (session_id) REFERENCES sessions(id)
);
CREATE INDEX IF NOT EXISTS idx_message_session_seq ON message_history(session_id, seq);
```

- [ ] **Step 3: 编译验证**

Run: `cargo check 2>&1`
Expected: 编译通过（ChatMessage 未提供 timestamp 的现有构造点由 serde default 自动填充 `Utc::now()`）

- [ ] **Step 4: 运行现有测试确认无回归**

Run: `cargo test 2>&1`
Expected: 所有现有测试 PASS

- [ ] **Step 5: Commit**

```bash
git add crates/models/src/lib.rs migrations/V3__add_message_history.sql
git commit -m "feat(models,migrations): add AgentState, ToolCall, PersonaProvider::invalidate, ChatMessage.timestamp, V3 migration"
```

---

### Task 2: MessageStore

**Files:**
- Modify: `F:/openLoom/crates/memory/src/store.rs`

- [ ] **Step 1: 在 store.rs 末尾新增 MessageStore**

在 `store.rs` 的最后一个 `#[cfg(test)]` 块之前插入：

```rust
// === MessageStore ===

pub struct MessageStore<'a> {
    conn: &'a Connection,
}

impl<'a> MessageStore<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn insert(
        &self,
        session_id: &str,
        seq: usize,
        role: &str,
        content: &str,
    ) -> anyhow::Result<i64> {
        self.conn.execute(
            "INSERT INTO message_history (session_id, seq, role, content, timestamp)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![session_id, seq, role, content, Utc::now().to_rfc3339()],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn recent(
        &self,
        session_id: &str,
        limit: usize,
    ) -> anyhow::Result<Vec<openloom_models::ChatMessage>> {
        let mut stmt = self.conn.prepare(
            "SELECT role, content, timestamp FROM message_history
             WHERE session_id = ?1 ORDER BY seq DESC LIMIT ?2",
        )?;
        let mut rows: Vec<openloom_models::ChatMessage> = stmt
            .query_map(rusqlite::params![session_id, limit as i64], |row| {
                let ts_str: String = row.get(2)?;
                Ok(openloom_models::ChatMessage {
                    role: row.get(0)?,
                    content: row.get(1)?,
                    timestamp: DateTime::parse_from_rfc3339(&ts_str)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows.reverse(); // DESC → chronological order
        Ok(rows)
    }

    pub fn max_seq(&self, session_id: &str) -> anyhow::Result<usize> {
        let seq: Option<i64> = self.conn.query_row(
            "SELECT MAX(seq) FROM message_history WHERE session_id = ?1",
            rusqlite::params![session_id],
            |row| row.get(0),
        )?;
        Ok(seq.unwrap_or(0) as usize)
    }

    pub fn insert_batch(
        &self,
        session_id: &str,
        messages: &[openloom_models::ChatMessage],
    ) -> anyhow::Result<()> {
        let mut seq = self.max_seq(session_id)? + 1;
        for msg in messages {
            self.insert(session_id, seq, &msg.role, &msg.content)?;
            seq += 1;
        }
        Ok(())
    }
}
```

- [ ] **Step 2: 写 MessageStore 单元测试**

在 `store.rs` 末尾测试模块中追加：

```rust
#[cfg(test)]
mod message_store_tests {
    use super::*;
    use tempfile::tempdir;

    fn setup_message_table(conn: &Connection) {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS message_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL,
                seq INTEGER NOT NULL,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                timestamp TEXT NOT NULL
            );",
        )
        .unwrap();
    }

    #[test]
    fn test_message_insert_and_recent() {
        let dir = tempdir().unwrap();
        let conn = Connection::open(dir.path().join("test.db")).unwrap();
        setup_message_table(&conn);
        let store = MessageStore::new(&conn);

        store.insert("s1", 1, "user", "hello").unwrap();
        store.insert("s1", 2, "assistant", "hi there").unwrap();
        store.insert("s1", 3, "user", "how are you").unwrap();

        let recent = store.recent("s1", 2).unwrap();
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].content, "hi there");
        assert_eq!(recent[1].content, "how are you");
    }

    #[test]
    fn test_message_max_seq() {
        let dir = tempdir().unwrap();
        let conn = Connection::open(dir.path().join("test.db")).unwrap();
        setup_message_table(&conn);
        let store = MessageStore::new(&conn);

        assert_eq!(store.max_seq("s1").unwrap(), 0);
        store.insert("s1", 1, "user", "a").unwrap();
        store.insert("s1", 2, "assistant", "b").unwrap();
        assert_eq!(store.max_seq("s1").unwrap(), 2);
    }

    #[test]
    fn test_message_empty_session() {
        let dir = tempdir().unwrap();
        let conn = Connection::open(dir.path().join("test.db")).unwrap();
        setup_message_table(&conn);
        let store = MessageStore::new(&conn);
        let recent = store.recent("nonexistent", 20).unwrap();
        assert!(recent.is_empty());
    }

    #[test]
    fn test_message_insert_batch() {
        let dir = tempdir().unwrap();
        let conn = Connection::open(dir.path().join("test.db")).unwrap();
        setup_message_table(&conn);
        let store = MessageStore::new(&conn);

        let msgs = vec![
            openloom_models::ChatMessage { role: "user".into(), content: "a".into(), timestamp: Utc::now() },
            openloom_models::ChatMessage { role: "assistant".into(), content: "b".into(), timestamp: Utc::now() },
        ];
        store.insert_batch("s1", &msgs).unwrap();
        assert_eq!(store.max_seq("s1").unwrap(), 2);
        let recent = store.recent("s1", 10).unwrap();
        assert_eq!(recent.len(), 2);
    }
}
```

- [ ] **Step 3: 运行 MessageStore 测试**

Run: `cargo test message_store 2>&1`
Expected: 4 tests PASS

- [ ] **Step 4: 运行全部测试确认无回归**

Run: `cargo test 2>&1`
Expected: 所有测试 PASS

- [ ] **Step 5: Commit**

```bash
git add crates/memory/src/store.rs
git commit -m "feat(memory): add MessageStore with insert, recent, max_seq, insert_batch"
```

---

### Task 3: CognitionsPersonaProvider

**Files:**
- Create: `F:/openLoom/crates/memory/src/persona.rs`
- Modify: `F:/openLoom/crates/memory/src/lib.rs`
- Modify: `F:/openLoom/crates/memory/Cargo.toml`

- [ ] **Step 0: 添加 memory crate 依赖**

`F:/openLoom/crates/memory/Cargo.toml` 在 `[dependencies]` 加：
```toml
async-trait = "0.1"
```

在 `[dev-dependencies]` 加（如果不存在该 section 则创建）：
```toml
tokio = { version = "1", features = ["rt"] }
```

- [ ] **Step 1: 创建 persona.rs**

`F:/openLoom/crates/memory/src/persona.rs`：

```rust
use openloom_models::PersonaProvider;
use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::Mutex;

pub struct CognitionsPersonaProvider {
    db_path: PathBuf,
    cache: Mutex<Option<String>>,
}

impl CognitionsPersonaProvider {
    pub fn new(db_path: PathBuf) -> Self {
        Self {
            db_path,
            cache: Mutex::new(None),
        }
    }
}

#[async_trait::async_trait]
impl PersonaProvider for CognitionsPersonaProvider {
    async fn summarize(&self) -> anyhow::Result<String> {
        // Hold lock for entire read-and-cache to avoid TOCTOU race
        let mut cache = self.cache.lock().unwrap();
        if let Some(ref cached) = *cache {
            return Ok(cached.clone());
        }

        // Open read-only connection
        let conn = Connection::open(&self.db_path)?;

        let now = chrono::Utc::now().timestamp();
        let mut stmt = conn.prepare(
            "SELECT trait, value, confidence, evidence_count, last_updated, source
             FROM cognitions WHERE subject = 'USER'",
        )?;

        struct ScoredRow {
            value: String,
            score: f64,
        }

        let mut rows: Vec<ScoredRow> = Vec::new();
        let query_rows = stmt.query_map([], |row| {
            let trait_name: String = row.get(0)?;
            let value: String = row.get(1)?;
            let confidence: f64 = row.get(2)?;
            let evidence_count: i64 = row.get(3)?;
            let last_updated: i64 = row.get(4)?;
            let source: String = row.get(5)?;
            Ok((trait_name, value, confidence, evidence_count, last_updated, source))
        })?;

        for row in query_rows {
            let (trait_name, value, confidence, evidence_count, last_updated, source) = row?;
            let days_since = ((now - last_updated) as f64 / 86400.0).max(0.0);
            let recency_decay = (-days_since / 30.0).exp();
            let base_score = confidence * (evidence_count as f64);
            let weighted_score = base_score * recency_decay;
            let source_priority = if source == "observed" { 1.0 } else { 0.0 };
            let final_score = weighted_score + source_priority * 0.001;

            // trait_name comes from the DB as-is (already Chinese from pipeline's generate_summary)
            // value comes from generate_summary (Chinese description)
            rows.push(ScoredRow {
                value: format!("{}：{}", trait_name, value),
                score: final_score,
            });
        }

        rows.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        rows.truncate(5);

        let summary = if rows.is_empty() {
            String::new()
        } else {
            let parts: Vec<String> = rows.iter().map(|r| r.value.clone()).collect();
            format!("用户画像：{}。", parts.join("；"))
        };

        *cache = Some(summary.clone());
        Ok(summary)
    }

    fn invalidate(&self) {
        self.cache.lock().unwrap().take();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn setup_cognitions_table(conn: &Connection) {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS cognitions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                subject TEXT NOT NULL,
                trait TEXT NOT NULL,
                value TEXT NOT NULL,
                confidence REAL,
                evidence_count INTEGER,
                first_seen INTEGER,
                last_updated INTEGER,
                version INTEGER DEFAULT 1,
                source TEXT NOT NULL DEFAULT 'observed'
            );",
        )
        .unwrap();
    }

    #[test]
    fn test_persona_empty_returns_empty_string() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let conn = Connection::open(&db_path).unwrap();
        setup_cognitions_table(&conn);

        let provider = CognitionsPersonaProvider::new(db_path.clone());
        let rt = tokio::runtime::Runtime::new().unwrap();
        let summary = rt.block_on(provider.summarize()).unwrap();
        assert!(summary.is_empty());
    }

    #[test]
    fn test_persona_with_cognitions() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let conn = Connection::open(&db_path).unwrap();
        setup_cognitions_table(&conn);
        let now = chrono::Utc::now().timestamp();

        conn.execute(
            "INSERT INTO cognitions (subject, trait, value, confidence, evidence_count, first_seen, last_updated, source)
             VALUES ('USER', 'risk_tendency', '用户存在赌徒补仓倾向', 0.91, 5, ?1, ?1, 'observed')",
            rusqlite::params![now],
        ).unwrap();
        conn.execute(
            "INSERT INTO cognitions (subject, trait, value, confidence, evidence_count, first_seen, last_updated, source)
             VALUES ('USER', 'trading_style', '用户偏好短线交易', 0.85, 3, ?1, ?1, 'observed')",
            rusqlite::params![now],
        ).unwrap();

        let provider = CognitionsPersonaProvider::new(db_path.clone());
        let rt = tokio::runtime::Runtime::new().unwrap();
        let summary = rt.block_on(provider.summarize()).unwrap();
        assert!(summary.contains("risk_tendency"));
        assert!(summary.contains("trading_style"));
        assert!(summary.starts_with("用户画像："));
    }

    #[test]
    fn test_persona_cache_hit() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let conn = Connection::open(&db_path).unwrap();
        setup_cognitions_table(&conn);
        let now = chrono::Utc::now().timestamp();
        conn.execute(
            "INSERT INTO cognitions (subject, trait, value, confidence, evidence_count, first_seen, last_updated, source)
             VALUES ('USER', 'risk_tendency', '赌徒补仓', 0.91, 5, ?1, ?1, 'observed')",
            rusqlite::params![now],
        ).unwrap();

        let provider = CognitionsPersonaProvider::new(db_path.clone());
        let rt = tokio::runtime::Runtime::new().unwrap();
        let s1 = rt.block_on(provider.summarize()).unwrap();
        let s2 = rt.block_on(provider.summarize()).unwrap();
        assert_eq!(s1, s2);
    }

    #[test]
    fn test_persona_invalidate() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let conn = Connection::open(&db_path).unwrap();
        setup_cognitions_table(&conn);
        let now = chrono::Utc::now().timestamp();
        conn.execute(
            "INSERT INTO cognitions (subject, trait, value, confidence, evidence_count, first_seen, last_updated, source)
             VALUES ('USER', 'risk_tendency', '赌徒补仓', 0.91, 5, ?1, ?1, 'observed')",
            rusqlite::params![now],
        ).unwrap();

        let provider = CognitionsPersonaProvider::new(db_path.clone());
        let rt = tokio::runtime::Runtime::new().unwrap();
        let s1 = rt.block_on(provider.summarize()).unwrap();
        provider.invalidate();
        let s2 = rt.block_on(provider.summarize()).unwrap();
        assert_eq!(s1, s2); // Same data, invalidate just re-queries
    }

    #[test]
    fn test_persona_mixed_sources_observed_first() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let conn = Connection::open(&db_path).unwrap();
        setup_cognitions_table(&conn);
        let now = chrono::Utc::now().timestamp();
        // inferred (lower priority) but higher raw score
        conn.execute(
            "INSERT INTO cognitions (subject, trait, value, confidence, evidence_count, first_seen, last_updated, source)
             VALUES ('USER', 'inferred_trait', '推断特质', 0.99, 20, ?1, ?1, 'inferred')",
            rusqlite::params![now],
        ).unwrap();
        // observed (higher priority) but lower raw score
        conn.execute(
            "INSERT INTO cognitions (subject, trait, value, confidence, evidence_count, first_seen, last_updated, source)
             VALUES ('USER', 'observed_trait', '观察特质', 0.5, 2, ?1, ?1, 'observed')",
            rusqlite::params![now],
        ).unwrap();

        let provider = CognitionsPersonaProvider::new(db_path.clone());
        let rt = tokio::runtime::Runtime::new().unwrap();
        let summary = rt.block_on(provider.summarize()).unwrap();
        // Both should appear (both in top 5), observed should come first
        let obs_pos = summary.find("observed_trait").unwrap();
        let inf_pos = summary.find("inferred_trait").unwrap();
        assert!(obs_pos < inf_pos, "observed trait should appear before inferred");
    }
}
```

- [ ] **Step 2: 注册 persona 模块**

`F:/openLoom/crates/memory/src/lib.rs` 加 `pub mod persona;`（在现有 mod 声明之间）：

```rust
pub mod aggregator;
pub mod event;
pub mod extractor;
pub mod persona;
pub mod pipeline;
pub mod store;
```

- [ ] **Step 3: 运行 persona 测试**

Run: `cargo test -p openloom-memory persona 2>&1`
Expected: 6 tests PASS

- [ ] **Step 4: 运行全部测试**

Run: `cargo test 2>&1`
Expected: 所有测试 PASS

- [ ] **Step 5: Commit**

```bash
git add crates/memory/src/persona.rs crates/memory/src/lib.rs crates/memory/Cargo.toml
git commit -m "feat(memory): add CognitionsPersonaProvider with weighted scoring, recency decay, and cache"
```

---

### Task 4: Weaver 签名重构

**Files:**
- Modify: `F:/openLoom/crates/weaver/src/lib.rs`

- [ ] **Step 1: 移除 PersonaProvider 依赖，改为接收字符串参数**

`F:/openLoom/crates/weaver/src/lib.rs` 完整替换：

```rust
use openloom_cache::KvCache;
use openloom_models::ChatMessage;
use std::sync::Arc;

pub struct AssembledPrompt {
    pub prompt: String,
    pub static_prefix_len: usize,
}

pub struct ContextWeaver {
    cache: Arc<dyn KvCache>,
}

impl ContextWeaver {
    pub fn new(cache: Arc<dyn KvCache>) -> Self {
        Self { cache }
    }

    pub fn assemble(
        &self,
        system_instruction: &str,
        user_message: &str,
        persona_summary: &str,
        skill_context: Option<&str>,
        working_memory: &[ChatMessage],
    ) -> AssembledPrompt {
        let prefix_hash = 0u64;
        let _ = self.cache.lookup(prefix_hash);

        let static_prefix = if persona_summary.is_empty() {
            system_instruction.to_string()
        } else {
            format!("{}\n{}", system_instruction, persona_summary)
        };
        let static_prefix_len = static_prefix.len();

        let skill_section = match skill_context {
            Some(ctx) if !ctx.is_empty() => format!("\n[Skill Context]\n{}\n", ctx),
            _ => String::new(),
        };

        let memory_section = if working_memory.is_empty() {
            String::new()
        } else {
            let memory_text: String = working_memory
                .iter()
                .map(|m| format!("{}: {}", m.role, m.content))
                .collect::<Vec<_>>()
                .join("\n");
            format!("\n[Conversation History]\n{}\n", memory_text)
        };

        let dynamic_section = format!(
            "{}{}\n[User Message]\n{}",
            skill_section, memory_section, user_message
        );
        let prompt = format!("{}\n{}", static_prefix, dynamic_section);

        AssembledPrompt { prompt, static_prefix_len }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openloom_cache::NoopCache;

    const SYSTEM_INSTRUCTION: &str = "You are openLoom, a private AI assistant running locally.";

    fn make_weaver() -> ContextWeaver {
        ContextWeaver::new(Arc::new(NoopCache))
    }

    fn make_msg(role: &str, content: &str) -> ChatMessage {
        ChatMessage {
            role: role.into(),
            content: content.into(),
            timestamp: chrono::Utc::now(),
        }
    }

    #[test]
    fn test_assemble_basic_message() {
        let weaver = make_weaver();
        let result = weaver.assemble(SYSTEM_INSTRUCTION, "hello", "", None, &[]);
        assert!(result.prompt.contains("hello"));
        assert!(result.prompt.contains(SYSTEM_INSTRUCTION));
        assert!(result.static_prefix_len > 0);
    }

    #[test]
    fn test_assemble_with_persona() {
        let weaver = make_weaver();
        let persona = "用户画像：短线交易；追高倾向。";
        let result = weaver.assemble(SYSTEM_INSTRUCTION, "hello", persona, None, &[]);
        assert!(result.prompt.contains(persona));
        let static_part = &result.prompt[..result.static_prefix_len];
        assert!(static_part.contains(persona));
    }

    #[test]
    fn test_assemble_with_skill_context() {
        let weaver = make_weaver();
        let result = weaver.assemble(
            SYSTEM_INSTRUCTION, "open file", "",
            Some("file-manager: list/read/write files"), &[],
        );
        assert!(result.prompt.contains("[Skill Context]"));
        assert!(result.prompt.contains("file-manager"));
    }

    #[test]
    fn test_assemble_with_working_memory() {
        let weaver = make_weaver();
        let memory = vec![make_msg("user", "hi"), make_msg("assistant", "hello")];
        let result = weaver.assemble(SYSTEM_INSTRUCTION, "how are you", "", None, &memory);
        assert!(result.prompt.contains("[Conversation History]"));
        assert!(result.prompt.contains("user: hi"));
        assert!(result.prompt.contains("assistant: hello"));
    }

    #[test]
    fn test_static_prefix_before_dynamic() {
        let weaver = make_weaver();
        let result = weaver.assemble(
            SYSTEM_INSTRUCTION, "test message", "用户画像：测试。",
            Some("skill context"), &[],
        );
        let static_part = &result.prompt[..result.static_prefix_len];
        let dynamic_part = &result.prompt[result.static_prefix_len..];
        assert!(static_part.contains(SYSTEM_INSTRUCTION));
        assert!(static_part.contains("用户画像"));
        assert!(!static_part.contains("test message"));
        assert!(dynamic_part.contains("test message"));
        assert!(dynamic_part.contains("[Skill Context]"));
    }
}
```

- [ ] **Step 2: 运行 weaver 测试**

Run: `cargo test -p openloom-weaver 2>&1`
Expected: 5 tests PASS

- [ ] **Step 3: Commit**

```bash
git add crates/weaver/src/lib.rs
git commit -m "refactor(weaver): remove PersonaProvider field, accept persona_summary as &str"
```

注意：此时 engine 测试会编译失败（weaver 签名变了），在 Task 5 中修复。

---

### Task 5: Engine Agent Loop + Persona + Message History 集成

**Files:**
- Modify: `F:/openLoom/crates/engine/src/lib.rs`

- [ ] **Step 1: 重构 Engine — 完整替换 lib.rs**

`F:/openLoom/crates/engine/src/lib.rs` 完整替换：

```rust
pub mod memory_thread;

use anyhow::Result;
use openloom_cache::NoopCache;
use openloom_inference::{CloudClient, CompletionRequest, InferenceEngine};
use openloom_models::*;
use openloom_memory::persona::CognitionsPersonaProvider;
use openloom_memory::store::{MessageStore, SessionStore};
use openloom_router::SmartRouter;
use openloom_skills::{SkillRegistry, builtins};
use openloom_weaver::ContextWeaver;
use std::path::PathBuf;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use tokio::sync::{broadcast, oneshot, RwLock};
use chrono::Utc;

pub use openloom_models::EngineEvent;

const SYSTEM_INSTRUCTION: &str = "You are openLoom, a private AI assistant running locally.
When you need to use a tool, respond with ONLY a JSON block on a single line:
{\"tool\": \"<skill_name>\", \"params\": {\"key\": \"value\"}}
Available tools: [tools]
When you have the final answer, respond in natural language without JSON.";

pub struct Engine {
    router: SmartRouter,
    skills: SkillRegistry,
    inference: Arc<InferenceEngine>,
    cloud: Option<Arc<dyn CloudClient>>,
    weaver: ContextWeaver,
    persona: Arc<dyn PersonaProvider>,
    memory_tx: std::sync::mpsc::Sender<memory_thread::ProcessRequest>,
    session_tx: std::sync::mpsc::Sender<SessionCommand>,
    event_bus: broadcast::Sender<EngineEvent>,
    agent_state: Arc<RwLock<AgentState>>,
    interruptible: AtomicBool,
    db_path: PathBuf,
}

enum SessionCommand {
    Create { reply: oneshot::Sender<SessionInfo> },
    List { reply: oneshot::Sender<Vec<SessionInfo>> },
    UpdateCount { id: String, count: usize },
}

pub struct EngineConfig {
    pub data_dir: PathBuf,
    pub threshold: usize,
    pub cloud_config: Option<openloom_models::ModelConfig>,
}

fn spawn_session_thread(db_path: PathBuf) -> std::sync::mpsc::Sender<SessionCommand> {
    let (tx, rx) = std::sync::mpsc::channel::<SessionCommand>();
    std::thread::spawn(move || {
        let conn = rusqlite::Connection::open(&db_path).expect("session db open");
        conn.execute_batch("PRAGMA journal_mode=WAL;").unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY, created_at TEXT NOT NULL, message_count INTEGER DEFAULT 0
            );"
        ).unwrap();
        let store = SessionStore::new(&conn);
        for cmd in rx {
            match cmd {
                SessionCommand::Create { reply } => {
                    let id = uuid::Uuid::new_v4().to_string();
                    let info = SessionInfo { id: id.clone(), created_at: Utc::now(), message_count: 0 };
                    let _ = store.insert(&info.id, info.created_at);
                    let _ = reply.send(info);
                }
                SessionCommand::List { reply } => {
                    let sessions = store.list_all(100).unwrap_or_default();
                    let _ = reply.send(sessions);
                }
                SessionCommand::UpdateCount { id, count } => {
                    let _ = store.update_message_count(&id, count);
                }
            }
        }
    });
    tx
}

impl Engine {
    pub fn new_test(db_path: PathBuf) -> Result<Self> {
        Self::new(EngineConfig {
            data_dir: db_path.parent().unwrap().to_path_buf(),
            threshold: 3,
            cloud_config: None,
        })
    }

    pub fn new(config: EngineConfig) -> Result<Self> {
        let inference = Arc::new(InferenceEngine::load_blocking(
            &config.data_dir.join("models").join("qwen3-1.7b-q4_k_m.gguf"), 0,
        )?);

        let mut router = SmartRouter::new_keywords_only(openloom_router::keywords::default_keyword_rules());
        let mut skills = SkillRegistry::new();
        builtins::register_all(&mut skills);
        for skill in skills.all_skills() {
            let manifest = skill.manifest();
            router.register_skill_triggers(skill.name(), manifest.triggers.clone());
        }

        let cloud: Option<Arc<dyn CloudClient>> = config.cloud_config.as_ref()
            .and_then(|cfg| openloom_inference::create_cloud_client(cfg).ok().map(Arc::from));
        router.set_cloud_available(cloud.is_some());

        let db_path = config.data_dir.join("data").join("db.sqlite");
        let _ = std::fs::create_dir_all(db_path.parent().unwrap());

        let persona: Arc<dyn PersonaProvider> = Arc::new(CognitionsPersonaProvider::new(db_path.clone()));
        let weaver = ContextWeaver::new(Arc::new(NoopCache));

        let (event_tx, _) = broadcast::channel(256);
        let memory_tx = memory_thread::spawn_memory_thread(db_path.clone(), config.threshold, event_tx.clone());
        let session_tx = spawn_session_thread(db_path.clone());

        let engine = Self {
            router, skills, inference, cloud, weaver, persona, memory_tx, session_tx,
            event_bus: event_tx,
            agent_state: Arc::new(RwLock::new(AgentState::Idle)),
            interruptible: AtomicBool::new(false),
            db_path,
        };

        engine.spawn_persona_watcher();
        Ok(engine)
    }

    // === Persona watcher ===

    fn spawn_persona_watcher(&self) {
        let persona = self.persona.clone();
        let mut rx = self.event_bus.subscribe();
        tokio::spawn(async move {
            while rx.recv().await.is_ok() {
                persona.invalidate();
            }
        });
    }

    // === Core handler ===

    pub async fn handle_message(&self, msg: ChatMessage, session_id: &str) -> Result<ChatResponse> {
        // Atomic mid-turn check: compare_exchange ensures only one caller enters
        if self.interruptible.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
            return Err(anyhow::anyhow!("Agent is busy, please wait"));
        }
        // Reset immediately for simple path; agent_loop will set it again
        self.interruptible.store(false, Ordering::SeqCst);

        let out = self.router.classify_sync(&msg.content);

        if out.complexity < 0.5 && out.skill_match.is_none() {
            let skill_ctx = out.skill_match.as_ref()
                .and_then(|name| self.skills.find_by_name(name).map(|s| s.context_md().to_string()));
            let working_memory = self.get_working_memory(session_id).unwrap_or_default();
            // Persona failure → empty string fallback
            let persona_summary = self.persona.summarize().await.unwrap_or_default();
            let assembled = self.weaver.assemble(
                SYSTEM_INSTRUCTION, &msg.content, &persona_summary, skill_ctx.as_deref(), &working_memory,
            );

            let response = match out.target_model {
                TargetModel::None => {
                    let name = out.skill_match.as_ref()
                        .ok_or_else(|| anyhow::anyhow!("skill_match is None"))?;
                    self.skills.invoke(name, serde_json::json!({"text": msg.content})).await?.to_string()
                }
                TargetModel::Local => {
                    self.inference.complete(CompletionRequest { prompt: assembled.prompt.clone(), ..Default::default() }).await?.text
                }
                TargetModel::Cloud => {
                    if let Some(ref cloud) = self.cloud {
                        cloud.complete(CompletionRequest { prompt: assembled.prompt.clone(), ..Default::default() }).await?.text
                    } else {
                        self.inference.complete(CompletionRequest { prompt: assembled.prompt.clone(), ..Default::default() }).await?.text
                    }
                }
            };

            // save_messages is non-fatal
            let _ = self.save_messages(session_id, &msg, &response);

            let prompt_tokens = self.inference.token_count(&assembled.prompt);
            let completion_tokens = self.inference.token_count(&response);
            let _ = self.event_bus.send(EngineEvent::TokenUsage {
                session_id: session_id.to_string(),
                model: "qwen3-1.7b".into(),
                prompt_tokens,
                completion_tokens,
            });

            return Ok(ChatResponse {
                response,
                session_id: session_id.to_string(),
                token_usage: TokenUsage { prompt_tokens, completion_tokens },
            });
        }

        // Complex → Agent Loop
        self.agent_loop(&msg, session_id).await
    }

    // === Agent Loop ===

    async fn agent_loop(&self, msg: &ChatMessage, session_id: &str) -> Result<ChatResponse> {
        *self.agent_state.write().await = AgentState::Thinking;
        self.interruptible.store(true, Ordering::SeqCst);

        let mut history: Vec<ChatMessage> = self.get_working_memory(session_id).unwrap_or_default();
        history.push(msg.clone());

        // Build skill list string for system prompt injection
        let skill_list = self.build_skill_list_string();

        let mut all_tool_messages: Vec<ChatMessage> = Vec::new();
        let mut last_response = String::new();

        let outcome = tokio::time::timeout(
            std::time::Duration::from_secs(120),
            async {
                for _iteration in 0..3 {
                    let persona_summary = self.persona.summarize().await.unwrap_or_default();
                    let system_with_tools = SYSTEM_INSTRUCTION.replace("[tools]", &skill_list);
                    let assembled = self.weaver.assemble(
                        &system_with_tools, "", &persona_summary, None, &history,
                    );

                    let response = self.invoke_model_raw(&assembled.prompt).await?;

                    if let Some(tool_call) = self.parse_tool_call(&response) {
                        *self.agent_state.write().await = AgentState::Acting;
                        let result = match self.execute_tool(&tool_call).await {
                            Ok(output) => output,
                            Err(e) => format!("Tool error: {}", e),
                        };
                        let ts = Utc::now();
                        history.push(ChatMessage { role: "assistant".into(), content: response.clone(), timestamp: ts });
                        history.push(ChatMessage { role: "tool".into(), content: result.clone(), timestamp: ts });
                        // Also save intermediate messages for persistence
                        all_tool_messages.push(ChatMessage { role: "assistant".into(), content: response, timestamp: ts });
                        all_tool_messages.push(ChatMessage { role: "tool".into(), content: result, timestamp: ts });
                    } else {
                        last_response = response;
                        break;
                    }
                }
                Ok::<_, anyhow::Error>(last_response)
            },
        ).await;

        *self.agent_state.write().await = AgentState::Idle;
        self.interruptible.store(false, Ordering::SeqCst);

        match outcome {
            Ok(Ok(ref response)) if response.is_empty() => {
                Err(anyhow::anyhow!("Agent loop produced no response after 3 iterations"))
            }
            Ok(Ok(response)) => {
                // Save all messages: user + tool interactions + final response
                let _ = self.save_all_messages(session_id, msg, &all_tool_messages, &response);

                let prompt_tokens = self.inference.token_count(&msg.content);
                let completion_tokens = self.inference.token_count(&response);
                let _ = self.event_bus.send(EngineEvent::TokenUsage {
                    session_id: session_id.to_string(),
                    model: "agent-loop".into(),
                    prompt_tokens,
                    completion_tokens,
                });
                Ok(ChatResponse {
                    response,
                    session_id: session_id.to_string(),
                    token_usage: TokenUsage { prompt_tokens, completion_tokens },
                })
            }
            Ok(Err(e)) => Err(e),
            Err(_elapsed) => Err(anyhow::anyhow!("Agent loop timed out after 120s")),
        }
    }

    fn build_skill_list_string(&self) -> String {
        let skills = self.skills.list_all();
        if skills.is_empty() {
            return "None".into();
        }
        skills.iter()
            .map(|s| format!("{}: {}", s.name, s.description))
            .collect::<Vec<_>>()
            .join(", ")
    }

    async fn invoke_model_raw(&self, prompt: &str) -> Result<String> {
        if let Some(ref cloud) = self.cloud {
            match cloud.complete(CompletionRequest { prompt: prompt.to_string(), ..Default::default() }).await {
                Ok(r) => return Ok(r.text),
                Err(e) => tracing::warn!("Cloud failed, falling back to local: {}", e),
            }
        }
        self.inference.complete(CompletionRequest { prompt: prompt.to_string(), ..Default::default() })
            .await
            .map(|r| r.text)
    }

    fn parse_tool_call(&self, response: &str) -> Option<ToolCall> {
        // Trim and find the first JSON object containing "tool" key
        let trimmed = response.trim();
        if let Some(start) = trimmed.find("{\"tool\"") {
            // Find matching closing brace by counting nesting
            let slice = &trimmed[start..];
            let mut depth = 0;
            let mut end = 0;
            for (i, ch) in slice.char_indices() {
                match ch {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            end = i;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            if end > 0 {
                let json_str = &slice[..=end];
                match serde_json::from_str::<ToolCall>(json_str) {
                    Ok(call) => return Some(call),
                    Err(e) => {
                        tracing::warn!("Failed to parse tool call JSON: {} — raw: {}", e, json_str);
                        return None;
                    }
                }
            }
        }
        None
    }

    async fn execute_tool(&self, call: &ToolCall) -> Result<String> {
        self.skills.invoke(&call.tool, call.params.clone()).await.map(|v| v.to_string())
    }

    // === Message persistence (non-fatal) ===

    fn save_messages(&self, session_id: &str, user_msg: &ChatMessage, assistant_response: &str) -> Result<()> {
        let conn = match rusqlite::Connection::open(&self.db_path) {
            Ok(c) => c,
            Err(e) => { tracing::error!("save_messages: {}", e); return Ok(()); }
        };
        let _ = conn.execute_batch("PRAGMA journal_mode=WAL;");
        let store = MessageStore::new(&conn);
        let next_seq = store.max_seq(session_id).unwrap_or(0) + 1;
        let _ = store.insert(session_id, next_seq, "user", &user_msg.content);
        let _ = store.insert(session_id, next_seq + 1, "assistant", assistant_response);
        let _ = self.session_tx.send(SessionCommand::UpdateCount {
            id: session_id.to_string(),
            count: next_seq + 1,
        });
        Ok(())
    }

    fn save_all_messages(&self, session_id: &str, user_msg: &ChatMessage, tool_msgs: &[ChatMessage], final_response: &str) -> Result<()> {
        let conn = match rusqlite::Connection::open(&self.db_path) {
            Ok(c) => c,
            Err(e) => { tracing::error!("save_all_messages: {}", e); return Ok(()); }
        };
        let _ = conn.execute_batch("PRAGMA journal_mode=WAL;");
        let store = MessageStore::new(&conn);
        let mut seq = store.max_seq(session_id).unwrap_or(0) + 1;
        let _ = store.insert(session_id, seq, "user", &user_msg.content);
        seq += 1;
        for msg in tool_msgs {
            let _ = store.insert(session_id, seq, &msg.role, &msg.content);
            seq += 1;
        }
        let _ = store.insert(session_id, seq, "assistant", final_response);
        let _ = self.session_tx.send(SessionCommand::UpdateCount {
            id: session_id.to_string(),
            count: seq,
        });
        Ok(())
    }

    fn get_working_memory(&self, session_id: &str) -> Result<Vec<ChatMessage>> {
        match rusqlite::Connection::open(&self.db_path) {
            Ok(conn) => {
                let store = MessageStore::new(&conn);
                store.recent(session_id, 20)
            }
            Err(e) => {
                tracing::warn!("get_working_memory: {}", e);
                Ok(Vec::new())
            }
        }
    }

    // === Public API ===

    pub async fn health_check(&self) -> HealthStatus {
        let gpu = InferenceEngine::detect_gpu();
        HealthStatus { status: "ok".into(), uptime: 0, gpu_info: gpu }
    }

    pub async fn create_session(&self) -> Result<SessionInfo> {
        let (tx, rx) = oneshot::channel();
        self.session_tx.send(SessionCommand::Create { reply: tx }).map_err(|e| anyhow::anyhow!("{}", e))?;
        rx.await.map_err(|e| anyhow::anyhow!("{}", e))
    }

    pub async fn list_sessions(&self) -> Result<Vec<SessionInfo>> {
        let (tx, rx) = oneshot::channel();
        self.session_tx.send(SessionCommand::List { reply: tx }).map_err(|e| anyhow::anyhow!("{}", e))?;
        rx.await.map_err(|e| anyhow::anyhow!("{}", e))
    }

    pub async fn list_cognitions(&self, subject: &str, limit: usize) -> Result<Vec<openloom_memory::store::CognitionRow>> {
        let conn = rusqlite::Connection::open(&self.db_path)?;
        let store = openloom_memory::store::CognitionStore::new(&conn);
        store.query_by_subject(subject, limit)
    }

    pub async fn persona_summary(&self) -> String {
        self.persona.summarize().await.unwrap_or_default()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<EngineEvent> {
        self.event_bus.subscribe()
    }

    pub fn list_skills(&self) -> Vec<openloom_skills::SkillInfo> {
        self.skills.list_all()
    }

    pub async fn invoke_skill(&self, name: &str, params: serde_json::Value) -> Result<serde_json::Value> {
        self.skills.invoke(name, params).await
    }

    pub async fn agent_state(&self) -> AgentState {
        self.agent_state.read().await.clone()
    }

    pub async fn shutdown(&self) -> Result<()> {
        tracing::info!("engine shutting down");
        Ok(())
    }
}

// === Tests ===

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    async fn setup_test_engine() -> (Engine, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let engine = Engine::new_test(db_path).unwrap();
        (engine, dir)
    }

    #[tokio::test]
    async fn test_create_and_list_sessions() {
        let (engine, _dir) = setup_test_engine().await;
        let s1 = engine.create_session().await.unwrap();
        let s2 = engine.create_session().await.unwrap();
        let sessions = engine.list_sessions().await.unwrap();
        assert_eq!(sessions.len(), 2);
        assert!(sessions.iter().any(|s| s.id == s1.id));
        assert!(sessions.iter().any(|s| s.id == s2.id));
    }

    #[tokio::test]
    async fn test_handle_message_llm_path() {
        let (engine, _dir) = setup_test_engine().await;
        let msg = ChatMessage { role: "user".into(), content: "hello".into(), timestamp: Utc::now() };
        let sid = engine.create_session().await.unwrap().id;
        let resp = engine.handle_message(msg, &sid).await.unwrap();
        assert_eq!(resp.session_id, sid);
    }

    #[tokio::test]
    async fn test_health_check() {
        let (engine, _dir) = setup_test_engine().await;
        let health = engine.health_check().await;
        assert_eq!(health.status, "ok");
    }

    #[tokio::test]
    async fn test_event_bus_subscribe() {
        let (engine, _dir) = setup_test_engine().await;
        let mut rx = engine.subscribe();
        let msg = ChatMessage { role: "user".into(), content: "hello".into(), timestamp: Utc::now() };
        let sid = engine.create_session().await.unwrap().id;
        engine.handle_message(msg, &sid).await.unwrap();
        let event = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv()).await;
        assert!(event.is_ok(), "should receive TokenUsage event");
    }

    #[tokio::test]
    async fn test_handle_message_skill_path() {
        let (engine, _dir) = setup_test_engine().await;
        let msg = ChatMessage { role: "user".into(), content: "帮我管理文件".into(), timestamp: Utc::now() };
        let sid = engine.create_session().await.unwrap().id;
        let resp = engine.handle_message(msg, &sid).await.unwrap();
        assert!(!resp.session_id.is_empty());
    }

    #[test]
    fn test_parse_tool_call_valid() {
        let (engine, _dir) = tokio::runtime::Runtime::new().unwrap()
            .block_on(setup_test_engine());
        let result = engine.parse_tool_call("{\"tool\": \"test\", \"params\": {\"k\": \"v\"}}");
        assert!(result.is_some());
        let call = result.unwrap();
        assert_eq!(call.tool, "test");
    }

    #[test]
    fn test_parse_tool_call_with_whitespace() {
        let (engine, _dir) = tokio::runtime::Runtime::new().unwrap()
            .block_on(setup_test_engine());
        let result = engine.parse_tool_call("  {\"tool\": \"test\", \"params\": {}}");
        assert!(result.is_some());
    }

    #[test]
    fn test_parse_tool_call_malformed_json() {
        let (engine, _dir) = tokio::runtime::Runtime::new().unwrap()
            .block_on(setup_test_engine());
        // Missing closing brace — parse should fail and return None
        let result = engine.parse_tool_call("{\"tool\": \"test\", \"params\": {}");
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_tool_call_no_json() {
        let (engine, _dir) = tokio::runtime::Runtime::new().unwrap()
            .block_on(setup_test_engine());
        let result = engine.parse_tool_call("This is a normal response");
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_tool_call_nested_braces() {
        let (engine, _dir) = tokio::runtime::Runtime::new().unwrap()
            .block_on(setup_test_engine());
        let json = "{\"tool\": \"test\", \"params\": {\"nested\": {\"a\": 1}}}";
        let result = engine.parse_tool_call(json);
        assert!(result.is_some());
    }

    #[test]
    fn test_agent_state_defaults_to_idle() {
        let (engine, _dir) = tokio::runtime::Runtime::new().unwrap()
            .block_on(setup_test_engine());
        let rt = tokio::runtime::Runtime::new().unwrap();
        let state = rt.block_on(engine.agent_state());
        assert_eq!(state, AgentState::Idle);
    }
}
```

- [ ] **Step 2: 编译检查**

Run: `cargo check 2>&1`
Expected: 编译通过

- [ ] **Step 3: 运行 engine 测试**

Run: `cargo test -p openloom-engine 2>&1`
Expected: 所有测试 PASS（包括新增的 parse_tool_call 和 agent_state 测试）

- [ ] **Step 4: Commit**

```bash
git add crates/engine/src/lib.rs
git commit -m "feat(engine): integrate Agent Loop, CognitionsPersonaProvider, message history, mid-turn protection"
```

---

### Task 6: Server dispatch 更新

**Files:**
- Modify: `F:/openLoom/crates/server/src/dispatch.rs`

- [ ] **Step 1: 更新 memory.persona、agent.status，新增 session.switch**

`F:/openLoom/crates/server/src/dispatch.rs` — 修改三处：

1. `memory.persona` handler（约第108-110行）替换为：

```rust
"memory.persona" => {
    let summary = engine.persona_summary().await;
    Ok(serde_json::json!({"summary": summary, "traits": []}))
}
```

2. `agent.status` handler（约第114-116行）替换为：

```rust
"agent.status" => {
    let state = engine.agent_state().await;
    Ok(serde_json::json!({"state": state, "active_session": null, "model_info": {"router": "qwen3-1.7b"}}))
}
```

3. 在 `"session.create"` handler 之后新增 `session.switch`：

```rust
"session.switch" => {
    let session_id = params.as_ref()
        .and_then(|p| p.get("session_id"))
        .and_then(|v| v.as_str())
        .unwrap_or("default");
    // Verify session exists
    let sessions = engine.list_sessions().await.map_err(|e| JsonRpcError {
        code: ErrorCode::InternalError,
        message: e.to_string(),
        data: None,
    })?;
    let found = sessions.iter().any(|s| s.id == session_id);
    if found {
        Ok(serde_json::json!({"session_id": session_id}))
    } else {
        // Auto-create if not found
        let session = engine.create_session().await.map_err(|e| JsonRpcError {
            code: ErrorCode::InternalError,
            message: e.to_string(),
            data: None,
        })?;
        Ok(serde_json::json!({"session_id": session.id}))
    }
}
```

- [ ] **Step 2: 编译检查**

Run: `cargo check 2>&1`
Expected: 编译通过

- [ ] **Step 3: 运行全部测试**

Run: `cargo test 2>&1`
Expected: 所有测试 PASS

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/dispatch.rs
git commit -m "feat(server): wire memory.persona/agent.status to real impl, add session.switch"
```

---

### Task 7: 最终验证

- [ ] **Step 1: 运行全部测试**

Run: `cargo test 2>&1`
Expected: 所有测试 PASS

- [ ] **Step 2: clippy 检查**

Run: `cargo clippy -- -D warnings 2>&1`
Expected: 0 warnings

- [ ] **Step 3: fmt 检查**

Run: `cargo fmt --check 2>&1`
Expected: 格式正确（有差异则 `cargo fmt` + 重新验证）

- [ ] **Step 4: release 编译**

Run: `cargo build --release 2>&1`
Expected: release 编译成功

- [ ] **Step 5: 最终 commit**

```bash
git add -A
git commit -m "chore: Phase 2 Milestone B complete — all tests pass, clippy clean, release build"
```

---

## 完成检查清单

- [ ] `cargo test` — 所有测试通过
- [ ] `cargo clippy -- -D warnings` — 零警告
- [ ] `cargo fmt --check` — 格式正确
- [ ] `cargo build --release` — release 编译成功

# Phase 0: Memory Kernel MVP 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 验证"事件→认知"管线可行 — 从对话文本中提取结构化事件，聚合行为模式，产出用户认知画像。

**Architecture:** Rust workspace 包含 3 个 crate：`memory`（事件提取+模式聚合+SQLite存储）、`models`（模型配置类型）、`cli`（CLI入口）。Phase 0 不引入 llama.cpp，事件提取纯用规则引擎。

**Tech Stack:** Rust 1.85+, Tokio, rusqlite (SQLite + FTS5), serde/serde_json, clap, tracing, anyhow, tempfile (test)

---

## 文件结构

```
F:/openLoom/
├── Cargo.toml                         ← workspace root
├── crates/
│   ├── memory/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs                 ← pub mod declarations, prelude
│   │       ├── event.rs               ← Event struct + EventType enum
│   │       ├── extractor.rs           ← RuleBasedExtractor: 文本→事件
│   │       ├── aggregator.rs          ← PatternAggregator: 事件→模式
│   │       ├── store.rs               ← SqliteEventStore: 持久化+查询
│   │       └── pipeline.rs            ← MemoryPipeline: 编排三阶段
│   ├── models/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       └── lib.rs                 ← ModelConfig, ModelType
│   └── cli/
│       ├── Cargo.toml
│       └── src/
│           └── main.rs                ← openloom analyze 命令
└── tests/
    └── memory_pipeline_tests.rs       ← 集成测试: 10场景验证
```

---

### Task 1: 项目脚手架

**Files:**
- Create: `F:/openLoom/Cargo.toml`
- Create: `F:/openLoom/crates/memory/Cargo.toml`
- Create: `F:/openLoom/crates/memory/src/lib.rs`
- Create: `F:/openLoom/crates/models/Cargo.toml`
- Create: `F:/openLoom/crates/models/src/lib.rs`
- Create: `F:/openLoom/crates/cli/Cargo.toml`
- Create: `F:/openLoom/crates/cli/src/main.rs`

- [ ] **Step 1: 创建 workspace 根 Cargo.toml**

```toml
[workspace]
resolver = "2"
members = ["crates/*"]

[workspace.package]
version = "0.1.0"
edition = "2024"
license = "Apache-2.0"
```

- [ ] **Step 2: 创建 memory crate**

`F:/openLoom/crates/memory/Cargo.toml`:
```toml
[package]
name = "openloom-memory"
version.workspace = true
edition.workspace = true

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
rusqlite = { version = "0.32", features = ["bundled", "vtab"] }
anyhow = "1"
tracing = "0.1"
chrono = { version = "0.4", features = ["serde"] }
```

`F:/openLoom/crates/memory/src/lib.rs`:
```rust
pub mod event;
pub mod extractor;
pub mod aggregator;
pub mod store;
pub mod pipeline;
```

- [ ] **Step 3: 创建 models crate**

`F:/openLoom/crates/models/Cargo.toml`:
```toml
[package]
name = "openloom-models"
version.workspace = true
edition.workspace = true

[dependencies]
serde = { version = "1", features = ["derive"] }
```

`F:/openLoom/crates/models/src/lib.rs`:
```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ModelType {
    Router,
    Summarizer,
    Reasoning,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub name: String,
    pub model_type: ModelType,
    pub path: Option<String>,
    pub context_size: usize,
}
```

- [ ] **Step 4: 创建 cli crate**

`F:/openLoom/crates/cli/Cargo.toml`:
```toml
[package]
name = "openloom"
version.workspace = true
edition.workspace = true

[[bin]]
name = "openloom"
path = "src/main.rs"

[dependencies]
openloom-memory = { path = "../memory" }
openloom-models = { path = "../models" }
clap = { version = "4", features = ["derive"] }
anyhow = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
```

`F:/openLoom/crates/cli/src/main.rs`:
```rust
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "openloom", about = "Local-first private AI assistant")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Analyze {
        #[arg(short, long)]
        input: String,
        #[arg(short, long, default_value = "profile.json")]
        output: String,
    },
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("openloom=info")
        .init();

    let cli = Cli::parse();
    match cli.command {
        Commands::Analyze { input, output } => {
            println!("Analyzing {} → {}", input, output);
            println!("Memory pipeline not yet implemented.");
        }
    }
    Ok(())
}
```

- [ ] **Step 5: 构建验证**

```bash
cd F:/openLoom && cargo build
```
Expected: `Compiling openloom-models ... Compiling openloom-memory ... Compiling openloom ... Finished`

- [ ] **Step 6: 运行 CLI 验证**

```bash
cargo run -- analyze --input test.log --output test.json
```
Expected: 打印 "Analyzing test.log → test.json" 和 "Memory pipeline not yet implemented."

- [ ] **Step 7: Commit**

```bash
cd F:/openLoom && git init && git add -A && git commit -m "feat: scaffold openloom workspace with memory, models, and cli crates"
```

---

### Task 2: Event 类型定义

**Files:**
- Create: `F:/openLoom/crates/memory/src/event.rs`

- [ ] **Step 1: 编写 Event 数据结构**

`F:/openLoom/crates/memory/src/event.rs`:
```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    /// 用户重复出现的行为模式
    BehaviorPattern,
    /// 用户明确表达的偏好
    Preference,
    /// 用户陈述的事实信息
    Fact,
    /// 用户与AI的关系变化
    Relationship,
    /// 用户传达的情绪状态
    EmotionalState,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Event {
    pub id: Option<i64>,
    pub timestamp: DateTime<Utc>,
    pub event_type: EventType,
    pub action: String,
    pub context: String,
    pub confidence: f64,
    pub source_session: Option<String>,
    /// 原始对话中触发此事件的文本片段
    pub source_text: String,
    pub payload: Option<serde_json::Value>,
}

impl Event {
    pub fn new(
        event_type: EventType,
        action: impl Into<String>,
        context: impl Into<String>,
        confidence: f64,
        source_text: impl Into<String>,
    ) -> Self {
        Self {
            id: None,
            timestamp: Utc::now(),
            event_type,
            action: action.into(),
            context: context.into(),
            confidence,
            source_session: None,
            source_text: source_text.into(),
            payload: None,
        }
    }

    pub fn with_payload(mut self, payload: serde_json::Value) -> Self {
        self.payload = Some(payload);
        self
    }

    pub fn with_session(mut self, session_id: impl Into<String>) -> Self {
        self.source_session = Some(session_id.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_creation() {
        let event = Event::new(
            EventType::BehaviorPattern,
            "loss_chase",
            "trading",
            0.87,
            "我又加仓了，虽然已经亏了很多",
        );
        assert_eq!(event.action, "loss_chase");
        assert_eq!(event.confidence, 0.87);
        assert!(event.id.is_none());
    }

    #[test]
    fn test_event_json_roundtrip() {
        let event = Event::new(
            EventType::Preference,
            "prefers_short_term",
            "trading_style",
            0.95,
            "我喜欢快进快出",
        );
        let json = serde_json::to_string(&event).unwrap();
        let decoded: Event = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.action, "prefers_short_term");
    }
}
```

- [ ] **Step 2: 运行测试**

```bash
cd F:/openLoom && cargo test -p openloom-memory
```
Expected: 2 tests PASS

- [ ] **Step 3: Commit**

```bash
git add crates/memory/src/event.rs && git commit -m "feat: define Event type with serialization and unit tests"
```

---

### Task 3: RuleBasedExtractor — 规则引擎提取事件

**Files:**
- Create: `F:/openLoom/crates/memory/src/extractor.rs`

- [ ] **Step 1: 编写测试**

```rust
// 在 extractor.rs 末尾添加

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::EventType;

    fn make_extractor() -> RuleBasedExtractor {
        RuleBasedExtractor::with_default_rules()
    }

    #[test]
    fn test_extract_loss_chase_pattern() {
        let extractor = make_extractor();
        let text = "虽然已经亏了30%，但是我觉得还能涨回来，我又加仓了";
        let events = extractor.extract(text, "trading");
        assert!(!events.is_empty());
        let loss_chase = events.iter().find(|e| e.action == "loss_chase");
        assert!(loss_chase.is_some());
        assert!(loss_chase.unwrap().confidence >= 0.7);
    }

    #[test]
    fn test_extract_preference() {
        let extractor = make_extractor();
        let text = "我还是更喜欢用Python写代码，Java太啰嗦了";
        let events = extractor.extract(text, "coding");
        let pref = events.iter().find(|e| e.event_type == EventType::Preference);
        assert!(pref.is_some());
    }

    #[test]
    fn test_no_false_positive() {
        let extractor = make_extractor();
        let text = "今天天气不错，我去公园散了会步";
        let events = extractor.extract(text, "casual");
        // 不应该把日常寒暄当行为模式
        let patterns: Vec<_> = events.iter().filter(|e| e.event_type == EventType::BehaviorPattern).collect();
        assert!(patterns.is_empty(), "casual chat should not produce behavior patterns");
    }

    #[test]
    fn test_emotional_state_detection() {
        let extractor = make_extractor();
        let text = "我今天真的很沮丧，工作上一堆破事，感觉什么都做不好";
        let events = extractor.extract(text, "mood");
        let emotion = events.iter().find(|e| e.event_type == EventType::EmotionalState);
        assert!(emotion.is_some());
    }
}
```

运行: `cargo test -p openloom-memory` — Expected: 4 FAIL (extractor not defined)

- [ ] **Step 2: 实现 RuleBasedExtractor**

```rust
use crate::event::{Event, EventType};
use regex::Regex;

pub struct ExtractionRule {
    pub pattern: Regex,
    pub event_type: EventType,
    pub action: String,
    pub min_confidence: f64,
}

pub struct RuleBasedExtractor {
    rules: Vec<ExtractionRule>,
}

impl RuleBasedExtractor {
    pub fn new(rules: Vec<ExtractionRule>) -> Self {
        Self { rules }
    }

    pub fn with_default_rules() -> Self {
        let rules = vec![
            // 行为模式
            ExtractionRule {
                pattern: Regex::new(r"(亏|跌|赔).*(加仓|补仓|买入|抄底)").unwrap(),
                event_type: EventType::BehaviorPattern,
                action: "loss_chase".into(),
                min_confidence: 0.75,
            },
            ExtractionRule {
                pattern: Regex::new(r"(追高|追涨|涨停.*买)").unwrap(),
                event_type: EventType::BehaviorPattern,
                action: "chase_high".into(),
                min_confidence: 0.75,
            },
            ExtractionRule {
                pattern: Regex::new(r"(不止损|舍不得割|扛着|死扛)").unwrap(),
                event_type: EventType::BehaviorPattern,
                action: "avoid_stop_loss".into(),
                min_confidence: 0.70,
            },
            // 偏好
            ExtractionRule {
                pattern: Regex::new(r"(喜欢|偏好|更爱|倾向).*(短线|快进快出|日内)").unwrap(),
                event_type: EventType::Preference,
                action: "prefers_short_term".into(),
                min_confidence: 0.80,
            },
            ExtractionRule {
                pattern: Regex::new(r"(喜欢|偏好|更爱|倾向).*(长线|价值投资|长期持有)").unwrap(),
                event_type: EventType::Preference,
                action: "prefers_long_term".into(),
                min_confidence: 0.80,
            },
            ExtractionRule {
                pattern: Regex::new(r"(喜欢|偏好|更爱|倾向).*(科技股|成长股|AI|芯片|新能源)").unwrap(),
                event_type: EventType::Preference,
                action: "prefers_tech_stocks".into(),
                min_confidence: 0.80,
            },
            // 通用偏好
            ExtractionRule {
                pattern: Regex::new(r"还是更?(喜欢|习惯|倾向)(用|做|看)").unwrap(),
                event_type: EventType::Preference,
                action: "general_preference".into(),
                min_confidence: 0.65,
            },
            // 情绪
            ExtractionRule {
                pattern: Regex::new(r"(沮丧|失落|难过|伤心|绝望|崩溃)").unwrap(),
                event_type: EventType::EmotionalState,
                action: "negative_emotional".into(),
                min_confidence: 0.70,
            },
            ExtractionRule {
                pattern: Regex::new(r"(开心|兴奋|激动|高兴|爽)").unwrap(),
                event_type: EventType::EmotionalState,
                action: "positive_emotional".into(),
                min_confidence: 0.70,
            },
            ExtractionRule {
                pattern: Regex::new(r"(焦虑|担心|害怕|紧张|不安)").unwrap(),
                event_type: EventType::EmotionalState,
                action: "anxious".into(),
                min_confidence: 0.70,
            },
        ];
        Self::new(rules)
    }

    pub fn extract(&self, text: &str, context: &str) -> Vec<Event> {
        let mut events = Vec::new();
        for rule in &self.rules {
            if rule.pattern.is_match(text) {
                events.push(Event::new(
                    rule.event_type.clone(),
                    rule.action.clone(),
                    context.to_string(),
                    rule.min_confidence,
                    text.to_string(),
                ));
            }
        }
        events
    }
}
```

- [ ] **Step 3: 更新 lib.rs 添加 regex 依赖**

在 `F:/openLoom/crates/memory/Cargo.toml` 的 `[dependencies]` 中添加:
```toml
regex = "1"
```

- [ ] **Step 4: 运行测试**

```bash
cd F:/openLoom && cargo test -p openloom-memory
```
Expected: ALL tests PASS

- [ ] **Step 5: Commit**

```bash
git add crates/memory/Cargo.toml crates/memory/src/extractor.rs && git commit -m "feat: add RuleBasedExtractor with 10 default rules and tests"
```

---

### Task 4: SqliteEventStore — 事件持久化

**Files:**
- Create: `F:/openLoom/crates/memory/src/store.rs`

- [ ] **Step 1: 编写测试**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{Event, EventType};
    use tempfile::tempdir;

    fn make_event(action: &str, confidence: f64) -> Event {
        Event::new(
            EventType::BehaviorPattern,
            action,
            "test_context",
            confidence,
            "test source text",
        )
    }

    #[test]
    fn test_insert_and_query_events() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let mut store = SqliteEventStore::open(&db_path).unwrap();

        let e1 = make_event("loss_chase", 0.87);
        let e2 = make_event("loss_chase", 0.91);
        let e3 = make_event("prefers_tech", 0.80);

        store.insert(&e1).unwrap();
        store.insert(&e2).unwrap();
        store.insert(&e3).unwrap();

        let all = store.query_all(10).unwrap();
        assert_eq!(all.len(), 3);

        let loss_events = store.query_by_action("loss_chase", 10).unwrap();
        assert_eq!(loss_events.len(), 2);

        let count = store.count_by_action("loss_chase").unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn test_event_fts_search() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let mut store = SqliteEventStore::open(&db_path).unwrap();

        let e = make_event("loss_chase", 0.87);
        store.insert(&e).unwrap();

        let results = store.search("loss").unwrap();
        assert!(!results.is_empty());
    }

    #[test]
    fn test_empty_store() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let store = SqliteEventStore::open(&db_path).unwrap();

        assert!(store.query_all(10).unwrap().is_empty());
        assert_eq!(store.count_by_action("anything").unwrap(), 0);
    }
}
```

- [ ] **Step 2: 实现 SqliteEventStore**

```rust
use crate::event::{Event, EventType};
use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use std::path::Path;

pub struct SqliteEventStore {
    conn: Connection,
}

impl SqliteEventStore {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        let mut store = Self { conn };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&mut self) -> Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp TEXT NOT NULL,
                type TEXT NOT NULL,
                action TEXT NOT NULL,
                context TEXT NOT NULL DEFAULT '',
                confidence REAL NOT NULL,
                source_session TEXT,
                source_text TEXT NOT NULL DEFAULT '',
                payload TEXT
            );

            CREATE VIRTUAL TABLE IF NOT EXISTS events_fts
            USING fts5(type, action, context, source_text, content='events', content_rowid='id');

            CREATE TRIGGER IF NOT EXISTS events_ai AFTER INSERT ON events BEGIN
                INSERT INTO events_fts(rowid, type, action, context, source_text)
                VALUES (new.id, new.type, new.action, new.context, new.source_text);
            END;

            CREATE TRIGGER IF NOT EXISTS events_ad AFTER DELETE ON events BEGIN
                INSERT INTO events_fts(events_fts, rowid, type, action, context, source_text)
                VALUES('delete', old.id, old.type, old.action, old.context, old.source_text);
            END;"
        )?;
        Ok(())
    }

    pub fn insert(&mut self, event: &Event) -> Result<i64> {
        let payload = event.payload.as_ref().map(|p| p.to_string());
        self.conn.execute(
            "INSERT INTO events (timestamp, type, action, context, confidence, source_session, source_text, payload)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                event.timestamp.to_rfc3339(),
                event.event_type_as_str(),
                event.action,
                event.context,
                event.confidence,
                event.source_session,
                event.source_text,
                payload,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn query_all(&self, limit: usize) -> Result<Vec<Event>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, timestamp, type, action, context, confidence, source_session, source_text, payload
             FROM events ORDER BY id DESC LIMIT ?1"
        )?;
        let events = stmt.query_map(params![limit as i64], Self::row_to_event)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(events)
    }

    pub fn query_by_action(&self, action: &str, limit: usize) -> Result<Vec<Event>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, timestamp, type, action, context, confidence, source_session, source_text, payload
             FROM events WHERE action = ?1 ORDER BY id DESC LIMIT ?2"
        )?;
        let events = stmt.query_map(params![action, limit as i64], Self::row_to_event)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(events)
    }

    pub fn count_by_action(&self, action: &str) -> Result<usize> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM events WHERE action = ?1",
            params![action],
            |row| row.get(0),
        )?;
        Ok(count as usize)
    }

    pub fn search(&self, query: &str) -> Result<Vec<Event>> {
        let mut stmt = self.conn.prepare(
            "SELECT e.id, e.timestamp, e.type, e.action, e.context, e.confidence,
                    e.source_session, e.source_text, e.payload
             FROM events e
             INNER JOIN events_fts fts ON e.id = fts.rowid
             WHERE events_fts MATCH ?1
             ORDER BY rank
             LIMIT 20"
        )?;
        let events = stmt.query_map(params![query], Self::row_to_event)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(events)
    }

    fn row_to_event(row: &rusqlite::Row) -> rusqlite::Result<Event> {
        let type_str: String = row.get(2)?;
        let event_type = Event::event_type_from_str(&type_str);
        Ok(Event {
            id: Some(row.get(0)?),
            timestamp: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(1)?)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now()),
            event_type,
            action: row.get(3)?,
            context: row.get(4)?,
            confidence: row.get(5)?,
            source_session: row.get(6)?,
            source_text: row.get(7)?,
            payload: row.get::<_, Option<String>>(8)?.and_then(|s| serde_json::from_str(&s).ok()),
        })
    }
}

// 为 Event 添加序列化辅助方法
impl Event {
    pub fn event_type_as_str(&self) -> &str {
        match self.event_type {
            EventType::BehaviorPattern => "behavior_pattern",
            EventType::Preference => "preference",
            EventType::Fact => "fact",
            EventType::Relationship => "relationship",
            EventType::EmotionalState => "emotional_state",
        }
    }

    pub fn event_type_from_str(s: &str) -> EventType {
        match s {
            "behavior_pattern" => EventType::BehaviorPattern,
            "preference" => EventType::Preference,
            "fact" => EventType::Fact,
            "relationship" => EventType::Relationship,
            "emotional_state" => EventType::EmotionalState,
            _ => EventType::Fact,
        }
    }
}
```

- [ ] **Step 3: 更新 Cargo.toml 添加测试依赖**

在 `F:/openLoom/crates/memory/Cargo.toml` 中添加:
```toml
[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 4: 运行测试**

```bash
cd F:/openLoom && cargo test -p openloom-memory
```
Expected: ALL tests PASS (event tests + extractor tests + store tests)

- [ ] **Step 5: Commit**

```bash
git add crates/memory/Cargo.toml crates/memory/src/store.rs crates/memory/src/event.rs && git commit -m "feat: add SqliteEventStore with FTS5 search and tests"
```

---

### Task 5: PatternAggregator — 模式聚合

**Files:**
- Create: `F:/openLoom/crates/memory/src/aggregator.rs`

- [ ] **Step 1: 编写测试**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{Event, EventType};

    fn make_event(action: &str, confidence: f64) -> Event {
        Event::new(EventType::BehaviorPattern, action, "test", confidence, "source")
    }

    #[test]
    fn test_threshold_not_met() {
        let mut agg = PatternAggregator::new(5);
        agg.observe(&make_event("loss_chase", 0.85));
        agg.observe(&make_event("loss_chase", 0.80));
        agg.observe(&make_event("loss_chase", 0.90));
        // 只有 3 次，未达阈值 5
        let triggered = agg.should_trigger("loss_chase");
        assert!(!triggered);
    }

    #[test]
    fn test_threshold_met() {
        let mut agg = PatternAggregator::new(3);
        agg.observe(&make_event("loss_chase", 0.85));
        agg.observe(&make_event("loss_chase", 0.80));
        agg.observe(&make_event("loss_chase", 0.90));
        assert!(agg.should_trigger("loss_chase"));
    }

    #[test]
    fn test_average_confidence() {
        let mut agg = PatternAggregator::new(2);
        agg.observe(&make_event("loss_chase", 0.80));
        agg.observe(&make_event("loss_chase", 0.90));
        let avg = agg.average_confidence("loss_chase");
        assert!(avg.is_some());
        assert!((avg.unwrap() - 0.85).abs() < 0.01);
    }

    #[test]
    fn test_drain_resets_counter() {
        let mut agg = PatternAggregator::new(2);
        agg.observe(&make_event("loss_chase", 0.80));
        agg.observe(&make_event("loss_chase", 0.90));
        assert!(agg.should_trigger("loss_chase"));
        agg.drain("loss_chase");
        assert!(!agg.should_trigger("loss_chase"));
    }

    #[test]
    fn test_list_active_patterns() {
        let mut agg = PatternAggregator::new(2);
        agg.observe(&make_event("loss_chase", 0.80));
        agg.observe(&make_event("loss_chase", 0.90));
        agg.observe(&make_event("chase_high", 0.85));

        let active = agg.active_patterns();
        assert!(active.contains(&"loss_chase".to_string()));
        // chase_high only has 1, below threshold
        assert!(!active.contains(&"chase_high".to_string()));
    }
}
```

- [ ] **Step 2: 实现 PatternAggregator**

```rust
use crate::event::Event;
use std::collections::HashMap;

#[derive(Debug, Clone)]
struct PatternCount {
    count: usize,
    confidence_sum: f64,
}

pub struct PatternAggregator {
    threshold: usize,
    patterns: HashMap<String, PatternCount>,
}

impl PatternAggregator {
    pub fn new(threshold: usize) -> Self {
        Self {
            threshold,
            patterns: HashMap::new(),
        }
    }

    pub fn observe(&mut self, event: &Event) {
        let entry = self.patterns.entry(event.action.clone()).or_insert(PatternCount {
            count: 0,
            confidence_sum: 0.0,
        });
        entry.count += 1;
        entry.confidence_sum += event.confidence;
    }

    pub fn count(&self, action: &str) -> usize {
        self.patterns.get(action).map(|p| p.count).unwrap_or(0)
    }

    pub fn should_trigger(&self, action: &str) -> bool {
        self.patterns
            .get(action)
            .map(|p| p.count >= self.threshold)
            .unwrap_or(false)
    }

    pub fn average_confidence(&self, action: &str) -> Option<f64> {
        self.patterns.get(action).map(|p| {
            if p.count == 0 {
                0.0
            } else {
                p.confidence_sum / p.count as f64
            }
        })
    }

    pub fn drain(&mut self, action: &str) -> Option<(usize, f64)> {
        self.patterns.remove(action).map(|p| {
            let avg = if p.count == 0 { 0.0 } else { p.confidence_sum / p.count as f64 };
            (p.count, avg)
        })
    }

    pub fn active_patterns(&self) -> Vec<String> {
        self.patterns
            .iter()
            .filter(|(_, p)| p.count >= self.threshold)
            .map(|(k, _)| k.clone())
            .collect()
    }
}
```

- [ ] **Step 3: 运行测试**

```bash
cd F:/openLoom && cargo test -p openloom-memory
```
Expected: ALL tests PASS

- [ ] **Step 4: Commit**

```bash
git add crates/memory/src/aggregator.rs && git commit -m "feat: add PatternAggregator with threshold-based triggering"
```

---

### Task 6: MemoryPipeline — 端到端编排

**Files:**
- Create: `F:/openLoom/crates/memory/src/pipeline.rs`

- [ ] **Step 1: 编写测试**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::EventType;
    use crate::extractor::RuleBasedExtractor;
    use crate::aggregator::PatternAggregator;
    use tempfile::tempdir;

    #[test]
    fn test_pipeline_end_to_end() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        let extractor = RuleBasedExtractor::with_default_rules();
        let mut aggregator = PatternAggregator::new(3);
        let mut store = crate::store::SqliteEventStore::open(&db_path).unwrap();

        let mut pipeline = MemoryPipeline::new(extractor, aggregator, store, 3);

        // 模拟 3 次"亏损后加仓"对话
        let sessions = vec![
            ("session_1", "亏了20%我还加仓了，我觉得会涨回来"),
            ("session_2", "又跌了，但我还是补了点仓"),
            ("session_3", "这次真亏麻了，但我不甘心又买了"),
        ];

        let mut triggered = Vec::new();
        for (session_id, text) in &sessions {
            let result = pipeline.process(session_id, text, "trading").unwrap();
            if let Some(cog) = result.cognition_triggered {
                triggered.push(cog);
            }
        }

        // 第 3 次应该触发认知更新
        assert_eq!(triggered.len(), 1);
        let cog = &triggered[0];
        assert_eq!(cog.action, "loss_chase");
        assert!(cog.confidence >= 0.7);

        // 事件应该已存储
        let stored = pipeline.store().query_all(10).unwrap();
        assert_eq!(stored.len(), 3);
    }

    #[test]
    fn test_pipeline_no_trigger_below_threshold() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        let extractor = RuleBasedExtractor::with_default_rules();
        let aggregator = PatternAggregator::new(5); // threshold 5
        let store = crate::store::SqliteEventStore::open(&db_path).unwrap();

        let mut pipeline = MemoryPipeline::new(extractor, aggregator, store, 5);

        let result = pipeline.process("s1", "亏了10%我又加仓了", "trading").unwrap();
        assert!(result.cognition_triggered.is_none());
        assert!(!result.events.is_empty());
    }
}
```

- [ ] **Step 2: 实现 MemoryPipeline**

```rust
use crate::aggregator::PatternAggregator;
use crate::event::Event;
use crate::extractor::RuleBasedExtractor;
use crate::store::SqliteEventStore;
use anyhow::Result;

#[derive(Debug, Clone)]
pub struct CognitionUpdate {
    pub action: String,
    pub trait_name: String,
    pub evidence_count: usize,
    pub confidence: f64,
    pub summary: String,
}

#[derive(Debug)]
pub struct PipelineResult {
    pub events: Vec<Event>,
    pub cognition_triggered: Option<CognitionUpdate>,
}

pub struct MemoryPipeline {
    extractor: RuleBasedExtractor,
    aggregator: PatternAggregator,
    store: SqliteEventStore,
}

impl MemoryPipeline {
    pub fn new(
        extractor: RuleBasedExtractor,
        aggregator: PatternAggregator,
        store: SqliteEventStore,
        _threshold: usize,
    ) -> Self {
        Self { extractor, aggregator, store }
    }

    pub fn process(
        &mut self,
        session_id: &str,
        text: &str,
        context: &str,
    ) -> Result<PipelineResult> {
        // 阶段 1: 事件提取
        let mut events = self.extractor.extract(text, context);
        for event in &mut events {
            event.source_session = Some(session_id.to_string());
        }

        // 存储事件
        let mut event_ids = Vec::new();
        for event in &events {
            let id = self.store.insert(event)?;
            event_ids.push(id);
        }

        // 更新事件 ID
        for (event, id) in events.iter_mut().zip(event_ids) {
            event.id = Some(id);
        }

        // 阶段 2: 模式聚合
        let mut cognition = None;
        for event in &events {
            self.aggregator.observe(event);
            if self.aggregator.should_trigger(&event.action) {
                let (count, avg_conf) = self.aggregator.drain(&event.action).unwrap_or_default();
                cognition = Some(CognitionUpdate {
                    action: event.action.clone(),
                    trait_name: self.action_to_trait(&event.action),
                    evidence_count: count,
                    confidence: avg_conf,
                    summary: self.generate_summary(&event.action, count, avg_conf),
                });
            }
        }

        Ok(PipelineResult {
            events,
            cognition_triggered: cognition,
        })
    }

    pub fn store(&self) -> &SqliteEventStore {
        &self.store
    }

    fn action_to_trait(&self, action: &str) -> String {
        match action {
            "loss_chase" => "risk_tendency".into(),
            "chase_high" => "entry_timing".into(),
            "avoid_stop_loss" => "risk_management".into(),
            "prefers_short_term" => "trading_style".into(),
            "prefers_long_term" => "trading_style".into(),
            "prefers_tech_stocks" => "sector_preference".into(),
            "negative_emotional" => "emotional_state".into(),
            "positive_emotional" => "emotional_state".into(),
            "anxious" => "emotional_state".into(),
            _ => "general_behavior".into(),
        }
    }

    fn generate_summary(&self, action: &str, count: usize, confidence: f64) -> String {
        match action {
            "loss_chase" => format!(
                "用户存在赌徒补仓倾向：在亏损状态下多次加仓（{}次观察，置信度{:.0}%）",
                count, confidence * 100.0
            ),
            "chase_high" => format!(
                "用户有追高行为模式：在股价上涨时追买（{}次观察，置信度{:.0}%）",
                count, confidence * 100.0
            ),
            "avoid_stop_loss" => format!(
                "用户倾向于不止损：面对亏损选择扛单（{}次观察，置信度{:.0}%）",
                count, confidence * 100.0
            ),
            "prefers_short_term" => format!(
                "用户偏好短线交易风格（{}次表达，置信度{:.0}%）",
                count, confidence * 100.0
            ),
            "prefers_long_term" => format!(
                "用户偏好长线价值投资（{}次表达，置信度{:.0}%）",
                count, confidence * 100.0
            ),
            "prefers_tech_stocks" => format!(
                "用户偏好科技/成长股投资（{}次表达，置信度{:.0}%）",
                count, confidence * 100.0
            ),
            "negative_emotional" => format!(
                "用户在交易中出现负面情绪（{}次观察，置信度{:.0}%）",
                count, confidence * 100.0
            ),
            "positive_emotional" => format!(
                "用户在交易中表现出正面情绪（{}次观察，置信度{:.0}%）",
                count, confidence * 100.0
            ),
            "anxious" => format!(
                "用户对市场波动表现出焦虑情绪（{}次观察，置信度{:.0}%）",
                count, confidence * 100.0
            ),
            _ => format!("检测到行为模式: {}（{}次观察，置信度{:.0}%）", action, count, confidence * 100.0),
        }
    }
}
```

- [ ] **Step 3: 运行测试**

```bash
cd F:/openLoom && cargo test -p openloom-memory
```
Expected: ALL tests PASS

- [ ] **Step 4: Commit**

```bash
git add crates/memory/src/pipeline.rs && git commit -m "feat: add MemoryPipeline orchestrating extract→aggregate→store"
```

---

### Task 7: CLI analyze 命令 — 端到端可运行

**Files:**
- Modify: `F:/openLoom/crates/cli/Cargo.toml`
- Modify: `F:/openLoom/crates/cli/src/main.rs`

- [ ] **Step 1: 更新 CLI 依赖**

在 `F:/openLoom/crates/cli/Cargo.toml` 的 `[dependencies]` 中添加:
```toml
serde_json = "1"
```

- [ ] **Step 2: 实现 analyze 命令**

`F:/openLoom/crates/cli/src/main.rs`:
```rust
use clap::{Parser, Subcommand};
use openloom_memory::aggregator::PatternAggregator;
use openloom_memory::extractor::RuleBasedExtractor;
use openloom_memory::pipeline::MemoryPipeline;
use openloom_memory::store::SqliteEventStore;
use std::fs;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "openloom", about = "Local-first private AI assistant")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Analyze {
        #[arg(short, long)]
        input: String,
        #[arg(short, long, default_value = "profile.json")]
        output: String,
        #[arg(short, long, default_value = "memory.db")]
        db: String,
        #[arg(short = 't', long, default_value = "3")]
        threshold: usize,
    },
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("openloom=info")
        .init();

    let cli = Cli::parse();
    match cli.command {
        Commands::Analyze { input, output, db, threshold } => {
            run_analyze(&input, &output, &db, threshold)?;
        }
    }
    Ok(())
}

fn run_analyze(input_path: &str, output_path: &str, db_path: &str, threshold: usize) -> anyhow::Result<()> {
    // 读取输入文件（每行一个对话片段，格式: session_id|context|text）
    let content = fs::read_to_string(input_path)?;

    let extractor = RuleBasedExtractor::with_default_rules();
    let aggregator = PatternAggregator::new(threshold);
    let db_file = PathBuf::from(db_path);
    let _ = fs::remove_file(&db_file);
    let store = SqliteEventStore::open(&db_file)?;

    let mut pipeline = MemoryPipeline::new(extractor, aggregator, store, threshold);

    let mut all_cognitions: Vec<serde_json::Value> = Vec::new();
    let mut total_events = 0;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let parts: Vec<&str> = line.splitn(3, '|').collect();
        if parts.len() < 3 {
            eprintln!("Skipping malformed line: {}", line);
            continue;
        }

        let (session_id, context, text) = (parts[0], parts[1], parts[2]);

        match pipeline.process(session_id, text, context) {
            Ok(result) => {
                total_events += result.events.len();
                if let Some(cog) = result.cognition_triggered {
                    println!("COGNITION TRIGGERED: {} ({})", cog.summary, cog.trait_name);
                    all_cognitions.push(serde_json::json!({
                        "trait": cog.trait_name,
                        "action": cog.action,
                        "evidence_count": cog.evidence_count,
                        "confidence": cog.confidence,
                        "summary": cog.summary,
                    }));
                }
            }
            Err(e) => {
                eprintln!("Error processing line: {}", e);
            }
        }
    }

    // 输出认知画像
    let profile = serde_json::json!({
        "total_events": total_events,
        "cognitions": all_cognitions,
        "generated_at": chrono::Utc::now().to_rfc3339(),
    });

    fs::write(output_path, serde_json::to_string_pretty(&profile)?)?;
    println!("\nProfile written to {}", output_path);
    println!("Total events extracted: {}", total_events);
    println!("Cognitions discovered: {}", all_cognitions.len());

    Ok(())
}
```

- [ ] **Step 3: 在 memory crate 中添加 chrono 的 Utc 依赖**

确认 `F:/openLoom/crates/memory/Cargo.toml` 已有 `chrono`:
```toml
chrono = { version = "0.4", features = ["serde"] }
```

同时在 `F:/openLoom/crates/cli/Cargo.toml` 添加:
```toml
chrono = "0.4"
```

- [ ] **Step 4: 创建测试用的对话数据**

`F:/openLoom/test_data/sample_chat.log`:
```
session_1|trading|亏了30%我真的好难受，但是我觉得到底了，又加仓了
session_2|trading|今天又跌了5个点，不过我觉得是洗盘，补了点
session_3|trading|我已经亏了快一半了，但是我不甘心就这么走，又买了一手
session_1|coding|我还是更喜欢用Rust写代码，Python虽然方便但类型太松了
session_2|coding|这个bug改了一整天，真的很沮丧，感觉自己好蠢
session_3|coding|终于跑通了！太开心了，Rust的编译器虽然严格但是真的帮了大忙
session_1|mood|最近科技股涨疯了，我追了AI芯片的票，希望不要站岗
session_2|mood|每天晚上睡不好，一直担心持仓会不会崩
```

- [ ] **Step 5: 运行端到端测试**

```bash
cd F:/openLoom && cargo run -- analyze --input test_data/sample_chat.log --output test_data/profile.json
```
Expected: 看到 "COGNITION TRIGGERED: 用户存在赌徒补仓倾向..." 和 "Profile written to..."

检查输出:
```bash
cat test_data/profile.json
```
验证包含 `cognitions` 数组且至少有 loss_chase 的认知。

- [ ] **Step 6: Commit**

```bash
git add crates/cli/Cargo.toml crates/cli/src/main.rs test_data/ && git commit -m "feat: implement openloom analyze CLI command with end-to-end pipeline"
```

---

### Task 8: 集成测试 — 10 场景验证

**Files:**
- Create: `F:/openLoom/tests/memory_pipeline_tests.rs`

- [ ] **Step 1: 编写 10 个场景的集成测试**

```rust
use openloom_memory::aggregator::PatternAggregator;
use openloom_memory::extractor::RuleBasedExtractor;
use openloom_memory::pipeline::MemoryPipeline;
use openloom_memory::store::SqliteEventStore;
use tempfile::tempdir;

fn setup_pipeline(threshold: usize) -> (MemoryPipeline, tempfile::TempDir) {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let extractor = RuleBasedExtractor::with_default_rules();
    let aggregator = PatternAggregator::new(threshold);
    let store = SqliteEventStore::open(&db_path).unwrap();
    let pipeline = MemoryPipeline::new(extractor, aggregator, store, threshold);
    (pipeline, dir)
}

fn feed_sessions(pipeline: &mut MemoryPipeline, sessions: &[(&str, &str, &str)]) -> Vec<String> {
    let mut triggered = Vec::new();
    for (sid, ctx, text) in sessions {
        let result = pipeline.process(sid, text, ctx).unwrap();
        if let Some(cog) = result.cognition_triggered {
            triggered.push(cog.summary);
        }
    }
    triggered
}

#[test]
fn scenario_1_loss_chase_detection() {
    // 场景：用户连续多次亏损加仓
    let (mut pipeline, _dir) = setup_pipeline(3);
    let sessions = vec![
        ("s1", "trading", "亏了20%我又加仓了，我觉得到底了"),
        ("s2", "trading", "已经连续跌了一周，但我还是补仓了"),
        ("s3", "trading", "又跌了，不甘心又买了一些"),
    ];
    let triggered = feed_sessions(&mut pipeline, &sessions);
    assert!(!triggered.is_empty(), "应该检测到loss_chase模式");
    assert!(triggered[0].contains("赌徒补仓"), "应该识别为赌徒补仓倾向");
}

#[test]
fn scenario_2_no_pattern_in_casual_chat() {
    // 场景：日常聊天不应产生行为模式
    let (mut pipeline, _dir) = setup_pipeline(2);
    let sessions = vec![
        ("s1", "casual", "今天天气真不错"),
        ("s2", "casual", "中午吃了碗面"),
        ("s3", "casual", "周末打算去爬山"),
    ];
    let triggered = feed_sessions(&mut pipeline, &sessions);
    assert!(triggered.is_empty(), "日常寒暄不应触发任何认知");
}

#[test]
fn scenario_3_trading_style_preference() {
    // 场景：用户表达交易风格偏好
    let (mut pipeline, _dir) = setup_pipeline(1);
    let result = pipeline.process("s1", "我更喜欢短线交易，快进快出比较刺激", "trading").unwrap();
    let has_pref = result.events.iter().any(|e| e.action == "prefers_short_term");
    assert!(has_pref, "应该检测到短线交易偏好");
}

#[test]
fn scenario_4_emotional_state_tracking() {
    // 场景：情绪识别
    let (mut pipeline, _dir) = setup_pipeline(1);
    let result = pipeline.process("s1", "今天真的很沮丧，感觉做什么都不顺", "mood").unwrap();
    let has_emotion = result.events.iter().any(|e| e.action == "negative_emotional");
    assert!(has_emotion, "应该检测到负面情绪");
}

#[test]
fn scenario_5_mixed_signals() {
    // 场景：同一会话多种信号
    let (mut pipeline, _dir) = setup_pipeline(1);
    let result = pipeline.process("s1", "我很喜欢科技股，这次AI芯片又追高了，亏了很多很难受", "trading").unwrap();
    let actions: Vec<&str> = result.events.iter().map(|e| e.action.as_str()).collect();
    assert!(actions.contains(&"prefers_tech_stocks"), "应检测到科技股偏好");
    assert!(actions.contains(&"chase_high"), "应检测到追高行为");
    assert!(actions.contains(&"negative_emotional"), "应检测到负面情绪");
}

#[test]
fn scenario_6_threshold_independence() {
    // 场景：不同的阈值产生不同结果
    let (mut pipeline_low, _dir) = setup_pipeline(1);
    let (mut pipeline_high, _dir2) = setup_pipeline(10);

    let sessions = vec![
        ("s1", "trading", "亏了10%我又加了"),
        ("s2", "trading", "又跌了我又补了"),
    ];

    let triggered_low = feed_sessions(&mut pipeline_low, &sessions);
    let triggered_high = feed_sessions(&mut pipeline_high, &sessions);

    // 低阈值每 1 次就触发，所以 2 次触发；高阈值 10 次，不触发
    assert!(!triggered_low.is_empty(), "低阈值应该触发");
    assert!(triggered_high.is_empty(), "高阈值不应触发");
}

#[test]
fn scenario_7_coding_style_preference() {
    // 场景：编程风格偏好
    let (mut pipeline, _dir) = setup_pipeline(1);
    let result = pipeline.process("s1", "我还是更喜欢用Rust写后端，Go的error handling太啰嗦了", "coding").unwrap();
    let has_pref = result.events.iter().any(|e| e.action == "general_preference");
    assert!(has_pref, "应检测到通用偏好");
}

#[test]
fn scenario_8_anxiety_detection() {
    // 场景：焦虑情绪检测
    let (mut pipeline, _dir) = setup_pipeline(1);
    let result = pipeline.process("s1", "我最近总是睡不好，一直担心市场会崩盘", "mood").unwrap();
    let has_anxiety = result.events.iter().any(|e| e.action == "anxious");
    assert!(has_anxiety, "应检测到焦虑情绪");
}

#[test]
fn scenario_9_empty_input() {
    // 场景：空输入不崩溃
    let (mut pipeline, _dir) = setup_pipeline(1);
    let result = pipeline.process("s1", "", "empty").unwrap();
    assert!(result.events.is_empty());
    assert!(result.cognition_triggered.is_none());
}

#[test]
fn scenario_10_sqlite_persistence() {
    // 场景：事件可跨会话持久化查询
    let (mut pipeline, _dir) = setup_pipeline(1);

    pipeline.process("s1", "亏了30%又加仓了", "trading").unwrap();
    pipeline.process("s2", "又跌了我又补了仓", "trading").unwrap();

    // 查询存储的事件
    let all = pipeline.store().query_all(10).unwrap();
    assert_eq!(all.len(), 2);
    let count = pipeline.store().count_by_action("loss_chase").unwrap();
    assert_eq!(count, 2);
}
```

- [ ] **Step 2: 创建集成测试的 Cargo 配置**

在 `F:/openLoom/Cargo.toml` 中添加:
```toml
[dev-dependencies]
tempfile = "3"
```

同时在 `F:/openLoom/crates/memory/Cargo.toml` 确认已有:
```toml
[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 3: 运行全部集成测试**

```bash
cd F:/openLoom && cargo test
```
Expected: 10 个场景测试 ALL PASS + 所有单元测试 PASS

- [ ] **Step 4: Commit**

```bash
git add tests/ Cargo.toml && git commit -m "test: add 10 scenario integration tests for memory pipeline"
```

---

### Task 9: 最终验证与清理

- [ ] **Step 1: 运行完整测试套件**

```bash
cd F:/openLoom && cargo test && cargo clippy -- -D warnings && cargo fmt --check
```
Expected: 全部 PASS

- [ ] **Step 2: 运行示例端到端流程**

```bash
cargo run -- analyze --input test_data/sample_chat.log --output test_data/profile.json --threshold 3
```

- [ ] **Step 3: 验证输出 JSON 结构正确**

```bash
cat test_data/profile.json
```
Expected 包含:
- `total_events` > 0
- `cognitions` 数组至少包含 loss_chase 的认知
- `generated_at` 有效时间戳

- [ ] **Step 4: 运行 release 构建**

```bash
cargo build --release
```
Expected: `target/release/openloom` 二进制生成，大小 < 10MB

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "chore: final cleanup and verification of Phase 0 MVP"
```

---

## 自我审查

**Spec 覆盖检查:**
- ✅ Event Extractor (RuleBasedExtractor) — Task 3
- ✅ SQLite Event Store — Task 4
- ✅ Pattern Aggregator — Task 5
- ✅ 手动认知更新 (CognitionUpdate) — Task 6
- ✅ CLI 原型 (`openloom analyze`) — Task 7
- ✅ 10 个预设场景验证 — Task 8
- ✅ FTS5 全文索引 — Task 4 (migration)
- ✅ 端到端流程 — Task 7 + 8

**占位符扫描:** 无 TBD/TODO，所有步骤包含实际代码。

**类型一致性:**
- `Event` 定义在 `event.rs`，所有其他文件引用 `crate::event::Event` ✓
- `EventType` 枚举值与 `extractor.rs` 中的规则匹配 ✓
- `CognitionUpdate` 在 `pipeline.rs` 定义，CLI 中使用一致的字段 ✓
- `MemoryPipeline::new` 签名在所有任务中一致 ✓

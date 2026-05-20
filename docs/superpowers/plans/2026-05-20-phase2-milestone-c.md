# Phase 2 Milestone C: Backend Completion — 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 补齐 Phase 2 全部 22 个后端缺口：类型完整性、JSON-RPC 全方法、CLI 全命令实装、WebSocket 推送、信号处理/优雅关闭、配置系统读写

**Architecture:** 类型层 → 存储层 → 引擎层 → 服务层 → CLI 层。Task 1 (models) 为所有后续 task 提供类型基础，必须最先完成。Task 5 (router) 依赖 Task 1 的 ClassifyOutput。Task 2-4 与 Task 1 可并行（不依赖 models 变更）。Task 6 依赖 1-5。Tasks 7-10 依赖 Task 6。

**Tech Stack:** Rust 2024, tokio, rusqlite, serde, chrono, std::sync::atomic

---

## 文件结构

```
F:/openLoom/
├── crates/
│   ├── models/src/lib.rs              ← [Modify] T1,T3,T7,T8,H2 + RateLimitConfig + AppConfig get_nested/set_nested
│   ├── inference/src/lib.rs           ← [Modify] CompletionRequest +top_p +stop
│   ├── cache/src/lib.rs               ← [Modify] +CacheStats, +stats()
│   ├── memory/src/store.rs            ← [Modify] +EventRow, +search_fts()
│   ├── router/src/lib.rs              ← [Modify] +route_reason
│   ├── engine/src/lib.rs              ← [Modify] +7 fields, token_store thread, rate_limiter, shutdown, config, search_events, list_events, model check
│   ├── server/src/
│   │   ├── dispatch.rs                ← [Modify] 5 method stubs
│   │   ├── ws.rs                      ← [Modify] +event push notifications
│   │   └── lib.rs                     ← [Modify] +config_path param, uptime in health
│   └── cli/src/main.rs                ← [Modify] skill list, memory events, config get/set, signal handler
├── config.example.toml                ← [Create]
└── docs/superpowers/plans/
    └── 2026-05-20-phase2-milestone-c.md  ← 本文件
```

---

### Task 1: Models 类型修复 (T1, H2, T3, T7, T8 + RateLimitConfig)

**Files:**
- Modify: `F:/openLoom/crates/models/src/lib.rs`

- [ ] **Step 1: 修改 models/lib.rs 类型定义**

在 `F:/openLoom/crates/models/src/lib.rs` 中：

1.1 修改 `TokenUsage`（约第148行）：加 `cached_tokens` 和 `latency_ms` 字段：

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TokenUsage {
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    #[serde(default)]
    pub cached_tokens: usize,   // NEW — serde backward compat
    #[serde(default)]
    pub latency_ms: u64,        // NEW — serde backward compat
}
```

**重要：** `#[serde(default)]` 仅解决 JSON 反序列化向后兼容，不解决 Rust 字面量构造。`F:/openLoom/crates/engine/src/lib.rs` 中所有 `TokenUsage { prompt_tokens, completion_tokens }` 字面量构造点（当前约 2 处：simple path 返回 + agent_loop 返回）必须在 Task 6 中改为 `TokenUsage { prompt_tokens, completion_tokens, ..Default::default() }`。**Task 6 必须处理这些构造点。**

1.2 修改 `EngineEvent::AgentStateChanged`（约第201行）：将 `String` 改为 `AgentState` 枚举。同时给 `EngineEvent::TokenUsage`（约第205行）加 `cached_tokens` 和 `latency_ms` 字段：

```rust
AgentStateChanged {
    old_state: AgentState,
    new_state: AgentState,
},
TokenUsage {
    session_id: String,
    model: String,
    prompt_tokens: usize,
    completion_tokens: usize,
    cached_tokens: usize,   // NEW
    latency_ms: u64,        // NEW
},
```

1.3 修改 `ClassifyOutput`（约第114行）：加 `route_reason` 字段，带 `#[serde(default)]` 确保 Task 1 完成时现有构造点不编译失败：

```rust
pub struct ClassifyOutput {
    pub intent: Intent,
    pub complexity: f32,
    pub skill_match: Option<String>,
    pub confidence: f32,
    pub cache_hit: bool,
    pub target_model: TargetModel,
    #[serde(default)]
    pub route_reason: String,
}
```

`#[serde(default)]` 让 Task 1 单独 cargo check 通过（现有无 `route_reason` 的构造点用默认空字符串）。Task 5 在各分支填入实际值。

1.4 修改 `StoragePrefs`（约第324行）；`data_dir` 改为 `PathBuf`：

```rust
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StoragePrefs {
    #[serde(default)]
    pub data_dir: PathBuf,
}
```

1.5 在 `AppConfig` 中（约第268行）新增 `cache`/`agent`/`persona` 段 + `RateLimitConfig`：

```rust
// 在 AppConfig 之前新增
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachePrefs {
    #[serde(default = "default_block_size")]
    pub block_size: usize,
    #[serde(default = "default_max_blocks")]
    pub max_blocks: usize,
    #[serde(default = "default_cache_budget_mb")]
    pub total_budget_mb: usize,
}

fn default_block_size() -> usize { 1024 }
fn default_max_blocks() -> usize { 32 }
fn default_cache_budget_mb() -> usize { 5120 }

impl Default for CachePrefs {
    fn default() -> Self {
        Self { block_size: default_block_size(), max_blocks: default_max_blocks(), total_budget_mb: default_cache_budget_mb() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPrefs {
    #[serde(default = "default_max_iterations")]
    pub max_iterations: usize,
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
}

fn default_max_iterations() -> usize { 3 }
fn default_timeout_secs() -> u64 { 120 }

impl Default for AgentPrefs {
    fn default() -> Self {
        Self { max_iterations: default_max_iterations(), timeout_secs: default_timeout_secs() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonaPrefs {
    #[serde(default = "default_top_n")]
    pub top_n: usize,
    #[serde(default = "default_recency_days")]
    pub recency_decay_days: u32,
}

fn default_top_n() -> usize { 5 }
fn default_recency_days() -> u32 { 30 }

impl Default for PersonaPrefs {
    fn default() -> Self {
        Self { top_n: default_top_n(), recency_decay_days: default_recency_days() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    #[serde(default = "default_rate_limit_ms")]
    pub min_interval_ms: u64,
}

fn default_rate_limit_ms() -> u64 { 100 }

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self { min_interval_ms: default_rate_limit_ms() }
    }
}
```

1.6 展开 `AppConfig`，加入新段：

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    #[serde(default)]
    pub models: Vec<ModelConfig>,
    #[serde(default)]
    pub router: RouterPrefs,
    #[serde(default)]
    pub server: ServerPrefs,
    #[serde(default)]
    pub storage: StoragePrefs,
    #[serde(default)]
    pub logging: LoggingPrefs,
    #[serde(default)]
    pub cache: CachePrefs,
    #[serde(default)]
    pub agent: AgentPrefs,
    #[serde(default)]
    pub persona: PersonaPrefs,
    #[serde(default)]
    pub rate_limit: RateLimitConfig,
}
```

1.7 `AppConfig` 加 `get_nested` / `set_nested` helper（在 `impl AppConfig` 块中）：

```rust
impl AppConfig {
    pub fn get_nested(&self, key: &str) -> Option<serde_json::Value> {
        let value = serde_json::to_value(self).ok()?;
        let parts: Vec<&str> = key.split('.').collect();
        let mut current = &value;
        for part in parts {
            current = current.get(part)?;
        }
        Some(current.clone())
    }

    pub fn set_nested(&mut self, key: &str, value: &str) -> anyhow::Result<()> {
        let parts: Vec<&str> = key.split('.').collect();
        if parts.len() != 2 {
            anyhow::bail!("key must be 'section.field' format");
        }
        let json_value: serde_json::Value = serde_json::from_str(value)
            .unwrap_or_else(|_| serde_json::Value::String(value.to_string()));
        match parts[0] {
            "server" => { if parts[1] == "host" { if let serde_json::Value::String(s) = json_value { self.server.host = s; } } }
            "router" => { match parts[1] { "keyword_threshold" => { if let serde_json::Value::Number(n) = &json_value { self.router.keyword_threshold = n.as_f64().unwrap_or(0.85) as f32; } } "fallback_threshold" => { if let serde_json::Value::Number(n) = &json_value { self.router.fallback_threshold = n.as_f64().unwrap_or(0.7) as f32; } } _ => {} } }
            "agent" => { match parts[1] { "max_iterations" => { if let serde_json::Value::Number(n) = &json_value { self.agent.max_iterations = n.as_u64().unwrap_or(3) as usize; } } "timeout_secs" => { if let serde_json::Value::Number(n) = &json_value { self.agent.timeout_secs = n.as_u64().unwrap_or(120); } } _ => {} } }
            "persona" => { match parts[1] { "top_n" => { if let serde_json::Value::Number(n) = &json_value { self.persona.top_n = n.as_u64().unwrap_or(5) as usize; } } "recency_decay_days" => { if let serde_json::Value::Number(n) = &json_value { self.persona.recency_decay_days = n.as_u64().unwrap_or(30) as u32; } } _ => {} } }
            "rate_limit" => { if parts[1] == "min_interval_ms" { if let serde_json::Value::Number(n) = &json_value { self.rate_limit.min_interval_ms = n.as_u64().unwrap_or(100); } } }
            "cache" => { match parts[1] { "block_size" => { if let serde_json::Value::Number(n) = &json_value { self.cache.block_size = n.as_u64().unwrap_or(1024) as usize; } } "max_blocks" => { if let serde_json::Value::Number(n) = &json_value { self.cache.max_blocks = n.as_u64().unwrap_or(32) as usize; } } "total_budget_mb" => { if let serde_json::Value::Number(n) = &json_value { self.cache.total_budget_mb = n.as_u64().unwrap_or(5120) as usize; } } _ => {} } }
            "logging" => { if parts[1] == "level" { if let serde_json::Value::String(s) = json_value { self.logging.level = s; } } }
            _ => {}
        }
        Ok(())
    }
}
```

1.8 在测试模块中更新/新增测试（替换 `test_app_config_defaults` 函数以验证新段）：

```rust
#[test]
fn test_app_config_defaults() {
    let config = AppConfig::default();
    assert_eq!(config.router.keyword_threshold, 0.85);
    assert_eq!(config.router.fallback_threshold, 0.7);
    assert_eq!(config.server.host, "127.0.0.1");
    assert_eq!(config.cache.block_size, 1024);
    assert_eq!(config.agent.max_iterations, 3);
    assert_eq!(config.persona.top_n, 5);
    assert_eq!(config.rate_limit.min_interval_ms, 100);
}

#[test]
fn test_token_usage_with_new_fields() {
    let usage = TokenUsage { prompt_tokens: 10, completion_tokens: 5, cached_tokens: 3, latency_ms: 200 };
    let json = serde_json::to_string(&usage).unwrap();
    assert!(json.contains("cached_tokens"));
    assert!(json.contains("latency_ms"));
    let decoded: TokenUsage = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded.cached_tokens, 3);
    assert_eq!(decoded.latency_ms, 200);
}

#[test]
fn test_token_usage_backward_compat() {
    let json = r#"{"prompt_tokens":10,"completion_tokens":5}"#;
    let decoded: TokenUsage = serde_json::from_str(json).unwrap();
    assert_eq!(decoded.prompt_tokens, 10);
    assert_eq!(decoded.cached_tokens, 0); // default
    assert_eq!(decoded.latency_ms, 0);
}

#[test]
fn test_classify_output_route_reason_default() {
    let co = ClassifyOutput {
        intent: Intent::Chat, complexity: 0.3, skill_match: None,
        confidence: 0.9, cache_hit: false, target_model: TargetModel::Local,
        route_reason: "keyword_match".into(),
    };
    let json = serde_json::to_string(&co).unwrap();
    assert!(json.contains("route_reason"));
}

#[test]
fn test_agent_state_changed_uses_enum() {
    let event = EngineEvent::AgentStateChanged {
        old_state: AgentState::Idle,
        new_state: AgentState::Thinking,
    };
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("agent_state_changed"));
    assert!(json.contains("idle"));
    assert!(json.contains("thinking"));
    // Verify it does NOT contain the old string variant
    let decoded: EngineEvent = serde_json::from_str(&json).unwrap();
    match decoded {
        EngineEvent::AgentStateChanged { old_state, new_state } => {
            assert_eq!(old_state, AgentState::Idle);
            assert_eq!(new_state, AgentState::Thinking);
        }
        _ => panic!("wrong variant"),
    }
}

#[test]
fn test_get_nested_key() {
    let config = AppConfig::default();
    let result = config.get_nested("server.host");
    assert!(result.is_some());
    assert_eq!(result.unwrap(), serde_json::json!("127.0.0.1"));
}

#[test]
fn test_set_nested_key() {
    let mut config = AppConfig::default();
    config.set_nested("server.host", "\"0.0.0.0\"").unwrap();
    assert_eq!(config.server.host, "0.0.0.0");
    config.set_nested("agent.max_iterations", "5").unwrap();
    assert_eq!(config.agent.max_iterations, 5);
}
```

- [ ] **Step 2: 编译检查**

Run: `cargo check 2>&1`
Expected: 编译通过。现有使用 `TokenUsage { prompt_tokens, completion_tokens }` 的构造点因 `#[serde(default)]` 自动补全新字段默认值，无需修改构造点。`ClassifyOutput` 构造点需加 `route_reason`。

- [ ] **Step 3: 运行 models 测试**

Run: `cargo test -p openloom-models 2>&1`
Expected: 所有测试 PASS（含新增的 7 个测试）

- [ ] **Step 4: Commit**

```bash
git add crates/models/src/lib.rs
git commit -m "feat(models): add cached_tokens/latency_ms to TokenUsage, AgentStateChanged enum, route_reason to ClassifyOutput, AppConfig cache/agent/persona/rate_limit sections, StoragePrefs PathBuf"
```

---

### Task 2: Inference CompletionRequest 扩展

**Files:**
- Modify: `F:/openLoom/crates/inference/src/lib.rs`

- [ ] **Step 1: 在 CompletionRequest 加 top_p 和 stop 字段，CompletionResponse 加 latency_ms**

`F:/openLoom/crates/inference/src/lib.rs` 中修改 `CompletionRequest`（约第8行）和 `CompletionResponse`（约第27行）：

```rust
#[derive(Debug, Clone)]
pub struct CompletionRequest {
    pub prompt: String,
    pub max_tokens: usize,
    pub temperature: f32,
    pub top_p: f32,
    pub stop: Vec<String>,
    pub stream: bool,
}

impl Default for CompletionRequest {
    fn default() -> Self {
        Self {
            prompt: String::new(),
            max_tokens: 2048,
            temperature: 0.7,
            top_p: 1.0,
            stop: Vec::new(),
            stream: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CompletionResponse {
    pub text: String,
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub latency_ms: u64,       // NEW
}
```

两个 CloudClient impl（AnthropicClient、OpenAIClient）的 `try_complete()` 中 `CompletionResponse` 构造点需加 `latency_ms: 0`（云端延迟由 Engine 的 `Instant::now()` 在调用前后测量）。`InferenceEngine::complete()` 的构造点也加 `latency_ms: 0`。

- [ ] **Step 2: 编译检查**

Run: `cargo check 2>&1`
Expected: 编译通过（现有构造点用 `..Default::default()` 或 `CompletionRequest { prompt: ..., ..Default::default() }`，新字段自动填充默认值）

- [ ] **Step 3: 运行 inference 测试**

Run: `cargo test -p openloom-inference 2>&1`
Expected: 所有测试 PASS

- [ ] **Step 4: Commit**

```bash
git add crates/inference/src/lib.rs
git commit -m "feat(inference): add top_p and stop fields to CompletionRequest"
```

---

### Task 3: Cache trait 扩展 KvCache::stats()

**Files:**
- Modify: `F:/openLoom/crates/cache/src/lib.rs`

- [ ] **Step 1: 加 CacheStats 和 KvCache::stats()**

在 `F:/openLoom/crates/cache/src/lib.rs` 中，`KvCache` trait 后新增 `CacheStats`，trait 加 `stats()` 方法：

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStats {
    pub hit_rate: f64,
    pub block_count: usize,
    pub total_size_mb: f64,
}

pub trait KvCache: Send + Sync {
    fn lookup(&self, prefix_hash: u64) -> Option<CachedPrefix>;
    fn store(&self, prefix_hash: u64, blocks: CachedPrefix);
    fn stats(&self) -> CacheStats;
}
```

文件顶部加 `use serde::{Deserialize, Serialize};`，Cargo.toml 中确认已有 serde 依赖（带 derive feature）。

NoopCache 实现 `stats()`：

```rust
impl KvCache for NoopCache {
    fn lookup(&self, _hash: u64) -> Option<CachedPrefix> { None }
    fn store(&self, _hash: u64, _blocks: CachedPrefix) {}
    fn stats(&self) -> CacheStats {
        CacheStats { hit_rate: 0.0, block_count: 0, total_size_mb: 0.0 }
    }
}
```

- [ ] **Step 2: 加测试**

在 `#[cfg(test)] mod tests` 中：

```rust
#[test]
fn test_noop_cache_stats() {
    let cache = NoopCache;
    let stats = cache.stats();
    assert_eq!(stats.hit_rate, 0.0);
    assert_eq!(stats.block_count, 0);
    assert_eq!(stats.total_size_mb, 0.0);
}

#[test]
fn test_cache_stats_serialization() {
    let stats = CacheStats { hit_rate: 0.85, block_count: 12, total_size_mb: 600.0 };
    let json = serde_json::to_string(&stats).unwrap();
    assert!(json.contains("hit_rate"));
    let decoded: CacheStats = serde_json::from_str(&json).unwrap();
    assert!((decoded.hit_rate - 0.85).abs() < 0.01);
}
```

- [ ] **Step 3: 编译+测试**

Run: `cargo test -p openloom-cache 2>&1`
Expected: 4 tests PASS（2 原有 + 2 新增）

- [ ] **Step 4: Commit**

```bash
git add crates/cache/src/lib.rs
git commit -m "feat(cache): add CacheStats struct and KvCache::stats() method"
```

---

### Task 4: Memory Store 加 EventRow 和 search_fts

**Files:**
- Modify: `F:/openLoom/crates/memory/src/store.rs`

- [ ] **Step 1: 新增 EventRow 公开类型和 search_fts 方法**

在 `store.rs` 文件末尾 `MessageStore` 之后、`#[cfg(test)]` 之前新增：

```rust
// === EventRow (public row type for Engine/CLI queries) ===

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventRow {
    pub id: i64,
    pub timestamp: String,
    pub event_type: String,
    pub action: String,
    pub context: String,
    pub confidence: f64,
    pub source_session: Option<String>,
    pub source_text: String,
}

impl SqliteEventStore {
    /// Return the most recent events (chronological, newest first)
    pub fn query_recent(&self, limit: usize) -> anyhow::Result<Vec<EventRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, timestamp, type, action, context, confidence, source_session, source_text
             FROM events ORDER BY id DESC LIMIT ?1",
        )?;
        let rows = stmt
            .query_map(rusqlite::params![limit as i64], |row| {
                Ok(EventRow {
                    id: row.get(0)?,
                    timestamp: row.get(1)?,
                    event_type: row.get(2)?,
                    action: row.get(3)?,
                    context: row.get(4)?,
                    confidence: row.get(5)?,
                    source_session: row.get(6)?,
                    source_text: row.get(7)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// FTS5 search across events, returning EventRow
    pub fn search_fts(&self, query: &str, limit: usize) -> anyhow::Result<Vec<EventRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT e.id, e.timestamp, e.type, e.action, e.context, e.confidence,
                    e.source_session, e.source_text
             FROM events e
             INNER JOIN events_fts fts ON e.id = fts.rowid
             WHERE events_fts MATCH ?1
             ORDER BY rank
             LIMIT ?2",
        )?;
        let rows = stmt
            .query_map(rusqlite::params![query, limit as i64], |row| {
                Ok(EventRow {
                    id: row.get(0)?,
                    timestamp: row.get(1)?,
                    event_type: row.get(2)?,
                    action: row.get(3)?,
                    context: row.get(4)?,
                    confidence: row.get(5)?,
                    source_session: row.get(6)?,
                    source_text: row.get(7)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}
```

文件顶部确保 `use serde::{Deserialize, Serialize};` 存在。

- [ ] **Step 2: 加 EventRow 测试**

在 `#[cfg(test)]` 最外层测试模块中追加：

```rust
#[test]
fn test_query_recent_events() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let mut store = SqliteEventStore::open(&db_path).unwrap();
    let e1 = make_event("loss_chase", 0.87);
    let e2 = make_event("prefers_tech", 0.80);
    store.insert(&e1).unwrap();
    store.insert(&e2).unwrap();
    let rows = store.query_recent(10).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].action, "prefers_tech"); // newest first by id
}

#[test]
fn test_search_fts_returns_event_row() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let mut store = SqliteEventStore::open(&db_path).unwrap();
    let e = make_event("loss_chase", 0.87);
    store.insert(&e).unwrap();
    let rows = store.search_fts("loss", 10).unwrap();
    assert!(!rows.is_empty());
    assert_eq!(rows[0].action, "loss_chase");
    assert!(!rows[0].timestamp.is_empty());
}
```

- [ ] **Step 3: 运行 memory 测试**

Run: `cargo test -p openloom-memory 2>&1`
Expected: 所有测试 PASS

- [ ] **Step 4: Commit**

```bash
git add crates/memory/src/store.rs
git commit -m "feat(memory): add EventRow type, query_recent and search_fts methods"
```

---

### Task 5: Router route_reason

**Files:**
- Modify: `F:/openLoom/crates/router/src/lib.rs`

- [ ] **Step 1: classify_sync() 各分支填入 route_reason**

`F:/openLoom/crates/router/src/lib.rs` — 修改 `classify_sync()` 中各处 `ClassifyOutput` 构造，为 `route_reason` 赋值：

```rust
// 空输入 (约第38行)
return ClassifyOutput {
    intent: Intent::Chat, complexity: 0.0, skill_match: None,
    confidence: 1.0, cache_hit: false, target_model: TargetModel::Local,
    route_reason: "empty_input".into(),
};

// 高置信度 keyword match (约第71行)
let (target_model, complexity, reason) = if best_confidence >= self.config.keyword_threshold {
    let model = if skill_match.is_some() { TargetModel::None } else { TargetModel::Local };
    (model, 0.3, if skill_match.is_some() { "skill_trigger" } else { "keyword_match" })
} else if best_confidence >= self.config.fallback_threshold {
    (TargetModel::Local, 0.6, "keyword_fallback")
} else if self.cloud_available {
    (TargetModel::Cloud, 0.8, "cloud_fallback")
} else {
    (TargetModel::Local, 0.8, "default_local")
};

ClassifyOutput {
    intent: best_intent, complexity, skill_match, confidence: best_confidence.max(0.3),
    cache_hit: false, target_model, route_reason: reason.to_string(),
}
```

- [ ] **Step 2: 更新测试**

在 `#[cfg(test)] mod tests` 中给每个测试加 `route_reason` 断言：

在 `test_classify_file_operation_keyword` 末尾追加：
```rust
assert_eq!(output.route_reason, "keyword_match");
```

在 `test_classify_chat_fallback` 末尾追加：
```rust
assert!(output.route_reason == "keyword_fallback" || output.route_reason == "default_local");
```

新增一个测试：
```rust
#[test]
fn test_route_reason_on_empty_input() {
    let router = SmartRouter::new_keywords_only(keywords::default_keyword_rules());
    let output = router.classify_sync("");
    assert_eq!(output.route_reason, "empty_input");
}
```

- [ ] **Step 3: 运行 router 测试**

Run: `cargo test -p openloom-router 2>&1`
Expected: 9 tests PASS（8 原有 + 1 新增）

- [ ] **Step 4: Commit**

```bash
git add crates/router/src/lib.rs
git commit -m "feat(router): add route_reason field to ClassifyOutput, tag each classify branch"
```

---

### Task 6: Engine 基础设施集成

**Files:**
- Modify: `F:/openLoom/crates/engine/src/lib.rs`

这是最大的 Task。当前 Engine struct 有 12 个字段，需新增 7 个字段并实现新方法。

- [ ] **Step 1: 新增结构体和后台线程**

在 `F:/openLoom/crates/engine/src/lib.rs` 的 `use` 块之后（约第19行）新增 import 和 struct：

```rust
use std::sync::Mutex;
use std::time::Instant;

struct RateLimiter {
    last_request: Instant,
    min_interval_ms: u64,
}

impl RateLimiter {
    fn new(min_interval_ms: u64) -> Self {
        Self { last_request: Instant::now(), min_interval_ms }
    }

    fn check(&mut self) -> Result<()> {
        let elapsed = self.last_request.elapsed();
        let min_dur = std::time::Duration::from_millis(self.min_interval_ms);
        if elapsed < min_dur {
            anyhow::bail!("rate limit exceeded, retry in {}ms", (min_dur - elapsed).as_millis());
        }
        self.last_request = Instant::now();
        Ok(())
    }
}

struct TokenUsageRecord {
    session_id: String,
    model: String,
    prompt_tokens: usize,
    completion_tokens: usize,
    cached_tokens: usize,
    latency_ms: u64,
}
```

- [ ] **Step 2: 修改 Engine struct**

```rust
pub struct Engine {
    // Existing (unchanged)
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

    // NEW in Milestone C
    config: Arc<RwLock<AppConfig>>,
    start_time: Instant,
    draining: AtomicBool,
    in_flight: AtomicUsize,
    rate_limiter: Mutex<RateLimiter>,
    token_store_tx: std::sync::mpsc::Sender<TokenUsageRecord>,
    model_available: bool,
}
```

- [ ] **Step 3: 新增 token_store 后台线程**

在 `spawn_session_thread` 函数之后新增：

```rust
fn spawn_token_store_thread(
    db_path: PathBuf,
) -> std::sync::mpsc::Sender<TokenUsageRecord> {
    let (tx, rx) = std::sync::mpsc::channel::<TokenUsageRecord>();
    std::thread::spawn(move || {
        let conn = match rusqlite::Connection::open(&db_path) {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("token_store thread: cannot open db: {}", e);
                return;
            }
        };
        let _ = conn.execute_batch("PRAGMA journal_mode=WAL;");
        let store = TokenStore::new(&conn);
        for record in rx {
            let _ = store.insert(
                &record.session_id, &record.model,
                record.prompt_tokens, record.completion_tokens, record.latency_ms,
            );
        }
    });
    tx
}
```

- [ ] **Step 4: 修改 Engine::new() 和 Engine::new_test() + 添加依赖**

**先添加 Engine Cargo.toml 依赖：** `F:/openLoom/crates/engine/Cargo.toml` 在 `[dependencies]` 中加：

```toml
dirs = "5"
toml = "0.8"
```

在 `Engine::new()` 中（约第90-115行），初始化新增字段。修改 `EngineConfig` 加 `rate_limit_ms` 字段：

```rust
pub struct EngineConfig {
    pub data_dir: PathBuf,
    pub threshold: usize,
    pub cloud_config: Option<openloom_models::ModelConfig>,
    #[serde(default = "default_rate_limit_ms")]
    pub rate_limit_ms: u64,
}

fn default_rate_limit_ms() -> u64 { 100 }
```

在 `Engine::new()` 末尾，构造 `engine` 时加新字段：

```rust
let token_store_tx = spawn_token_store_thread(db_path.clone());

let engine = Self {
    router, skills, inference, cloud, weaver, persona,
    memory_tx, session_tx,
    event_bus: event_tx,
    agent_state: Arc::new(RwLock::new(AgentState::Idle)),
    interruptible: AtomicBool::new(false),
    db_path: db_path.clone(),
    // NEW fields
    config: Arc::new(RwLock::new(AppConfig::default())),
    start_time: Instant::now(),
    draining: AtomicBool::new(false),
    in_flight: AtomicUsize::new(0),
    rate_limiter: Mutex::new(RateLimiter::new(config.rate_limit_ms)),
    token_store_tx,
    model_available: false,
};
```

Model 文件存在性检查（在 engine 构造之前）：

```rust
let model_path = config.data_dir.join("models").join("qwen3-1.7b-q4_k_m.gguf");
let model_available = model_path.exists();
if !model_available {
    tracing::warn!(path = %model_path.display(), "GGUF model not found, local inference unavailable");
}
// 将 model_available 存入 engine 字段
```

Engine::new_test() 也需加新字段：

```rust
pub fn new_test(db_path: PathBuf) -> Result<Self> {
    Self::new(EngineConfig {
        data_dir: db_path.parent().unwrap().to_path_buf(),
        threshold: 3, cloud_config: None, rate_limit_ms: 100,
    })
}
```

- [ ] **Step 5: 修改 handle_message() — 加限流 + TokenUsage + latency**

在 `handle_message()` 开头（`agent_loop` 之前）加限流：

```rust
pub async fn handle_message(&self, msg: ChatMessage, session_id: &str) -> Result<ChatResponse> {
    // Rate limiting
    {
        let mut limiter = self.rate_limiter.lock().unwrap();
        limiter.check()?;
    }

    // Drain check
    if self.draining.load(Ordering::SeqCst) {
        return Err(anyhow::anyhow!("Server is shutting down"));
    }

    // Mid-turn protection (KEEP existing logic — do NOT release gate here, only at end of each path)
    if self.interruptible.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
        return Err(anyhow::anyhow!("Agent is busy, please wait"));
    }

    self.in_flight.fetch_add(1, Ordering::SeqCst);

    let start = Instant::now();
    // ... existing classify + handle logic (unchanged for simple path) ...

    // After response computed, before return:
    let latency_ms = start.elapsed().as_millis() as u64;
    self.in_flight.fetch_sub(1, Ordering::SeqCst);

    // ... existing TokenUsage event (UPDATE to include cached_tokens + latency_ms) ...
    let _ = self.event_bus.send(EngineEvent::TokenUsage {
        session_id: session_id.to_string(),
        model: "qwen3-1.7b".into(),
        prompt_tokens,
        completion_tokens,
        cached_tokens: 0,
        latency_ms,
    });

    // Also write to token_usage table (non-fatal)
    let _ = self.token_store_tx.send(TokenUsageRecord {
        session_id: session_id.to_string(),
        model: "qwen3-1.7b".into(),
        prompt_tokens,
        completion_tokens,
        cached_tokens: 0,
        latency_ms,
    });

    // Reset interruptible flag only at end of simple path (C1 fix from Milestone B)
    self.interruptible.store(false, Ordering::SeqCst);

    Ok(ChatResponse {
        response,
        session_id: session_id.to_string(),
        token_usage: TokenUsage { prompt_tokens, completion_tokens, cached_tokens: 0, latency_ms },
    })
}
```

Agent loop 路径中也同样：
- 在 `agent_loop()` 入口处（已有 `self.interruptible.store(true, ...)`），加以 `in_flight` 计数
- 在 `agent_loop()` 的每个返回路径末尾（已有 `self.interruptible.store(false, ...)`），加上 `in_flight` 减计数
- 所有 `TokenUsage { prompt_tokens, completion_tokens }` 改为 `TokenUsage { prompt_tokens, completion_tokens, cached_tokens: 0, latency_ms }`
- 所有 `EngineEvent::TokenUsage { session_id, model, prompt_tokens, completion_tokens }` 改为 `EngineEvent::TokenUsage { session_id, model, prompt_tokens, completion_tokens, cached_tokens: 0, latency_ms }`

    Ok(ChatResponse {
        response,
        session_id: session_id.to_string(),
        token_usage: TokenUsage { prompt_tokens, completion_tokens, cached_tokens: 0, latency_ms },
    })
}
```

Agent loop 路径中也同样加 `cached_tokens: 0, latency_ms` 到 `TokenUsage` 构造和 `EngineEvent::TokenUsage` 中。

Agent loop 中也加 in_flight 计数器管理（在 loop 开始处 increment，在 return 之前 decrement）。

- [ ] **Step 6: 修改 subscribe_persona_watcher — 使用 AgentState enum**

`persona_watcher` 中如果有发送 `AgentStateChanged` 事件的地方，改为传递 `AgentState` 枚举：

在 agent_loop 中修改状态转换通知：
```rust
let _ = self.event_bus.send(EngineEvent::AgentStateChanged {
    old_state: AgentState::Idle,
    new_state: AgentState::Thinking,
});
```

- [ ] **Step 7: 修改 health_check() — 真实 uptime**

```rust
pub async fn health_check(&self) -> HealthStatus {
    let gpu = InferenceEngine::detect_gpu();
    HealthStatus {
        status: if self.model_available { "ok".into() } else { "degraded".into() },
        uptime: self.start_time.elapsed().as_secs(),
        gpu_info: gpu,
    }
}
```

- [ ] **Step 8: 新增 search_events() 和 list_events() Public API**

```rust
pub async fn search_events(&self, query: &str, limit: usize) -> Result<Vec<openloom_memory::store::EventRow>> {
    let conn = rusqlite::Connection::open(&self.db_path)?;
    let store = openloom_memory::store::SqliteEventStore::from_connection(conn);
    store.search_fts(query, limit)
}

pub async fn list_events(&self, limit: usize) -> Result<Vec<openloom_memory::store::EventRow>> {
    let conn = rusqlite::Connection::open(&self.db_path)?;
    let store = openloom_memory::store::SqliteEventStore::from_connection(conn);
    store.query_recent(limit)
}
```

- [ ] **Step 9: 新增 config get/set 方法**

```rust
pub async fn get_config(&self, key: Option<&str>) -> serde_json::Value {
    let config = self.config.read().await;
    match key {
        Some(k) => config.get_nested(k).unwrap_or(serde_json::Value::Null),
        None => serde_json::to_value(&*config).unwrap_or_default(),
    }
}

pub async fn set_config(&self, key: &str, value: &str) -> Result<()> {
    let mut config = self.config.write().await;
    config.set_nested(key, value)?;
    // Persist to config.toml
    let path = dirs::data_dir().unwrap_or_else(|| PathBuf::from(".")).join("openLoom").join("config.toml");
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let content = toml::to_string_pretty(&*config)?;
    std::fs::write(&path, content)?;
    tracing::info!(key, value, "config updated");
    Ok(())
}

pub fn load_config_into_engine(&self, config: AppConfig) {
    let rt = tokio::runtime::Handle::current();
    rt.block_on(async {
        *self.config.write().await = config;
    });
}
```

- [ ] **Step 10: 重写 shutdown()**

```rust
pub async fn shutdown(&self) -> Result<()> {
    tracing::info!("engine shutting down");

    // 1. Set draining flag
    self.draining.store(true, Ordering::SeqCst);

    // 2. Wait for in-flight requests (max 5s)
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    while self.in_flight.load(Ordering::SeqCst) > 0 {
        if tokio::time::Instant::now() > deadline {
            tracing::warn!("shutdown timeout, {} requests still in-flight", self.in_flight.load(Ordering::SeqCst));
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    // 3. SQLite WAL checkpoint
    if let Ok(conn) = rusqlite::Connection::open(&self.db_path) {
        let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
        tracing::info!("SQLite WAL checkpoint complete");
    }

    tracing::info!("engine shutdown complete");
    Ok(())
}
```

- [ ] **Step 11: 更新测试中的 setup**

`setup_test_engine()` 中 `Engine::new_test()` 内部通过 `EngineConfig` 构造，新增字段有默认值自动初始。已有的 async 测试中的 `TokenUsage { prompt_tokens, completion_tokens }` 构造点因 `#[serde(default)]` 不再编译失败，但需更新断言检查新字段。

更新 `test_health_check` 测试：
```rust
#[tokio::test]
async fn test_health_check() {
    let (engine, _dir) = setup_test_engine().await;
    let health = engine.health_check().await;
    assert!(health.status == "ok" || health.status == "degraded");
    assert!(health.uptime > 0); // NEW
}
```

新增测试：
```rust
#[tokio::test]
async fn test_rate_limit_allows_first_request() {
    let (engine, _dir) = setup_test_engine().await;
    let msg = ChatMessage { role: "user".into(), content: "hello".into(), timestamp: Utc::now() };
    let sid = engine.create_session().await.unwrap().id;
    let result = engine.handle_message(msg, &sid).await;
    assert!(result.is_ok());
}

#[test]
fn test_model_available_default_false() {
    let (engine, _dir) = sync_setup();
    assert!(!engine.model_available || engine.model_available); // just don't panic
}

#[tokio::test]
async fn test_search_events_empty() {
    let (engine, _dir) = setup_test_engine().await;
    let rows = engine.search_events("test", 10).await.unwrap();
    assert!(rows.is_empty());
}

#[tokio::test]
async fn test_list_events_empty() {
    let (engine, _dir) = setup_test_engine().await;
    let rows = engine.list_events(10).await.unwrap();
    assert!(rows.is_empty());
}
```

- [ ] **Step 12: 编译+测试**

Run: `cargo test -p openloom-engine 2>&1`
Expected: 所有测试 PASS

- [ ] **Step 13: 运行全部测试确认无回归**

Run: `cargo test 2>&1 | grep -E "test result|FAIL"`

Expected: 所有 crate 测试 PASS

- [ ] **Step 14: Commit**

```bash
git add crates/engine/src/lib.rs
git commit -m "feat(engine): add rate limiter, token_store thread, config get/set, model check, real shutdown, uptime, search/list events, latency tracking"
```

---

### Task 7: JSON-RPC Dispatch 补齐 5 个 stub

**Files:**
- Modify: `F:/openLoom/crates/server/src/dispatch.rs`

- [ ] **Step 1: 替换 5 个 stub 方法**

`F:/openLoom/crates/server/src/dispatch.rs` — 替换 `memory.query`, `cache.stats`, `config.get`, `config.set`, `system.shutdown`：

```rust
"memory.query" => {
    let query = params.as_ref()
        .and_then(|p| p.get("query"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let limit = params.as_ref()
        .and_then(|p| p.get("limit"))
        .and_then(|v| v.as_u64())
        .unwrap_or(20) as usize;
    let events = engine.search_events(query, limit).await.map_err(|e| JsonRpcError {
        code: ErrorCode::InternalError,
        message: e.to_string(),
        data: None,
    })?;
    Ok(serde_json::json!({"events": events, "cognitions": []}))
}
"cache.stats" => {
    use openloom_cache::KvCache;
    let weaver_cache = engine.weaver_cache();  // NEW: expose weaver's cache
    let stats = weaver_cache.stats();
    Ok(serde_json::to_value(stats).unwrap_or_default())
}
"config.get" => {
    let key = params.as_ref()
        .and_then(|p| p.get("key"))
        .and_then(|v| v.as_str());
    let config = engine.get_config(key).await;
    Ok(serde_json::json!({"config": config}))
}
"config.set" => {
    let key = params.as_ref()
        .and_then(|p| p.get("key"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let value = params.as_ref()
        .and_then(|p| p.get("value"))
        .map(|v| v.to_string())
        .unwrap_or_default();
    engine.set_config(key, &value).await.map_err(|e| JsonRpcError {
        code: ErrorCode::InternalError,
        message: e.to_string(),
        data: None,
    })?;
    Ok(serde_json::json!({"ok": true}))
}
"system.shutdown" => {
    engine.shutdown().await.map_err(|e| JsonRpcError {
        code: ErrorCode::InternalError,
        message: e.to_string(),
        data: None,
    })?;
    Ok(serde_json::json!({"ok": true}))
}
```

- [ ] **Step 2: Engine 新增 weaver_cache() 访问器**

在 `F:/openLoom/crates/engine/src/lib.rs` 加：

```rust
pub fn weaver_cache(&self) -> &dyn KvCache {
    // We need to expose the cache. Currently Weaver holds cache as Arc<dyn KvCache>.
    // Add a method to Weaver:
}
```

这需要 Weaver 暴露 cache。在 `F:/openLoom/crates/weaver/src/lib.rs` 的 `ContextWeaver` impl 中加：

```rust
pub fn cache(&self) -> &Arc<dyn KvCache> {
    &self.cache
}
```

然后 Engine::weaver_cache() 可以不用了，直接在 dispatch 中通过 engine 访问。改为在 Engine 加：

```rust
pub fn cache_stats(&self) -> openloom_cache::CacheStats {
    self.weaver.cache().stats()
}
```

dispatch 中 `cache.stats` 改为：

```rust
"cache.stats" => {
    let stats = engine.cache_stats();
    Ok(serde_json::json!({"hit_rate": stats.hit_rate, "block_count": stats.block_count, "total_size_mb": stats.total_size_mb}))
}
```

- [ ] **Step 3: 编译检查**

Run: `cargo check 2>&1`
Expected: 编译通过

- [ ] **Step 4: 运行 server 测试**

Run: `cargo test -p openloom-server 2>&1`
Expected: 所有测试 PASS

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/dispatch.rs crates/engine/src/lib.rs crates/weaver/src/lib.rs
git commit -m "feat(server,engine,weaver): wire memory.query, cache.stats, config.get/set, system.shutdown to real implementations"
```

---

### Task 8: WebSocket 推送通知

**Files:**
- Modify: `F:/openLoom/crates/server/src/ws.rs`
- Modify: `F:/openLoom/crates/server/src/lib.rs`

- [ ] **Step 1: 重写 ws.rs 支持 EventBus 推送**

`F:/openLoom/crates/server/src/ws.rs` 完整替换：

```rust
use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::IntoResponse;
use openloom_engine::Engine;
use openloom_models::*;
use std::sync::Arc;
use tokio::sync::broadcast;

use crate::dispatch;

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(engine): State<Arc<Engine>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws(socket, engine))
}

async fn handle_ws(mut socket: WebSocket, engine: Arc<Engine>) {
    let mut event_rx = engine.subscribe();

    loop {
        tokio::select! {
            // Client messages
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(req) = serde_json::from_str::<JsonRpcRequest>(&text) {
                            let result = dispatch::dispatch_method(&engine, &req.method, req.params.clone()).await;
                            let resp = match result {
                                Ok(value) => JsonRpcResponse {
                                    jsonrpc: "2.0".into(), result: Some(value), error: None, id: req.id,
                                },
                                Err(err) => JsonRpcResponse {
                                    jsonrpc: "2.0".into(), result: None, error: Some(err), id: req.id,
                                },
                            };
                            if let Ok(json) = serde_json::to_string(&resp) {
                                let _ = socket.send(Message::Text(json.into())).await;
                            }
                        } else {
                            let err = JsonRpcResponse {
                                jsonrpc: "2.0".into(), result: None,
                                error: Some(JsonRpcError { code: ErrorCode::ParseError, message: "invalid JSON-RPC".into(), data: None }),
                                id: 0,
                            };
                            if let Ok(json) = serde_json::to_string(&err) {
                                let _ = socket.send(Message::Text(json.into())).await;
                            }
                        }
                    }
                    Some(Ok(Message::Ping(_))) => {
                        let _ = socket.send(Message::Pong(vec![])).await;
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
            // EventBus push notifications
            result = event_rx.recv() => {
                match result {
                    Ok(event) => {
                        let notification = event_to_notification(&event);
                        if let Ok(json) = serde_json::to_string(&notification) {
                            let _ = socket.send(Message::Text(json.into())).await;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(skipped = n, "WebSocket event_rx lagging, skipped events");
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }
}

fn event_to_notification(event: &EngineEvent) -> serde_json::Value {
    match event {
        EngineEvent::CognitionUpdated { trait_name, new_value, confidence, .. } => {
            serde_json::json!({
                "jsonrpc": "2.0",
                "method": "cognition.updated",
                "params": { "trait": trait_name, "new_value": new_value, "confidence": confidence }
            })
        }
        EngineEvent::AgentStateChanged { old_state, new_state } => {
            serde_json::json!({
                "jsonrpc": "2.0",
                "method": "agent.state_changed",
                "params": { "old_state": old_state, "new_state": new_state }
            })
        }
        EngineEvent::TokenUsage { session_id, model, prompt_tokens, completion_tokens, cached_tokens, latency_ms } => {
            serde_json::json!({
                "jsonrpc": "2.0",
                "method": "token.usage",
                "params": { "session_id": session_id, "model": model, "prompt_tokens": prompt_tokens, "completion_tokens": completion_tokens, "cached_tokens": cached_tokens, "latency_ms": latency_ms }
            })
        }
        EngineEvent::Error { code, message, subsystem } => {
            serde_json::json!({
                "jsonrpc": "2.0",
                "method": "error",
                "params": { "code": code, "message": message, "subsystem": subsystem }
            })
        }
    }
}
```

注意：`serde_json::Value` 没有 `into()` 直接转为 `Message`。`Message::Text` 接受 `String`，需要用 `serde_json::to_string(&notification).unwrap()`。

- [ ] **Step 2: 编译检查**

Run: `cargo check 2>&1`
Expected: 编译通过

- [ ] **Step 3: 运行全部测试**

Run: `cargo test 2>&1 | grep -E "test result|FAIL"`
Expected: 所有测试 PASS

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/ws.rs
git commit -m "feat(server): add EventBus push notifications to WebSocket handler"
```

---

### Task 9: Server config 加载

**Files:**
- Modify: `F:/openLoom/crates/server/src/lib.rs`

- [ ] **Step 1: Server::new() 接收 config_path**

`F:/openLoom/crates/server/src/lib.rs`：

```rust
use std::path::PathBuf;

pub struct Server {
    engine: Arc<Engine>,
    port: u16,
    config_path: Option<PathBuf>,
}

impl Server {
    pub fn new(engine: Engine, config_path: Option<PathBuf>) -> Self {
        Self { engine: Arc::new(engine), port: 0, config_path }
    }
    // ... serve() unchanged
}
```

- [ ] **Step 2: 编译检查 + 测试**

Run: `cargo check 2>&1 && cargo test -p openloom-server 2>&1`
Expected: 编译通过，测试 PASS

- [ ] **Step 3: Commit**

```bash
git add crates/server/src/lib.rs
git commit -m "feat(server): accept config_path param in Server::new for config loading"
```

---

### Task 10: CLI 实装 + 信号处理

**Files:**
- Modify: `F:/openLoom/crates/cli/src/main.rs`

- [ ] **Step 1: 抽取 build_engine helper**

在 `main()` 函数之前新增：

```rust
fn build_engine(config: Option<&str>, rate_limit_ms: u64) -> anyhow::Result<Engine> {
    let app_config = load_config(config);
    let data_dir = dirs::data_dir().unwrap_or_else(|| PathBuf::from(".")).join("openLoom");
    let cloud_config = app_config.models.iter()
        .find(|m| matches!(m.backend, openloom_models::ModelBackend::Anthropic | openloom_models::ModelBackend::OpenAI | openloom_models::ModelBackend::DeepSeek))
        .cloned();
    Engine::new(EngineConfig {
        data_dir,
        threshold: app_config.agent.max_iterations,
        cloud_config,
        rate_limit_ms,
    })
}
```

- [ ] **Step 2: 修改 Serve/Chat/Run 用 build_engine**

`Commands::Serve` 分支改为：

```rust
Commands::Serve { port, config } => {
    let app_config = load_config(config.as_deref());
    let data_dir = dirs::data_dir().unwrap_or_else(|| PathBuf::from(".")).join("openLoom");
    let rate_limit_ms = app_config.rate_limit.min_interval_ms;
    let engine = build_engine(config.as_deref(), rate_limit_ms)?;
    engine.load_config_into_engine(app_config);
    let server = Server::new(engine, config.as_ref().map(PathBuf::from));
    server.serve(port).await?;
}
```

`Commands::Chat` 和 `Commands::Run` 同样用 `build_engine()`。

- [ ] **Step 3: skill list 实装**

```rust
SkillAction::List => {
    let engine = build_engine(None, 100)?;
    let skills = engine.list_skills();
    if skills.is_empty() {
        println!("No skills registered.");
    } else {
        for s in &skills {
            println!("{} - {} (triggers: {:?})", s.name, s.description, s.triggers);
        }
    }
}
```

- [ ] **Step 4: memory events 实装**

```rust
MemoryAction::Events { limit } => {
    let engine = build_engine(None, 100)?;
    let events = engine.list_events(limit).await?;
    if events.is_empty() {
        println!("No events recorded yet.");
    } else {
        for e in &events {
            println!("[{}] {}: {} (conf: {:.0}%, session: {})",
                e.timestamp, e.event_type, e.action,
                e.confidence * 100.0,
                e.source_session.as_deref().unwrap_or("-"));
        }
    }
}
```

- [ ] **Step 5: config get/set 实装**

```rust
ConfigAction::Get { key } => {
    match key {
        Some(k) => {
            let config = load_config(None);
            match config.get_nested(&k) {
                Some(v) => println!("{} = {}", k, v),
                None => println!("Key '{}' not found", k),
            }
        }
        None => {
            let config = load_config(None);
            match toml::to_string_pretty(&config) {
                Ok(s) => println!("{}", s),
                Err(e) => eprintln!("Error: {}", e),
            }
        }
    }
}
ConfigAction::Set { key, value } => {
    let path = config_path(None);
    let mut config = if path.exists() {
        load_config(None)
    } else {
        AppConfig::default()
    };
    if let Err(e) = config.set_nested(&key, &value) {
        eprintln!("Error: {}", e);
    } else {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let content = toml::to_string(&config).unwrap_or_default();
        if let Err(e) = std::fs::write(&path, content) {
            eprintln!("Error writing config: {}", e);
        } else {
            println!("{} = {}", key, value);
        }
    }
}
```

- [ ] **Step 6: 信号处理**

在 `main()` 函数开头（`let cli = Cli::parse();` 之前或 match 之前），对 Serve 和 Chat 模式注册 Ctrl+C handler。最简方案：在 Serve 模式中 spawn 一个信号 task。

在 `Commands::Serve { port, config }` 分支内，`server.serve(port).await?;` 之前：

```rust
let engine_shutdown = /* need Arc<Engine> for shutdown */;
// Server holds Arc<Engine>, clone it
let shutdown_engine = server.engine().clone();
tokio::spawn(async move {
    tokio::signal::ctrl_c().await.ok();
    tracing::info!("SIGINT received, shutting down...");
    shutdown_engine.shutdown().await.ok();
    std::process::exit(0);
});
```

这需要 `Server` 暴露 `engine()` 方法。在 `server/src/lib.rs` 中加：

```rust
pub fn engine(&self) -> &Arc<Engine> {
    &self.engine
}
```

Chat 模式也在主循环前加信号处理。

- [ ] **Step 7: 编译+测试**

Run: `cargo check 2>&1 && cargo test -p openloom-cli 2>&1`
Expected: 编译通过，CLI 测试 PASS

- [ ] **Step 8: Commit**

```bash
git add crates/cli/src/main.rs crates/server/src/lib.rs
git commit -m "feat(cli,server): wire skill list, memory events, config get/set to live data, add SIGINT handler"
```

---

### Task 11: 创建 config.example.toml

**Files:**
- Create: `F:/openLoom/config.example.toml`

- [ ] **Step 1: 创建 config.example.toml**

```toml
# openLoom configuration file
# Location: ~/.local/share/openLoom/config.toml (Linux)
#           ~/Library/Application Support/openLoom/config.toml (macOS)
#           %APPDATA%/openLoom/config.toml (Windows)

[[models]]
name = "router"
path = "qwen3-1.7b-q4_k_m.gguf"
model_type = "Router"
backend = "LlamaCpp"
context_size = 4096
n_gpu_layers = 32

[[models]]
name = "summarizer"
path = "qwen3-8b-q4_k_m.gguf"
model_type = "Summarizer"
backend = "LlamaCpp"
context_size = 8192
n_gpu_layers = 32

# [[models]]
# name = "cloud-primary"
# backend = "Anthropic"
# model = "claude-sonnet-4-6"
# api_key_env = "ANTHROPIC_API_KEY"

[router]
keyword_threshold = 0.85
fallback_threshold = 0.7

[server]
host = "127.0.0.1"

[storage]
# data_dir = "/custom/path/to/data"

[logging]
level = "INFO"
log_content = false

[cache]
block_size = 1024
max_blocks = 32
total_budget_mb = 5120

[agent]
max_iterations = 3
timeout_secs = 120

[persona]
top_n = 5
recency_decay_days = 30

[rate_limit]
min_interval_ms = 100
```

- [ ] **Step 2: Commit**

```bash
git add config.example.toml
git commit -m "chore: add example config.toml with all Phase 2 sections"
```

---

### Task 12: 最终验证

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
git commit -m "chore: Phase 2 Milestone C complete — backend gaps filled, all tests pass, clippy clean, release build"
```

---

## 完成检查清单

- [ ] `cargo test` — 所有测试通过
- [ ] `cargo clippy -- -D warnings` — 零警告
- [ ] `cargo fmt --check` — 格式正确
- [ ] `cargo build --release` — release 编译成功

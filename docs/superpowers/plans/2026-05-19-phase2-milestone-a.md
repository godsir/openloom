# Phase 2 Milestone A: Infrastructure + Smart Prompts — 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 搭建 Phase 2 基础设施——云端模型、多会话持久化、Context Weaver、规则版认知写入

**Architecture:** 新增 cache/weaver 两个 crate，扩展 models/inference/memory/engine/server/cli 六个 crate。CloudClient trait 与 InferenceEngine 平级，ContextWeaver 做 4 步 prompt 组装（KV stub + persona stub + skill + working memory），session 用专用线程+通道持久化。

**Tech Stack:** Rust 2024, reqwest, async-trait, rusqlite, tokio, axum, clap

---

## 文件结构

```
F:/openLoom/
├── crates/
│   ├── models/src/lib.rs                    ← [Modify] TargetModel::Cloud, ModelConfig.model, PersonaProvider, NoopPersonaProvider
│   ├── cache/                               ← [Create] 新 crate
│   │   ├── Cargo.toml
│   │   └── src/lib.rs                       ← KvCache trait + NoopCache + CachedPrefix
│   ├── inference/
│   │   ├── Cargo.toml                       ← [Modify] +reqwest
│   │   └── src/lib.rs                       ← [Modify] CloudClient trait + AnthropicClient + OpenAIClient + factory
│   ├── memory/src/
│   │   └── store.rs                         ← [Modify] +CognitionStore, +SessionStore, +TokenStore, +from_connection
│   ├── skills/src/lib.rs                    ← [Modify] +find_by_name
│   ├── weaver/                              ← [Create] 新 crate
│   │   ├── Cargo.toml
│   │   └── src/lib.rs                       ← ContextWeaver + AssembledPrompt + assemble()
│   ├── engine/
│   │   ├── Cargo.toml                       ← [Modify] +weaver, +cache, +reqwest
│   │   ├── src/lib.rs                       ← [Modify] Engine struct, handle_message, session thread, get_working_memory
│   │   └── src/memory_thread.rs             ← [Modify] +CognitionStore write
│   ├── server/
│   │   ├── src/dispatch.rs                  ← [Create] 公共派发函数
│   │   ├── src/jsonrpc.rs                   ← [Modify] 改用 dispatch
│   │   └── src/ws.rs                        ← [Modify] 改用 dispatch
│   └── cli/src/main.rs                      ← [Modify] +session, +memory cognitions commands
```

---

### Task 1: models + cache + SkillRegistry

**Files:**
- Modify: `F:/openLoom/crates/models/src/lib.rs`
- Create: `F:/openLoom/crates/cache/Cargo.toml`
- Create: `F:/openLoom/crates/cache/src/lib.rs`
- Modify: `F:/openLoom/crates/skills/src/lib.rs`

- [ ] **Step 1: 扩展 models/lib.rs**

`F:/openLoom/crates/models/src/lib.rs` — 在现有 `TargetModel` enum 加 `Cloud`，在 `ModelConfig` struct 加 `model` 字段，新增 `PersonaProvider` trait + `NoopPersonaProvider`：

```rust
// === TargetModel 加 Cloud ===
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TargetModel {
    Local,
    None,
    Cloud,
}

// === ModelConfig 加 model 字段 ===
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub name: String,
    #[serde(default)]
    pub model_type: ModelType,
    #[serde(default)]
    pub backend: ModelBackend,
    #[serde(default)]
    pub model: Option<String>,
    pub path: Option<String>,
    #[serde(default = "default_context_size")]
    pub context_size: usize,
    #[serde(default)]
    pub n_gpu_layers: usize,
    pub api_key_env: Option<String>,
}

// === PersonaProvider trait ===
#[async_trait::async_trait]
pub trait PersonaProvider: Send + Sync {
    async fn summarize(&self) -> anyhow::Result<String>;
}

pub struct NoopPersonaProvider;

#[async_trait::async_trait]
impl PersonaProvider for NoopPersonaProvider {
    async fn summarize(&self) -> anyhow::Result<String> {
        Ok(String::new())
    }
}
```

- [ ] **Step 2: 创建 cache crate**

`F:/openLoom/crates/cache/Cargo.toml`:
```toml
[package]
name = "openloom-cache"
version.workspace = true
edition.workspace = true

[dependencies]
```

`F:/openLoom/crates/cache/src/lib.rs`:
```rust
pub struct CachedPrefix {
    pub blocks: Vec<u8>,
    pub token_count: usize,
}

pub trait KvCache: Send + Sync {
    fn lookup(&self, prefix_hash: u64) -> Option<CachedPrefix>;
    fn store(&self, prefix_hash: u64, blocks: CachedPrefix);
}

pub struct NoopCache;

impl KvCache for NoopCache {
    fn lookup(&self, _hash: u64) -> Option<CachedPrefix> { None }
    fn store(&self, _hash: u64, _blocks: CachedPrefix) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_noop_cache_always_miss() {
        let cache = NoopCache;
        assert!(cache.lookup(0).is_none());
    }

    #[test]
    fn test_noop_cache_store_is_noop() {
        let cache = NoopCache;
        let blocks = CachedPrefix { blocks: vec![1, 2, 3], token_count: 10 };
        cache.store(42, blocks);
        assert!(cache.lookup(42).is_none());
    }
}
```

- [ ] **Step 3: 添加 SkillRegistry::find_by_name**

在 `F:/openLoom/crates/skills/src/lib.rs` 的 `impl SkillRegistry` 中添加：
```rust
pub fn find_by_name(&self, name: &str) -> Option<&dyn Skill> {
    self.skills.iter().find(|s| s.name() == name).map(|s| s.as_ref())
}
```

- [ ] **Step 4: 验证**

```bash
cd F:/openLoom && cargo build -p openloom-models -p openloom-cache -p openloom-skills && cargo test -p openloom-models -p openloom-cache -p openloom-skills
```

- [ ] **Step 5: Commit**

```bash
git add crates/models/src/lib.rs crates/cache/ crates/skills/src/lib.rs
git commit -m "feat: add TargetModel::Cloud, ModelConfig.model, PersonaProvider, KvCache+NoopCache crate, SkillRegistry::find_by_name"
```

---

### Task 2: CloudClient trait + implementations

**Files:**
- Modify: `F:/openLoom/crates/inference/Cargo.toml`
- Modify: `F:/openLoom/crates/inference/src/lib.rs`

- [ ] **Step 1: 更新 inference Cargo.toml**

在 `[dependencies]` 中添加：
```toml
reqwest = { version = "0.12", features = ["json"] }
async-trait = "0.1"
base64 = "0.22"
```

- [ ] **Step 2: 添加 CloudClient trait + implementations**

在 `F:/openLoom/crates/inference/src/lib.rs` 末尾添加：

```rust
use async_trait::async_trait;
use openloom_models::ModelBackend;
use reqwest::Client as HttpClient;

#[async_trait]
pub trait CloudClient: Send + Sync {
    async fn complete(&self, req: CompletionRequest) -> anyhow::Result<CompletionResponse>;
    async fn complete_stream(&self, req: CompletionRequest, tx: tokio::sync::mpsc::Sender<String>) -> anyhow::Result<()>;
    fn provider(&self) -> ModelBackend;
    fn model_name(&self) -> &str;
}

pub struct AnthropicClient {
    api_key: String,
    model: String,
    http: HttpClient,
}

impl AnthropicClient {
    pub fn new(api_key: String, model: String) -> Self {
        Self { api_key, model, http: HttpClient::new() }
    }

    async fn complete_with_retry(&self, req: &CompletionRequest, retries: usize) -> anyhow::Result<CompletionResponse> {
        let mut last_err = None;
        for attempt in 0..=retries {
            if attempt > 0 {
                let delay = 2u64.pow(attempt as u32) * 500;
                tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
            }
            match self.try_complete(req).await {
                Ok(resp) => return Ok(resp),
                Err(e) => {
                    tracing::warn!(attempt, error = %e, "Anthropic API call failed");
                    last_err = Some(e);
                }
            }
        }
        Err(last_err.unwrap())
    }

    async fn try_complete(&self, req: &CompletionRequest) -> anyhow::Result<CompletionResponse> {
        let body = serde_json::json!({
            "model": self.model,
            "max_tokens": req.max_tokens,
            "messages": [{"role": "user", "content": &req.prompt}],
        });

        let resp = self.http
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("Anthropic API error {}: {}", status, text);
        }

        let json: serde_json::Value = resp.json().await?;
        let text = json["content"][0]["text"].as_str().unwrap_or("").to_string();
        let prompt_tokens = json["usage"]["input_tokens"].as_u64().unwrap_or(0) as usize;
        let completion_tokens = json["usage"]["output_tokens"].as_u64().unwrap_or(0) as usize;

        Ok(CompletionResponse { text, prompt_tokens, completion_tokens })
    }
}

#[async_trait]
impl CloudClient for AnthropicClient {
    async fn complete(&self, req: CompletionRequest) -> anyhow::Result<CompletionResponse> {
        self.complete_with_retry(&req, 3).await
    }

    async fn complete_stream(&self, _req: CompletionRequest, _tx: tokio::sync::mpsc::Sender<String>) -> anyhow::Result<()> {
        anyhow::bail!("Anthropic streaming not yet implemented");
    }

    fn provider(&self) -> ModelBackend { ModelBackend::Anthropic }
    fn model_name(&self) -> &str { &self.model }
}

pub struct OpenAIClient {
    api_key: String,
    model: String,
    base_url: String,
    http: HttpClient,
}

impl OpenAIClient {
    pub fn new(api_key: String, model: String, base_url: String) -> Self {
        Self { api_key, model, base_url, http: HttpClient::new() }
    }

    async fn complete_with_retry(&self, req: &CompletionRequest, retries: usize) -> anyhow::Result<CompletionResponse> {
        let mut last_err = None;
        for attempt in 0..=retries {
            if attempt > 0 {
                let delay = 2u64.pow(attempt as u32) * 500;
                tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
            }
            match self.try_complete(req).await {
                Ok(resp) => return Ok(resp),
                Err(e) => {
                    tracing::warn!(attempt, error = %e, "API call failed");
                    last_err = Some(e);
                }
            }
        }
        Err(last_err.unwrap())
    }

    async fn try_complete(&self, req: &CompletionRequest) -> anyhow::Result<CompletionResponse> {
        let body = serde_json::json!({
            "model": self.model,
            "max_tokens": req.max_tokens,
            "messages": [{"role": "user", "content": &req.prompt}],
        });

        let resp = self.http
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("API error {}: {}", status, text);
        }

        let json: serde_json::Value = resp.json().await?;
        let text = json["choices"][0]["message"]["content"].as_str().unwrap_or("").to_string();
        let prompt_tokens = json["usage"]["prompt_tokens"].as_u64().unwrap_or(0) as usize;
        let completion_tokens = json["usage"]["completion_tokens"].as_u64().unwrap_or(0) as usize;

        Ok(CompletionResponse { text, prompt_tokens, completion_tokens })
    }
}

#[async_trait]
impl CloudClient for OpenAIClient {
    async fn complete(&self, req: CompletionRequest) -> anyhow::Result<CompletionResponse> {
        self.complete_with_retry(&req, 3).await
    }

    async fn complete_stream(&self, _req: CompletionRequest, _tx: tokio::sync::mpsc::Sender<String>) -> anyhow::Result<()> {
        anyhow::bail!("OpenAI streaming not yet implemented");
    }

    fn provider(&self) -> ModelBackend { ModelBackend::OpenAI }
    fn model_name(&self) -> &str { &self.model }
}

pub fn create_cloud_client(config: &openloom_models::ModelConfig) -> anyhow::Result<Box<dyn CloudClient>> {
    let api_key = std::env::var(config.api_key_env.as_deref().unwrap_or(""))
        .map_err(|_| anyhow::anyhow!("API key env var not set"))?;
    let model = config.model.clone().unwrap_or_default();
    if model.is_empty() {
        anyhow::bail!("model name not configured");
    }
    match config.backend {
        ModelBackend::Anthropic => Ok(Box::new(AnthropicClient::new(api_key, model))),
        ModelBackend::OpenAI => Ok(Box::new(OpenAIClient::new(api_key, model, "https://api.openai.com/v1".into()))),
        ModelBackend::DeepSeek => Ok(Box::new(OpenAIClient::new(api_key, model, "https://api.deepseek.com/v1".into()))),
        ModelBackend::LlamaCpp => anyhow::bail!("LlamaCpp is not a cloud backend"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_cloud_client_llama_errors() {
        let config = openloom_models::ModelConfig {
            backend: ModelBackend::LlamaCpp,
            ..Default::default()
        };
        assert!(create_cloud_client(&config).is_err());
    }

    #[test]
    fn test_create_cloud_client_missing_api_key() {
        let config = openloom_models::ModelConfig {
            backend: ModelBackend::Anthropic,
            model: Some("claude-sonnet-4-6".into()),
            api_key_env: Some("NONEXISTENT_ENV_VAR".into()),
            ..Default::default()
        };
        assert!(create_cloud_client(&config).is_err());
    }

    #[test]
    fn test_cloud_client_trait_object() {
        let client: Box<dyn CloudClient> = Box::new(AnthropicClient::new("key".into(), "claude".into()));
        assert_eq!(client.provider(), ModelBackend::Anthropic);
        assert_eq!(client.model_name(), "claude");
    }
}
```

- [ ] **Step 3: 验证**

```bash
cd F:/openLoom && cargo test -p openloom-inference
```

Expected: 5 existing + 3 new = 8 tests PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/inference/
git commit -m "feat(inference): add CloudClient trait, AnthropicClient, OpenAIClient with retry logic"
```

---

### Task 3: memory V2 表读写层

**Files:**
- Modify: `F:/openLoom/crates/memory/src/store.rs`

- [ ] **Step 1: 添加 CognitionStore, SessionStore, TokenStore, from_connection**

在 `F:/openLoom/crates/memory/src/store.rs` 末尾添加（在现有 `SqliteEventStore` impl 之后）：

```rust
use chrono::{DateTime, Utc};

// === Row types ===

pub struct CognitionRow {
    pub id: i64,
    pub subject: String,
    pub trait_name: String,
    pub value: String,
    pub confidence: f64,
    pub evidence_count: usize,
    pub first_seen: i64,
    pub last_updated: i64,
    pub version: i64,
}

pub struct TokenUsageRow {
    pub id: i64,
    pub timestamp: String,
    pub session_id: String,
    pub model: String,
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub cached_tokens: usize,
    pub latency_ms: u64,
}

// === SqliteEventStore 补充 ===

impl SqliteEventStore {
    pub fn from_connection(conn: Connection) -> Self {
        Self { conn }
    }
}

// === CognitionStore ===

pub struct CognitionStore {
    conn: Connection,
}

impl CognitionStore {
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }

    pub fn insert(
        &self,
        subject: &str,
        trait_name: &str,
        value: &str,
        confidence: f64,
        evidence_count: usize,
    ) -> anyhow::Result<i64> {
        let now = Utc::now().timestamp();
        self.conn.execute(
            "INSERT INTO cognitions (subject, trait, value, confidence, evidence_count, first_seen, last_updated, version)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1)",
            rusqlite::params![subject, trait_name, value, confidence, evidence_count, now, now],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn query_by_subject(&self, subject: &str, limit: usize) -> anyhow::Result<Vec<CognitionRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, subject, trait, value, confidence, evidence_count, first_seen, last_updated, version
             FROM cognitions WHERE subject = ?1 ORDER BY last_updated DESC LIMIT ?2"
        )?;
        let rows = stmt.query_map(rusqlite::params![subject, limit as i64], |row| {
            Ok(CognitionRow {
                id: row.get(0)?,
                subject: row.get(1)?,
                trait_name: row.get(2)?,
                value: row.get(3)?,
                confidence: row.get(4)?,
                evidence_count: row.get(5)?,
                first_seen: row.get(6)?,
                last_updated: row.get(7)?,
                version: row.get(8)?,
            })
        })?.collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn latest_version(&self, subject: &str, trait_name: &str) -> Option<i64> {
        self.conn.query_row(
            "SELECT version FROM cognitions WHERE subject = ?1 AND trait = ?2 ORDER BY version DESC LIMIT 1",
            rusqlite::params![subject, trait_name],
            |row| row.get(0),
        ).ok()
    }
}

// === SessionStore ===

pub struct SessionStore {
    conn: Connection,
}

impl SessionStore {
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }

    pub fn insert(&self, id: &str, created_at: DateTime<Utc>) -> anyhow::Result<()> {
        self.conn.execute(
            "INSERT INTO sessions (id, created_at, message_count) VALUES (?1, ?2, 0)",
            rusqlite::params![id, created_at.to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn list_all(&self, limit: usize) -> anyhow::Result<Vec<crate::SessionInfo>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, created_at, message_count FROM sessions ORDER BY created_at DESC LIMIT ?1"
        )?;
        let rows = stmt.query_map(rusqlite::params![limit as i64], |row| {
            let created_at_str: String = row.get(1)?;
            Ok(crate::SessionInfo {
                id: row.get(0)?,
                created_at: DateTime::parse_from_rfc3339(&created_at_str)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                message_count: row.get(2)?,
            })
        })?.collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn update_message_count(&self, id: &str, count: usize) -> anyhow::Result<()> {
        self.conn.execute(
            "UPDATE sessions SET message_count = ?1 WHERE id = ?2",
            rusqlite::params![count, id],
        )?;
        Ok(())
    }
}

// === TokenStore ===

pub struct TokenStore {
    conn: Connection,
}

impl TokenStore {
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }

    pub fn insert(
        &self,
        session_id: &str,
        model: &str,
        prompt_tokens: usize,
        completion_tokens: usize,
        latency_ms: u64,
    ) -> anyhow::Result<()> {
        self.conn.execute(
            "INSERT INTO token_usage (timestamp, session_id, model, prompt_tokens, completion_tokens, cached_tokens, latency_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6)",
            rusqlite::params![Utc::now().to_rfc3339(), session_id, model, prompt_tokens, completion_tokens, latency_ms],
        )?;
        Ok(())
    }

    pub fn query_by_session(&self, session_id: &str, limit: usize) -> anyhow::Result<Vec<TokenUsageRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, timestamp, session_id, model, prompt_tokens, completion_tokens, cached_tokens, latency_ms
             FROM token_usage WHERE session_id = ?1 ORDER BY id DESC LIMIT ?2"
        )?;
        let rows = stmt.query_map(rusqlite::params![session_id, limit as i64], |row| {
            Ok(TokenUsageRow {
                id: row.get(0)?,
                timestamp: row.get(1)?,
                session_id: row.get(2)?,
                model: row.get(3)?,
                prompt_tokens: row.get(4)?,
                completion_tokens: row.get(5)?,
                cached_tokens: row.get(6)?,
                latency_ms: row.get(7)?,
            })
        })?.collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn total_usage(&self) -> anyhow::Result<(usize, usize)> {
        let (prompt, completion): (i64, i64) = self.conn.query_row(
            "SELECT COALESCE(SUM(prompt_tokens), 0), COALESCE(SUM(completion_tokens), 0) FROM token_usage",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        Ok((prompt as usize, completion as usize))
    }
}

#[cfg(test)]
mod store_v2_tests {
    use super::*;
    use tempfile::tempdir;

    fn setup() -> (tempfile::TempDir, Connection) {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch("PRAGMA journal_mode=WAL;").unwrap();
        // Create tables (normally done by refinery, but tests need inline DDL)
        conn.execute_batch(include_str!("../../../../migrations/V2__add_cognitions_sessions.sql")).unwrap();
        (dir, conn)
    }

    #[test]
    fn test_cognition_insert_and_query() {
        let (_dir, conn) = setup();
        let store = CognitionStore::new(conn);
        store.insert("USER", "risk_tendency", "gambler_chase", 0.91, 5).unwrap();
        let rows = store.query_by_subject("USER", 10).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].trait_name, "risk_tendency");
        assert_eq!(rows[0].value, "gambler_chase");
        assert!(rows[0].first_seen > 0);
    }

    #[test]
    fn test_session_insert_and_list() {
        let (_dir, conn) = setup();
        let store = SessionStore::new(conn);
        store.insert("s1", Utc::now()).unwrap();
        store.insert("s2", Utc::now()).unwrap();
        let sessions = store.list_all(10).unwrap();
        assert_eq!(sessions.len(), 2);
    }

    #[test]
    fn test_token_insert_and_total() {
        let (_dir, conn) = setup();
        let store = TokenStore::new(conn);
        store.insert("s1", "test-model", 100, 50, 200).unwrap();
        store.insert("s1", "test-model", 200, 100, 300).unwrap();
        let (prompt, completion) = store.total_usage().unwrap();
        assert_eq!(prompt, 300);
        assert_eq!(completion, 150);
    }
}
```

注意：`include_str!` 的路径是相对于 `crates/memory/src/store.rs` 的。`../../../../migrations/` 从 `crates/memory/src/` 向上 4 级 = `F:/openLoom/` → `migrations/`。如果编译报错，改为绝对路径或只用 `CREATE TABLE IF NOT EXISTS` 内联 DDL。

- [ ] **Step 2: 验证**

```bash
cd F:/openLoom && cargo test -p openloom-memory -- store_v2_tests
```

Expected: 3 new tests PASS.

- [ ] **Step 3: 全量验证**

```bash
cd F:/openLoom && cargo test --workspace && cargo clippy --workspace -- -D warnings
```

- [ ] **Step 4: Commit**

```bash
git add crates/memory/src/store.rs
git commit -m "feat(memory): add CognitionStore, SessionStore, TokenStore, from_connection for V2 tables"
```

---

### Task 4: weaver crate

**Files:**
- Create: `F:/openLoom/crates/weaver/Cargo.toml`
- Create: `F:/openLoom/crates/weaver/src/lib.rs`

- [ ] **Step 1: 创建 weaver crate**

`F:/openLoom/crates/weaver/Cargo.toml`:
```toml
[package]
name = "openloom-weaver"
version.workspace = true
edition.workspace = true

[dependencies]
openloom-models = { path = "../models" }
openloom-cache = { path = "../cache" }
serde_json = "1"
anyhow = "1"
```

`F:/openLoom/crates/weaver/src/lib.rs`:
```rust
use openloom_cache::KvCache;
use openloom_models::{ChatMessage, PersonaProvider};
use std::sync::Arc;

pub struct AssembledPrompt {
    pub prompt: String,
    pub static_prefix_len: usize,
}

pub struct ContextWeaver {
    cache: Arc<dyn KvCache>,
    persona: Arc<dyn PersonaProvider>,
}

const SYSTEM_INSTRUCTION: &str = "You are openLoom, a private AI assistant running locally.";

impl ContextWeaver {
    pub fn new(cache: Arc<dyn KvCache>, persona: Arc<dyn PersonaProvider>) -> Self {
        Self { cache, persona }
    }

    pub fn assemble(
        &self,
        user_message: &str,
        skill_context: Option<&str>,
        working_memory: &[ChatMessage],
    ) -> AssembledPrompt {
        // Step 1: KV Cache lookup (stub: always miss → compute static prefix inline)
        let prefix_hash = 0u64; // Stub: no real hashing in Phase 2
        let _ = self.cache.lookup(prefix_hash);

        // Step 2: Persona summary (stub: empty string in Milestone A)
        let persona_summary = ""; // NoopPersonaProvider returns empty

        // Static prefix (cache-aligned: goes first)
        let static_prefix = format!("{}\n{}", SYSTEM_INSTRUCTION, persona_summary);
        let static_prefix_len = static_prefix.len();

        // Step 3: Skill context (≤200 tokens)
        let skill_section = match skill_context {
            Some(ctx) if !ctx.is_empty() => format!("\n[Skill Context]\n{}\n", ctx),
            _ => String::new(),
        };

        // Step 4: Working memory (~200 tokens)
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

        // Dynamic section (appended after static prefix to protect cache)
        let dynamic_section = format!("{}{}\n[User Message]\n{}", skill_section, memory_section, user_message);
        let prompt = format!("{}\n{}", static_prefix, dynamic_section);

        AssembledPrompt {
            prompt,
            static_prefix_len,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openloom_cache::NoopCache;
    use openloom_models::NoopPersonaProvider;

    #[test]
    fn test_assemble_basic_message() {
        let cache = Arc::new(NoopCache);
        let persona = Arc::new(NoopPersonaProvider);
        let weaver = ContextWeaver::new(cache, persona);

        let result = weaver.assemble("hello", None, &[]);
        assert!(result.prompt.contains("hello"));
        assert!(result.prompt.contains(SYSTEM_INSTRUCTION));
        assert!(result.static_prefix_len > 0);
    }

    #[test]
    fn test_assemble_with_skill_context() {
        let weaver = ContextWeaver::new(Arc::new(NoopCache), Arc::new(NoopPersonaProvider));
        let result = weaver.assemble("open file", Some("file-manager: list/read/write files"), &[]);
        assert!(result.prompt.contains("[Skill Context]"));
        assert!(result.prompt.contains("file-manager"));
    }

    #[test]
    fn test_assemble_with_working_memory() {
        let weaver = ContextWeaver::new(Arc::new(NoopCache), Arc::new(NoopPersonaProvider));
        let memory = vec![
            ChatMessage { role: "user".into(), content: "hi".into() },
            ChatMessage { role: "assistant".into(), content: "hello".into() },
        ];
        let result = weaver.assemble("how are you", None, &memory);
        assert!(result.prompt.contains("[Conversation History]"));
        assert!(result.prompt.contains("user: hi"));
        assert!(result.prompt.contains("assistant: hello"));
    }

    #[test]
    fn test_static_prefix_before_dynamic() {
        let weaver = ContextWeaver::new(Arc::new(NoopCache), Arc::new(NoopPersonaProvider));
        let result = weaver.assemble("test message", Some("skill context"), &[]);
        let static_part = &result.prompt[..result.static_prefix_len];
        let dynamic_part = &result.prompt[result.static_prefix_len..];
        assert!(static_part.contains(SYSTEM_INSTRUCTION));
        assert!(!static_part.contains("test message"));
        assert!(dynamic_part.contains("test message"));
        assert!(dynamic_part.contains("[Skill Context]"));
    }
}
```

- [ ] **Step 2: 验证**

```bash
cd F:/openLoom && cargo test -p openloom-weaver
```

Expected: 4 tests PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/weaver/
git commit -m "feat(weaver): add ContextWeaver with 4-step prompt assembly"
```

---

### Task 5: engine 大改

**Files:**
- Modify: `F:/openLoom/crates/engine/Cargo.toml`
- Modify: `F:/openLoom/crates/engine/src/lib.rs`
- Modify: `F:/openLoom/crates/engine/src/memory_thread.rs`
- Modify: `F:/openLoom/crates/router/src/lib.rs`

- [ ] **Step 1: 更新 engine Cargo.toml**

在 `[dependencies]` 中添加：
```toml
openloom-weaver = { path = "../weaver" }
openloom-cache = { path = "../cache" }
```

- [ ] **Step 2: Router 加 cloud_available 支持**

在 `F:/openLoom/crates/router/src/lib.rs` 的 `SmartRouter` struct 中添加字段，修改 `classify_sync`:

```rust
pub struct SmartRouter {
    config: RouterConfig,
    skill_triggers: Vec<(String, Vec<String>)>,
    cloud_available: bool,  // NEW
}

impl SmartRouter {
    pub fn set_cloud_available(&mut self, available: bool) {
        self.cloud_available = available;
    }

    // 在 classify_sync 中，skill_match None 分支改为：
    // } else if self.cloud_available && best_confidence < self.config.fallback_threshold {
    //     (TargetModel::Cloud, 0.8)
    // } else {
    //     (TargetModel::Local, 0.8)
    // }
```

具体修改 `classify_sync` 中 `target_model` 计算逻辑（在第 69-80 行附近）：
```rust
let (target_model, complexity) = if best_confidence >= self.config.keyword_threshold {
    let model = if skill_match.is_some() {
        TargetModel::None
    } else {
        TargetModel::Local
    };
    (model, 0.3)
} else if best_confidence >= self.config.fallback_threshold {
    (TargetModel::Local, 0.6)
} else if self.cloud_available {
    (TargetModel::Cloud, 0.8)
} else {
    (TargetModel::Local, 0.8)
};
```

- [ ] **Step 3: 改造 Engine struct + new() + session 线程**

`F:/openLoom/crates/engine/src/lib.rs` — 完整重写 Engine 相关部分。关键变更：

```rust
use openloom_cache::{KvCache, NoopCache};
use openloom_models::NoopPersonaProvider;
use openloom_weaver::ContextWeaver;
use openloom_inference::CloudClient;
use std::sync::mpsc;
use tokio::sync::{broadcast, oneshot};

pub struct Engine {
    router: SmartRouter,
    skills: SkillRegistry,
    inference: Arc<InferenceEngine>,
    cloud: Option<Arc<dyn CloudClient>>,
    weaver: ContextWeaver,
    memory_tx: mpsc::Sender<memory_thread::ProcessRequest>,
    session_tx: mpsc::Sender<SessionCommand>,
    event_bus: broadcast::Sender<EngineEvent>,
}

enum SessionCommand {
    Create { reply: oneshot::Sender<SessionInfo> },
    List { reply: oneshot::Sender<Vec<SessionInfo>> },
    UpdateCount { id: String, count: usize },
}

// Session thread (same pattern as memory_thread)
fn spawn_session_thread(db_path: PathBuf) -> mpsc::Sender<SessionCommand> {
    let (tx, rx) = mpsc::channel::<SessionCommand>();
    std::thread::spawn(move || {
        let conn = Connection::open(&db_path).expect("session db open");
        conn.execute_batch("PRAGMA journal_mode=WAL;").unwrap();
        let store = SessionStore::new(conn);
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
```

Engine::new() 更新：
```rust
pub fn new(config: EngineConfig) -> Result<Self> {
    let inference = Arc::new(InferenceEngine::load_blocking(
        &config.data_dir.join("models").join("qwen3-1.7b-q4_k_m.gguf"), 0,
    )?);

    let mut router = SmartRouter::new_keywords_only(openloom_router::keywords::default_keyword_rules());
    let mut skills = SkillRegistry::new();
    builtins::register_all(&mut skills);
    for skill in skills.all_skills() {
        let m = skill.manifest();
        router.register_skill_triggers(skill.name(), m.triggers.clone());
    }

    // Cloud client from config
    let cloud: Option<Arc<dyn CloudClient>> = config.cloud_config.as_ref().and_then(|cfg| {
        openloom_inference::create_cloud_client(cfg).ok().map(Arc::from)
    });
    router.set_cloud_available(cloud.is_some());

    // Weaver with stubs
    let weaver = ContextWeaver::new(
        Arc::new(NoopCache),
        Arc::new(NoopPersonaProvider),
    );

    // Event bus + memory thread + session thread
    let (event_tx, _) = broadcast::channel(256);
    let db_path = config.data_dir.join("data").join("db.sqlite");
    let _ = std::fs::create_dir_all(db_path.parent().unwrap());
    let memory_tx = memory_thread::spawn_memory_thread(db_path.clone(), config.threshold, event_tx.clone());
    let session_tx = spawn_session_thread(db_path);

    Ok(Self { router, skills, inference, cloud, weaver, memory_tx, session_tx, event_bus })
}
```

handle_message 完整重写（集成 Weaver + Cloud 路由）：
```rust
pub async fn handle_message(&self, msg: ChatMessage, session_id: &str) -> Result<ChatResponse> {
    let out = self.router.classify_sync(&msg.content);

    let skill_ctx = out.skill_match.as_ref().and_then(|name| {
        self.skills.find_by_name(name).map(|s| s.context_md().to_string())
    });
    let working_memory = self.get_working_memory(session_id)?;
    let assembled = self.weaver.assemble(&msg.content, skill_ctx.as_deref(), &working_memory);

    let response = match out.target_model {
        TargetModel::None => {
            let name = out.skill_match.as_ref()
                .ok_or_else(|| anyhow::anyhow!("skill_match is None but target_model is None"))?;
            self.skills.invoke(name, serde_json::json!({"text": msg.content})).await?.to_string()
        }
        TargetModel::Local => {
            self.inference.complete(CompletionRequest { prompt: assembled.prompt, ..Default::default() }).await?.text
        }
        TargetModel::Cloud => {
            if let Some(ref cloud) = self.cloud {
                cloud.complete(CompletionRequest { prompt: assembled.prompt, ..Default::default() }).await?.text
            } else {
                self.inference.complete(CompletionRequest { prompt: assembled.prompt, ..Default::default() }).await?.text
            }
        }
    };

    let _ = self.memory_tx.send(memory_thread::ProcessRequest {
        session_id: session_id.to_string(), text: msg.content.clone(), context: out.intent.to_string(),
    });

    let prompt_tokens = self.inference.token_count(&assembled.prompt);
    let completion_tokens = self.inference.token_count(&response);
    let _ = self.event_bus.send(EngineEvent::TokenUsage {
        session_id: session_id.to_string(), model: "qwen3-1.7b".into(), prompt_tokens, completion_tokens,
    });

    Ok(ChatResponse { response, session_id: session_id.to_string(), token_usage: TokenUsage { prompt_tokens, completion_tokens } })
}

fn get_working_memory(&self, _session_id: &str) -> Result<Vec<ChatMessage>> {
    Ok(Vec::new())
}

pub async fn create_session(&self) -> Result<SessionInfo> {
    let (tx, rx) = oneshot::channel();
    self.session_tx.send(SessionCommand::Create { reply: tx }).map_err(|e| anyhow::anyhow!(e))?;
    rx.await.map_err(|e| anyhow::anyhow!(e))
}

pub async fn list_sessions(&self) -> Result<Vec<SessionInfo>> {
    let (tx, rx) = oneshot::channel();
    self.session_tx.send(SessionCommand::List { reply: tx }).map_err(|e| anyhow::anyhow!(e))?;
    rx.await.map_err(|e| anyhow::anyhow!(e))
}
```

- [ ] **Step 4: 改造 memory_thread — 写 cognitions 表**

在 `F:/openLoom/crates/engine/src/memory_thread.rs` 中，修改 `spawn_memory_thread`，传入 `CognitionStore`：

```rust
pub fn spawn_memory_thread(
    db_path: PathBuf,
    threshold: usize,
    event_tx: broadcast::Sender<EngineEvent>,
) -> mpsc::Sender<ProcessRequest> {
    let (tx, rx) = mpsc::channel::<ProcessRequest>();
    std::thread::spawn(move || {
        let conn = Connection::open(&db_path).expect("memory db open");
        conn.execute_batch("PRAGMA journal_mode=WAL;").unwrap();
        // refinery migrations
        refinery::embed_migrations!("../../migrations");
        let _ = embedded::migrations::runner().run(&mut conn);

        let extractor = RuleBasedExtractor::with_default_rules();
        let aggregator = PatternAggregator::new(threshold);
        let store = SqliteEventStore::from_connection(conn); // 共用连接
        let cognition_store = CognitionStore::new(/* 从 store 获取连接？需要修改设计 */);
        // ...
    });
    tx
}
```

**注意：** `from_connection` 会将 Connection 所有权移入 `SqliteEventStore`，之后无法再传给 `CognitionStore`。解决方案：
- 方案 A: 让 `SqliteEventStore` 暴露 `conn()` → `&Connection`，`CognitionStore` 接受 `&Connection`
- 方案 B: `spawn_memory_thread` 先创建 Connection，传给 stores 做 `&Connection`
- **推荐方案 A**：在 SqliteEventStore 加 `pub fn conn(&self) -> &Connection { &self.conn }`，CognitionStore 改为 `new(conn: &Connection)`。最简单。

实现：
```rust
// SqliteEventStore 加方法
impl SqliteEventStore {
    pub fn conn(&self) -> &Connection { &self.conn }
}

// CognitionStore 改为借用
impl CognitionStore {
    pub fn new(conn: &Connection) -> Self { Self { conn: conn.clone() } }  // Connection clone is cheap (ref-counted)
}

// memory_thread 中：
let store = SqliteEventStore::open_with_migrations(&db_path).expect("db");
let cognition_store = CognitionStore::new(store.conn());

// 在 cognition 触发处写 DB：
if let Some(cog) = result.cognition_triggered {
    let _ = cognition_store.insert("USER", &cog.trait_name, &cog.summary, cog.confidence, cog.evidence_count);
    let _ = event_tx.send(EngineEvent::CognitionUpdated { ... });
}
```

- [ ] **Step 5: 验证**

```bash
cd F:/openLoom && cargo build && cargo test --workspace && cargo clippy --workspace -- -D warnings
```

修复所有编译错误和 clippy 警告。

- [ ] **Step 6: Commit**

```bash
git add crates/engine/ crates/router/src/lib.rs
git commit -m "feat(engine): integrate Weaver, CloudClient, session thread, cognition DB write"
```

---

### Task 6: server + CLI

**Files:**
- Create: `F:/openLoom/crates/server/src/dispatch.rs`
- Modify: `F:/openLoom/crates/server/src/jsonrpc.rs`
- Modify: `F:/openLoom/crates/server/src/ws.rs`
- Modify: `F:/openLoom/crates/cli/src/main.rs`

- [ ] **Step 1: 抽取公共 dispatch.rs**

将 `jsonrpc.rs` 和 `ws.rs` 的方法派发逻辑抽取到 `dispatch.rs`：

```rust
// crates/server/src/dispatch.rs
use openloom_engine::Engine;
use openloom_models::*;
use std::sync::Arc;

pub async fn dispatch_method(
    engine: &Engine,
    method: &str,
    params: Option<serde_json::Value>,
) -> Result<serde_json::Value, JsonRpcError> {
    match method {
        "system.health" => {
            let health = engine.health_check().await;
            Ok(serde_json::to_value(health).unwrap_or_default())
        }
        "chat.send" => { /* ... same as current ws.rs dispatch ... */ }
        "skill.list" => { /* ... */ }
        "skill.invoke" => { /* ... */ }
        "system.shutdown" => Ok(serde_json::json!({"ok": true})),
        "session.list" => {
            let sessions = engine.list_sessions().await.map_err(|e| JsonRpcError {
                code: ErrorCode::InternalError, message: e.to_string(), data: None,
            })?;
            Ok(serde_json::to_value(sessions).unwrap_or_default())
        }
        "session.create" => {
            let session = engine.create_session().await.map_err(|e| JsonRpcError {
                code: ErrorCode::InternalError, message: e.to_string(), data: None,
            })?;
            Ok(serde_json::to_value(session).unwrap_or_default())
        }
        "memory.cognitions" => Ok(serde_json::json!({"cognitions": [], "note": "Query via CLI or Phase 2 API"})),
        "memory.persona" => Ok(serde_json::json!({"summary": "Phase 2 Milestone B", "traits": []})),
        "agent.status" => Ok(serde_json::json!({"state": "idle", "active_session": null, "model_info": {"router": "qwen3-1.7b"}})),
        "cache.stats" => Ok(serde_json::json!({"hit_rate": 0.0, "block_count": 0, "total_size_mb": 0})),
        "memory.query" => Ok(serde_json::json!({"events": [], "cognitions": []})),
        _ => Err(JsonRpcError { code: ErrorCode::MethodNotFound, message: format!("method '{}' not found", method), data: None }),
    }
}
```

`jsonrpc.rs` 和 `ws.rs` 改为调用 `dispatch::dispatch_method()`。

- [ ] **Step 2: CLI 新增命令**

在 `F:/openLoom/crates/cli/src/main.rs` 添加 `Session` 子命令和更新 `Memory` 子命令：

```rust
#[derive(Subcommand)]
enum Commands {
    // ... existing ...
    Session {
        #[command(subcommand)]
        action: SessionAction,
    },
    // ...
}

#[derive(Subcommand)]
enum SessionAction {
    List,
    Create,
}

// 在 main() match 中：
Commands::Session { action } => {
    let data_dir = dirs::data_dir().unwrap_or_else(|| PathBuf::from(".")).join("openLoom");
    let engine = Engine::new(EngineConfig { data_dir, threshold: 3, cloud_config: None })?;
    match action {
        SessionAction::List => {
            let sessions = engine.list_sessions().await?;
            for s in &sessions {
                println!("{}  {}  ({} msgs)", s.id, s.created_at, s.message_count);
            }
        }
        SessionAction::Create => {
            let s = engine.create_session().await?;
            println!("Created session: {}", s.id);
        }
    }
}
```

更新 `MemoryAction::Cognitions`：
```rust
MemoryAction::Cognitions => {
    println!("Cognitions for USER:");
    println!("(Phase 2 Milestone B: Persona Projector integration)");
}
```

- [ ] **Step 3: 验证**

```bash
cd F:/openLoom && cargo build && cargo test --workspace && cargo clippy --workspace -- -D warnings
```

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/dispatch.rs crates/server/src/jsonrpc.rs crates/server/src/ws.rs crates/cli/src/main.rs
git commit -m "feat(server,cli): extract dispatch.rs, add session/memory JSON-RPC methods, add CLI session commands"
```

---

### Task 7: 集成测试 + 最终验证

本 task 手动执行（无 subagent）。

- [ ] **Step 1: 运行全量测试**

```bash
cd F:/openLoom && cargo test --workspace
```

- [ ] **Step 2: Clippy + fmt**

```bash
cd F:/openLoom && cargo clippy --workspace -- -D warnings && cargo fmt --check
```

- [ ] **Step 3: Release build**

```bash
cd F:/openLoom && cargo build --release
```

- [ ] **Step 4: 功能验证**

```bash
# 启动 serve
cargo run -- serve --port 19880 &
sleep 3

# 测试新端点
curl http://127.0.0.1:19880/health
curl -X POST http://127.0.0.1:19880/api -H "Content-Type: application/json" -d '{"jsonrpc":"2.0","method":"session.create","params":{},"id":1}'
curl -X POST http://127.0.0.1:19880/api -H "Content-Type: application/json" -d '{"jsonrpc":"2.0","method":"session.list","params":{},"id":2}'
curl -X POST http://127.0.0.1:19880/api -H "Content-Type: application/json" -d '{"jsonrpc":"2.0","method":"chat.send","params":{"messages":[{"role":"user","content":"hello"}],"session_id":"test"},"id":3}'

# 关闭
curl -X POST http://127.0.0.1:19880/api -H "Content-Type: application/json" -d '{"jsonrpc":"2.0","method":"system.shutdown","params":{},"id":4}'
```

- [ ] **Step 5: 最终 Commit**

```bash
git add -A
git commit -m "chore: Milestone A complete — all tests pass, clippy clean, release build"
```

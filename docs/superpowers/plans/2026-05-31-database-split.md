# 数据库三路拆分实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 `~/.loom/data/memory.db` 拆分为 `loom.db`（配置）、`memory.db`（记忆）、`session.db`（会话）三个独立 SQLite 数据库。

**Architecture:** 创建三个独立的 Connection 封装（`ConfigDb`, `MemoryDb`, `SessionDb`），每个持有自己的 `rusqlite::Connection` 并只运行相关迁移。`LoomMemoryStore` 持有全部三个封装，子 Store 通过正确的 Connection 实例化。

**Tech Stack:** Rust 2024, rusqlite (bundled), refinery migrations, loom-core/loom-memory/lume-cli

---

## 表分配

| 表 | 目标库 | 迁移文件 |
|---|---|---|
| `model_configs` | `loom.db` | V11 |
| `agent_configs` | `loom.db` | V9 (agent_configs 部分) |
| `mcp_servers` | `loom.db` | V13 |
| `kg_nodes`, `kg_edges`, `kg_aliases`, `kg_evidence`, `kg_nodes_fts` | `memory.db` | V8 |
| `cognitions`, `cognition_snapshots` | `memory.db` | V2 (cognitions), V4 |
| `events`, `events_fts` | `memory.db` | V1 |
| `sessions` | `session.db` | V2 (sessions), V6, V9 (ALTER), V10 (ALTER), V17 |
| `message_history` | `session.db` | V3, V5 |
| `token_usage` | `session.db` | V14, V16 |
| `bridge_sessions`, `bridge_messages`, `bridge_known_users` | `session.db` | V7 |

---

### Task 1: 创建 ConfigDb 封装

**Files:**
- Create: `backend/crates/loom-memory/src/config_db.rs`
- Modify: `backend/crates/loom-memory/src/lib.rs`
- Create: `migrations/loom/V1__config.sql`

**目标：** 创建 `ConfigDb` 结构体，持有独立的 Connection，运行 loom-specific 迁移。

- [ ] **Step 1: 新建 loom 迁移文件**

创建 `F:/openLoom/migrations/loom/V1__config.sql`:
```sql
CREATE TABLE IF NOT EXISTS model_configs (
    name              TEXT PRIMARY KEY,
    model             TEXT,
    model_type        TEXT NOT NULL DEFAULT 'Router',
    backend           TEXT NOT NULL DEFAULT 'LmStudio',
    base_url          TEXT,
    api_key_env       TEXT,
    context_size      INTEGER NOT NULL DEFAULT 4096,
    max_output_tokens INTEGER,
    is_active         INTEGER NOT NULL DEFAULT 0,
    backend_label     TEXT,
    capabilities      TEXT NOT NULL DEFAULT '{}',
    api_format        TEXT,
    input_price       REAL,
    output_price      REAL,
    cache_read_price  REAL,
    cache_write_price REAL,
    created_at        TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at        TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS agent_configs (
    name              TEXT PRIMARY KEY,
    persona           TEXT NOT NULL DEFAULT '',
    model             TEXT,
    temperature       REAL,
    max_iterations    INTEGER,
    system_prompt_override TEXT NOT NULL DEFAULT '',
    avatar            TEXT,
    memory_enabled    INTEGER NOT NULL DEFAULT 1,
    created_at        TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at        TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS mcp_servers (
    name                TEXT PRIMARY KEY,
    transport           TEXT NOT NULL DEFAULT 'stdio',
    command             TEXT,
    args_json           TEXT,
    url                 TEXT,
    headers_json        TEXT,
    env_json            TEXT,
    cwd                 TEXT,
    startup_timeout_secs INTEGER NOT NULL DEFAULT 30,
    tool_timeout_secs   INTEGER NOT NULL DEFAULT 60,
    enabled_tools_json  TEXT,
    disabled_tools_json TEXT,
    autostart           INTEGER NOT NULL DEFAULT 1
);
```

- [ ] **Step 2: 创建 ConfigDb 结构体**

创建 `F:/openLoom/backend/crates/loom-memory/src/config_db.rs`:
```rust
use anyhow::Result;
use rusqlite::Connection;
use std::path::Path;

embed_migrations!("../../../../migrations/loom");

pub struct ConfigDb {
    conn: Connection,
}

impl ConfigDb {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode=DELETE;
             PRAGMA foreign_keys=ON;",
        )?;
        migrations::runner().run(&mut conn)?;
        Ok(Self { conn })
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }
}
```

- [ ] **Step 3: 注册模块**

修改 `F:/openLoom/backend/crates/loom-memory/src/lib.rs`，在 `pub mod graph;` 后添加：
```rust
pub mod config_db;
```

- [ ] **Step 4: 编译验证**

```bash
cargo build -p loom-memory
```

预期：编译通过。

---

### Task 2: 创建 MemoryDb 封装

**Files:**
- Create: `backend/crates/loom-memory/src/memory_db.rs`
- Create: `migrations/memory/V1__memory.sql`
- Modify: `backend/crates/loom-memory/src/lib.rs`

**目标：** `MemoryDb` 持有独立的 Connection，管理 events / kg / cognitions。

- [ ] **Step 1: 新建 memory 迁移文件**

创建 `F:/openLoom/migrations/memory/V1__memory.sql`:
```sql
-- events
CREATE TABLE IF NOT EXISTS events (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp      TEXT NOT NULL,
    event_type     TEXT NOT NULL,
    action         TEXT NOT NULL,
    context        TEXT NOT NULL,
    confidence     REAL NOT NULL DEFAULT 1.0,
    source_session TEXT,
    source_text    TEXT NOT NULL DEFAULT '',
    payload        TEXT
);
CREATE VIRTUAL TABLE IF NOT EXISTS events_fts USING fts5(event_type, action, context);

-- cognitions
CREATE TABLE IF NOT EXISTS cognitions (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    subject         TEXT NOT NULL,
    trait           TEXT NOT NULL,
    value           TEXT NOT NULL,
    confidence      REAL NOT NULL DEFAULT 0.5,
    evidence_count  INTEGER NOT NULL DEFAULT 1,
    first_seen      INTEGER NOT NULL DEFAULT (strftime('%s','now')),
    last_updated    INTEGER NOT NULL DEFAULT (strftime('%s','now')),
    version         INTEGER NOT NULL DEFAULT 1,
    scope           TEXT NOT NULL DEFAULT 'global'
);
CREATE TABLE IF NOT EXISTS cognition_snapshots (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    cognition_id    INTEGER NOT NULL,
    version         INTEGER NOT NULL,
    trait           TEXT NOT NULL,
    value           TEXT NOT NULL,
    confidence      REAL,
    evidence_count  INTEGER,
    snapshot_at     INTEGER NOT NULL,
    FOREIGN KEY (cognition_id) REFERENCES cognitions(id)
);

-- knowledge graph
CREATE TABLE IF NOT EXISTS kg_nodes (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    name           TEXT NOT NULL,
    entity_type    TEXT NOT NULL DEFAULT 'Concept',
    description    TEXT NOT NULL DEFAULT '',
    confidence     REAL NOT NULL DEFAULT 0.5,
    evidence_count INTEGER NOT NULL DEFAULT 1,
    first_seen     INTEGER NOT NULL DEFAULT (strftime('%s','now')),
    last_updated   INTEGER NOT NULL DEFAULT (strftime('%s','now')),
    scope          TEXT NOT NULL DEFAULT 'global',
    access_count   INTEGER NOT NULL DEFAULT 0,
    last_accessed  INTEGER NOT NULL DEFAULT (strftime('%s','now'))
);
CREATE TABLE IF NOT EXISTS kg_edges (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    source_id       INTEGER NOT NULL,
    target_id       INTEGER NOT NULL,
    relation_type   TEXT NOT NULL DEFAULT 'related_to',
    fact            TEXT NOT NULL DEFAULT '',
    confidence      REAL NOT NULL DEFAULT 0.5,
    evidence_count  INTEGER NOT NULL DEFAULT 1,
    first_seen      INTEGER NOT NULL DEFAULT (strftime('%s','now')),
    last_updated    INTEGER NOT NULL DEFAULT (strftime('%s','now')),
    scope           TEXT NOT NULL DEFAULT 'global',
    FOREIGN KEY (source_id) REFERENCES kg_nodes(id),
    FOREIGN KEY (target_id) REFERENCES kg_nodes(id)
);
CREATE TABLE IF NOT EXISTS kg_aliases (
    node_id INTEGER NOT NULL,
    alias   TEXT NOT NULL,
    PRIMARY KEY (node_id, alias),
    FOREIGN KEY (node_id) REFERENCES kg_nodes(id) ON DELETE CASCADE
);
CREATE TABLE IF NOT EXISTS kg_evidence (
    node_id     INTEGER,
    edge_id     INTEGER,
    cognition_id INTEGER,
    event_id    INTEGER,
    FOREIGN KEY (node_id) REFERENCES kg_nodes(id),
    FOREIGN KEY (edge_id) REFERENCES kg_edges(id)
);
CREATE VIRTUAL TABLE IF NOT EXISTS kg_nodes_fts USING fts5(name, description);
```

- [ ] **Step 2: 创建 MemoryDb 结构体**

创建 `F:/openLoom/backend/crates/loom-memory/src/memory_db.rs`:
```rust
use anyhow::Result;
use rusqlite::Connection;
use std::path::Path;

embed_migrations!("../../../../migrations/memory");

pub struct MemoryDb {
    conn: Connection,
}

impl MemoryDb {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode=DELETE;
             PRAGMA foreign_keys=ON;",
        )?;
        migrations::runner().run(&mut conn)?;
        Ok(Self { conn })
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }
}
```

- [ ] **Step 3: 注册模块**

修改 `F:/openLoom/backend/crates/loom-memory/src/lib.rs`，添加：
```rust
pub mod memory_db;
```

- [ ] **Step 4: 编译验证**

```bash
cargo build -p loom-memory
```

---

### Task 3: 创建 SessionDb 封装

**Files:**
- Create: `backend/crates/loom-memory/src/session_db.rs`
- Create: `migrations/session/V1__session.sql`
- Modify: `backend/crates/loom-memory/src/lib.rs`

- [ ] **Step 1: 新建 session 迁移文件**

创建 `F:/openLoom/migrations/session/V1__session.sql`:
```sql
CREATE TABLE IF NOT EXISTS sessions (
    id                TEXT PRIMARY KEY,
    created_at        TEXT NOT NULL DEFAULT (datetime('now')),
    message_count     INTEGER NOT NULL DEFAULT 0,
    title             TEXT,
    pinned_at         TEXT,
    agent_config_name TEXT,
    summary           TEXT NOT NULL DEFAULT '',
    summary_at_count  INTEGER NOT NULL DEFAULT 0,
    workspace_path    TEXT
);

CREATE TABLE IF NOT EXISTS message_history (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL,
    seq        INTEGER NOT NULL,
    role       TEXT NOT NULL,
    content    TEXT NOT NULL,
    timestamp  TEXT NOT NULL,
    metadata   TEXT
);

CREATE TABLE IF NOT EXISTS token_usage (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id         TEXT NOT NULL,
    model              TEXT NOT NULL,
    prompt_tokens      INTEGER NOT NULL DEFAULT 0,
    completion_tokens  INTEGER NOT NULL DEFAULT 0,
    cached_tokens      INTEGER NOT NULL DEFAULT 0,
    cached_read_tokens INTEGER NOT NULL DEFAULT 0,
    cached_write_tokens INTEGER NOT NULL DEFAULT 0,
    latency_ms         INTEGER NOT NULL DEFAULT 0,
    context_window     INTEGER NOT NULL DEFAULT 0,
    created_at         TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS bridge_sessions (
    id          TEXT PRIMARY KEY,
    bridge_name TEXT NOT NULL,
    channel_id  TEXT NOT NULL,
    peer_id     TEXT NOT NULL,
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE TABLE IF NOT EXISTS bridge_messages (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    bridge_name  TEXT NOT NULL,
    channel_id   TEXT NOT NULL,
    peer_id      TEXT NOT NULL,
    message_id   TEXT NOT NULL,
    content      TEXT NOT NULL,
    role         TEXT NOT NULL,
    timestamp    TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE TABLE IF NOT EXISTS bridge_known_users (
    bridge_name TEXT NOT NULL,
    peer_id     TEXT NOT NULL,
    display_name TEXT,
    PRIMARY KEY (bridge_name, peer_id)
);
```

- [ ] **Step 2: 创建 SessionDb 结构体**

创建 `F:/openLoom/backend/crates/loom-memory/src/session_db.rs`:
```rust
use anyhow::Result;
use rusqlite::Connection;
use std::path::Path;

embed_migrations!("../../../../migrations/session");

pub struct SessionDb {
    conn: Connection,
}

impl SessionDb {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode=DELETE;
             PRAGMA foreign_keys=ON;",
        )?;
        migrations::runner().run(&mut conn)?;
        Ok(Self { conn })
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }
}
```

- [ ] **Step 3: 注册模块**

修改 `F:/openLoom/backend/crates/loom-memory/src/lib.rs`，添加：
```rust
pub mod session_db;
```

- [ ] **Step 4: 编译验证**

```bash
cargo build -p loom-memory
```

---

### Task 4: 更新 LoomMemoryStore 使用三个数据库

**Files:**
- Modify: `backend/crates/lume-cli/src/memory.rs`
- Modify: `backend/crates/lume-cli/src/main.rs`

- [ ] **Step 1: 修改 LoomMemoryStore 结构体**

修改 `F:/openLoom/backend/crates/lume-cli/src/memory.rs` 的导入和结构体：
```rust
use loom_memory::{
    AgentConfigStore, CognitionStore, CognitionsPersonaProvider, GraphStore, McpConfigStore,
    McpServerRow, ModelConfigStore, NewEvent, SqliteEventStore,
    config_db::ConfigDb,
    memory_db::MemoryDb,
    session_db::SessionDb,
};

pub struct LoomMemoryStore {
    config_db: std::sync::Mutex<ConfigDb>,
    memory_db: std::sync::Mutex<MemoryDb>,
    session_db: std::sync::Mutex<SessionDb>,
}
```

- [ ] **Step 2: 修改 open 方法**

```rust
impl LoomMemoryStore {
    pub fn open(data_dir: &std::path::Path) -> Result<Self> {
        std::fs::create_dir_all(data_dir)?;
        let config_db = ConfigDb::open(&data_dir.join("loom.db"))?;
        let memory_db = MemoryDb::open(&data_dir.join("memory.db"))?;
        let session_db = SessionDb::open(&data_dir.join("session.db"))?;
        // Ensure default session row
        session_db.conn().execute(
            "INSERT OR IGNORE INTO sessions (id, created_at, message_count) VALUES ('default', datetime('now'), 0)",
            [],
        )?;
        Ok(Self {
            config_db: std::sync::Mutex::new(config_db),
            memory_db: std::sync::Mutex::new(memory_db),
            session_db: std::sync::Mutex::new(session_db),
        })
    }
}
```

- [ ] **Step 3: 迁移旧的 memory.db 数据**

在 `open()` 中添加迁移逻辑：检测旧 `memory.db` 是否存在，如果存在则 ATTACH 并复制数据到新库：
```rust
// In open(): migrate data from old memory.db if it exists
let old_db_path = data_dir.join("memory.db.old_path_placeholder");
// Migration: if old memory.db exists and new dbs are empty,
// ATTACH old db, copy tables to correct new db
```

（具体迁移逻辑在 Task 7 中实现）

- [ ] **Step 4: 编译验证**

```bash
cargo build -p lume-cli
```

预期：大量编译错误（call sites 仍使用旧的 `store.conn()` 模式）。

---

### Task 5: 更新所有子 Store 调用点

**Files:**
- Modify: `backend/crates/lume-cli/src/memory.rs`

**目标：** 将 memory.rs 中所有 `store.conn()` 替换为对应数据库的 `store.config_db.conn()` / `store.memory_db.conn()` / `store.session_db.conn()`。

- [ ] **Step 1: 配置类操作 → config_db**

将以下行的 `store.lock().unwrap()` 改为 `store.config_db.lock().unwrap()`：
- AgentConfigStore (line 387-402)
- ModelConfigStore (line 439-464)
- McpConfigStore (line 494-536)
- 对应的 conn 引用改为 `config.conn()`

- [ ] **Step 2: 记忆类操作 → memory_db**

将以下行的 `store.lock().unwrap()` 改为 `store.memory_db.lock().unwrap()`：
- extract_cognitions (line 161-163)
- feed_knowledge_graph (line 298-299)
- save_extracted_entities (line 362-363)
- query_kg_context (line 540-541)
- kg_* 方法 (line 627-984)
- cognition_list / cognition_list_subjects / cognition_snapshots (line 936-966)
- get_persona (line 283-284)
- prune_memory (line 627)
- 对应的 conn 引用改为 `memory_db.conn()`

- [ ] **Step 3: 会话类操作 → session_db**

将以下行的 `store.lock().unwrap()` 改为 `store.session_db.lock().unwrap()`：
- save_turn (line 50-89)
- load_history (line 116-157)
- delete_message (line 95-112)
- list_sessions (line 601-617)
- ensure_session (line 617-622)
- delete_session (line 769-798)
- rename_session (line 800-809)
- get_summary / save_summary / get_summary_at_count / get_message_count (line 810-864)
- record_token_usage / get_token_summary / get_token_history (line 997-1075)
- 对应的 conn 引用改为 `session_db.conn()`

- [ ] **Step 4: AgentConfigStore 的 sessions 相关方法**

`set_session_binding` / `get_session_binding` / `set_session_workspace` / `get_session_workspace` 操作的是 `sessions` 表（session.db）。需要修改 AgentConfigStore 接受两个 Connection 引用，或者在 memory.rs 中直接执行这些 SQL：
```rust
async fn save_session_agent_name(&self, session_id: &str, agent_config_name: &str) -> Result<()> {
    let db = self.session_db.lock().unwrap();
    db.conn().execute(
        "UPDATE sessions SET agent_config_name = ?1 WHERE id = ?2",
        rusqlite::params![agent_config_name, session_id],
    )?;
    Ok(())
}
```

类似处理 `save_session_workspace` / `get_session_workspace`。

- [ ] **Step 5: 编译验证并修复错误**

```bash
cargo build -p lume-cli 2>&1 | head -50
```

根据编译错误逐个修复，直到零错误。

---

### Task 6: 更新 main.rs 的数据库路径

**Files:**
- Modify: `backend/crates/lume-cli/src/main.rs`

- [ ] **Step 1: 修改 open 调用**

将三处 `memory::LoomMemoryStore::open(&db_path)` 改为传递数据目录：
```rust
let data_dir = loom_dir.join("data");
match memory::LoomMemoryStore::open(&data_dir) {
```

不再需要 `db_path` 变量 — `LoomMemoryStore::open` 内部会创建 `loom.db` / `memory.db` / `session.db`。

---

### Task 7: 旧数据自动迁移

**Files:**
- Modify: `backend/crates/lume-cli/src/memory.rs`

- [ ] **Step 1: 实现迁移函数**

在 `LoomMemoryStore::open()` 的末尾，添加迁移逻辑：
```rust
// Migrate from old single memory.db if present
let old_db = data_dir.join("memory.db");
if old_db.exists() {
    // Check if new dbs are empty
    let conf_count: i64 = config_db.conn().query_row(
        "SELECT COUNT(*) FROM model_configs", [], |r| r.get(0)
    ).unwrap_or(0);

    if conf_count == 0 {
        // ATTACH old db, copy tables
        config_db.conn().execute(
            "ATTACH DATABASE ?1 AS old", rusqlite::params![old_db.to_str().unwrap()],
        )?;
        config_db.conn().execute_batch(
            "INSERT INTO model_configs    SELECT * FROM old.model_configs;
             INSERT INTO agent_configs    SELECT * FROM old.agent_configs;
             INSERT INTO mcp_servers      SELECT * FROM old.mcp_servers;"
        )?;
        memory_db.conn().execute(
            "ATTACH DATABASE ?1 AS old", rusqlite::params![old_db.to_str().unwrap()],
        )?;
        memory_db.conn().execute_batch(
            "INSERT INTO events          SELECT * FROM old.events;
             INSERT INTO cognitions       SELECT * FROM old.cognitions;
             INSERT INTO cognition_snapshots SELECT * FROM old.cognition_snapshots;
             INSERT INTO kg_nodes         SELECT * FROM old.kg_nodes;
             INSERT INTO kg_edges         SELECT * FROM old.kg_edges;
             INSERT INTO kg_aliases       SELECT * FROM old.kg_aliases;
             INSERT INTO kg_evidence      SELECT * FROM old.kg_evidence;"
        )?;
        // Rebuild FTS5 indexes (virtual tables can't be copied via INSERT)
        memory_db.conn().execute_batch(
            "INSERT INTO events_fts (event_type, action, context)
             SELECT event_type, action, context FROM events;
             INSERT INTO kg_nodes_fts (name, description)
             SELECT name, description FROM kg_nodes;"
        )?;
        session_db.conn().execute(
            "ATTACH DATABASE ?1 AS old", rusqlite::params![old_db.to_str().unwrap()],
        )?;
        session_db.conn().execute_batch(
            "INSERT INTO sessions         SELECT * FROM old.sessions;
             INSERT INTO message_history  SELECT * FROM old.message_history;
             INSERT INTO token_usage      SELECT * FROM old.token_usage;
             INSERT INTO bridge_sessions  SELECT * FROM old.bridge_sessions;
             INSERT INTO bridge_messages  SELECT * FROM old.bridge_messages;
             INSERT INTO bridge_known_users SELECT * FROM old.bridge_known_users;"
        )?;

        tracing::info!("migrated data from legacy memory.db to loom.db / memory.db / session.db");
    }
}
```

- [ ] **Step 2: 编译验证**

```bash
cargo build --release -p lume-cli
```

---

### Task 8: 清理旧代码

**Files:**
- Remove: `backend/crates/loom-memory/src/store.rs` 中废弃的 `SqliteEventStore`
- 或者保留但标记 deprecated

- [ ] **Step 1: 移除对旧 store.rs 的依赖**

确认所有代码已迁移到新三库模式后，移除 `use loom_memory::SqliteEventStore` 引用。

- [ ] **Step 2: 完整编译验证**

```bash
cargo build --release --workspace
```

---

### Task 9: 测试验证

- [ ] **Step 1: 全新安装测试**

删除 `~/.loom/data/`，启动 `lume serve`，验证三个 .db 文件均被创建。

- [ ] **Step 2: 旧数据迁移测试**

保留旧的 `~/.loom/data/memory.db`，启动新版本，验证数据正确迁移到三个新库。

- [ ] **Step 3: 功能验证**

发送一条聊天消息，验证：
- `session.db` 中有新的 message_history 行
- `memory.db` 中有新的 events 行和可能的 KG 节点
- `loom.db` 配置数据不变

- [ ] **Step 4: 运行现有测试**

```bash
cargo test -p loom-memory -p loom-core -p lume-cli
```

---

## 验证

1. `cargo build --release --workspace` — 全量编译通过
2. `cargo test -p loom-memory -p loom-core` — 现有测试通过
3. 手动测试：删除旧 data 目录 → 启动 → 三个 db 文件生成
4. 手动测试：旧 memory.db 存在 → 启动 → 数据迁移到三个新库
5. 手动测试：聊天 → session.db 记录新消息，memory.db 更新 KG

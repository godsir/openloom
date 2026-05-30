# Memory System Panel Redesign - Implementation Plan

**Goal:** Redesign the frontend settings panel from "认知图谱" to "记忆系统" with two tabs (Knowledge Graph + Maintenance), expose scope-level data visibility, add cognition records with evolution timeline, and fix existing UI bugs.

**Architecture:** Backend-first approach adding 4 new JSON-RPC methods (cognitions.list, cognitions.snapshots, cognitions.subjects, kg.prune) and scope parameter support to existing methods. Frontend splits into KnowledgeGraphTab and MaintenanceTab components with extended Zustand store.

**Tech Stack:** Rust (backend), React 19 + TypeScript + Zustand (frontend), SQLite (database)

---

## Phase 1: Backend Types

### Task 1: Add scope field to KgNode and new cognition types

**Files:**
- Modify: `backend/crates/loom-types/src/kg.rs:1-39`
- Modify: `backend/crates/loom-memory/src/graph.rs:9-19`

- [ ] **Step 1: Add scope field to KgNode**

Open `backend/crates/loom-types/src/kg.rs` and add `scope` field to `KgNode`:

```rust
/// A single entity node in the knowledge graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KgNode {
    pub node_id: i64,
    pub name: String,
    pub entity_type: String,
    pub description: String,
    pub confidence: f64,
    pub scope: String,  // NEW: "global" or session id
}
```

- [ ] **Step 2: Add scope field to GraphRow**

Open `backend/crates/loom-memory/src/graph.rs:9-19` and add `scope` field:

```rust
/// A row from a graph query.
#[derive(Debug, Clone)]
pub struct GraphRow {
    pub node_id: i64,
    pub name: String,
    pub entity_type: String,
    pub description: String,
    pub confidence: f64,
    pub relation_type: Option<String>,
    pub distance: Option<usize>,
    pub scope: String,  // NEW: "global" or session id
}
```

- [ ] **Step 3: Add Cognition and CognitionHistory types**

Append to `backend/crates/loom-types/src/kg.rs`:

```rust
/// A cognition record — versioned trait/value pairs for a subject.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cognition {
    pub id: i64,
    pub subject: String,
    pub trait_name: String,
    pub value: String,
    pub confidence: f64,
    pub evidence_count: usize,
    pub first_seen: i64,
    pub last_updated: i64,
    pub version: i64,
    pub scope: String,
}

/// A historical snapshot of a cognition record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CognitionHistory {
    pub id: i64,
    pub version: i64,
    pub trait_name: String,
    pub value: String,
    pub confidence: f64,
    pub evidence_count: usize,
    pub snapshot_at: i64,
}
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check -p loom-types`
Expected: No errors

- [ ] **Step 5: Commit**

```bash
git add backend/crates/loom-types/src/kg.rs backend/crates/loom-memory/src/graph.rs
git commit -m "feat: add scope field to KgNode and new cognition types"
```

---

## Phase 2: Backend Storage Layer

### Task 2: Update GraphStore to support scope filtering

**Files:**
- Modify: `backend/crates/loom-memory/src/graph.rs:210-240,270-300,310-345,355-390,488-507`

- [ ] **Step 1: Update search_entities to return scope**

Open `backend/crates/loom-memory/src/graph.rs:219-240`. Find the `search_entities` method and update the SQL query to include scope:

```rust
pub fn search_entities(&self, query: &str, limit: usize) -> Result<Vec<GraphRow>> {
    let pattern = if query.chars().all(|c| c.is_ascii()) {
        format!("{}*", query)
    } else {
        query.to_string()
    };

    let mut stmt = self.conn.prepare(
        "SELECT n.id, n.name, n.entity_type, n.description, n.confidence,
                CAST(NULL AS TEXT) as relation_type, CAST(NULL AS INTEGER) as distance, n.scope
         FROM kg_nodes_fts f
         JOIN kg_nodes n ON f.rowid = n.id
         WHERE kg_nodes_fts MATCH ?1
         ORDER BY rank LIMIT ?2",
    )?;
    let rows = stmt.query_map(rusqlite::params![pattern, limit as i64], |row| {
        Ok(GraphRow {
            node_id: row.get(0)?,
            name: row.get(1)?,
            entity_type: row.get(2)?,
            description: row.get::<_, Option<String>>(3)?.unwrap_or_default(),
            confidence: row.get(4)?,
            relation_type: None,
            distance: None,
            scope: row.get(7)?,  // NEW
        })
    })?;
    rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
}
```

- [ ] **Step 2: Update neighbors method to return scope**

Find the `neighbors` method around line 270-300. Update both SELECT statements to include `n.scope` and `tn.scope`/`sn.scope`:

```rust
pub fn neighbors(
    &self,
    node_name: &str,
    scope: Option<&str>,
    limit: usize,
) -> Result<Vec<GraphRow>> {
    let scope_filter = scope.unwrap_or("global");
    let mut stmt = self.conn.prepare(
        "SELECT n.id, n.name, n.entity_type, n.description, e.confidence,
                e.relation_type, CAST(NULL AS INTEGER) as distance, n.scope
         FROM kg_nodes src
         JOIN kg_edges e ON (src.id = e.source_id OR src.id = e.target_id)
         JOIN kg_nodes n ON (e.target_id = n.id OR e.source_id = n.id)
         WHERE src.name = ?1 AND n.id != src.id
               AND (e.scope = ?2 OR e.scope = 'global')
         ORDER BY e.confidence DESC LIMIT ?3",
    )?;
    let rows = stmt.query_map(rusqlite::params![node_name, scope_filter, limit], |row| {
        Ok(GraphRow {
            node_id: row.get(0)?,
            name: row.get(1)?,
            entity_type: row.get(2)?,
            description: row.get::<_, Option<String>>(3)?.unwrap_or_default(),
            confidence: row.get(4)?,
            relation_type: row.get(5)?,
            distance: None,
            scope: row.get(7)?,  // NEW
        })
    })?;
    rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
}
```

- [ ] **Step 3: Update walk method to return scope**

Find the `walk` method around line 355-390. Update the recursive CTE to include scope:

```rust
pub fn walk(
    &self,
    start_name: &str,
    max_depth: u8,
    scope: Option<&str>,
    limit: usize,
) -> Result<Vec<GraphRow>> {
    let scope_filter = scope.unwrap_or("global");
    let mut stmt = self.conn.prepare(
        "WITH RECURSIVE walk(id, name, entity_type, description, depth, visited, scope) AS (
            SELECT n.id, n.name, n.entity_type, n.description, 0, ',' || n.id || ',', n.scope
            FROM kg_nodes n WHERE n.name = ?1 AND (n.scope = ?2 OR n.scope = 'global')
            UNION ALL
            SELECT CASE WHEN e.source_id = w.id THEN tn.id ELSE sn.id END,
                   CASE WHEN e.source_id = w.id THEN tn.name ELSE sn.name END,
                   CASE WHEN e.source_id = w.id THEN tn.entity_type ELSE sn.entity_type END,
                   CASE WHEN e.source_id = w.id THEN tn.description ELSE sn.description END,
                   w.depth + 1,
                   w.visited || CASE WHEN e.source_id = w.id THEN tn.id ELSE sn.id END || ',',
                   CASE WHEN e.source_id = w.id THEN tn.scope ELSE sn.scope END
            FROM walk w
            JOIN kg_edges e ON (e.source_id = w.id OR e.target_id = w.id)
            JOIN kg_nodes sn ON sn.id = e.source_id
            JOIN kg_nodes tn ON tn.id = e.target_id
            WHERE w.depth < ?3
                  AND w.visited NOT LIKE '%,' || CASE WHEN e.source_id = w.id THEN tn.id ELSE sn.id END || ',%'
                  AND (e.scope = ?2 OR e.scope = 'global')
                  AND ((CASE WHEN e.source_id = w.id THEN tn.scope ELSE sn.scope END) = ?2
                       OR (CASE WHEN e.source_id = w.id THEN tn.scope ELSE sn.scope END) = 'global')
         )
         SELECT DISTINCT id, name, entity_type, description, MIN(depth), scope
         FROM walk WHERE depth > 0
         GROUP BY id
         ORDER BY depth, name
         LIMIT ?4",
    )?;
    let rows = stmt.query_map(
        rusqlite::params![start_name, scope_filter, max_depth, limit],
        |row| {
            Ok(GraphRow {
                node_id: row.get(0)?,
                name: row.get(1)?,
                entity_type: row.get(2)?,
                description: row.get::<_, Option<String>>(3)?.unwrap_or_default(),
                confidence: 0.0,
                relation_type: None,
                distance: Some(row.get::<_, i64>(4)? as usize),
                scope: row.get(5)?,  // NEW
            })
        },
    )?;
    rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
}
```

- [ ] **Step 4: Update list_nodes to support scope filtering**

Find the `list_nodes` method around line 488-507. Add optional scope parameter:

```rust
/// List recent nodes with pagination and optional scope filter.
pub fn list_nodes(&self, limit: usize, offset: usize, scope: Option<&str>) -> Result<Vec<GraphRow>> {
    let sql = match scope {
        Some(s) => format!(
            "SELECT n.id, n.name, n.entity_type, n.description, n.confidence,
                    CAST(NULL AS TEXT) as relation_type, CAST(NULL AS INTEGER) as distance, n.scope
             FROM kg_nodes n WHERE n.scope = ?3 OR n.scope = 'global'
             ORDER BY n.last_updated DESC LIMIT ?1 OFFSET ?2"
        ),
        None => format!(
            "SELECT n.id, n.name, n.entity_type, n.description, n.confidence,
                    CAST(NULL AS TEXT) as relation_type, CAST(NULL AS INTEGER) as distance, n.scope
             FROM kg_nodes n ORDER BY n.last_updated DESC LIMIT ?1 OFFSET ?2"
        ),
    };
    let mut stmt = self.conn.prepare(&sql)?;
    let rows = if scope.is_some() {
        stmt.query_map(rusqlite::params![limit as i64, offset as i64, scope.unwrap()], |row| {
            Ok(GraphRow {
                node_id: row.get(0)?,
                name: row.get(1)?,
                entity_type: row.get(2)?,
                description: row.get::<_, Option<String>>(3)?.unwrap_or_default(),
                confidence: row.get(4)?,
                relation_type: None,
                distance: None,
                scope: row.get(7)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?
    } else {
        stmt.query_map(rusqlite::params![limit as i64, offset as i64], |row| {
            Ok(GraphRow {
                node_id: row.get(0)?,
                name: row.get(1)?,
                entity_type: row.get(2)?,
                description: row.get::<_, Option<String>>(3)?.unwrap_or_default(),
                confidence: row.get(4)?,
                relation_type: None,
                distance: None,
                scope: row.get(7)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?
    };
    Ok(rows)
}
```

- [ ] **Step 5: Verify compilation**

Run: `cargo check -p loom-memory`
Expected: Compilation errors in other crates (will fix in next tasks)

- [ ] **Step 6: Commit**

```bash
git add backend/crates/loom-memory/src/graph.rs
git commit -m "feat: add scope support to GraphStore methods"
```

---

## Phase 3: Backend CognitionStore Extensions

### Task 3: Add new methods to CognitionStore

**Files:**
- Modify: `backend/crates/loom-memory/src/store.rs:250-301`

- [ ] **Step 1: Add list_subjects method**

Open `backend/crates/loom-memory/src/store.rs` and add this method to `CognitionStore` impl (after `snapshots_for`):

```rust
pub fn list_subjects(&self) -> Result<Vec<String>> {
    let mut stmt = self.conn.prepare(
        "SELECT DISTINCT subject FROM cognitions ORDER BY MAX(last_updated) DESC",
    )?;
    let rows = stmt.query_map([], |row| row.get(0))?;
    Ok(rows.collect::<std::result::Result<Vec<String>, _>>()?)
}
```

- [ ] **Step 2: Add scope filtering to query_by_subject**

Update the `query_by_subject` method signature and implementation:

```rust
pub fn query_by_subject(
    &self,
    subject: &str,
    scope: Option<&str>,  // NEW parameter
    limit: usize,
    offset: usize,
) -> Result<Vec<CognitionRow>> {
    let sql = match scope {
        Some(s) => format!(
            "SELECT id, subject, trait, value, confidence, evidence_count, first_seen, last_updated, version, scope
             FROM cognitions WHERE subject = ?1 AND scope = ?4
             ORDER BY last_updated DESC LIMIT ?2 OFFSET ?3"
        ),
        None => format!(
            "SELECT id, subject, trait, value, confidence, evidence_count, first_seen, last_updated, version, scope
             FROM cognitions WHERE subject = ?1
             ORDER BY last_updated DESC LIMIT ?2 OFFSET ?3"
        ),
    };
    let mut stmt = self.conn.prepare(&sql)?;
    let rows = if let Some(s) = scope {
        stmt.query_map(params![subject, limit as i64, offset as i64, s], |row| {
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
                scope: row.get(9)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?
    } else {
        stmt.query_map(params![subject, limit as i64, offset as i64], |row| {
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
                scope: row.get(9)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?
    };
    Ok(rows)
}
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p loom-memory`
Expected: Compilation errors in calling code (will fix in next tasks)

- [ ] **Step 4: Commit**

```bash
git add backend/crates/loom-memory/src/store.rs
git commit -m "feat: add list_subjects and scope filtering to CognitionStore"
```

---

## Phase 4: Backend MemoryStore Trait

### Task 4: Add new methods to MemoryStore trait

**Files:**
- Modify: `backend/crates/loom-core/src/orchestrator.rs:46-153`

- [ ] **Step 1: Add new trait methods**

Open `backend/crates/loom-core/src/orchestrator.rs` and add these methods to the `MemoryStore` trait (after `kg_delete_edge` around line 123):

```rust
// Cognition records
async fn cognition_list(
    &self,
    subject: &str,
    scope: Option<&str>,
    limit: usize,
    offset: usize,
) -> Result<Vec<loom_types::Cognition>>;
async fn cognition_list_subjects(&self) -> Result<Vec<String>>;
async fn cognition_snapshots(&self, cognition_id: i64) -> Result<Vec<loom_types::CognitionHistory>>;
// Knowledge graph maintenance
async fn kg_prune(&self, older_than_days: i64) -> Result<usize>;
```

- [ ] **Step 2: Update kg_list_nodes signature**

Find the `kg_list_nodes` method in the trait (around line 121) and add scope parameter:

```rust
async fn kg_list_nodes(&self, limit: usize, offset: usize, scope: Option<&str>) -> Result<Vec<loom_types::KgNode>>;
```

- [ ] **Step 3: Commit**

```bash
git add backend/crates/loom-core/src/orchestrator.rs
git commit -m "feat: add cognition and prune methods to MemoryStore trait"
```

---

### Task 5: Implement new methods in LoomMemoryStore

**Files:**
- Modify: `backend/crates/lume-cli/src/memory.rs:758-779`

- [ ] **Step 1: Implement cognition_list**

Open `backend/crates/lume-cli/src/memory.rs` and add this implementation after `kg_delete_edge`:

```rust
async fn cognition_list(
    &self,
    subject: &str,
    scope: Option<&str>,
    limit: usize,
    offset: usize,
) -> Result<Vec<loom_types::Cognition>> {
    let store = self.store.lock().unwrap();
    let cognitions = CognitionStore::new(store.conn());
    let rows = cognitions.query_by_subject(subject, scope, limit, offset)?;
    Ok(rows
        .into_iter()
        .map(|r| loom_types::Cognition {
            id: r.id,
            subject: r.subject,
            trait_name: r.trait_name,
            value: r.value,
            confidence: r.confidence,
            evidence_count: r.evidence_count,
            first_seen: r.first_seen,
            last_updated: r.last_updated,
            version: r.version,
            scope: r.scope,
        })
        .collect())
}
```

- [ ] **Step 2: Implement cognition_list_subjects**

Add after `cognition_list`:

```rust
async fn cognition_list_subjects(&self) -> Result<Vec<String>> {
    let store = self.store.lock().unwrap();
    let cognitions = CognitionStore::new(store.conn());
    cognitions.list_subjects()
}
```

- [ ] **Step 3: Implement cognition_snapshots**

Add after `cognition_list_subjects`:

```rust
async fn cognition_snapshots(&self, cognition_id: i64) -> Result<Vec<loom_types::CognitionHistory>> {
    let store = self.store.lock().unwrap();
    let cognitions = CognitionStore::new(store.conn());
    let snapshots = cognitions.snapshots_for(cognition_id)?;
    Ok(snapshots
        .into_iter()
        .map(|s| loom_types::CognitionHistory {
            id: s.id,
            version: s.version,
            trait_name: s.trait_name,
            value: s.value,
            confidence: s.confidence,
            evidence_count: s.evidence_count,
            snapshot_at: s.snapshot_at,
        })
        .collect())
}
```

- [ ] **Step 4: Implement kg_prune**

Add after `cognition_snapshots`:

```rust
async fn kg_prune(&self, older_than_days: i64) -> Result<usize> {
    let store = self.store.lock().unwrap();
    let graph = GraphStore::new(store.conn());
    graph.prune_stale(older_than_days, 1000)
}
```

- [ ] **Step 5: Update kg_list_nodes implementation**

Find the existing `kg_list_nodes` implementation (around line 758) and update it:

```rust
async fn kg_list_nodes(&self, limit: usize, offset: usize, scope: Option<&str>) -> Result<Vec<loom_types::KgNode>> {
    let store = self.store.lock().unwrap();
    let graph = GraphStore::new(store.conn());
    let rows = graph.list_nodes(limit, offset, scope)?;
    Ok(rows
        .iter()
        .map(|r| loom_types::KgNode {
            node_id: r.node_id,
            name: r.name.clone(),
            entity_type: r.entity_type.clone(),
            description: r.description.clone(),
            confidence: r.confidence,
            scope: r.scope.clone(),  // NEW
        })
        .collect())
}
```

- [ ] **Step 6: Add missing import**

At the top of `memory.rs`, ensure `CognitionStore` is imported. Find the imports section and add:

```rust
use loom_memory::{CognitionStore, GraphStore};
```

- [ ] **Step 7: Verify compilation**

Run: `cargo check -p lume-cli`
Expected: Compilation errors in orchestrator (will fix in next task)

- [ ] **Step 8: Commit**

```bash
git add backend/crates/lume-cli/src/memory.rs
git commit -m "feat: implement cognition and prune methods in LoomMemoryStore"
```

---

## Phase 5: Backend Orchestrator

### Task 6: Add new orchestrator methods

**Files:**
- Modify: `backend/crates/loom-core/src/orchestrator.rs:577-599`

- [ ] **Step 1: Add cognition_list method**

Open `backend/crates/loom-core/src/orchestrator.rs` and add these public methods to the `Orchestrator` impl (after `kg_delete_edge` around line 599):

```rust
pub async fn cognition_list(
    &self,
    subject: &str,
    scope: Option<&str>,
    limit: usize,
    offset: usize,
) -> Result<Vec<loom_types::Cognition>> {
    if let Some(ref store) = *self.memory_store.read().await {
        store.cognition_list(subject, scope, limit, offset).await
    } else {
        Ok(Vec::new())
    }
}

pub async fn cognition_list_subjects(&self) -> Result<Vec<String>> {
    if let Some(ref store) = *self.memory_store.read().await {
        store.cognition_list_subjects().await
    } else {
        Ok(Vec::new())
    }
}

pub async fn cognition_snapshots(&self, cognition_id: i64) -> Result<Vec<loom_types::CognitionHistory>> {
    if let Some(ref store) = *self.memory_store.read().await {
        store.cognition_snapshots(cognition_id).await
    } else {
        Ok(Vec::new())
    }
}

pub async fn kg_prune(&self, older_than_days: i64) -> Result<usize> {
    if let Some(ref store) = *self.memory_store.read().await {
        store.kg_prune(older_than_days).await
    } else {
        Ok(0)
    }
}
```

- [ ] **Step 2: Update kg_list_nodes method**

Find the existing `kg_list_nodes` method (around line 577) and update its signature:

```rust
pub async fn kg_list_nodes(
    &self,
    limit: usize,
    offset: usize,
    scope: Option<&str>,  // NEW parameter
) -> Result<Vec<loom_types::KgNode>> {
    if let Some(ref store) = *self.memory_store.read().await {
        store.kg_list_nodes(limit, offset, scope).await
    } else {
        Ok(Vec::new())
    }
}
```

- [ ] **Step 3: Update kg_search to include scope**

Find the `kg_search` method (around line 512) and update the mapping to include scope:

```rust
pub async fn kg_search(
    &self,
    query: &str,
    limit: usize,
) -> Result<Vec<loom_types::KgNode>> {
    if let Some(ref store) = *self.memory_store.read().await {
        let results = store.search_knowledge(query, limit).await?;
        Ok(results
            .into_iter()
            .map(|(name, entity_type, description, confidence)| {
                loom_types::KgNode {
                    node_id: 0,
                    name,
                    entity_type,
                    description,
                    confidence,
                    scope: "global".to_string(),  // search_knowledge doesn't return scope yet
                }
            })
            .collect())
    } else {
        Ok(Vec::new())
    }
}
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check -p loom-core`
Expected: No errors

- [ ] **Step 5: Commit**

```bash
git add backend/crates/loom-core/src/orchestrator.rs
git commit -m "feat: add cognition and prune methods to Orchestrator"
```

---

## Phase 6: Backend RPC Dispatch

### Task 7: Add new RPC methods and scope parameters

**Files:**
- Modify: `backend/crates/loom-server/src/dispatch.rs:1657-1685`

- [ ] **Step 1: Add scope parameter to kg.list**

Open `backend/crates/loom-server/src/dispatch.rs` and find the `"kg.list"` handler (around line 1657). Update it:

```rust
"kg.list" => {
    let limit = p.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as usize;
    let offset = p.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    let scope = p.get("scope").and_then(|v| v.as_str());  // NEW
    let nodes = state.orchestrator.kg_list_nodes(limit, offset, scope).await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
    Ok(json!({ "nodes": nodes }))
}
```

- [ ] **Step 2: Add cognitions.list handler**

Find the end of the Knowledge Graph section (after `"kg.edge.delete"` around line 1685) and add:

```rust
// === Cognition Records ===
"cognitions.list" => {
    let subject = p.get("subject").and_then(|v| v.as_str()).unwrap_or("USER");
    let scope = p.get("scope").and_then(|v| v.as_str());
    let limit = p.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as usize;
    let offset = p.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    let rows = state.orchestrator.cognition_list(subject, scope, limit, offset).await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
    Ok(json!({ "rows": rows }))
}
```

- [ ] **Step 3: Add cognitions.snapshots handler**

Add after `"cognitions.list"`:

```rust
"cognitions.snapshots" => {
    let cognition_id = p.get("cognition_id").and_then(|v| v.as_i64()).unwrap_or(0);
    if cognition_id == 0 {
        return Err(err(ErrorCode::InvalidRequest, "cognition_id required"));
    }
    let snapshots = state.orchestrator.cognition_snapshots(cognition_id).await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
    Ok(json!({ "snapshots": snapshots }))
}
```

- [ ] **Step 4: Add cognitions.subjects handler**

Add after `"cognitions.snapshots"`:

```rust
"cognitions.subjects" => {
    let subjects = state.orchestrator.cognition_list_subjects().await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
    Ok(json!({ "subjects": subjects }))
}
```

- [ ] **Step 5: Add kg.prune handler**

Add after `"cognitions.subjects"`:

```rust
"kg.prune" => {
    let older_than_days = p.get("older_than_days").and_then(|v| v.as_i64()).unwrap_or(30);
    let pruned_count = state.orchestrator.kg_prune(older_than_days).await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
    Ok(json!({ "pruned_count": pruned_count }))
}
```

- [ ] **Step 6: Verify compilation**

Run: `cargo check -p loom-server`
Expected: No errors

- [ ] **Step 7: Run backend tests**

Run: `cargo test -p loom-memory -p loom-core`
Expected: All tests pass

- [ ] **Step 8: Commit**

```bash
git add backend/crates/loom-server/src/dispatch.rs
git commit -m "feat: add cognition and prune RPC methods"
```

---

## Phase 7: Frontend Types & Store

### Task 8: Add TypeScript types

**Files:**
- Modify: `frontend/src/renderer/src/types/bindings.ts`

- [ ] **Step 1: Add scope field to KgNode**

Open `frontend/src/renderer/src/types/bindings.ts` and find the `KgNode` interface. Add scope field:

```typescript
export interface KgNode {
  node_id: number
  name: string
  entity_type: string
  description: string
  confidence: number
  scope: string  // NEW
}
```

- [ ] **Step 2: Add Cognition types**

Append to the file:

```typescript
export interface Cognition {
  id: number
  subject: string
  trait_name: string
  value: string
  confidence: number
  evidence_count: number
  first_seen: number
  last_updated: number
  version: number
  scope: string
}

export interface CognitionHistory {
  id: number
  version: number
  trait_name: string
  value: string
  confidence: number
  evidence_count: number
  snapshot_at: number
}
```

- [ ] **Step 3: Commit**

```bash
git add frontend/src/renderer/src/types/bindings.ts
git commit -m "feat: add Cognition types and scope field to KgNode"
```

---

### Task 9: Extend Zustand store

**Files:**
- Modify: `frontend/src/renderer/src/stores/kg.ts`

- [ ] **Step 1: Add cognition state and actions to interface**

Open `frontend/src/renderer/src/stores/kg.ts` and update the `KgSlice` interface:

```typescript
import { StateCreator } from 'zustand'
import { loomRpc } from '../services/jsonrpc'
import type { KgNode, KgEdge, KgGraph, KgStats, Cognition, CognitionHistory } from '../types/bindings'

export interface KgSlice {
  kgSearchResults: KgNode[]
  kgGraph: KgGraph | null
  kgSelectedNode: KgNode | null
  kgStats: KgStats | null
  kgNodeList: KgNode[]
  // NEW: Cognition state
  cognitionList: Cognition[]
  cognitionSubjects: string[]
  cognitionSnapshots: Record<number, CognitionHistory[]>
  // Existing actions
  kgSearch: (query: string) => Promise<void>
  kgExpandNode: (nodeName: string) => Promise<void>
  kgWalkFrom: (startName: string, maxDepth?: number) => Promise<void>
  kgLoadStats: () => Promise<void>
  kgClearGraph: () => void
  kgListNodes: (scope?: string) => Promise<void>  // UPDATED: add scope
  kgNodeDelete: (name: string) => Promise<void>
  kgEdgeDelete: (source: string, target: string, relation: string) => Promise<void>
  // NEW: Cognition actions
  cognitionListBySubject: (subject: string, scope?: string) => Promise<void>
  cognitionListSubjects: () => Promise<void>
  cognitionLoadSnapshots: (cognitionId: number) => Promise<void>
  kgPrune: (olderThanDays: number) => Promise<void>
}
```

- [ ] **Step 2: Add initial state**

Update the initial state in `createKgSlice`:

```typescript
export const createKgSlice: StateCreator<KgSlice> = (set, get) => ({
  kgSearchResults: [],
  kgGraph: null,
  kgSelectedNode: null,
  kgStats: null,
  kgNodeList: [],
  // NEW
  cognitionList: [],
  cognitionSubjects: [],
  cognitionSnapshots: {},
  // ... rest of actions
```

- [ ] **Step 3: Update kgListNodes to support scope**

Find the `kgListNodes` action and update it:

```typescript
kgListNodes: async (scope) => {
  const result = await loomRpc<{ nodes: KgNode[] }>('kg.list', { limit: 50, scope })
  set({ kgNodeList: result.nodes ?? [] })
},
```

- [ ] **Step 4: Fix kgClearGraph**

Find the `kgClearGraph` action and update it to NOT clear search results:

```typescript
kgClearGraph: () => set({ kgGraph: null, kgSelectedNode: null }),
```

- [ ] **Step 5: Fix kgNodeDelete to refresh stats**

Find the `kgNodeDelete` action and add stats refresh:

```typescript
kgNodeDelete: async (name) => {
  await loomRpc('kg.node.delete', { name })
  set(s => ({
    kgNodeList: s.kgNodeList.filter(n => n.name !== name),
    kgGraph: s.kgGraph ? {
      nodes: s.kgGraph.nodes.filter(n => n.name !== name),
      edges: s.kgGraph.edges.filter(e => e.source !== name && e.target !== name),
    } : null,
  }))
  // NEW: Refresh stats
  await get().kgLoadStats()
},
```

- [ ] **Step 6: Fix kgEdgeDelete to refresh stats**

Find the `kgEdgeDelete` action and add stats refresh:

```typescript
kgEdgeDelete: async (source, target, relation) => {
  await loomRpc('kg.edge.delete', { source, target, relation })
  set(s => ({
    kgGraph: s.kgGraph ? {
      ...s.kgGraph,
      edges: s.kgGraph.edges.filter(
        e => !(e.source === source && e.target === target && e.relation_type === relation)
      ),
    } : null,
  }))
  // NEW: Refresh stats
  await get().kgLoadStats()
},
```

- [ ] **Step 7: Add cognition actions**

Add these new actions after `kgEdgeDelete`:

```typescript
cognitionListBySubject: async (subject, scope) => {
  const result = await loomRpc<{ rows: Cognition[] }>('cognitions.list', {
    subject,
    scope,
    limit: 50,
    offset: 0,
  })
  set({ cognitionList: result.rows ?? [] })
},

cognitionListSubjects: async () => {
  const result = await loomRpc<{ subjects: string[] }>('cognitions.subjects', {})
  set({ cognitionSubjects: result.subjects ?? [] })
},

cognitionLoadSnapshots: async (cognitionId) => {
  const result = await loomRpc<{ snapshots: CognitionHistory[] }>('cognitions.snapshots', {
    cognition_id: cognitionId,
  })
  set(s => ({
    cognitionSnapshots: {
      ...s.cognitionSnapshots,
      [cognitionId]: result.snapshots ?? [],
    },
  }))
},

kgPrune: async (olderThanDays) => {
  await loomRpc('kg.prune', { older_than_days: olderThanDays })
  // Refresh stats and node list
  await get().kgLoadStats()
  await get().kgListNodes()
},
```

- [ ] **Step 8: Commit**

```bash
git add frontend/src/renderer/src/stores/kg.ts
git commit -m "feat: extend store with cognition actions and scope support"
```

---

## Phase 8: Frontend Components

### Task 10: Extract and fix KnowledgeGraphTab

**Files:**
- Create: `frontend/src/renderer/src/components/kg/KnowledgeGraphTab.tsx`
- Modify: `frontend/src/renderer/src/components/kg/KnowledgeGraphPanel.module.css`

- [ ] **Step 1: Create KnowledgeGraphTab component**

Create `frontend/src/renderer/src/components/kg/KnowledgeGraphTab.tsx` with the content from the current `KnowledgeGraphPanel` (lines 42-368), but with these changes:

**Change 1: Update ENTITY_COLORS to 7 types (lines 7-17)**

```typescript
const ENTITY_COLORS: Record<string, string> = {
  Person: '#22d3ee',
  Technology: '#a78bfa',
  Topic: '#fb923c',
  Project: '#34d399',
  Concept: '#f472b6',
  Tool: '#818cf8',
  Organization: '#fbbf24',
}
const DEFAULT_COLOR = '#9ca3af'
```

**Change 2: Add scope filter state and UI**

Add to component state (around line 57):

```typescript
const [scopeFilter, setScopeFilter] = useState<string>('all')
```

Add scope dropdown in the search row (after the search button):

```tsx
<select
  className={styles.scopeSelect}
  value={scopeFilter}
  onChange={e => {
    setScopeFilter(e.target.value)
    kgListNodes(e.target.value === 'all' ? undefined : e.target.value)
  }}
>
  <option value="all">全部</option>
  <option value="global">全局</option>
  <option value="session">会话级</option>
</select>
```

**Change 3: Update kgListNodes call to pass scope**

Find the `useEffect` that calls `kgListNodes()` (around line 64) and update:

```typescript
useEffect(() => { kgLoadStats(); kgListNodes(scopeFilter === 'all' ? undefined : scopeFilter) }, [kgLoadStats, kgListNodes, scopeFilter])
```

**Change 4: Add scope badge to entity list items**

In the entity list rendering (around line 251), add scope badge after the name:

```tsx
<div key={n.node_id || n.name} className={styles.nodeItem}>
  <div className={styles.nodeItemInfo}>
    <span className={styles.nodeItemName}>{n.name}</span>
    {n.scope && n.scope !== 'global' && (
      <span className={styles.scopeBadge}>{n.scope.slice(0, 6)}</span>
    )}
    <span
      className={styles.nodeItemType}
      style={{ color: entityColor(n.entity_type), background: entityColor(n.entity_type) + '18' }}
    >{n.entity_type}</span>
    <span className={styles.nodeItemConf}>{(n.confidence * 100).toFixed(0)}%</span>
  </div>
  {/* ... rest of item */}
</div>
```

**Change 5: Fix search-graph integration**

Update the tab switch logic (around line 67-71):

```typescript
useEffect(() => {
  if (activeTab === 'graph' && !kgGraph) {
    // If search has results, walk from first result
    if (kgSearchResults.length > 0) {
      kgWalkFrom(kgSearchResults[0].name, 2)
    } else if (kgNodeList.length > 0) {
      kgWalkFrom(kgNodeList[0].name, 2)
    }
  }
}, [activeTab, kgGraph, kgNodeList, kgSearchResults, kgWalkFrom])
```

**Change 6: Remove forced tab switch on empty search**

Find the search input's `onChange` handler (around line 211) and remove the `setActiveTab('list')` line:

```tsx
<input
  className={styles.searchInput}
  value={query}
  onChange={e => setQuery(e.target.value)}  // REMOVED: setActiveTab('list')
  onKeyDown={handleKeyDown}
  placeholder="筛选实体..."
/>
```

**Change 7: Export as default**

```typescript
export default function KnowledgeGraphTab() {
  // ... component body
}
```

- [ ] **Step 2: Add scope badge styles**

Open `frontend/src/renderer/src/components/kg/KnowledgeGraphPanel.module.css` and add:

```css
.scopeSelect {
  padding: 6px 10px;
  background: rgba(255, 255, 255, 0.05);
  border: 1px solid rgba(255, 255, 255, 0.1);
  border-radius: 6px;
  color: rgba(255, 255, 255, 0.7);
  font-size: 13px;
  margin-left: 8px;
}

.scopeBadge {
  display: inline-block;
  padding: 2px 6px;
  background: rgba(251, 191, 36, 0.15);
  color: #fbbf24;
  border-radius: 4px;
  font-size: 11px;
  margin-left: 6px;
  font-family: monospace;
}
```

- [ ] **Step 3: Commit**

```bash
git add frontend/src/renderer/src/components/kg/KnowledgeGraphTab.tsx frontend/src/renderer/src/components/kg/KnowledgeGraphPanel.module.css
git commit -m "feat: extract KnowledgeGraphTab with scope filter and bug fixes"
```

---

### Task 11: Create MaintenanceTab

**Files:**
- Create: `frontend/src/renderer/src/components/kg/MaintenanceTab.tsx`
- Modify: `frontend/src/renderer/src/components/kg/KnowledgeGraphPanel.module.css`

- [ ] **Step 1: Create MaintenanceTab component**

Create `frontend/src/renderer/src/components/kg/MaintenanceTab.tsx`:

```typescript
import { useState, useEffect } from 'react'
import { useStore } from '../../stores'
import type { Cognition, CognitionHistory } from '../../types/bindings'
import styles from './KnowledgeGraphPanel.module.css'

function formatTimestamp(ts: number): string {
  const date = new Date(ts * 1000)
  return date.toISOString().split('T')[0]
}

function ScopeBadge({ scope }: { scope: string }) {
  if (scope === 'global') return null
  return <span className={styles.scopeBadge}>{scope.slice(0, 6)}</span>
}

function EvolutionTimeline({ snapshots }: { snapshots: CognitionHistory[] }) {
  if (snapshots.length === 0) return <div className={styles.timelineEmpty}>无历史记录</div>
  
  const sorted = [...snapshots].sort((a, b) => a.version - b.version)
  
  return (
    <div className={styles.timeline}>
      {sorted.map((snap, i) => (
        <div key={snap.id} className={styles.timelineItem}>
          <div className={styles.timelineVersion}>v{snap.version}</div>
          <div className={styles.timelineContent}>
            <div className={styles.timelineValue}>{snap.value}</div>
            <div className={styles.timelineMeta}>
              {(snap.confidence * 100).toFixed(0)}% | {formatTimestamp(snap.snapshot_at)}
            </div>
          </div>
          {i < sorted.length - 1 && <div className={styles.timelineArrow}>→</div>}
        </div>
      ))}
    </div>
  )
}

function CognitionRow({ cognition }: { cognition: Cognition }) {
  const [expanded, setExpanded] = useState(false)
  const cognitionSnapshots = useStore(s => s.cognitionSnapshots)
  const cognitionLoadSnapshots = useStore(s => s.cognitionLoadSnapshots)
  
  const snapshots = cognitionSnapshots[cognition.id] ?? []
  
  const handleExpand = () => {
    if (!expanded && snapshots.length === 0) {
      cognitionLoadSnapshots(cognition.id)
    }
    setExpanded(!expanded)
  }
  
  return (
    <div className={styles.cognitionRow}>
      <div className={styles.cognitionHeader} onClick={handleExpand}>
        <button className={styles.expandToggle}>{expanded ? '▼' : '▶'}</button>
        <span className={styles.cognitionTrait}>{cognition.trait_name}</span>
        <span className={styles.cognitionValue}>{cognition.value}</span>
        <span className={styles.cognitionConf}>{(cognition.confidence * 100).toFixed(0)}%</span>
        <span className={styles.cognitionVersion}>v{cognition.version}</span>
        <ScopeBadge scope={cognition.scope} />
      </div>
      {expanded && (
        <div className={styles.cognitionExpanded}>
          <EvolutionTimeline snapshots={snapshots} />
        </div>
      )}
    </div>
  )
}

export default function MaintenanceTab() {
  const cognitionList = useStore(s => s.cognitionList)
  const cognitionSubjects = useStore(s => s.cognitionSubjects)
  const cognitionListBySubject = useStore(s => s.cognitionListBySubject)
  const cognitionListSubjects = useStore(s => s.cognitionListSubjects)
  const kgStats = useStore(s => s.kgStats)
  const kgLoadStats = useStore(s => s.kgLoadStats)
  const kgPrune = useStore(s => s.kgPrune)
  const showConfirm = useStore(s => s.showConfirm)
  
  const [subject, setSubject] = useState('USER')
  const [scopeFilter, setScopeFilter] = useState('all')
  const [pruning, setPruning] = useState(false)
  
  useEffect(() => {
    cognitionListSubjects()
    kgLoadStats()
  }, [cognitionListSubjects, kgLoadStats])
  
  useEffect(() => {
    cognitionListBySubject(subject, scopeFilter === 'all' ? undefined : scopeFilter)
  }, [subject, scopeFilter, cognitionListBySubject])
  
  const handlePrune = async () => {
    const ok = await showConfirm(
      '清理图谱',
      '确定清理 30 天以上低置信度实体？此操作不可撤销。',
      true
    )
    if (!ok) return
    setPruning(true)
    try {
      await kgPrune(30)
    } finally {
      setPruning(false)
    }
  }
  
  return (
    <div className={styles.maintenanceTab}>
      <div className={styles.section}>
        <div className={styles.sectionTitle}>认知记录</div>
        <div className={styles.filterRow}>
          <label className={styles.filterLabel}>Subject:</label>
          <select
            className={styles.scopeSelect}
            value={subject}
            onChange={e => setSubject(e.target.value)}
          >
            {cognitionSubjects.map(s => (
              <option key={s} value={s}>{s}</option>
            ))}
          </select>
          <label className={styles.filterLabel}>Scope:</label>
          <select
            className={styles.scopeSelect}
            value={scopeFilter}
            onChange={e => setScopeFilter(e.target.value)}
          >
            <option value="all">全部</option>
            <option value="global">全局</option>
            <option value="session">会话级</option>
          </select>
        </div>
        <div className={styles.cognitionList}>
          {cognitionList.length === 0 ? (
            <div className={styles.emptyState}>暂无认知记录</div>
          ) : (
            cognitionList.map(c => <CognitionRow key={c.id} cognition={c} />)
          )}
        </div>
      </div>
      
      <div className={styles.section}>
        <div className={styles.sectionTitle}>图谱维护</div>
        {kgStats && (
          <div className={styles.maintenanceStats}>
            当前统计: 实体 {kgStats.node_count}, 关系 {kgStats.edge_count}
          </div>
        )}
        <button
          className={styles.pruneBtn}
          onClick={handlePrune}
          disabled={pruning}
        >
          {pruning ? '清理中...' : '清理 30 天以上低置信度实体'}
        </button>
      </div>
    </div>
  )
}
```

- [ ] **Step 2: Add MaintenanceTab styles**

Append to `frontend/src/renderer/src/components/kg/KnowledgeGraphPanel.module.css`:

```css
.maintenanceTab {
  padding: 16px 0;
}

.section {
  margin-bottom: 32px;
}

.sectionTitle {
  font-size: 15px;
  font-weight: 600;
  color: rgba(255, 255, 255, 0.9);
  margin-bottom: 12px;
}

.filterRow {
  display: flex;
  gap: 8px;
  align-items: center;
  margin-bottom: 16px;
}

.filterLabel {
  font-size: 13px;
  color: rgba(255, 255, 255, 0.6);
}

.cognitionList {
  border: 1px solid rgba(255, 255, 255, 0.1);
  border-radius: 8px;
  overflow: hidden;
}

.cognitionRow {
  border-bottom: 1px solid rgba(255, 255, 255, 0.05);
}

.cognitionRow:last-child {
  border-bottom: none;
}

.cognitionHeader {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 12px 16px;
  cursor: pointer;
  transition: background 0.15s;
}

.cognitionHeader:hover {
  background: rgba(255, 255, 255, 0.03);
}

.expandToggle {
  background: none;
  border: none;
  color: rgba(255, 255, 255, 0.5);
  cursor: pointer;
  font-size: 10px;
  padding: 0;
  width: 12px;
}

.cognitionTrait {
  font-weight: 500;
  color: rgba(255, 255, 255, 0.9);
  min-width: 120px;
}

.cognitionValue {
  color: rgba(255, 255, 255, 0.7);
  flex: 1;
}

.cognitionConf {
  color: #22d3ee;
  font-size: 12px;
  font-weight: 500;
}

.cognitionVersion {
  color: rgba(255, 255, 255, 0.5);
  font-size: 12px;
  font-family: monospace;
}

.cognitionExpanded {
  padding: 16px 16px 16px 40px;
  background: rgba(255, 255, 255, 0.02);
}

.timeline {
  display: flex;
  align-items: flex-start;
  gap: 12px;
  flex-wrap: wrap;
}

.timelineItem {
  display: flex;
  align-items: flex-start;
  gap: 8px;
}

.timelineVersion {
  background: rgba(34, 211, 238, 0.15);
  color: #22d3ee;
  padding: 2px 8px;
  border-radius: 4px;
  font-size: 11px;
  font-family: monospace;
  font-weight: 600;
}

.timelineContent {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.timelineValue {
  color: rgba(255, 255, 255, 0.8);
  font-size: 13px;
}

.timelineMeta {
  color: rgba(255, 255, 255, 0.5);
  font-size: 11px;
}

.timelineArrow {
  color: rgba(255, 255, 255, 0.3);
  font-size: 16px;
  line-height: 1;
  margin-top: 4px;
}

.timelineEmpty {
  color: rgba(255, 255, 255, 0.4);
  font-size: 13px;
  font-style: italic;
}

.emptyState {
  padding: 24px;
  text-align: center;
  color: rgba(255, 255, 255, 0.4);
  font-size: 13px;
}

.maintenanceStats {
  color: rgba(255, 255, 255, 0.7);
  font-size: 13px;
  margin-bottom: 12px;
}

.pruneBtn {
  padding: 10px 16px;
  background: rgba(239, 68, 68, 0.15);
  color: #ef4444;
  border: 1px solid rgba(239, 68, 68, 0.3);
  border-radius: 6px;
  font-size: 13px;
  cursor: pointer;
  transition: all 0.15s;
}

.pruneBtn:hover:not(:disabled) {
  background: rgba(239, 68, 68, 0.25);
}

.pruneBtn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
```

- [ ] **Step 3: Commit**

```bash
git add frontend/src/renderer/src/components/kg/MaintenanceTab.tsx frontend/src/renderer/src/components/kg/KnowledgeGraphPanel.module.css
git commit -m "feat: create MaintenanceTab with cognition records and prune"
```

---

### Task 12: Refactor KnowledgeGraphPanel

**Files:**
- Modify: `frontend/src/renderer/src/components/kg/KnowledgeGraphPanel.tsx:1-368`
- Modify: `frontend/src/renderer/src/components/shared/SettingsModal.tsx:84`

- [ ] **Step 1: Simplify KnowledgeGraphPanel**

Replace the entire content of `frontend/src/renderer/src/components/kg/KnowledgeGraphPanel.tsx` with:

```typescript
import { useState } from 'react'
import KnowledgeGraphTab from './KnowledgeGraphTab'
import MaintenanceTab from './MaintenanceTab'
import styles from './KnowledgeGraphPanel.module.css'

export default function KnowledgeGraphPanel() {
  const [activeTab, setActiveTab] = useState<'kg' | 'maintenance'>('kg')
  
  return (
    <div className={styles.panel}>
      <div className={styles.mainTabs}>
        <button
          className={`${styles.mainTab} ${activeTab === 'kg' ? styles.mainTabActive : ''}`}
          onClick={() => setActiveTab('kg')}
        >知识图谱</button>
        <button
          className={`${styles.mainTab} ${activeTab === 'maintenance' ? styles.mainTabActive : ''}`}
          onClick={() => setActiveTab('maintenance')}
        >维护</button>
      </div>
      
      {activeTab === 'kg' && <KnowledgeGraphTab />}
      {activeTab === 'maintenance' && <MaintenanceTab />}
    </div>
  )
}
```

- [ ] **Step 2: Add main tab styles**

Add to `frontend/src/renderer/src/components/kg/KnowledgeGraphPanel.module.css`:

```css
.mainTabs {
  display: flex;
  gap: 4px;
  margin-bottom: 20px;
  border-bottom: 1px solid rgba(255, 255, 255, 0.1);
}

.mainTab {
  padding: 10px 16px;
  background: none;
  border: none;
  color: rgba(255, 255, 255, 0.6);
  font-size: 14px;
  cursor: pointer;
  transition: all 0.15s;
  border-bottom: 2px solid transparent;
  margin-bottom: -1px;
}

.mainTab:hover {
  color: rgba(255, 255, 255, 0.9);
}

.mainTabActive {
  color: rgba(255, 255, 255, 0.9);
  border-bottom-color: #22d3ee;
}
```

- [ ] **Step 3: Update SettingsModal label**

Open `frontend/src/renderer/src/components/shared/SettingsModal.tsx:84` and change the label:

```typescript
{ id: 'kg', label: '记忆系统' },  // CHANGED from '认知图谱'
```

- [ ] **Step 4: Commit**

```bash
git add frontend/src/renderer/src/components/kg/KnowledgeGraphPanel.tsx frontend/src/renderer/src/components/shared/SettingsModal.tsx frontend/src/renderer/src/components/kg/KnowledgeGraphPanel.module.css
git commit -m "feat: refactor KnowledgeGraphPanel into two tabs and rename to 记忆系统"
```

---

## Phase 9: Final Testing

### Task 13: Integration testing

- [ ] **Step 1: Run backend tests**

Run: `cargo test -p loom-memory -p loom-core -p loom-server`
Expected: All tests pass

- [ ] **Step 2: Build backend**

Run: `cargo build -p loom-server`
Expected: Build succeeds

- [ ] **Step 3: Check frontend compilation**

Run: `cd frontend && npm run build`
Expected: Build succeeds

- [ ] **Step 4: Manual testing**

Start the application and verify:
1. Settings panel shows "记忆系统" label
2. Knowledge Graph tab displays entities with scope badges
3. Deletion operations refresh statistics immediately
4. Legend shows exactly 7 entity types
5. Search results can transition to graph view
6. Maintenance tab shows cognition records
7. Cognition expansion shows evolution timeline
8. Prune button works and refreshes stats

- [ ] **Step 5: Final commit**

```bash
git add -A
git commit -m "feat: complete memory system panel redesign"
```

---

## Summary

This plan implements the memory system panel redesign in 13 tasks across 9 phases:

1. **Backend types** (Task 1): Add scope field and cognition types
2. **Backend storage** (Tasks 2-3): Update GraphStore and CognitionStore
3. **Backend trait** (Tasks 4-5): Extend MemoryStore trait and implementation
4. **Backend orchestrator** (Task 6): Add new orchestrator methods
5. **Backend RPC** (Task 7): Add 4 new RPC methods
6. **Frontend types** (Task 8): Add TypeScript types
7. **Frontend store** (Task 9): Extend Zustand store
8. **Frontend components** (Tasks 10-12): Create new components
9. **Testing** (Task 13): Integration testing

Total: ~30-40 minutes of implementation time following TDD principles with frequent commits.

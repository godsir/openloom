# Token 消耗统计仪表盘 — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在设置页面新增"用量"Tab，展示完整 Token 消耗统计仪表盘（KPI 卡片 + 折线图 + 饼图 + 柱状图 + 明细表），数据来自 SQLite 持久化 + WebSocket 实时推送。

**Architecture:** 后端在 MemoryStore trait 新增 record_token_usage 和两个查询方法，orchestrator 在 TokenUsage 事件时调用持久化。前端新增 tokenStats Zustand slice + TokenUsagePanel 组件，使用 ECharts 渲染图表。

**Tech Stack:** Rust (rusqlite, refinery migration), React 19 + ECharts 5 + CSS Module + Zustand

---

### Task 1: DB Migration V14

**Files:**
- Create: `migrations/V14__token_usage.sql`

- [ ] **Step 1: Create migration SQL file**

```sql
CREATE TABLE token_usage (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id         TEXT NOT NULL,
    model              TEXT NOT NULL,
    prompt_tokens      INTEGER NOT NULL DEFAULT 0,
    completion_tokens  INTEGER NOT NULL DEFAULT 0,
    cached_tokens      INTEGER NOT NULL DEFAULT 0,
    latency_ms         INTEGER NOT NULL DEFAULT 0,
    context_window     INTEGER NOT NULL DEFAULT 0,
    created_at         TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_token_usage_model ON token_usage(model);
CREATE INDEX idx_token_usage_created ON token_usage(created_at);
```

- [ ] **Step 2: Verify migration compiles**

Run: `cargo check -p loom-memory`

Expected: Compiles cleanly (refinery embed_migrations picks up new file)

- [ ] **Step 3: Commit**

```bash
git add migrations/V14__token_usage.sql
git commit -m "feat: add token_usage table migration V14"
```

---

### Task 2: Extend MemoryStore trait + implement persistence

**Files:**
- Modify: `backend/crates/loom-core/src/orchestrator.rs:46-129` (trait)
- Modify: `backend/crates/lume-cli/src/memory.rs` (LoomMemoryStore impl)

- [ ] **Step 1: Add trait methods to MemoryStore**

In `orchestrator.rs`, inside the `MemoryStore` trait (after `rename_session` at ~line 129), add:

```rust
    // Token usage tracking
    async fn record_token_usage(
        &self,
        session_id: &str,
        model: &str,
        prompt_tokens: usize,
        completion_tokens: usize,
        cached_tokens: usize,
        latency_ms: u64,
        context_window: usize,
    ) -> Result<()>;
    async fn get_token_summary(
        &self,
        from: &str,
        to: &str,
    ) -> Result<serde_json::Value>;
    async fn get_token_history(
        &self,
        from: &str,
        to: &str,
        granularity: &str,
    ) -> Result<serde_json::Value>;
```

- [ ] **Step 2: Implement in LoomMemoryStore**

In `lume-cli/src/memory.rs`, add these methods to the `impl MemoryStore for LoomMemoryStore` block (before the closing `}`):

```rust
    async fn record_token_usage(
        &self,
        session_id: &str,
        model: &str,
        prompt_tokens: usize,
        completion_tokens: usize,
        cached_tokens: usize,
        latency_ms: u64,
        context_window: usize,
    ) -> Result<()> {
        let store = self.store.lock().unwrap();
        store.conn().execute(
            "INSERT INTO token_usage (session_id, model, prompt_tokens, completion_tokens, cached_tokens, latency_ms, context_window) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![session_id, model, prompt_tokens as i64, completion_tokens as i64, cached_tokens as i64, latency_ms as i64, context_window as i64],
        )?;
        Ok(())
    }

    async fn get_token_summary(&self, from: &str, to: &str) -> Result<serde_json::Value> {
        let store = self.store.lock().unwrap();
        let conn = store.conn();

        let totals: (i64, i64, i64, i64, f64) = conn.query_row(
            "SELECT COALESCE(SUM(prompt_tokens), 0), COALESCE(SUM(completion_tokens), 0), COALESCE(SUM(cached_tokens), 0), COUNT(*), COALESCE(AVG(latency_ms), 0) FROM token_usage WHERE created_at >= ?1 AND created_at <= ?2",
            rusqlite::params![from, to],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
        )?;

        let cache_total: i64 = conn.query_row(
            "SELECT COALESCE(SUM(prompt_tokens), 0) FROM token_usage WHERE created_at >= ?1 AND created_at <= ?2",
            rusqlite::params![from, to],
            |row| row.get(0),
        )?;

        let cache_hit_rate = if totals.0 > 0 { totals.2 as f64 / cache_total as f64 } else { 0.0 };

        let mut stmt = conn.prepare(
            "SELECT model, SUM(prompt_tokens) as p, SUM(completion_tokens) as c, SUM(cached_tokens) as ca, COUNT(*) as r, AVG(latency_ms) as l, AVG(CAST(prompt_tokens AS REAL) / NULLIF(context_window, 0)) as cu FROM token_usage WHERE created_at >= ?1 AND created_at <= ?2 GROUP BY model ORDER BY r DESC",
        )?;
        let by_model: Vec<serde_json::Value> = stmt
            .query_map(rusqlite::params![from, to], |row| {
                Ok(serde_json::json!({
                    "model": row.get::<_, String>(0)?,
                    "prompt": row.get::<_, i64>(1)?,
                    "completion": row.get::<_, i64>(2)?,
                    "cached": row.get::<_, i64>(3)?,
                    "requests": row.get::<_, i64>(4)?,
                    "avg_latency_ms": (row.get::<_, f64>(5)? * 10.0).round() / 10.0,
                    "avg_context_utilization": (row.get::<_, f64>(6)? * 100.0).round() / 100.0,
                }))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(serde_json::json!({
            "total_prompt_tokens": totals.0,
            "total_completion_tokens": totals.1,
            "total_cached_tokens": totals.2,
            "total_requests": totals.3,
            "avg_latency_ms": (totals.4 * 10.0).round() / 10.0,
            "cache_hit_rate": (cache_hit_rate * 100.0).round() / 100.0,
            "by_model": by_model,
        }))
    }

    async fn get_token_history(&self, from: &str, to: &str, granularity: &str) -> Result<serde_json::Value> {
        let store = self.store.lock().unwrap();
        let conn = store.conn();

        let date_format = match granularity {
            "hour" => "%Y-%m-%d %H:00",
            "week" => "%Y-%W",
            _ => "%Y-%m-%d", // day
        };

        let sql = format!(
            "SELECT strftime('{}', created_at) as bucket, model, SUM(prompt_tokens) as p, SUM(completion_tokens) as c, SUM(cached_tokens) as ca, COUNT(*) as cnt FROM token_usage WHERE created_at >= ?1 AND created_at <= ?2 GROUP BY bucket, model ORDER BY bucket ASC",
            date_format
        );

        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params![from, to], |row| {
            Ok((
                row.get::<_, String>(0)?, // bucket
                row.get::<_, String>(1)?, // model
                row.get::<_, i64>(2)?,    // prompt
                row.get::<_, i64>(3)?,    // completion
                row.get::<_, i64>(4)?,    // cached
                row.get::<_, i64>(5)?,    // count
            ))
        })?;

        let mut buckets: std::collections::BTreeMap<String, serde_json::Value> = std::collections::BTreeMap::new();
        for row in rows {
            let (bucket, model, p, c, ca, cnt) = row?;
            let entry = buckets.entry(bucket.clone()).or_insert_with(|| serde_json::json!({
                "date": bucket,
                "prompt": 0,
                "completion": 0,
                "cached": 0,
                "requests": 0,
                "by_model": {},
            }));
            entry["prompt"] = serde_json::json!(entry["prompt"].as_i64().unwrap_or(0) + p);
            entry["completion"] = serde_json::json!(entry["completion"].as_i64().unwrap_or(0) + c);
            entry["cached"] = serde_json::json!(entry["cached"].as_i64().unwrap_or(0) + ca);
            entry["requests"] = serde_json::json!(entry["requests"].as_i64().unwrap_or(0) + cnt);
            entry["by_model"][&model] = serde_json::json!({
                "prompt": p,
                "completion": c,
                "requests": cnt,
            });
        }

        Ok(serde_json::json!({
            "points": buckets.into_values().collect::<Vec<_>>(),
        }))
    }
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p loom-core -p lume-cli`

Expected: Compiles with no errors

- [ ] **Step 4: Commit**

```bash
git add backend/crates/loom-core/src/orchestrator.rs backend/crates/lume-cli/src/memory.rs
git commit -m "feat: add token usage persistence methods to MemoryStore trait and LoomMemoryStore"
```

---

### Task 3: Call record_token_usage from orchestrator

**Files:**
- Modify: `backend/crates/loom-core/src/orchestrator.rs:1316-1325`

- [ ] **Step 1: Add persistence call after TokenUsage event publish**

In `orchestrator.rs`, find the `StreamDelta::Usage { .. } => {` block around line 1316. Replace the block with:

```rust
                    StreamDelta::Usage {
                        prompt_tokens,
                        completion_tokens,
                        cached_tokens,
                        latency_ms,
                        ..
                    } => {
                        let _ = event_bus.publish(AgentEvent::TokenUsage {
                            agent_id: forward_agent_id.clone(),
                            session_id: forward_session_id.clone(),
                            model: usage_model.clone(),
                            prompt_tokens: prompt_tokens as usize,
                            completion_tokens: completion_tokens as usize,
                            context_window: usage_ctx,
                        });
                        // Persist token usage to SQLite for historical stats
                        if let Some(store) = &*self.memory_store.read().await {
                            let _ = store.record_token_usage(
                                &forward_session_id,
                                &usage_model,
                                prompt_tokens as usize,
                                completion_tokens as usize,
                                cached_tokens as usize,
                                latency_ms,
                                usage_ctx,
                            ).await;
                        }
                    }
```

- [ ] **Step 2: Verify StreamDelta has cached_tokens and latency_ms fields**

Run: `cargo check -p loom-core`

If `StreamDelta::Usage` doesn't have `cached_tokens` and `latency_ms` fields, use 0 for both.

- [ ] **Step 3: Commit**

```bash
git add backend/crates/loom-core/src/orchestrator.rs
git commit -m "feat: persist TokenUsage to SQLite on each turn"
```

---

### Task 4: Add JSON-RPC API handlers

**Files:**
- Modify: `backend/crates/loom-server/src/dispatch.rs`

- [ ] **Step 1: Add stats.token_summary and stats.token_history handlers**

In `dispatch.rs`, inside `dispatch_method`, before the `// Fallback` comment (line ~1678), add:

```rust
        // === Token Usage Stats ===
        "stats.token_summary" => {
            let from = p.get("from").and_then(|v| v.as_str()).unwrap_or("1970-01-01");
            let to = p.get("to").and_then(|v| v.as_str()).unwrap_or("2099-12-31");
            let summary = match state.orchestrator.memory_store().await {
                Some(store) => store.get_token_summary(from, to).await
                    .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?,
                None => serde_json::json!({
                    "total_prompt_tokens": 0, "total_completion_tokens": 0,
                    "total_cached_tokens": 0, "total_requests": 0,
                    "avg_latency_ms": 0, "cache_hit_rate": 0, "by_model": []
                }),
            };
            Ok(summary)
        }

        "stats.token_history" => {
            let from = p.get("from").and_then(|v| v.as_str()).unwrap_or("1970-01-01");
            let to = p.get("to").and_then(|v| v.as_str()).unwrap_or("2099-12-31");
            let granularity = p.get("granularity").and_then(|v| v.as_str()).unwrap_or("day");
            let history = match state.orchestrator.memory_store().await {
                Some(store) => store.get_token_history(from, to, granularity).await
                    .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?,
                None => serde_json::json!({ "points": [] }),
            };
            Ok(history)
        }
```

- [ ] **Step 2: Add token query methods to Orchestrator**

The orchestrator already holds `memory_store: Arc<RwLock<Option<Box<dyn MemoryStore>>>>` and follows the pattern of `ensure_session_persisted` etc. Add these methods to the Orchestrator impl (after `delete_session_persisted`):

```rust
    pub async fn token_summary(&self, from: &str, to: &str) -> Result<serde_json::Value> {
        match &*self.memory_store.read().await {
            Some(store) => store.get_token_summary(from, to).await,
            None => Ok(serde_json::json!({
                "total_prompt_tokens": 0, "total_completion_tokens": 0,
                "total_cached_tokens": 0, "total_requests": 0,
                "avg_latency_ms": 0, "cache_hit_rate": 0, "by_model": []
            })),
        }
    }

    pub async fn token_history(&self, from: &str, to: &str, granularity: &str) -> Result<serde_json::Value> {
        match &*self.memory_store.read().await {
            Some(store) => store.get_token_history(from, to, granularity).await,
            None => Ok(serde_json::json!({ "points": [] })),
        }
    }
```

- [ ] **Step 3: Update dispatch.rs handlers to call orchestrator methods**

```rust
        // === Token Usage Stats ===
        "stats.token_summary" => {
            let from = p.get("from").and_then(|v| v.as_str()).unwrap_or("1970-01-01");
            let to = p.get("to").and_then(|v| v.as_str()).unwrap_or("2099-12-31");
            let summary = state.orchestrator.token_summary(from, to).await
                .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
            Ok(summary)
        }

        "stats.token_history" => {
            let from = p.get("from").and_then(|v| v.as_str()).unwrap_or("1970-01-01");
            let to = p.get("to").and_then(|v| v.as_str()).unwrap_or("2099-12-31");
            let granularity = p.get("granularity").and_then(|v| v.as_str()).unwrap_or("day");
            let history = state.orchestrator.token_history(from, to, granularity).await
                .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
            Ok(history)
        }
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check -p loom-core -p loom-server`

Expected: Compiles cleanly

- [ ] **Step 5: Commit**

```bash
git add backend/crates/loom-core/src/orchestrator.rs backend/crates/loom-server/src/dispatch.rs
git commit -m "feat: add stats.token_summary and stats.token_history JSON-RPC APIs"
```

---

### Task 5: Update WS forwarding to include cached_tokens and latency_ms

**Files:**
- Modify: `backend/crates/loom-server/src/ws.rs:145`

- [ ] **Step 1: Add cached_tokens and latency_ms to WS notification**

In `ws.rs`, find the `AgentEvent::TokenUsage { .. }` match arm (~line 145) and update:

```rust
        AgentEvent::TokenUsage { agent_id: _, session_id, model, prompt_tokens, completion_tokens, context_window } => {
            json!({
                "session_id": session_id,
                "model": model,
                "prompt_tokens": prompt_tokens,
                "completion_tokens": completion_tokens,
                "context_window": context_window,
                "cached_tokens": 0,
                "latency_ms": 0,
            })
        }
```

First check if `TokenUsage` event in event_bus has `cached_tokens` and `latency_ms`. If the event_bus AgentEvent::TokenUsage doesn't have them, add them:

In `backend/crates/loom-core/src/event_bus.rs`, update the TokenUsage variant:

```rust
    TokenUsage {
        agent_id: AgentId,
        session_id: String,
        model: String,
        prompt_tokens: usize,
        completion_tokens: usize,
        cached_tokens: usize,
        latency_ms: u64,
        context_window: usize,
    },
```

Then in `orchestrator.rs`, update the publish call to include these fields. And in `ws.rs`, update the destructure and json output.

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p loom-core -p loom-server`

- [ ] **Step 3: Commit**

```bash
git add backend/crates/loom-core/src/event_bus.rs backend/crates/loom-core/src/orchestrator.rs backend/crates/loom-server/src/ws.rs
git commit -m "feat: include cached_tokens and latency_ms in TokenUsage event"
```

---

### Task 6: Install ECharts dependency

**Files:**
- Modify: `frontend/package.json`

- [ ] **Step 1: Install echarts and echarts-for-react**

```bash
cd frontend && npm install echarts echarts-for-react
```

- [ ] **Step 2: Verify install**

Run: `ls frontend/node_modules/echarts/package.json`
Expected: File exists

- [ ] **Step 3: Commit**

```bash
git add frontend/package.json frontend/package-lock.json
git commit -m "chore: add echarts and echarts-for-react dependencies"
```

---

### Task 7: Create tokenStats Zustand slice

**Files:**
- Create: `frontend/src/renderer/src/stores/tokenStats.ts`

- [ ] **Step 1: Create the store slice**

```typescript
import { StateCreator } from 'zustand'
import { loomRpc } from '../services/jsonrpc'

export interface TokenUsageRecord {
  session_id: string
  model: string
  prompt: number
  completion: number
  cached: number
  latency_ms: number
  context_window: number
}

export interface TokenSummary {
  total_prompt_tokens: number
  total_completion_tokens: number
  total_cached_tokens: number
  total_requests: number
  avg_latency_ms: number
  cache_hit_rate: number
  by_model: Array<{
    model: string
    prompt: number
    completion: number
    cached: number
    requests: number
    avg_latency_ms: number
    avg_context_utilization: number
  }>
}

export interface TokenHistoryPoint {
  date: string
  prompt: number
  completion: number
  cached: number
  requests: number
  by_model: Record<string, { prompt: number; completion: number; requests: number }>
}

export interface TokenStatsSlice {
  // Real-time session counters (current process lifetime)
  sessionTotal: { prompt: number; completion: number; cached: number; requests: number }
  sessionByModel: Record<string, { prompt: number; completion: number; cached: number; requests: number }>

  // Historical data from backend
  summary: TokenSummary | null
  history: TokenHistoryPoint[]
  loading: boolean
  timeRange: 'all' | '7d' | '30d'

  // Actions
  recordUsage: (usage: TokenUsageRecord) => void
  loadSummary: (from: string, to: string) => Promise<void>
  loadHistory: (from: string, to: string, granularity: string) => Promise<void>
  setTimeRange: (range: 'all' | '7d' | '30d') => void
}

export const createTokenStatsSlice: StateCreator<TokenStatsSlice> = (set, get) => ({
  sessionTotal: { prompt: 0, completion: 0, cached: 0, requests: 0 },
  sessionByModel: {},
  summary: null,
  history: [],
  loading: false,
  timeRange: 'all',

  recordUsage: (usage) => {
    set((s) => {
      const total = {
        prompt: s.sessionTotal.prompt + usage.prompt,
        completion: s.sessionTotal.completion + usage.completion,
        cached: s.sessionTotal.cached + usage.cached,
        requests: s.sessionTotal.requests + 1,
      }
      const prev = s.sessionByModel[usage.model] || { prompt: 0, completion: 0, cached: 0, requests: 0 }
      const byModel = {
        ...s.sessionByModel,
        [usage.model]: {
          prompt: prev.prompt + usage.prompt,
          completion: prev.completion + usage.completion,
          cached: prev.cached + usage.cached,
          requests: prev.requests + 1,
        },
      }
      return { sessionTotal: total, sessionByModel: byModel }
    })
  },

  loadSummary: async (from, to) => {
    set({ loading: true })
    try {
      const data = await loomRpc<TokenSummary>('stats.token_summary', { from, to })
      set({ summary: data, loading: false })
    } catch {
      set({ loading: false })
    }
  },

  loadHistory: async (from, to, granularity) => {
    set({ loading: true })
    try {
      const data = await loomRpc<{ points: TokenHistoryPoint[] }>('stats.token_history', { from, to, granularity })
      set({ history: data.points || [], loading: false })
    } catch {
      set({ loading: false })
    }
  },

  setTimeRange: (range) => {
    set({ timeRange: range })
    const now = new Date()
    let from = '1970-01-01'
    if (range === '7d') {
      const d = new Date(now.getTime() - 7 * 86400000)
      from = d.toISOString().slice(0, 10)
    } else if (range === '30d') {
      const d = new Date(now.getTime() - 30 * 86400000)
      from = d.toISOString().slice(0, 10)
    }
    const to = now.toISOString().slice(0, 10)
    get().loadSummary(from, to)
    get().loadHistory(from, to, 'day')
  },
})
```

- [ ] **Step 2: Commit**

```bash
git add frontend/src/renderer/src/stores/tokenStats.ts
git commit -m "feat: add tokenStats Zustand slice with real-time aggregation and history queries"
```

---

### Task 8: Register TokenStatsSlice in store index

**Files:**
- Modify: `frontend/src/renderer/src/stores/index.ts`

- [ ] **Step 1: Import and add to AppStore type and create call**

In `index.ts`:

Add import at top:
```typescript
import { createTokenStatsSlice, TokenStatsSlice } from './tokenStats'
```

Add to `AppStore` type:
```typescript
export type AppStore = ConnectionSlice &
  UiSlice &
  ModelSlice &
  AgentSlice &
  SessionSlice &
  ChatSlice &
  StreamingSlice &
  InputSlice &
  SelectionSlice &
  ToastSlice &
  ConfirmSlice &
  KgSlice &
  LightboxSlice &
  TokenStatsSlice
```

Add to `create` call:
```typescript
export const useStore = create<AppStore>()((...a) => ({
  ...createConnectionSlice(...a),
  ...createUiSlice(...a),
  ...createModelSlice(...a),
  ...createAgentSlice(...a),
  ...createSessionSlice(...a),
  ...createChatSlice(...a),
  ...createStreamingSlice(...a),
  ...createInputSlice(...a),
  ...createSelectionSlice(...a),
  ...createToastSlice(...a),
  ...createConfirmSlice(...a),
  ...createKgSlice(...a),
  ...createLightboxSlice(...a),
  ...createTokenStatsSlice(...a),
}))
```

- [ ] **Step 2: Verify TypeScript compilation**

Run: `cd frontend && npx tsc --noEmit`

Expected: No type errors

- [ ] **Step 3: Commit**

```bash
git add frontend/src/renderer/src/stores/index.ts
git commit -m "feat: register TokenStatsSlice in store"
```

---

### Task 9: Wire token_usage WS events to tokenStats slice

**Files:**
- Modify: `frontend/src/renderer/src/services/bootstrap.ts:43-58`

- [ ] **Step 1: Add recordUsage call in token_usage handler**

In `bootstrap.ts`, inside the `case 'chat.token_usage':` block (~line 43), add after `useStore.getState().setSessionUsage(sessionId, usage)`:

```typescript
          // Also aggregate into token stats
          useStore.getState().recordUsage({
            session_id: sessionId,
            model: usage.model,
            prompt: usage.prompt,
            completion: usage.completion,
            cached: (p.cached_tokens as number) || 0,
            latency_ms: (p.latency_ms as number) || 0,
            context_window: usage.contextWindow,
          })
```

- [ ] **Step 2: Verify TypeScript compilation**

Run: `cd frontend && npx tsc --noEmit`

Expected: No type errors

- [ ] **Step 3: Commit**

```bash
git add frontend/src/renderer/src/services/bootstrap.ts
git commit -m "feat: wire WS token_usage events to tokenStats slice"
```

---

### Task 10: Create TokenUsagePanel component + styles

**Files:**
- Create: `frontend/src/renderer/src/components/shared/TokenUsagePanel.tsx`
- Create: `frontend/src/renderer/src/components/shared/TokenUsagePanel.module.css`

- [ ] **Step 1: Create CSS module**

```css
.panel {
  display: flex;
  flex-direction: column;
  gap: 20px;
}

/* ── KPI cards row ── */
.kpiRow {
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  gap: 12px;
}

.kpiCard {
  padding: 16px;
  border-radius: var(--r-md);
  background: rgba(255, 255, 255, 0.02);
  border: 1px solid var(--border);
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.kpiLabel {
  font-size: 10px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.5px;
  color: var(--text-muted);
}

.kpiValue {
  font-size: 24px;
  font-weight: 700;
  color: var(--text);
  font-family: var(--font-mono);
}

.kpiValueAccent {
  color: var(--accent);
}

.kpiSub {
  font-size: 10px;
  color: var(--text-muted);
}

/* ── Time range selector ── */
.timeRangeRow {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.timeRangeToggle {
  display: flex;
  gap: 0;
  border: 1px solid var(--border);
  border-radius: var(--r-sm);
  overflow: hidden;
}

.timeRangeBtn {
  height: 28px;
  padding: 0 14px;
  font-size: 11px;
  font-weight: 500;
  color: var(--text-secondary);
  background: none;
  border: none;
  cursor: pointer;
  transition: all 0.15s ease;
}

.timeRangeBtn:hover {
  color: var(--text);
  background: rgba(255, 255, 255, 0.03);
}

.timeRangeBtnActive {
  background: var(--accent-subtle);
  color: var(--accent);
}

/* ── Charts grid ── */
.chartsGrid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 16px;
}

.chartCard {
  padding: 16px;
  border-radius: var(--r-md);
  background: rgba(255, 255, 255, 0.02);
  border: 1px solid var(--border);
}

.chartTitle {
  font-size: 11px;
  font-weight: 600;
  color: var(--text-secondary);
  margin: 0 0 12px;
}

.chartFull {
  grid-column: 1 / -1;
  height: 320px;
}

/* ── Model table ── */
.modelTable {
  width: 100%;
  border-collapse: collapse;
}

.modelTable th {
  text-align: left;
  padding: 8px 12px;
  font-size: 10px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.5px;
  color: var(--text-muted);
  border-bottom: 1px solid var(--border);
}

.modelTable td {
  padding: 8px 12px;
  font-size: 11px;
  color: var(--text-secondary);
  border-bottom: 1px solid var(--border);
  font-family: var(--font-mono);
}

.modelTableCell {
  font-family: inherit;
  font-size: 12px;
  color: var(--text);
  font-weight: 500;
}

.modelTable tr:hover td {
  background: rgba(255, 255, 255, 0.02);
}

/* ── Empty state ── */
.emptyState {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 48px 24px;
  text-align: center;
}

.emptyIcon {
  font-size: 32px;
  margin-bottom: 12px;
  opacity: 0.3;
}

.emptyTitle {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-secondary);
  margin: 0 0 4px;
}

.emptyDesc {
  font-size: 11px;
  color: var(--text-muted);
  margin: 0;
}
```

- [ ] **Step 2: Create TokenUsagePanel component**

```tsx
import { useEffect, useMemo } from 'react'
import ReactECharts from 'echarts-for-react'
import { useStore } from '../../stores'
import styles from './TokenUsagePanel.module.css'

function formatNumber(n: number): string {
  if (n >= 1_000_000) return (n / 1_000_000).toFixed(1) + 'M'
  if (n >= 1_000) return (n / 1_000).toFixed(1) + 'K'
  return n.toLocaleString()
}

const CHART_BASE_OPTIONS = {
  backgroundColor: 'transparent',
  textStyle: { color: '#94a3b8', fontSize: 11 },
  grid: { top: 40, right: 16, bottom: 24, left: 48 },
  tooltip: {
    backgroundColor: '#1e293b',
    borderColor: '#334155',
    textStyle: { color: '#e2e8f0', fontSize: 12 },
  },
}

export default function TokenUsagePanel() {
  const sessionTotal = useStore((s) => s.sessionTotal)
  const summary = useStore((s) => s.summary)
  const history = useStore((s) => s.history)
  const loading = useStore((s) => s.loading)
  const timeRange = useStore((s) => s.timeRange)
  const setTimeRange = useStore((s) => s.setTimeRange)

  useEffect(() => {
    setTimeRange('all')
  }, [])

  // ── Trend chart option ──
  const trendOption = useMemo(() => {
    const dates = history.map((p) => p.date)
    const promptSeries = history.map((p) => p.prompt)
    const completionSeries = history.map((p) => p.completion)
    const cachedSeries = history.map((p) => p.cached)

    return {
      ...CHART_BASE_OPTIONS,
      legend: { data: ['Prompt', 'Completion', 'Cached'], textStyle: { color: '#94a3b8' } },
      xAxis: { type: 'category' as const, data: dates, axisLine: { lineStyle: { color: '#334155' } } },
      yAxis: { type: 'value' as const, axisLine: { lineStyle: { color: '#334155' } }, splitLine: { lineStyle: { color: '#1e293b' } } },
      series: [
        { name: 'Prompt', type: 'line', data: promptSeries, smooth: true, lineStyle: { color: '#22d3ee' }, itemStyle: { color: '#22d3ee' }, symbol: 'none' },
        { name: 'Completion', type: 'line', data: completionSeries, smooth: true, lineStyle: { color: '#a78bfa' }, itemStyle: { color: '#a78bfa' }, symbol: 'none' },
        { name: 'Cached', type: 'line', data: cachedSeries, smooth: true, lineStyle: { color: '#34d399', type: 'dashed' }, itemStyle: { color: '#34d399' }, symbol: 'none' },
      ],
    }
  }, [history])

  // ── Model distribution pie ──
  const pieOption = useMemo(() => {
    const data = (summary?.by_model || []).map((m) => ({
      name: m.model,
      value: m.prompt + m.completion,
    }))
    return {
      ...CHART_BASE_OPTIONS,
      tooltip: { ...CHART_BASE_OPTIONS.tooltip, trigger: 'item' as const, formatter: '{b}: {c} ({d}%)' },
      grid: undefined,
      series: [{
        type: 'pie' as const,
        radius: ['50%', '78%'],
        center: ['50%', '50%'],
        data,
        emphasis: { itemStyle: { shadowBlur: 10 } },
        label: { color: '#94a3b8', fontSize: 10 },
      }],
    }
  }, [summary])

  // ── Model ranking bar ──
  const barOption = useMemo(() => {
    const models = (summary?.by_model || []).map((m) => m.model)
    const counts = (summary?.by_model || []).map((m) => m.requests)
    return {
      ...CHART_BASE_OPTIONS,
      xAxis: { type: 'value' as const, axisLine: { lineStyle: { color: '#334155' } }, splitLine: { lineStyle: { color: '#1e293b' } } },
      yAxis: { type: 'category' as const, data: models, axisLine: { lineStyle: { color: '#334155' } } },
      series: [{
        type: 'bar' as const,
        data: counts.map((v, i) => ({ value: v, itemStyle: { color: i === 0 ? '#22d3ee' : '#475569' } })),
        barWidth: 16,
      }],
    }
  }, [summary])

  const hasData = (summary && summary.total_requests > 0) || sessionTotal.requests > 0

  return (
    <>
      <div className={styles.panel}>
        {/* ── KPI Row ── */}
        <div className={styles.kpiRow}>
          <div className={styles.kpiCard}>
            <span className={styles.kpiLabel}>总消耗</span>
            <span className={styles.kpiValue}>
              {formatNumber((summary?.total_prompt_tokens || 0) + (summary?.total_completion_tokens || 0))}
            </span>
            <span className={styles.kpiSub}>
              P: {formatNumber(summary?.total_prompt_tokens || 0)} / C: {formatNumber(summary?.total_completion_tokens || 0)}
            </span>
          </div>
          <div className={styles.kpiCard}>
            <span className={styles.kpiLabel}>请求次数</span>
            <span className={styles.kpiValue}>{formatNumber(summary?.total_requests || 0)}</span>
          </div>
          <div className={styles.kpiCard}>
            <span className={styles.kpiLabel}>缓存命中率</span>
            <span className={`${styles.kpiValue} ${styles.kpiValueAccent}`}>
              {summary?.cache_hit_rate != null ? (summary.cache_hit_rate * 100).toFixed(1) + '%' : '—'}
            </span>
            <span className={styles.kpiSub}>Cached: {formatNumber(summary?.total_cached_tokens || 0)}</span>
          </div>
          <div className={styles.kpiCard}>
            <span className={styles.kpiLabel}>平均延迟</span>
            <span className={styles.kpiValue}>
              {summary?.avg_latency_ms != null ? (summary.avg_latency_ms / 1000).toFixed(1) + 's' : '—'}
            </span>
          </div>
        </div>

        {/* ── Session real-time badges ── */}
        {sessionTotal.requests > 0 && (
          <div style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
            <span style={{ fontSize: 10, color: 'var(--text-muted)' }}>本次会话已消耗：</span>
            <span style={{ fontSize: 11, fontWeight: 600, color: 'var(--accent)', fontFamily: 'var(--font-mono)' }}>
              {formatNumber(sessionTotal.prompt + sessionTotal.completion)} tokens ({sessionTotal.requests} 请求)
            </span>
          </div>
        )}

        {/* ── Time range selector ── */}
        <div className={styles.timeRangeRow}>
          <span style={{ fontSize: 11, color: 'var(--text-muted)' }}>
            {loading ? '加载中...' : hasData ? `${history.length} 个数据点` : ''}
          </span>
          <div className={styles.timeRangeToggle}>
            {(['all', '7d', '30d'] as const).map((r) => (
              <button
                key={r}
                className={`${styles.timeRangeBtn} ${timeRange === r ? styles.timeRangeBtnActive : ''}`}
                onClick={() => setTimeRange(r)}
              >
                {r === 'all' ? '全部' : r === '7d' ? '近7天' : '近30天'}
              </button>
            ))}
          </div>
        </div>

        {!hasData && !loading ? (
          <div className={styles.emptyState}>
            <div className={styles.emptyIcon}>📊</div>
            <h4 className={styles.emptyTitle}>暂无数据</h4>
            <p className={styles.emptyDesc}>发送消息后，Token 消耗会自动记录并在此展示</p>
          </div>
        ) : (
          <>
            {/* ── Trend chart ── */}
            {history.length > 0 && (
              <div className={`${styles.chartCard} ${styles.chartFull}`}>
                <h4 className={styles.chartTitle}>消耗趋势</h4>
                <ReactECharts option={trendOption} style={{ height: 280 }} opts={{ renderer: 'canvas' }} />
              </div>
            )}

            {/* ── Pie + Bar side by side ── */}
            {summary && summary.by_model.length > 0 && (
              <div className={styles.chartsGrid}>
                <div className={styles.chartCard}>
                  <h4 className={styles.chartTitle}>模型分布</h4>
                  <ReactECharts option={pieOption} style={{ height: 260 }} opts={{ renderer: 'canvas' }} />
                </div>
                <div className={styles.chartCard}>
                  <h4 className={styles.chartTitle}>调用次数排名</h4>
                  <ReactECharts option={barOption} style={{ height: 260 }} opts={{ renderer: 'canvas' }} />
                </div>
              </div>
            )}

            {/* ── Model detail table ── */}
            {summary && summary.by_model.length > 0 && (
              <div className={styles.chartCard}>
                <h4 className={styles.chartTitle}>模型明细</h4>
                <table className={styles.modelTable}>
                  <thead>
                    <tr>
                      <th>模型</th>
                      <th>请求数</th>
                      <th>Prompt</th>
                      <th>Completion</th>
                      <th>缓存</th>
                      <th>平均延迟</th>
                    </tr>
                  </thead>
                  <tbody>
                    {summary.by_model.map((m) => (
                      <tr key={m.model}>
                        <td className={styles.modelTableCell}>{m.model}</td>
                        <td>{m.requests.toLocaleString()}</td>
                        <td>{formatNumber(m.prompt)}</td>
                        <td>{formatNumber(m.completion)}</td>
                        <td>{formatNumber(m.cached)}</td>
                        <td>{m.avg_latency_ms > 0 ? (m.avg_latency_ms / 1000).toFixed(1) + 's' : '—'}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
          </>
        )}
      </div>
    </>
  )
}
```

Note: The `formatNumber` function is defined inline in the component file (not a separate import).

- [ ] **Step 3: Fix the import issue — move formatNumber inline**

In the TSX file, remove `import { formatNumber } from './TokenUsagePanel'` — it's defined in the same file.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/renderer/src/components/shared/TokenUsagePanel.tsx frontend/src/renderer/src/components/shared/TokenUsagePanel.module.css
git commit -m "feat: add TokenUsagePanel component with KPI cards, ECharts, and model table"
```

---

### Task 11: Add "用量" tab to SettingsModal

**Files:**
- Modify: `frontend/src/renderer/src/components/shared/SettingsModal.tsx`

- [ ] **Step 1: Add import and tab**

Add import at top:
```typescript
import TokenUsagePanel from './TokenUsagePanel'
```

Add `'token'` to the `Tab` type:
```typescript
type Tab = 'software' | 'agent' | 'models' | 'mcp' | 'lsp' | 'skills' | 'plugins' | 'kg' | 'token' | 'about'
```

Add tab to `tabs` array (between 'kg' and 'about'):
```typescript
  { id: 'kg', label: '认知图谱' },
  { id: 'token', label: '用量' },
  { id: 'about', label: '关于' },
```

Add the tab render (before the `about` tab):
```tsx
          {tab === 'token' && (
            <>
              <div className={styles.contentHeader}>
                <h3 className={styles.sectionTitle}>Token 用量</h3>
                <p className={styles.sectionDesc}>查看 Token 消耗统计和历史趋势</p>
              </div>
              <div className={styles.contentBody}>
                <TokenUsagePanel />
              </div>
            </>
          )}
```

- [ ] **Step 2: Verify TypeScript compilation**

Run: `cd frontend && npx tsc --noEmit`

Expected: No type errors

- [ ] **Step 3: Commit**

```bash
git add frontend/src/renderer/src/components/shared/SettingsModal.tsx
git commit -m "feat: add Token用量 tab to SettingsModal"
```

---

### Task 12: Build & verify end-to-end

- [ ] **Step 1: Build backend**

Run: `cargo build -p lume-cli --release`

Expected: Successful build

- [ ] **Step 2: Build frontend**

Run: `cd frontend && npx electron-vite build`

Expected: Successful build

- [ ] **Step 3: Run and verify**

Start the server: `./target/release/lume.exe serve --port 8080`

Manual verification:
1. Send a chat message to trigger token usage
2. Open Settings → 用量 tab
3. Verify KPI cards show data
4. Verify charts render
5. Switch time ranges (all / 7d / 30d)
6. Verify empty state shows "暂无数据" when no history

- [ ] **Step 4: Commit any final fixes**

```bash
git add -A
git commit -m "fix: final adjustments for token stats dashboard"
```

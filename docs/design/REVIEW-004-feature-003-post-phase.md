# Neutral Review Report — Post-Phase

**Feature**: 003 — Plan/SDD/Todo Workflow
**Review Type**: Post-Phase
**Date**: 2026-06-08
**Reviewer**: Neutral Reviewer (Claude Opus)
**Decision**: **APPROVE WITH AMENDMENTS** (7 blocking amendments required)

---

## 1. Summary

Feature 003 implementation is structurally sound at the skeleton level — all 9 JSON-RPC methods are registered, Zustand slices follow the StateCreator pattern, both panels conditionally render without React Router, and plan prompt templates exist. However, the implementation is **incomplete in critical areas**: no WebSocket push events are emitted, plan markdown files are never written to disk, todo extraction from plan content is entirely absent, and the PlanPanel has no autosave/load-content pipeline. These are not cosmetic issues — they represent missing core functionality required by the acceptance criteria. Seven blocking amendments must be addressed.

---

## 2. Amendment Verification (5 from Pre-Implementation REVIEW-001 Section 2.3)

### A1: WS Push Format for New Events

**Verdict: FAIL — NOT IMPLEMENTED**

The pre-implementation review (REVIEW-001 amendment 1 for Feature 003) required:
> Specify the JSON push message format for each new AgentEvent variant (PlanCreated, PlanUpdated, GoalSet, TodoStatusChanged) in event_bus.rs and the WS bridge.

**Evidence**: `F:\openloom\backend\crates\loom-core\src\event_bus.rs` contains **zero** plan/todo/goal variants in the `AgentEvent` enum (lines 11-89). The existing variants are: StateChanged, SubagentSpawned, SubagentCompleted, SubagentErrored, ToolStarted, ToolCompleted, StreamDelta, StreamEnd, TokenUsage, PermissionRequest, MemoryUpdated. No `PlanCreated`, `PlanUpdated`, `GoalSet`, or `TodoStatusChanged`.

**Backend impact**: `F:\openloom\backend\crates\loom-server\src\dispatch\plan.rs` does not call `state.event_bus.publish(...)` anywhere.

**Consequence**: Frontend cannot receive real-time push notifications when a plan is created/updated or a todo status changes. The frontend must poll (which it doesn't do either).

### A2: SessionData Struct Verified or Fallback Used?

**Verdict: FAIL — MISSING FIELDS**

The pre-implementation review (REVIEW-001 amendment 2) required:
> Verify `SessionData` exists, or define it. If it doesn't exist, specify the exact struct definition and how it integrates with the existing session management.

**Evidence**: `F:\openloom\backend\crates\loom-server\src\dispatch\session.rs` lines 23-33 define `SessionData` with fields: `id`, `created_at`, `updated_at`, `message_count`, `title`, `messages`, `agent_config_name`. It has **no** `active_plan_id: Option<String>` or `goal: Option<ThreadGoal>` fields as required by the design.

**Consequence**: Thread-scoped goal and active plan pointer cannot be persisted alongside sessions.

### A3: Right-Panel Coexistence with Feature 005 Documented?

**Verdict: PARTIAL FAIL**

The pre-implementation review (REVIEW-001 amendment 3) required:
> Document how PlanPanel/TodoPanel and WriteAssistantPanel coexist when both features are active. Recommendation: right sidebar is exclusively for Plan/Todo OR WriteAssistant, not both simultaneously.

**Evidence**: `F:\openloom\frontend\src\renderer\src\components\app\AppShell.tsx` lines 131-132 render both panels unconditionally:
```tsx
<PlanPanel />
<TodoPanel />
```
Each panel checks its own `planPanelOpen`/`todoPanelOpen` flag internally and returns `null` when closed. There is **no mode-gating** (no check of `appMode`). If Feature 005 adds a WriteAssistantPanel at the same DOM level, all three panels could be visible simultaneously.

**Consequence**: Cross-feature layout conflict when Feature 005 is merged.

### A4: uuid Crate Verified for Plan IDs?

**Verdict: PASS (with implementation deviation noted)**

The `uuid` crate with `v7` feature exists in both `loom-server/Cargo.toml` (line 21: `uuid = { version = "1", features = ["v7"] }`) and `loom-core/Cargo.toml` (line 24: same).

**Deviation**: `F:\openloom\backend\crates\loom-server\src\dispatch\plan.rs` line 34 uses `uuid::Uuid::new_v4().to_string()` instead of `uuid::Uuid::now_v7().to_string()`. The design specified UUID v7 for time-ordered plan IDs. The session.rs handler (line 47) correctly uses `now_v7()`. This is a minor inconsistency — UUID v4 is still valid but lacks time-ordering.

### A5: FNV1a to DefaultHasher Replacement?

**Verdict: N/A (no hashing implemented)**

The pre-implementation review (REVIEW-001 amendment 5) required:
> Either (a) add the `fnv` crate to loom-core/Cargo.toml, or (b) use `std::collections::hash_map::DefaultHasher` for deterministic but non-cryptographic hashing.

**Evidence**: Searched all of `F:\openloom\backend\crates\loom-core\src\` — neither `fnv` nor `DefaultHasher` is used anywhere. The todo ID generation pipeline (extracting checkboxes from plan markdown, hashing content for merge deduplication) is **not implemented**. The TODOS in-memory map (`plan.rs` line 146) is never populated by any code path.

**Consequence**: The amendment is not technically violated (the wrong approach wasn't used), but the underlying functionality (todo extraction) is entirely missing.

---

## 3. Architecture Compliance

### 3.1 Backend Invariants

| Invariant | Status | Evidence |
|-----------|--------|----------|
| B-1 Types in loom-types | **PASS** | `PlanArtifact`, `TodoItem`, `ThreadGoal`, `PlanStatus`, `TodoStatus`, `TodoSource`, `GoalStatus`, and request/response types all defined in `F:\openloom\backend\crates\loom-types\src\plan.rs` (120 lines, under 250 limit). Re-exported via `F:\openloom\backend\crates\loom-types\src\lib.rs` line 48: `pub use plan::*;`. |
| B-2 JSON-RPC 2.0 | **PASS** | All 9 methods follow `{ jsonrpc: "2.0", method: "plan.*" }` format. Frontend uses `loomRpc<T>()` from `F:\openloom\frontend\src\renderer\src\services\jsonrpc.ts`. |
| B-3 Dispatch chain | **PASS** | `plan::handle` registered at `F:\openloom\backend\crates\loom-server\src\dispatch\mod.rs` line 96, placed after `cron::handle` and before the fallthrough `Err(MethodNotFound)`. Signature matches: `pub async fn handle(state: &AppState, method: &str, p: &Value) -> Option<Result<Value, JsonRpcError>>`. |
| B-4 Crate boundaries | **PASS** | Types in loom-types, dispatch in loom-server, prompts in loom-core. No new crates created. |
| B-5 CloudClient trait | N/A | No LLM calls in plan handler itself (plan creation/querying is in-memory). Prompt injection happens at a higher level not audited here. |
| B-6 EventBus | **FAIL** | Zero new `AgentEvent` variants for plan/todo/goal. No `state.event_bus.publish(...)` calls in `plan.rs`. See Amendment 1. |
| B-7 SQLite persistence | **FAIL** | Design (L-033) specifies plan markdown files stored in `.loom/plans/`. The implementation uses only in-memory `HashMap` (`plan.rs` lines 144-149: `static PLANS: LazyLock<Arc<RwLock<HashMap<String, PlanArtifact>>>>`). No filesystem I/O in the entire dispatch handler. Plan data is lost on restart. |
| B-8 Explicit migration | N/A | No preference keys added. |

### 3.2 Frontend Invariants

| Invariant | Status | Evidence |
|-----------|--------|----------|
| F-1 Zustand slices | **PASS** | `createPlanSlice: StateCreator<PlanSlice>` at `plan.ts` line 26. `createTodoSlice: StateCreator<TodoSlice>` at `todo.ts` line 34. Both registered in `stores/index.ts`: PlanSlice at line 19/39/60, TodoSlice at line 20/40/61. |
| F-2 contextBridge | N/A | No new IPC methods. |
| F-3 JSON-RPC frontend | **PASS** | All backend calls use `loomRpc<T>(...)` imported dynamically from `services/jsonrpc`. |
| F-4 StreamBufferManager | N/A | No streaming in plan/todo. |
| F-5 No React Router | **PASS** | `PlanPanel` returns `null` when `!planPanelOpen` (line 15). `TodoPanel` returns `null` when `!todoPanelOpen` (line 15). No routing. |
| F-6 Tailwind + CSS vars | **PASS** | Both panels use `var(--border)`, `var(--bg-card)`, `var(--text)`, `var(--text-muted)`, `var(--accent)`, `var(--accent-soft)`, `var(--font-mono)`, `var(--surface-subtle)`. No hardcoded hex/rgb colors. |

### 3.3 Loom-Rootedness Checklist

| ID | Item | Status | Evidence/Issue |
|----|------|--------|----------------|
| L-022 | Plan types in loom-types/src/plan.rs | PASS | 120 lines, all types present. |
| L-023 | plan.rs under 250 lines | PASS | Exactly 120 lines. |
| L-024 | plan/goal handlers in dispatch/mod.rs | PASS (deviation) | `plan::handle` registered at line 96. However, goal.* methods are handled inside plan.rs, not in a separate goal.rs as the design implied. This is acceptable for MVP — the design document can be updated to reflect this consolidation. |
| L-025 | SlashRouter extended via BuiltinCommands | NOT VERIFIED | `plan_prompts.rs` provides the prompt templates but there is no `handle` for `/plan`, `/execute`, or `/goal` slash commands visible in the audited files. The orchestrator integration for BuiltinCommands is not in scope of these files. |
| L-026 | create_plan tool in builtin_tools.rs | NOT VERIFIED | `builtin_tools.rs` not in the audit file list. |
| L-027 | New AgentEvent variants | **FAIL** | Zero new variants in `event_bus.rs`. |
| L-028 | WS push via EventBus-to-WS bridge | **FAIL** | No events → no bridge. |
| L-029 | PlanSlice + TodoSlice StateCreator pattern | PASS | Confirmed. |
| L-030 | Zero cross-slice imports | PASS | Neither `plan.ts` nor `todo.ts` imports other slice files. |
| L-031 | PlanPanel conditionally rendered | PASS | Via `planPanelOpen` flag. |
| L-032 | TodoPanel conditionally rendered | PASS | Via `todoPanelOpen` flag. |
| L-033 | Plan files on filesystem | **FAIL** | In-memory HashMap only. |
| L-034 | Autosave via plan.update RPC with debounce | **FAIL** | `PlanPanel.tsx` line 38: `onChange={e => setPlanContent(e.target.value)}` — updates local store only. No `loomRpc('plan.update', ...)` call. No debounce timer. Content edits are not persisted to the backend. |
| L-035 | TodoPanel toggle via todo.update_status RPC | PASS | `todo.ts` line 56 calls `loomRpc('todo.update_status', ...)`. |

---

## 4. Implementation Completeness

### 4.1 JSON-RPC Methods (All 9 Registered)

| Method | Handler | Status | Notes |
|--------|---------|--------|-------|
| `plan.create` | `handle_plan_create` (line 30) | PASS | Uses uuid v4 instead of v7. Only stores in memory. |
| `plan.get` | `handle_plan_get` (line 56) | PASS | Uses `ErrorCode::SessionNotFound` for plan-not-found (semantically wrong). |
| `plan.list` | `handle_plan_list` (line 67) | PASS | Filters by workspace_root. |
| `plan.update` | `handle_plan_update` (line 77) | PASS | Only updates status field. `content` parameter from `UpdatePlanRequest` is ignored. |
| `plan.delete` | `handle_plan_delete` (line 93) | PASS | Removes from in-memory map only. |
| `todo.list` | `handle_todo_list` (line 102) | PASS | Returns all todos unfiltered. |
| `todo.update_status` | `handle_todo_update_status` (line 108) | PASS | Valid status transition. |
| `goal.set` | `handle_goal_set` (line 119) | PASS | One goal per session_id (overwrite). |
| `goal.status` | `handle_goal_status` (line 133) | PASS | Returns goal for session_id. |

### 4.2 PlanPanel

| Feature | Status | Evidence |
|---------|--------|----------|
| Plan list display | PASS | `PlanPanel.tsx` lines 55-63: maps `plans` to buttons. |
| Selected plan detail | PASS | Lines 28-46: shows title, status badge, textarea. |
| Toggle open/close | PASS | Lines 24-25: close button calls `togglePlanPanel`. |
| Autosave on edit | **FAIL** | No `plan.update` RPC call. No debounce timer. |
| Content loaded from backend | **FAIL** | `setActivePlan` resets `planContent` to `''` (plan.ts line 44). No `plan.get` or filesystem read follows. |

### 4.3 TodoPanel

| Feature | Status | Evidence |
|---------|--------|----------|
| Todo list with status cycling | PASS | `TodoPanel.tsx` lines 42-58: click toggles status. Status cycle: pending→in_progress→completed→pending. |
| Goal display | PASS | Lines 28-33: shows goal description with status badge. |
| Counter badges | PASS | Lines 35-38: displays pending/in progress/done counts. |
| Empty state | PASS | Line 59-62: "No todos yet" message. |

### 4.4 Plan Prompts

| Prompt | File:Line | Status |
|--------|-----------|--------|
| `build_plan_draft_prompt` | `plan_prompts.rs:4` | PASS — instructs LLM to create structured plan. |
| `build_plan_execute_prompt` | `plan_prompts.rs:30` | PASS — instructs LLM to read and execute plan. |
| `build_goal_prompt` | `plan_prompts.rs:47` | PASS — injects goal context. |

---

## 5. Code Quality

### 5.1 TODO Markers / Stubs

**PASS**. No TODO, FIXME, HACK, or XXX markers found in any audited file.

### 5.2 Error Handling

| Handler | Validation | Error Code | Verdict |
|---------|-----------|------------|---------|
| `plan.create` | Deserializes params, errors on invalid JSON | `InvalidRequest` / `InternalError` | PASS |
| `plan.get` | Validates non-empty plan_id | `InvalidRequest` / **`SessionNotFound`** | **ISSUE** — `SessionNotFound` (-32005) used for plan-not-found. Plans are not sessions. Should use `InternalError` or a dedicated error code. |
| `plan.list` | No required params | N/A | PASS (workspace_root optional) |
| `plan.update` | Validates non-empty plan_id | `InvalidRequest` / **`SessionNotFound`** | **ISSUE** — Same `SessionNotFound` misuse. |
| `plan.delete` | Validates non-empty plan_id | `InvalidRequest` | PASS |
| `todo.list` | No validation | N/A | PASS |
| `todo.update_status` | Deserializes to typed struct | `InvalidRequest` | PASS |
| `goal.set` | Deserializes to typed struct | `InvalidRequest` | PASS |
| `goal.status` | Validates non-empty session_id | `InvalidRequest` | PASS |

### 5.3 Error Code Misuse

**File**: `F:\openloom\backend\crates\loom-server\src\dispatch\plan.rs`
- Line 63: `ErrorCode::SessionNotFound` — plan not found
- Line 84: `ErrorCode::SessionNotFound` — plan not found

These should use `ErrorCode::InternalError` or a new dedicated error code (e.g., `PlanNotFound = -32008`). Reusing `SessionNotFound` for a different domain is semantically wrong and could confuse frontend error handling.

### 5.4 TypeScript

No TypeScript compilation errors could be verified from static analysis alone. The code is syntactically valid TypeScript. The `PlanArtifact` and `TodoItem` frontend interfaces match the Rust struct field naming (with snake_case serialization matching). The `status` field type uses string literals that match the serde rename_all output.

Notable: `todo.ts` line 56 passes `session_id: ''` (empty string) to `todo.update_status`. The `UpdateTodoStatusRequest` on the backend has `session_id: String` but it's not validated. This is a cosmetic issue — the field is declared but unused in the handler.

### 5.5 Additional Issues Found

1. **`plan.update` ignores content**: `F:\openloom\backend\crates\loom-server\src\dispatch\plan.rs` lines 85-88 only process the `status` field. The `content` field from `UpdatePlanRequest` (which has `content: Option<String>`) is never read or written anywhere. The design specifies that editing plan markdown in PlanPanel triggers autosave via `plan.update` — but the handler doesn't store content.

2. **`plan.create` does not write files**: The design (L-033) explicitly specifies plan markdown files at `.loom/plans/<uuid>.md`. The handler only stores `PlanArtifact` metadata in memory. No markdown file is created. The `relative_path` field is set but nothing is written to that path.

3. **Todo extraction pipeline missing**: There is no code that parses plan markdown content to extract `- [ ]` checkbox items into `TodoItem` objects. The `TODOS` map is never populated.

4. **`plan.create` response type mismatch**: The handler returns `PlanArtifact` directly (line 53), but `CreatePlanResponse` (in types) defines `{ plan: PlanArtifact, plan_content: String }`. The response wrapper is unused.

---

## 6. Anti-Pattern Scan

| Anti-Pattern | Found? | File/Location | Severity |
|-------------|--------|---------------|----------|
| React Context for runtime state | NO | — | — |
| JSONL event sourcing | NO | — | — |
| Bundled runtime | NO | — | — |
| Implicit migration | NO | — | — |
| Hardcoded Chinese strings | NO | — | N/A (no Chinese strings in audited code) |
| Over-engineered MCP | NO | — | — |
| **Separate plan.db** | NO | (uses in-memory, should use filesystem per design) | — |
| **React Router** | NO | Conditional rendering. | — |
| **JSON-based plan metadata files** | NO | (no filesystem persistence at all) | — |
| **Plan Panel autosave via direct filesystem** | N/A | (no autosave implemented) | — |

---

## 7. Integration Verification

| System | How Verified | Result |
|--------|-------------|--------|
| SlashRouter coexistence | `plan_prompts.rs` provides prompt templates; actual SlashRouter integration not in audit scope. | UNVERIFIED |
| SessionStore / SessionData | `SessionData` exists (session.rs:23) but missing `active_plan_id` and `goal` fields. | FAIL |
| Dispatch chain ordering | `plan::handle` at mod.rs:96, after `cron::handle`, before fallthrough. No method name conflicts with existing handlers (`plan.*`, `todo.*`, `goal.*`). | PASS |
| ChatWorkspace layout | PlanPanel (width:340) + TodoPanel (width:340) rendered in AppShell.tsx:131-132 alongside ChatWorkspace. When both are open, they occupy 680px of horizontal space. No responsive breakpoints. | RISK |
| FileEdit tool interaction | Not verifiable — no file-watching or re-extraction logic exists. | NOT IMPLEMENTED |
| EventBus WS bridge | No new events to bridge. | NOT IMPLEMENTED |

---

## 8. Cross-Feature Impact

| Concern | Status |
|---------|--------|
| Store slice count | Before: 18. After: 20 (+plan, +todo). Max: 25. OK. |
| IPC method count | No change. |
| Dispatch handler count | Before: 12. After: 13 (+plan combining plan.*, todo.*, goal.*). Max: 20. OK. |
| New npm dependencies | None. |
| New Cargo dependencies | None (uuid was already present in both loom-server and loom-core). |

---

## 9. Findings

### 9.1 Blocking Issues (must fix before proceeding)

1. **No EventBus push events (A1)** — `event_bus.rs` has zero PlanCreated/PlanUpdated/GoalSet/TodoStatusChanged variants. `plan.rs` does not call `event_bus.publish()`. Without push events, the frontend cannot react to plan lifecycle changes without polling, and the design's acceptance criteria #8 ("Agent marking a checkbox triggers WebSocket push that updates TodoPanel") cannot be met.
   - **File**: `F:\openloom\backend\crates\loom-core\src\event_bus.rs` (add 4 variants)
   - **File**: `F:\openloom\backend\crates\loom-server\src\dispatch\plan.rs` (publish events in handlers)

2. **No filesystem persistence (B-7 exception)** — Plan markdown files are never written to `.loom/plans/`. The entire plan/todo system is in-memory and lost on restart. The design explicitly justifies filesystem storage over SQLite, but the implementation uses neither.
   - **File**: `F:\openloom\backend\crates\loom-server\src\dispatch\plan.rs` (add filesystem I/O in `handle_plan_create`, `handle_plan_update`)

3. **No autosave/debounce in PlanPanel (L-034)** — Editing the plan textarea updates local Zustand state only. There is no `loomRpc('plan.update', ...)` call and no 650ms debounce timer. Plan content edits are not persisted.
   - **File**: `F:\openloom\frontend\src\renderer\src\components\plan\PlanPanel.tsx` (add debounced update RPC)
   - **File**: `F:\openloom\frontend\src\renderer\src\stores\plan.ts` (add updatePlanContent action)

4. **No plan content loading** — `setActivePlan` resets `planContent` to empty string without fetching content from the backend or filesystem. When a user selects a plan, the textarea remains blank.
   - **File**: `F:\openloom\frontend\src\renderer\src\stores\plan.ts` line 44 (`setActivePlan`)
   - **File**: `F:\openloom\frontend\src\renderer\src\components\plan\PlanPanel.tsx` (need to call loadContent on plan selection)

5. **No todo extraction pipeline** — No code parses plan markdown to extract `- [ ]` checkboxes as `TodoItem` objects. The `TODOS` in-memory map is never populated. TodoPanel will always show "No todos yet."
   - **File**: `F:\openloom\backend\crates\loom-server\src\dispatch\plan.rs` (add extraction logic)

6. **`SessionData` missing plan/goal fields (A2)** — The struct needs `active_plan_id: Option<String>` and `goal: Option<ThreadGoal>` fields to persist thread-scoped plan/goal state.
   - **File**: `F:\openloom\backend\crates\loom-server\src\dispatch\session.rs` line 23

7. **`plan.update` ignores content field** — The handler only processes `status`; the `content` field from `UpdatePlanRequest` is silently dropped. This breaks the autosave flow even if the frontend is fixed.
   - **File**: `F:\openloom\backend\crates\loom-server\src\dispatch\plan.rs` lines 85-88

### 9.2 Amendments (should fix within 48 hours)

- **Wrong error code**: Replace `ErrorCode::SessionNotFound` with `ErrorCode::InternalError` at `plan.rs` lines 63, 84. Alternatively, add a `PlanNotFound` error code variant.
- **AppShell right-panel coordination (A3)**: Add mode-gating in `AppShell.tsx` so PlanPanel/TodoPanel are hidden when `appMode !== 'chat'`. This prevents layout conflicts with Feature 005's WriteAssistantPanel.
- **UUID v4 → v7**: Change `Uuid::new_v4()` to `Uuid::now_v7()` at `plan.rs` line 34 for consistency with the design and `session.rs` line 47.
- **`plan.create` response type**: Use `CreatePlanResponse` struct (with `plan_content` field) instead of returning `PlanArtifact` directly at `plan.rs` line 53. The `plan_content` should contain the generated markdown content.

### 9.3 Suggestions (optional, at implementer's discretion)

- **Extract `goal::handle` to a separate module**: Currently plan.rs handles both `plan.*`, `todo.*`, and `goal.*` methods. A separate `dispatch/goal.rs` would improve modularity and match the original design (L-024).
- **Add `plan.update_content` as a separate method**: Rather than overloading `plan.update` for both metadata and content, a dedicated `plan.update_content` method would be cleaner. The current `UpdatePlanRequest` has both fields but the handler ignores `content`.
- **Persist the in-memory maps to disk on graceful shutdown**: Even after filesystem persistence is added, consider serializing the PLANS/TODOS/GOALS maps to disk for fast startup recovery without re-parsing all markdown files.

---

## 10. Decision

**APPROVE WITH AMENDMENTS**

The implementation has correct architecture foundations (dispatch chain, Zustand slices, conditional rendering, JSON-RPC methods) but is missing critical runtime functionality. The seven blocking issues must be resolved before this feature can be considered complete:

1. Add EventBus variants + publish calls for plan/todo/goal lifecycle
2. Implement filesystem persistence (`.loom/plans/<id>.md`)
3. Implement autosave with 650ms debounce in PlanPanel
4. Implement plan content loading on plan selection
5. Implement todo extraction from plan markdown
6. Add `active_plan_id` and `goal` to `SessionData`
7. Fix `plan.update` to handle content field

Amendments (error codes, UUID version, response type, AppShell mode-gating) should be addressed within 48 hours per framework Section 7.1.

---

## 11. Sign-off

**Reviewer**: Claude Opus (Neutral Reviewer)
**Date**: 2026-06-08
**Next Review**: Re-verification after amendments addressed, or Cross-Feature Review after Features 001-002 clearance.

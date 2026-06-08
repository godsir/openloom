# Neutral Review Report — Pre-Implementation

**Review Type**: Pre-Implementation (all 6 features)
**Date**: 2026-06-08
**Reviewer**: Neutral Reviewer (Claude Opus)
**Aggregate Decision**: ALL SIX features are APPROVED WITH AMENDMENTS

---

## 1. Executive Summary

All six design documents demonstrate strong architectural alignment with openLoom invariants. Each design explicitly addresses the Zustand StateCreator pattern, the contextBridge/preload IPC boundary, JSON-RPC 2.0 dispatch chain, and crate boundary discipline. Anti-patterns from DeepSeek-GUI are consistently identified and avoided.

**No feature merits REJECTION.** No fundamental architectural conflicts exist. However, **all six features require specific amendments** before implementation can begin. The amendments are concentrated in:
- Cross-feature layout conflicts (003 + 005 right-panel space)
- Shared-file merge ordering (stores/index.ts, dispatch/mod.rs, InputArea.tsx)
- Missing struct fields or incomplete implementation stubs
- Undefined helper functions referenced but not specified

The aggregate store slice count post-implementation will be **22 slices** (current 17 + 5 new: selection-context, plan, todo, completion, write). This is within the 25-slice limit.

---

## 2. Per-Feature Detailed Review

---

### 2.1 Feature 001 — Prompt-Cache Fingerprint

**Verdict**: APPROVE WITH AMENDMENTS (4 amendments required)

#### 2.1.1 Architecture Compliance

| Invariant | Status | Evidence |
|-----------|--------|----------|
| B-1 Types in loom-types | PASS | `CompletionRequest.prefix_digest` added to `loom-types/src/inference.rs`. `PrefixDigest` placed in `loom-context/src/lib.rs` per justified exception — tightly coupled to `ContextAssembler::compute_prefix_digest()`. |
| B-2 JSON-RPC 2.0 | N/A | No new JSON-RPC methods. Pure internal pipe. |
| B-3 Dispatch chain | N/A | No new dispatch handlers. |
| B-4 Crate boundaries | PASS | `PrefixDigest` in loom-context, `CacheStatus` in loom-inference/cache.rs. `sha2` (workspace) and `hex` added to loom-context/Cargo.toml. |
| B-5 CloudClient trait | PASS | Adds default stubs (`set_prefix_digest`, `prefix_digest_snapshot`, `prefix_digest_restore`). No new required methods. |
| B-6 EventBus | N/A | No new events. |
| B-7 SQLite persistence | N/A | No persistence changes. |
| B-8 Explicit migration | N/A | Backward-compatible; all new fields are `Option`. |

#### 2.1.2 Loom-Rootedness Checklist

| ID | Item | Status | Evidence/Issue |
|----|------|--------|----------------|
| L-001 | `PrefixDigest` in loom-context | PASS | Defined in `loom-context/src/lib.rs` Section 3.1/Step 2. |
| L-002 | `CacheStatus` in loom-inference/cache.rs | PASS | Defined alongside PrefixCache upgrade in Section 3.2. |
| L-003 | `prefix_digest` as `Option<PrefixDigest>` | PASS | Both `CompletionRequest.prefix_digest` and `AgentLoopConfig.prefix_digest` are `Option`. |
| L-004 | `CloudClient` trait methods as default stubs | PASS | `set_prefix_digest()`, `prefix_digest_snapshot()`, `prefix_digest_restore()` all have default no-op implementations. |
| L-005 | SHA256 via `sha2.workspace = true` | PASS | Step 1 references `sha2.workspace = true`. Verified: `sha2` is declared in workspace Cargo.toml. |
| L-006 | Legacy `check()` method preserved | **FAIL** | The design's Step 3 shows an upgraded `check()` that returns `(false, hash)` — always false. This breaks backward compatibility for orchestrator internal calls. **AMENDMENT 1**: the legacy `check()` in the upgraded PrefixCache must preserve the original DefaultHasher logic, not always return false. |
| L-007 | AnthropicClient gates `cache_control` behind `ModelBackend::Anthropic` | PASS | Step 5 checks `matches!(cache_status, CacheStatus::Hit)` and the risk table notes gating behind `ModelBackend::Anthropic`. However, the implementation code in Step 5 does NOT explicitly check the model backend — it only checks `CacheStatus::Hit`. **AMENDMENT 2**: Add a `self.model.starts_with("claude-")` or `self.provider() == ModelBackend::Anthropic` guard before injecting `cache_control`. |
| L-008 | Logging uses `tracing::info!` | PASS | All log statements in the design use `tracing::info!`. |
| L-009 | Digest computed in `run_agent_turn_inner/streaming_inner` | PASS | Step 4b places the digest computation before the iteration loop. |
| L-010 | `sha2` referenced as `workspace = true` | PASS | Step 1 confirms `sha2.workspace = true` in Cargo.toml. |

#### 2.1.3 Anti-Pattern Scan

| Anti-Pattern | Found? | Severity |
|-------------|--------|----------|
| Hardcoded provider logic | **YES** | High — See Amendment 2 above. The cache_control injection must be gated on provider type. |
| Over-hashing (hashing dynamic suffix) | No | PrefixDigest is computed from stable prefix only. `compute_prefix_digest()` does NOT receive history. |
| New crate for cache types | No | All additions are in existing crates. |

#### 2.1.4 Integration Gaps

| System | Status |
|--------|--------|
| Existing PrefixCache users (orchestrator, summary engine, vision) | **Gap** — The legacy `check()` method returning `(false, hash)` would break these callers. Amendment 1 addresses this. |
| All 5 CloudClient implementors | PASS — Default trait stubs ensure compilation without changes. |
| AgentLoopConfig Clone | **Unverified** — The design says AgentLoopConfig "derives Clone (was manual)". The actual `AgentLoopConfig` struct in `loom-core/src/agent_loop.rs` is not shown in the snippets. Need to verify that all fields implement Clone. `EventBus` is already `#[derive(Debug, Clone)]`. |
| EventBus snapshot/restore | PASS — `prefix_digest_snapshot/restore` are new methods that coexist with the existing `prefix_hash_snapshot/restore`. |

#### 2.1.5 Missing Details

1. `CacheStatus` enum is defined in Section 3.2 but not annotated with `#[derive(Debug, Clone, Copy, PartialEq, Eq)]` — the missing `Copy` derive may cause issues in match arms. **AMENDMENT 3**: Add explicit derive macros including `Copy`.
2. `AgentLoopConfig` is cloned via `..config.clone()` in Step 4b. The design claims AgentLoopConfig "derives Clone (was manual)" but this must be verified against the actual struct. If any field does not implement Clone (e.g., ToolRegistry, which is `Arc`-wrapped), this will fail. **AMENDMENT 4**: Verify that all AgentLoopConfig fields implement Clone, or use a `clone_turn_config()` helper that only clones cache-relevant fields.

#### 2.1.6 Decision

**APPROVE WITH AMENDMENTS**. Address Amendments 1-4 before implementation begins.

---

### 2.2 Feature 002 — Inline Selection Editor

**Verdict**: APPROVE WITH AMENDMENTS (4 amendments required)

#### 2.2.1 Architecture Compliance

| Invariant | Status | Evidence |
|-----------|--------|----------|
| B-1 through B-8 | N/A | Frontend-only feature. Minor IPC change to `read-file` handler is backward-compatible. |
| F-1 Zustand slices | **PASS** | `createSelectionContextSlice: StateCreator<SelectionContextSlice>` pattern. Registered in `stores/index.ts`. Zero cross-slice imports. |
| F-2 contextBridge | **PASS** | `readFile` signature extended with optional `options` parameter in both `LoomApi` interface AND `exposeInMainWorld`. Existing callers pass only `filePath` and get full content. |
| F-3 JSON-RPC frontend | **PASS** | Uses existing `loomRpc` and `sendMessage`. No direct fetch(). |
| F-4 StreamBufferManager | N/A | No streaming changes. |
| F-5 No React Router | **PASS** | `InlineInputOverlay` mounted conditionally in `App.tsx`, not via route. |
| F-6 Tailwind + CSS vars | **PASS** | CSS uses `var(--bg-card)`, `var(--border)`, `var(--text)`, `var(--accent)`, `var(--bg)`, `var(--r-md)`, `var(--r-sm)`, `var(--text-muted)`. No hardcoded hex/rgb values. |

#### 2.2.2 Loom-Rootedness Checklist

| ID | Item | Status | Evidence/Issue |
|----|------|--------|----------------|
| L-011 | SelectionContextSlice uses StateCreator pattern | PASS | Confirmed in Section 4. |
| L-012 | Registered in stores/index.ts | PASS | Confirmed in Section 4. |
| L-013 | Zero cross-slice imports | PASS | No imports from other slice files. |
| L-014 | read-file IPC backward compatible | PASS | Optional `options` parameter — existing callers unaffected. |
| L-015 | LoomApi interface updated | PASS | `readFile` extended in both interface and exposeInMainWorld. |
| L-016 | InlineInputOverlay mounted in App.tsx | PASS | Confirmed in Section 6.2. |
| L-017 | Portal render to document.body | PASS | `ReactDOM.createPortal` used. |
| L-018 | InputArea reads quotedSelections from store | PASS | Confirmed in Section 6.3. |
| L-019 | quotedSelections optional in SendMessageOptions | PASS | Section 3.3 shows `quotedSelections?: QuotedSelection[]`. |
| L-020 | quoted_selection blocks in message payload | PASS | Serialized via existing `chat.send` RPC flow. |
| L-021 | QuotedSelectionCard onRemove made optional | PASS | Section 9.2 makes `onRemove?: () => void`. |

#### 2.2.3 Anti-Pattern Scan

| Anti-Pattern | Found? | Severity |
|-------------|--------|----------|
| React Context for inline input | No | All state in Zustand. |
| Direct DOM manipulation | No | Position computed from `getBoundingClientRect()`. |
| New IPC channel for selection data | No | Flows through `chat.send` blocks. |
| Hardcoded Ctrl+Shift+I without escape | No | Documented as configurable future work (Section 7.3). |

#### 2.2.4 Integration Gaps

| System | Status |
|--------|--------|
| InputArea textarea | PASS — `InputArea.tsx` is read from the store, not directly modified. QuotedSelectionCard is rendered above textarea, same pattern as AttachedFiles. |
| sendMessage flow | PASS — Backward compatible: `quotedSelections` defaults to `[]`. |
| ChatWorkspace message rendering | PASS — `UserMessage` gets new block renderer; `AssistantMessage` ignores `quoted_selection` blocks. |
| QuotedSelectionCard existing usage | **Unverified** — The design claims QuotedSelectionCard "already exists and is well-formed but unused." This must be verified. If QuotedSelectionCard has existing callers that rely on `onRemove` being required, the prop change is breaking. |

#### 2.2.5 Missing Details

1. **Function signature mismatch**: The store's `openInlineInput` accepts `(sel: SelectionRange, rect: DOMRect)` (2 params), but the keyboard handler in Section 7.2 calls `store.openInlineInput({...}, rect, filePath, startLine, endLine)` (5 params). **AMENDMENT 1**: Align the `openInlineInput` signature with the actual call site. Extend `SelectionContextSlice.openInlineInput` to accept the optional file-position metadata.

2. **File metadata injection deferred**: Section 10 acknowledges that `data-file-path`, `data-start-line`, `data-end-line` attributes on DOM elements are a "small follow-up change." But the inline selection feature DEPENDS on these attributes for file path resolution. Without them, ALL selections from code blocks will have `filePath: ''`. **AMENDMENT 2**: Include the data-attribute injection in the implementation plan as part of the feature, not as a follow-up. At minimum, specify the exact component(s) where code-block elements are rendered and how attributes will be added.

3. **QuotedSelection ID generation**: The `QuotedSelection` type has `id: string` for "removal tracking" (Section 3.1), but the design never specifies how IDs are generated. The `addQuotedSelection` action in the store does NOT generate IDs — it expects the caller to provide them. **AMENDMENT 3**: Either (a) have the store action generate the ID internally (e.g., `crypto.randomUUID()`), or (b) document that the caller (InlineInput onConfirm handler) is responsible for ID generation.

4. **InlineInput local state**: The `setInlineInstructionText` action is declared but its body says `/* local to InlineInput, see component */`. The store action is effectively a no-op. The component uses local React state instead. This is fine architecturally but the store action should either be implemented or removed to avoid confusion. **AMENDMENT 4**: Either implement `setInlineInstructionText` in the store or remove it and use only local React state in the component.

#### 2.2.6 Decision

**APPROVE WITH AMENDMENTS**. Address Amendments 1-4 before implementation begins.

---

### 2.3 Feature 003 — Plan/SDD/Todo Workflow

**Verdict**: APPROVE WITH AMENDMENTS (5 amendments required)

#### 2.3.1 Architecture Compliance

| Invariant | Status | Evidence |
|-----------|--------|----------|
| B-1 Types in loom-types | **PASS** | `PlanArtifact`, `TodoItem`, `ThreadGoal`, `PlanStatus`, `TodoStatus`, `TodoSource`, `GoalStatus` all in `loom-types/src/plan.rs`. Consumers documented. Under 250 lines (borderline but acceptable). |
| B-2 JSON-RPC 2.0 | **PASS** | 10 new methods: `plan.create/update/get/list/delete/approve_step`, `todo.list/update_status`, `goal.set/status`. |
| B-3 Dispatch chain | **PASS** | `plan::handle` and `goal::handle` follow the `pub async fn handle(state, method, &p) -> Option<Result<Value, JsonRpcError>>` pattern. |
| B-4 Crate boundaries | **PASS** | Types in loom-types, dispatch in loom-server, prompts in loom-core, builtin commands in loom-core. |
| B-5 CloudClient trait | N/A | |
| B-6 EventBus | **PASS** | `PlanCreated`, `PlanUpdated`, `GoalSet`, `TodoStatusChanged` added to `AgentEvent` enum. |
| B-7 SQLite persistence | **EXCEPTION (ACCEPTED)** | Plan markdown files stored on filesystem under `.loom/plans/`. Design explicitly justifies: plans are human-readable, git-trackable documents. YAML frontmatter for metadata. No separate SQLite DB. Exception documented and accepted. |
| B-8 Explicit migration | **PASS** | `SessionData` extended with `Option` fields. |
| F-1 Zustand slices | **PASS** | `createPlanSlice: StateCreator<PlanSlice>` and `createTodoSlice: StateCreator<TodoSlice>`. Zero cross-slice imports. |
| F-2 contextBridge | N/A | |
| F-3 JSON-RPC frontend | **PASS** | All operations via `loomRpc`. |
| F-4 StreamBufferManager | N/A | |
| F-5 No React Router | **PASS** | `PlanPanel` and `TodoPanel` conditionally rendered via `planPanelOpen`/`todoPanelOpen`. |
| F-6 Tailwind + CSS vars | Expected | CSS modules will follow var() pattern. |

#### 2.3.2 Loom-Rootedness Checklist

| ID | Item | Status | Evidence/Issue |
|----|------|--------|----------------|
| L-022 | Plan types in loom-types/src/plan.rs | PASS | Confirmed in Section 3.1. |
| L-023 | plan.rs under 250 lines | **BORDERLINE** | The types shown are ~260 lines including doc comments. May need to split enums into a sub-module if additional types are added during implementation. |
| L-024 | plan/goal handlers in dispatch/mod.rs | PASS | Confirmed. Placement after cron handler. |
| L-025 | SlashRouter extended via BuiltinCommands layer | PASS | `BuiltinCommands` runs BEFORE SlashRouter. Returns `Handled`, `Rewrite`, or `Passthrough`. |
| L-026 | create_plan tool in builtin_tools.rs | PASS | `ToolDef` pattern confirmed. Tool handler: create/update operations. |
| L-027 | New AgentEvent variants | **PASS** | Section 9 notes 4 new variants. However, the actual `AgentEvent` enum in `event_bus.rs` currently has 11 variants. Adding 4 more is within reason. |
| L-028 | WS push via existing EventBus-to-WS bridge | **Gap** | The EventBus is a simple `tokio::sync::broadcast` channel. The WS bridge conversion (AgentEvent -> JSON push message) is referenced but NOT specified. The design says "The WebSocket handler in ws.rs already broadcasts AgentEvent variants" — I can see EventBus is a broadcast channel, so subscribers (including WS handler) receive all events. But the conversion logic for the new variants is not explicitly designed. **AMENDMENT 1**: Specify the JSON push message format for each new AgentEvent variant in `ws.rs` or a dedicated event->WS conversion module. |
| L-029 | PlanSlice and TodoSlice use StateCreator | PASS | Confirmed. |
| L-030 | Zero cross-slice imports | PASS | Neither imports other slice files. |
| L-031 | PlanPanel conditionally rendered | PASS | Via `planPanelOpen` store value. |
| L-032 | TodoPanel conditionally rendered | PASS | Via `todoPanelOpen` store value. |
| L-033 | Plan files on filesystem (not SQLite) | PASS | Exception documented and justified. |
| L-034 | Autosave via plan.update RPC | PASS | 650ms debounce. |
| L-035 | TodoPanel toggle via todo.update_status RPC | PASS | Not direct filesystem writes. |

#### 2.3.3 Anti-Pattern Scan

| Anti-Pattern | Found? | Severity |
|-------------|--------|----------|
| Separate plan.db | No | Uses filesystem for plans, SessionData for pointers. |
| React Router | No | Conditional rendering. |
| JSON-based plan metadata files | No | YAML frontmatter in markdown. |
| Hardcoded slash commands replacing SlashRouter | No | BuiltinCommands complement (run before) SlashRouter. |

#### 2.3.4 Integration Gaps

| System | Status |
|--------|--------|
| SlashRouter coexistence | PASS — BuiltinCommands runs BEFORE SlashRouter. If a registered skill is named "plan" or "goal", BuiltinCommands takes priority. If BuiltinCommands returns Passthrough, SlashRouter gets a chance. |
| SessionStore / SessionData extension | **Gap** — The design references `SessionData` struct in `dispatch/session.rs` with fields `active_plan_id` and `goal`. I cannot verify this struct exists in the current codebase (the dispatch/session.rs file was not shown in full). **AMENDMENT 2**: Verify `SessionData` exists, or define it. If it doesn't exist, specify the exact struct definition and how it integrates with the existing session management. |
| Dispatch chain ordering | PASS — Plan/Goal handlers placed after cron. But the design should specify the exact line position since multiple features modify dispatch/mod.rs. |
| ChatWorkspace layout | **Cross-feature concern** — PlanPanel and TodoPanel occupy the right sidebar. Feature 005 (Write Mode) also wants right-side panels (WriteAssistantPanel). **AMENDMENT 3**: Document how PanelPanel/TodoPanel and WriteAssistantPanel coexist when both features are active. Recommendation: right sidebar is exclusively for Plan/Todo OR WriteAssistant, not both simultaneously. ModeRouter should manage which right panel is shown. |
| FileEdit tool interaction | PASS — Agent can edit plan markdown files via FileEdit. The design says "file change must trigger plan re-extraction." This is specified in Section 3.5. |
| EventBus WS bridge | See Amendment 1. |

#### 2.3.5 Missing Details

1. **UUID v7 generation**: The design specifies UUID v7 for plan IDs (time-ordered UUIDs). Rust's `uuid` crate supports v7 since v1.6. Verify that `uuid` is in workspace dependencies. **AMENDMENT 4**: Add `uuid` crate to dependencies if not already present, or use `Uuid::now_v7()` if already available.

2. **FNV1a hash**: Todo ID generation uses `fnv1a_hash()` but the Rust std library doesn't include FNV. The `fnv` crate is needed. Not listed in dependencies. **AMENDMENT 5**: Either (a) add the `fnv` crate to loom-core/Cargo.toml, or (b) use `std::collections::hash_map::DefaultHasher` (already available) for deterministic but non-cryptographic hashing.

3. **PlanPanel markdown editor**: The design references a "Markdown editor/viewer (CodeMirror or textarea)" — the implementation choice affects dependency requirements. With Feature 004 and 005 both using CodeMirror 6, using CodeMirror for the plan panel would be consistent but adds complexity.

4. **`content_hash` stability in merge logic**: The frontend `mergePlanTodos()` algorithm (Section 4.4) uses `content_hash` to determine whether content changed. But if the same checkbox text appears in different plans (e.g., "Add tests"), the hash would match across plans. The merge algorithm should also compare `source.plan_id`.

#### 2.3.6 Decision

**APPROVE WITH AMENDMENTS**. Address Amendments 1-5 before implementation begins.

---

### 2.4 Feature 004 — FIM Code Completions

**Verdict**: APPROVE WITH AMENDMENTS (4 amendments required)

#### 2.4.1 Architecture Compliance

| Invariant | Status | Evidence |
|-----------|--------|----------|
| B-1 Types in loom-types | N/A | FIM types (request/response) are in dispatch/completion.rs and services/fim.rs — they're REST-call-specific, not shared types. |
| B-2 JSON-RPC 2.0 | **PASS** | `completion.fim` follows JSON-RPC 2.0. Errors returned as `{ ok: false, message }` result (not JSON-RPC errors) — explicitly designed to avoid global error handling. |
| B-3 Dispatch chain | **PASS** | `completion::handle` registered in dispatch/mod.rs. |
| B-4 Crate boundaries | **PASS** | `FimService` in `loom-server/src/services/fim.rs`, NOT in loom-inference. Justified: bypasses CloudClient trait. |
| B-5 CloudClient trait | **EXCEPTION (ACCEPTED)** | DeepSeek's `/fim/completions` has fundamentally different request/response shape than `/chat/completions`. Exception is documented, narrow in scope, and flagged as non-precedent-setting. |
| B-6 EventBus | N/A | |
| B-7 SQLite persistence | N/A | |
| B-8 Explicit migration | **PASS** | Feature flag via `AppConfig.settings.fim.enabled`. Toggled at runtime. |
| F-1 Zustand slices | **PASS** | `createCompletionSlice: StateCreator<CompletionSlice>`. Zero cross-slice imports. |
| F-2 contextBridge | N/A | |
| F-3 JSON-RPC frontend | **PASS** | `loomRpc('completion.fim', ...)` — not direct fetch(). |
| F-4 StreamBufferManager | N/A | FIM is not streaming — v1 uses non-streaming endpoint. |
| F-5 No React Router | **PASS** | CodeMirrorInput conditionally rendered by feature flag. |
| F-6 Tailwind + CSS vars | **PASS** | Ghost text uses `var(--color-text-muted)`. |

#### 2.4.2 Loom-Rootedness Checklist

| ID | Item | Status | Evidence/Issue |
|----|------|--------|----------------|
| L-036 | completion.fim registered in dispatch/mod.rs | PASS | `completion::handle` pattern. |
| L-037 | Errors returned as `{ ok: false }` | PASS | Not as JSON-RPC errors. |
| L-038 | FIM provider resolution via existing configs | **Gap** — Design says to scan `state.orchestrator.model_configs()`. The actual public API of Orchestrator needs verification. **AMENDMENT 1**: Verify that `Orchestrator` exposes a `model_configs()` method, or add a method to retrieve configured models with their backends. |
| L-039 | FimService in loom-server (not loom-inference) | PASS | In `loom-server/src/services/fim.rs`. |
| L-040 | CodeMirrorInput as conditional drop-in replacement | PASS | Rendered conditionally based on `fimEnabled` feature flag. |
| L-041 | CompletionSlice uses StateCreator pattern | PASS | Confirmed. |
| L-042 | Zero cross-slice imports | PASS | |
| L-043 | Textarea fallback preserved | PASS | When FIM is disabled, textarea renders. Error boundary should be added for graceful degradation (mentioned in Phase 2, Day 10). |
| L-044 | sendMessage works identically | PASS | CodeMirrorInput syncs to React text state via updateListener. sendMessage reads text from the same path. |
| L-045 | Ghost text via ViewPlugin + DecorationSet | PASS | Not by appending text to document. |
| L-046 | Tab acceptance via keymap with Prec.highest | PASS | |
| L-047 | Abort via generation counter (not AbortSignal) | PASS | `fimAbortGeneration` counter. The design explicitly chooses not to modify `loomRpc` for AbortSignal support. |

#### 2.4.3 Anti-Pattern Scan

| Anti-Pattern | Found? | Severity |
|-------------|--------|----------|
| Bundled model runtime | No | External DeepSeek API call. |
| Textarea replaced without fallback | No | Conditional rendering with feature flag. |
| Direct fetch() from renderer | No | Goes through `loomRpc` -> WebSocket -> backend. |
| Adding @codemirror/autocomplete only for ghost text | No (justified) | Used for debounce orchestration, not popup display. Ghost text rendered via custom ViewPlugin. |
| Cross-slice imports | No | CompletionSlice is independent. |

#### 2.4.4 Integration Gaps

| System | Status |
|--------|--------|
| InputArea textarea mode | PASS — When FIM is disabled, textarea renders identically to before. |
| sendMessage flow | PASS — Messages from CodeMirror input produce identical payloads to messages from textarea. |
| WebSocket connection | PASS — FIM payloads are <5KB. Non-streaming. No starvation risk. |
| Dispatch chain | PASS — completion::handle won't shadow existing methods. |
| @codemirror/autocomplete | PASS — Only new npm dependency. Verified: not currently in package.json (only codemirror, @codemirror/view, @codemirror/state, @codemirror/language, @codemirror/lang-markdown are present). |
| 004 + 005 CodeMirror isolation | **Cross-feature concern** — 004's FIM ViewPlugin must NOT activate in 005's WriteMarkdownEditor. The CompletionSource should check `useStore.getState().appMode` and return null when in Write mode. **AMENDMENT 2**: Add an explicit guard in the CompletionSource: `if (useStore.getState().appMode !== 'chat') return null`. |

#### 2.4.5 Missing Details

1. **DeepSeek API key resolution**: The design says "scan existing model configs" for a DeepSeek model. But DeepSeek's FIM endpoint uses the SAME API key as the chat API. The `FimService` needs to extract the API key from the matched model config. **AMENDMENT 3**: Specify how the API key is retrieved — from `ModelConfig.api_key` or equivalent. If the model config doesn't expose the raw API key to the server layer, an accessor method must be added.

2. **GhostTextWidget DOM rendering**: The `GhostTextWidget` class (Section 5.4) is referenced but its implementation (how it creates a DOM element with dimmed text) is not specified. CodeMirror's `WidgetType` requires `toDOM()` and `eq()` methods. **AMENDMENT 4**: Provide the GhostTextWidget class implementation (at minimum: the `toDOM()` method that creates a `<span>` with the ghost text CSS class).

3. **Rate limiting**: The risk assessment notes "Client-side rate limit: max 1 request per 500ms regardless of debounce" but this is not implemented in the design. The debounce logic (300ms short, 2000ms long) does not include a hard rate limit.

4. **IME composition**: The risk assessment mentions "Disable FIM during IME composition (check `view.composing`)" but the implementation doesn't include this check in the CompletionSource.

#### 2.4.6 Decision

**APPROVE WITH AMENDMENTS**. Address Amendments 1-4 before implementation begins.

---

### 2.5 Feature 005 — Write Mode Workspace

**Verdict**: APPROVE WITH AMENDMENTS (5 amendments required)

#### 2.5.1 Architecture Compliance

| Invariant | Status | Evidence |
|-----------|--------|----------|
| B-1 Types in loom-types | N/A | VFS types are in dispatch/vfs.rs — REST-call-specific, not shared types. |
| B-2 JSON-RPC 2.0 | **PASS** | `vfs.*` methods follow JSON-RPC 2.0. |
| B-3 Dispatch chain | **PASS** | `vfs::handle` registered in dispatch/mod.rs. Placement before cron per design recommendation. |
| B-4 Crate boundaries | **PASS** | VFS in `loom-server/src/dispatch/vfs.rs`. |
| B-5 CloudClient trait | **PASS** | Write mode inline AI uses existing `chat.send` -> `CloudClient::complete_stream_structured()`. |
| B-6 EventBus | N/A | |
| B-7 SQLite persistence | **PASS** | Write threads are regular sessions with `mode: 'write'` metadata. Preferences via `setPreference`. |
| B-8 Explicit migration | **PASS** | New preference keys: `defaultWorkspaceRoot`, `recentWorkspaceRoots`, `writePreviewMode`, `writeAssistantOpen`. |
| F-1 Zustand slices | **PASS** | `createWriteSlice: StateCreator<WriteSlice>`. Zero cross-slice imports. `appMode` added to `UiSlice` (existing). |
| F-2 contextBridge | **PASS** | 6 new IPC methods added to both `LoomApi` interface and `exposeInMainWorld`. `registerWriteIpc()` registered in `main/ipc/index.ts`. Total IPC methods: 30 -> 36 (within 45 limit). |
| F-3 JSON-RPC frontend | **PASS** | All VFS operations via `loomRpc`. |
| F-4 StreamBufferManager | **PASS** | Inline AI uses existing chat.send -> StreamBufferManager. |
| F-5 No React Router | **PASS** | `ModeRouter` conditionally renders `ChatWorkspace` or `WriteWorkspaceView` based on `appMode`. |
| F-6 Tailwind + CSS vars | Expected | New CSS modules should follow var() pattern. |

#### 2.5.2 Loom-Rootedness Checklist

| ID | Item | Status | Evidence/Issue |
|----|------|--------|----------------|
| L-048 | WriteSlice uses StateCreator pattern | PASS | Confirmed. |
| L-049 | Zero cross-slice imports | PASS | |
| L-050 | appMode in UiSlice | PASS | Not a new slice. |
| L-051 | ModeRouter conditional rendering | PASS | Based on `appMode` store value. |
| L-052 | vfs.* methods in dispatch/mod.rs | PASS | |
| L-053 | Path traversal protection | PASS | Section 6.4: canonicalize path, verify within workspace root. |
| L-054 | IPC methods added to LoomApi + exposeInMainWorld | PASS | |
| L-055 | registerWriteIpc follows existing pattern | PASS | `ipcMain.handle(...)` pattern. |
| L-056 | exportWriteDocument registered via registerWriteIpc | PASS | Via dynamic import(`./export-write`). |
| L-057 | CodeMirror for markdown editor | PASS | Uses existing `codemirror` + `@codemirror/*` packages. No TipTap. |
| L-058 | Write threads as regular sessions | PASS | `mode: 'write'` metadata tag. |
| L-059 | WriteAssistantPanel reuses existing message components | PASS | UserMessage, AssistantMessage reused. |
| L-060 | WriteMarkdownPreview uses existing markdown-it + highlight.js | **PARTIAL** — The design says "use existing markdown-it" for export but Section 8.3 and Appendix A mention react-markdown as an option for preview. react-markdown is NOT in package.json. **AMENDMENT 1**: Commit to ONE approach for the preview component. Recommendation: use `dangerouslySetInnerHTML` with `markdown-it` (already a dependency) + the existing `utils/markdown-sanitizer.ts`. Do NOT add react-markdown as a new dependency unless the justification (SSR, component customization) is explicitly documented. |
| L-061 | Export uses Electron printToPDF | PASS | No Puppeteer. |
| L-062 | Write preferences via window.loom.setPreference() | PASS | |

#### 2.5.3 Anti-Pattern Scan

| Anti-Pattern | Found? | Severity |
|-------------|--------|----------|
| React Context | No | All shared state in Zustand WriteSlice. |
| TipTap instead of CodeMirror | No | Explicitly uses CodeMirror 6. |
| Separate WebSocket | No | Shares the same connection as Chat mode. |
| Server-side export | No | Export in Electron main process. |
| Hardcoded workspace root | No | User-configurable via preferences. |
| New react-markdown dependency | **RISK** | See Amendment 1. |

#### 2.5.4 Integration Gaps

| System | Status |
|--------|--------|
| ChatWorkspace | PASS — State preserved during mode switch (messages, streaming, input text). |
| Sidebar / AppShell | **Cross-feature concern** — With Feature 003's PlanPanel/TodoPanel ALSO occupying right sidebar space, the layout becomes: sidebar | main | right-panel. When both WriteAssistantPanel AND PlanPanel are open, they compete for the same space. **AMENDMENT 2**: Specify the right-panel ownership rules. Proposal: Write mode hides PlanPanel/TodoPanel; Chat mode hides WriteAssistantPanel. The right panel is mode-exclusive. |
| Session system | PASS — Write threads appear in session list alongside chat sessions. |
| WebSocket | PASS — Single WS connection. Stream events annotated with session_id (already in StreamDelta). |
| Imported packages | **Gap** — Appendix A lists `react-markdown`, `katex`, `mermaid`, `fflate` as "existing dependencies." Katex is listed but not mentioned in the implementation. `react-markdown` is not in package.json. `fflate` for DOCX is optional. |
| Theme system | PASS — Expected to use CSS custom properties. |
| 005 + 002 (QuotedSelectionCard) | **Cross-feature concern** — 002's `QuotedSelectionCard` has `onRemove` made optional. 005's `WriteSlice` has its own `quotedSelections: QuotedSelection[]` and `addQuotedSelection`/`removeQuotedSelection` actions — but uses a DIFFERENT `QuotedSelection` interface from 002 (no `id` field, has `charCount`). These types must be unified. **AMENDMENT 3**: Use a SINGLE `QuotedSelection` type definition. Place it in a shared types file (not in a slice file). Both Feature 002 and Feature 005 should import the same type. |

#### 2.5.5 Missing Details

1. **`createWriteSession()` function undefined**: The store action `initializeWorkspace` calls `createWriteSession(root)` (line 524 of the design) but this function is never defined. It is critical — it creates the backend session for the Write Assistant thread. **AMENDMENT 4**: Specify the `createWriteSession` helper: it should call `loomRpc('session.create', { title: 'Write Assistant', metadata: { mode: 'write', workspace_root: root } })` or equivalent.

2. **Image binary upload**: The `image-drop-handler` extension (Section 8.3.4) saves pasted images to `<workspaceRoot>/images/pasted-<timestamp>.png` via `vfs.writeFile`. But `vfs.writeFile` accepts `content: string` (UTF-8 text) — binary image data can't be sent as a JSON string. **AMENDMENT 5**: Either (a) add a binary-capable IPC method for image upload (e.g., `writeWorkspaceImage(filePath, base64Data)` via contextBridge), or (b) use the main process IPC directly (`ipcRenderer.invoke('write-workspace-image', ...)`) bypassing the JSON-RPC backend.

3. **DOCX export "lightweight OOXML builder"**: Building OOXML from scratch is significantly harder than the estimate suggests. OOXML is a ZIP of XML files. While doable without `html-to-docx`, the implementation complexity is underestimated.

4. **File watching initialization**: The `watchFile` and `unwatchFile` IPC methods use Node's `fs.watch`. On startup, all previously open files need to be re-watched. The design does not specify how previously open files are tracked or restored on app restart.

5. **Lazy-loaded directory with very large trees**: The `loadDirectory` action caps at 5000 entries but the frontend renders them all in the tree. A tree with 5000 entries will have severe performance issues. Virtual scrolling or pagination should be considered but is not addressed.

#### 2.5.6 Decision

**APPROVE WITH AMENDMENTS**. Address Amendments 1-5 before implementation begins.

---

### 2.6 Feature 006 — Session Compaction

**Verdict**: APPROVE WITH AMENDMENTS (5 amendments required)

#### 2.6.1 Architecture Compliance

| Invariant | Status | Evidence |
|-----------|--------|----------|
| B-1 Types in loom-types | **PASS** | `CompactionConfig` in `loom-types/src/config/compaction.rs`. Registered in `config/mod.rs` and re-exported in `lib.rs`. |
| B-2 JSON-RPC 2.0 | N/A | No new JSON-RPC methods (internal backend feature). |
| B-3 Dispatch chain | N/A | No new dispatch handlers. |
| B-4 Crate boundaries | **PASS** | `CompactionResult` + `CompactionStrategy` in `loom-context/src/compaction.rs` (new module). `CompactionConfig` in loom-types. Logic in loom-context, orchestrator wiring in loom-core. |
| B-5 CloudClient trait | **PASS** | LLM summarization uses `build_auxiliary_client("summary")` -> `CloudClient::complete()`. |
| B-6 EventBus | **PASS** | `CompactionEvent` added to `AgentEvent`. `CompactionPerformed` added to `EngineEvent`. |
| B-7 SQLite persistence | **PASS** | Compaction is for LLM call only. Raw history preserved in DB. |
| B-8 Explicit migration | **PASS** | Feature flag via `compaction_config.enabled`. Default: false during rollout. |
| F-1 through F-6 | N/A | Backend-only feature. |

#### 2.6.2 Loom-Rootedness Checklist

| ID | Item | Status | Evidence/Issue |
|----|------|--------|----------------|
| L-063 | CompactionConfig in loom-types/src/config/compaction.rs | PASS | |
| L-064 | Registered in config/mod.rs + lib.rs | PASS | |
| L-065 | CompactionResult in loom-context/src/compaction.rs | PASS | Not in loom-types — contains implementation detail. |
| L-066 | compact() implemented on ContextAssembler | **PASS** | Replaces stub at lib.rs:145. But the stub signature changes — see Amendment 1. |
| L-067 | Heuristic logic in loom-context/src/compaction.rs | PASS | |
| L-068 | CompactionEvent in EngineEvent + AgentEvent | **PASS** — but dual emission is redundant. The `EngineEvent::CompactionPerformed` has more fields than `AgentEvent::CompactionEvent`. **AMENDMENT 1**: Justify the dual event emission, or consolidate to a single event type. If both are needed, document which consumers subscribe to which. |
| L-069 | Orchestrator compaction step placement | PASS | AFTER summary check, BEFORE system prompt assembly. |
| L-070 | Mid-turn compaction is heuristic-only | PASS | Explicitly stated. |
| L-071 | CompactionConfig in AgentLoopConfig with Default impl | PASS | |
| L-072 | reset_prefix() forces next check to be a miss | PASS | But depends on Feature 001's PrefixCache upgrade. |
| L-073 | Feature flag gate | **FAIL** — The `CompactionConfig` struct in Section 4.1 does NOT include an `enabled` field. Section 10.1 defines: `pub enabled: bool, // default: false during rollout`. This field is missing from the struct definition. **AMENDMENT 2**: Add `pub enabled: bool` to the CompactionConfig struct definition in Section 4.1. |
| L-074 | Auxiliary client for LLM summarization | PASS | `build_auxiliary_client("summary")`. |
| L-075 | temperature=0.0, reasoningEffort=off | PASS | |

#### 2.6.3 Anti-Pattern Scan

| Anti-Pattern | Found? | Severity |
|-------------|--------|----------|
| JSONL session compaction | No | In-memory only; DB preserves full history. |
| Implicit compaction | No | CompactionEvent emitted. |
| Compaction modifying database | No | Compacted history is for LLM only. Full history saved. |
| LLM call per mid-turn iteration | No | Mid-turn is heuristic-only. |

#### 2.6.4 Integration Gaps

| System | Status |
|--------|--------|
| Existing SummaryEngine | PASS — Compaction complements (doesn't replace) summarization. Summary is additive context; compaction reduces history size. |
| truncate_history() | PASS — Compaction runs BEFORE truncation in the flow. |
| PrefixCache (Feature 001) | **Cross-feature concern** — 006 adds `reset_prefix()` to PrefixCache; 001 rewrites PrefixCache to SHA256-based. The `reset_prefix()` method must work with BOTH the old DefaultHasher path and the new PrefixDigest path. **AMENDMENT 3**: Add explicit integration requirement: `reset_prefix()` must clear `last_digest` (from 001) AND `last_hash` (legacy). Test with both paths. |
| sanitize_message_sequence() | PASS — Existing sanitizer handles orphaned tool-result messages. |
| Session save/load | PASS — Compacted history not written to DB. |
| Token budget check | PASS — Preserved at agent_loop.rs. Compaction reduces chance of hitting the wall. |

#### 2.6.5 Missing Details

1. **`compact()` stub API breaking change**: The current stub at `loom-context/src/lib.rs:145` has signature `pub async fn compact(&self, _history: &[Message]) -> Result<Vec<Message>>`. The new version has signature `pub async fn compact(&self, history: &[Message], config: &CompactionConfig, llm_client: Option<&dyn CloudClient>) -> Result<CompactionResult>`. The return type changes from `Vec<Message>` to `CompactionResult`. All existing callers must be updated. **AMENDMENT 4**: Audit all callers of `compact()` before changing the signature. If there are callers, provide a migration plan.

2. **`collapse_repetitive_loops()` is `todo!()`**: The implementation in Section 5.2 includes `todo!("see detailed implementation in appendix")` but no appendix provides the implementation. **AMENDMENT 5**: Provide the `collapse_repetitive_loops()` implementation. At minimum, specify: how tool-calls are identified as identical (tool name + arguments JSON equality?), how many pairs are collapsed per loop detection pass, and the format of the collapsed summary message.

3. **`build_auxiliary_client("summary")` verification**: The design references this function to create an LLM client for summarization. Need to verify this method exists on `Orchestrator`. If not, specify its implementation.

4. **Orchestrator line reference**: Section 5.5.2 says "After the summary check block (after line 4759, before the system prompt assembly)" — `orchestrator.rs` may have a different line count in the actual codebase. The reference should be updated based on the actual file.

5. **Token counting for mixed content**: `count_tokens()` in Section 5.2 only counts `ContentPart::Text` tokens. `ContentPart::ToolResult`, `ContentPart::ToolCall`, `ContentPart::Thinking`, `ContentPart::Image` — these content types also consume tokens but are not counted. The token estimate will be inaccurate for tool-heavy conversations.

6. **`AgentLoopConfig` Integration**: The design adds `compaction_config` to `AgentLoopConfig` (Section 5.6.2). But this config already has many fields. Adding this to the orchestrator's config construction (both `process_message_streaming` and `process_message_with_config`) means multiple code paths must be updated.

#### 2.6.6 Decision

**APPROVE WITH AMENDMENTS**. Address Amendments 1-5 before implementation begins.

---

## 3. Cross-Feature Concerns

### 3.1 Conflicting Modifications

| Shared File | Modified By | Conflict Type | Resolution |
|------------|-------------|---------------|------------|
| `stores/index.ts` | 002, 003, 004, 005 | AppStore type union + create() factory | Merge order: 002 → 003 → 004 → 005. Each feature appends its slice. |
| `dispatch/mod.rs` | 003, 004, 005 | Handler registration order | Merge order: 003 (plan+goal) → 004 (completion) → 005 (vfs). Placement rules documented in each design. |
| `preload/index.ts` | 002, 005 | LoomApi interface + exposeInMainWorld | 002 adds readFile options. 005 adds 6 new methods. Non-overlapping changes. |
| `loom-inference/src/cache.rs` | 001, 006 | 001 rewrites PrefixCache (DefaultHasher → PrefixDigest). 006 adds reset_prefix(). | 001 must merge FIRST. 006's reset_prefix() MUST work with the upgraded PrefixCache. Add integration test. |
| `loom-core/src/agent_loop.rs` | 001, 006 | 001 adds prefix_digest computation + CompletionRequest field. 006 adds mid-turn compaction check. | Both can coexist in the iteration loop. 001 runs BEFORE the iteration loop (digest computation); 006 runs INSIDE the iteration loop (compaction check). |
| `loom-core/src/orchestrator.rs` | 003, 006 | 003 adds BuiltinCommands layer. 006 adds compaction step. | Both are inserted at different points in process_message_streaming. Unlikely to conflict directly. |
| `InputArea.tsx` | 002, 004 | 002 adds QuotedSelectionCard rendering. 004 adds conditional CodeMirrorInput. | 002's changes are ABOVE the textarea; 004 replaces the textarea itself. Low conflict risk. |
| `AppShell.tsx` | 002, 003, 005 | Layout changes for right panels + mode routing. | 005's ModeRouter is the primary change. 002's InlineInputOverlay mounts in App.tsx (not AppShell). 003's PlanPanel resides inside the right panel area. Coordinate layout around ModeRouter. |
| `main/ipc/index.ts` | 002, 005 | registerIpcHandlers() | 005 adds registerWriteIpc(). 002 extends existing read-file handler. Both are additive. |
| `QuotedSelectionCard.tsx` | 002, 005 | 002 makes onRemove optional. 005 references the component. | 002 should merge first. |

### 3.2 Shared Data Types Conflict

| Type | Feature 002 | Feature 005 | Issue |
|------|------------|------------|-------|
| `QuotedSelection` | Has `id: string`, `charCount: number`, NO `filePath` field in the interface (it's in QuotedSelectionBlock) | Has NO `id`, has `charCount: number`, `filePath: string` included | **Must unify**. Define a single `QuotedSelection` type in a shared location (not in a slice file). Both features import from the same source. See 005 Amendment 3. |

### 3.3 Right-Panel Layout Conflict (003 + 005)

Both features want to use the right sidebar area:
- 003: PlanPanel + TodoPanel (tabbed or stacked)
- 005: WriteAssistantPanel (collapsible)

**Recommended resolution**: Mode-exclusive right panel visibility. When appMode = 'chat', the right panel shows PlanPanel/TodoPanel (Feature 003). When appMode = 'write', the right panel shows WriteAssistantPanel (Feature 005). ModeRouter manages this. See 003 Amendment 3 and 005 Amendment 2.

### 3.4 CodeMirror Instance Isolation (004 + 005)

Both features create CodeMirror `EditorView` instances:
- 004: In InputArea (chat mode), for FIM ghost text
- 005: In WriteMarkdownEditor (write mode), for markdown editing

The FIM CompletionSource must not activate in Write mode. The CompletionSource should check `useStore.getState().appMode` and return null when not in chat mode. See 004 Amendment 2.

### 3.5 Merge Order Recommendation

Confirmed from the framework's Section 8.4 with one adjustment:

1. **001** (Prompt-Cache) — backend-only, touches PrefixCache
2. **006** (Compaction) — builds on 001's PrefixCache changes
3. **003** (Plan/SDD/Todo) — full-stack, adds dispatch handlers, modifies orchestrator
4. **002** (Inline Selection) — frontend, modifies InputArea (before 004 which replaces textarea)
5. **004** (FIM) — modifies InputArea, adds dispatch handler
6. **005** (Write Mode) — largest surface area, goes last to absorb all other changes

### 3.6 Aggregate Metrics

| Metric | Current | After 6 Features | Limit | Status |
|--------|---------|-----------------|-------|--------|
| Zustand slices | 17 | 22 (new: selection-context, plan, todo, completion, write) | 25 | OK |
| contextBridge methods | 30 | 36 (+6 for 005) | 45 | OK |
| Dispatch sub-handlers | 12 | 15 (+plan, +goal, +completion, +vfs, -none) | 20 | OK |
| Backend crates | 15 | 15 (no new crates) | 18 | OK |
| npm dependencies | current | +1 (`@codemirror/autocomplete`) | +5 | OK |
| Cargo dependencies | current | +2 (`hex`, `sha2` already workspace) | +3 | OK |

Note: 003 may require `fnv` or `uuid` crate — see 003 Amendment 5.
005 may add `react-markdown` — see 005 Amendment 1.
006 may require verification of `build_auxiliary_client()` availability.

---

## 4. Amendment Requirements — Consolidated

### Feature 001 (4 amendments)
1. **Fix legacy `check()` stub**: Preserve original DefaultHasher logic in the upgraded PrefixCache. Do not always return false.
2. **Gate cache_control on ModelBackend::Anthropic**: Add provider check before injecting `cache_control` breakpoint in `AnthropicClient::lower_messages()`.
3. **Add Copy derive to CacheStatus**: `#[derive(Debug, Clone, Copy, PartialEq, Eq)]`.
4. **Verify AgentLoopConfig Clone**: Ensure all fields implement Clone, or use a `clone_turn_config()` helper.

### Feature 002 (4 amendments)
1. **Align openInlineInput signature**: Extend the store action to accept optional filePath/startLine/endLine.
2. **Include data-attribute injection in implementation**: Do not defer to follow-up. Specify exact components and attribute injection.
3. **Specify QuotedSelection ID generation**: Either store generates IDs, or document that caller is responsible.
4. **Implement or remove setInlineInstructionText**: Currently a no-op in the store slice.

### Feature 003 (5 amendments)
1. **Specify WS push format for new AgentEvent variants**: Document JSON conversion in the WS bridge.
2. **Verify or define SessionData struct**: Check if it exists in dispatch/session.rs; create if not.
3. **Document right-panel coexistence with 005**: PlanPanel/TodoPanel hidden when in Write mode.
4. **Verify/add uuid crate for UUID v7**: Check workspace Cargo.toml.
5. **Resolve FNV1a hash dependency**: Add `fnv` crate or use `DefaultHasher` from std.

### Feature 004 (4 amendments)
1. **Verify Orchestrator::model_configs() exists**: Or add the accessor method.
2. **Add appMode guard in CompletionSource**: Skip FIM when in Write mode.
3. **Specify DeepSeek API key resolution**: Document how the API key is retrieved from model config.
4. **Provide GhostTextWidget DOM implementation**: At minimum, the toDOM() method specification.

### Feature 005 (5 amendments)
1. **Commit to single preview approach**: Use `markdown-it` + `dangerouslySetInnerHTML` (no new react-markdown dependency).
2. **Specify right-panel ownership**: Mode-exclusive visibility — Write mode hides PlanPanel/TodoPanel.
3. **Unify QuotedSelection type with 002**: Single type definition in shared location.
4. **Define createWriteSession helper**: Specify the session.create RPC call.
5. **Handle binary image upload**: Add IPC method or clarify binary data path for image paste.

### Feature 006 (5 amendments)
1. **Justify or consolidate dual event emission**: EngineEvent + AgentEvent redundancy.
2. **Add enabled field to CompactionConfig**: Missing from Section 4.1 struct definition.
3. **reset_prefix() must work with both PrefixCache paths**: Clear last_digest AND last_hash.
4. **Audit compact() callers before API change**: Breaking signature change from Vec<Message> to CompactionResult.
5. **Implement collapse_repetitive_loops()**: Replace todo!() with actual specification.

---

## 5. Aggregate Verdict

| Feature | Decision |
|---------|----------|
| 001 — Prompt-Cache Fingerprint | APPROVE WITH AMENDMENTS |
| 002 — Inline Selection Editor | APPROVE WITH AMENDMENTS |
| 003 — Plan/SDD/Todo Workflow | APPROVE WITH AMENDMENTS |
| 004 — FIM Code Completions | APPROVE WITH AMENDMENTS |
| 005 — Write Mode Workspace | APPROVE WITH AMENDMENTS |
| 006 — Session Compaction | APPROVE WITH AMENDMENTS |

**Total amendments: 27 across all 6 features.**

Amendments must be addressed within 48 hours per the review framework (Section 7.1). The reviewer will re-check only on amended items after the implementer confirms amendments are resolved.

---

## 6. Sign-off

**Reviewer**: Claude Opus (Neutral Reviewer)
**Date**: 2026-06-08
**Next Review**: Mid-Phase Review for Feature 001 (followed by Features 006, 003, 002, 004, 005 per merge order)

# 007 — Neutral Review Framework

## Status: In Effect
## Created: 2026-06-08
## Applies To: Features 001-006
## Review Authority: Neutral Reviewer (blocks merge if violations found)

---

## Table of Contents

1. [Purpose & Authority](#1-purpose--authority)
2. [Loom Architecture Invariants](#2-loom-architecture-invariants)
3. [Review Process](#3-review-process)
4. [Per-Feature Review Checklists](#4-per-feature-review-checklists)
   - [001 — Prompt-Cache Fingerprint](#41-001--prompt-cache-fingerprint)
   - [002 — Inline Selection Editor](#42-002--inline-selection-editor)
   - [003 — Plan/SDD/Todo Workflow](#43-003--plansddtodo-workflow)
   - [004 — FIM Code Completions](#44-004--fim-code-completions)
   - [005 — Write Mode Workspace](#45-005--write-mode-workspace)
   - [006 — Session Compaction](#46-006--session-compaction)
5. [Review Report Template](#5-review-report-template)
6. [Review Schedule](#6-review-schedule)
7. [Escalation Rules](#7-escalation-rules)
8. [Cross-Feature Interaction Matrix](#8-cross-feature-interaction-matrix)

---

## 1. Purpose & Authority

### 1.1 What This Framework Is

This document defines the **neutral review audit** required after each implementation phase of features 001-006. It codifies the rules, checklists, and decision authority needed to ensure every feature stays rooted in Loom's architecture rather than blindly copying DeepSeek-GUI patterns.

### 1.2 Who Follows It

- **Implementers**: Must design and code against the Loom-rootedness checklists in Section 4.
- **Neutral Reviewer**: An independent engineer (not the feature implementer) who audits the work using this framework.
- **Project Lead**: Receives review reports and enforces decisions.

### 1.3 Reviewer Powers

The neutral reviewer has four decision outcomes:

| Decision | Meaning | Consequence |
|----------|---------|-------------|
| **Approve** | No issues found. Feature respects all invariants. | Proceed to next phase. |
| **Approve with Amendments** | Minor deviations found. Specific changes required. | Implementer must address amendments within 48 hours. Reviewer re-checks only the amended items. |
| **Reject** | Fundamental architectural conflict. Feature cannot proceed as designed. | Requires redesign. A follow-up review is scheduled after the redesign. Work on this feature is blocked. |
| **Suggest Alternative** | Current approach works but a Loom-native alternative is better. | Not blocking. Implementer decides whether to adopt the suggestion. If adopted, re-review the changed portion. |

### 1.4 What the Reviewer Checks

The reviewer does NOT review code style, naming conventions, or test coverage (those are covered by the standard code-review process). The neutral reviewer focuses exclusively on:

1. **Architecture Compliance** — Does the feature use Loom's existing infrastructure correctly?
2. **Anti-Pattern Avoidance** — Does the feature avoid known DeepSeek-GUI patterns that conflict with Loom?
3. **Integration Integrity** — Do existing Loom systems still work after the feature is integrated?
4. **Cross-Feature Compatibility** — Do multiple features compose without conflict?

---

## 2. Loom Architecture Invariants

These rules are **non-negotiable**. Any violation is an automatic **Reject** unless the design document explicitly justifies the deviation and the reviewer accepts the justification.

### 2.1 Backend Invariants

#### B-1: Types in loom-types Only
- **Rule**: All shared data structures (structs, enums, type aliases consumed by more than one crate) MUST be defined in `loom-types`.
- **Verification**: `grep -r "pub struct\|pub enum" backend/crates/loom-types/src/ | wc -l` vs `grep -r "pub struct\|pub enum" backend/crates/loom-{core,server,inference,context,memory}/src/ | wc -l`. Types in non-types crates must have a documented justification.
- **Module Limit**: No file in `loom-types/src/` exceeds 250 lines.

#### B-2: JSON-RPC 2.0 Protocol
- **Rule**: All backend-frontend communication MUST use JSON-RPC 2.0 over WebSocket (primary) or HTTP POST (secondary). No custom protocols, no REST endpoints, no GraphQL.
- **Verification**: Every new backend method follows `{ "jsonrpc": "2.0", "method": "...", "params": {...} }` format. No raw HTTP endpoints.

#### B-3: Dispatch Chain Pattern
- **Rule**: New JSON-RPC methods MUST be registered as a sub-handler in `dispatch/mod.rs` following the `if let Some(result) = module::handle(state, method, &p).await` pattern.
- **Verification**: New dispatch modules export `pub async fn handle(state: &AppState, method: &str, p: &Value) -> Option<Result<Value, JsonRpcError>>`.

#### B-4: Crate Boundary Discipline
- **Rule**: New functionality MUST be placed in the existing crate closest to its responsibility. New crates require explicit justification.
- **Existing crate responsibilities**:
  - `loom-types` — canonical types, config, events
  - `loom-core` — orchestration, agent loop, builtin tools, slash routing
  - `loom-inference` — CloudClient trait, providers, caching, engine
  - `loom-context` — context assembly, prompt construction, compaction
  - `loom-memory` — KG, cognition, summaries, embeddings
  - `loom-server` — HTTP/WS server, dispatch handlers, app state
  - `loom-mcp` — MCP protocol
  - `loom-lsp` — LSP integration
  - `loom-skills` — skill execution
  - `loom-plugins` — plugin lifecycle
  - `loom-marketplace` — plugin/skill registry
  - `loom-cron` — scheduled tasks
  - `loom-bridge` — external process bridge
  - `loom-cli` — CLI interface
  - `loom-security` — sandbox, permissions

#### B-5: CloudClient Trait
- **Rule**: All LLM calls MUST go through a `CloudClient` implementor. No direct HTTP calls to LLM APIs outside of `loom-inference`.
- **Exception**: Feature 004 (FIM) explicitly bypasses CloudClient for DeepSeek's `/fim/completions` endpoint because it has a different request/response shape than chat completions. This exception is documented in 004's design and must be re-validated at each review.

#### B-6: EventBus for Cross-Crate Communication
- **Rule**: Cross-cutting events (plan created, compaction performed, tools changed) MUST be published on the EventBus (`loom-core/src/event_bus.rs`). Direct cross-crate calls for event-like notifications are prohibited.
- **Verification**: New `AgentEvent` variants are documented with their producing crate and consuming crate(s).

#### B-7: SQLite Persistence
- **Rule**: Persistent state MUST use the existing 3-DB split (config.db, memory.db, session.db). No new database files without architectural review.
- **Exception**: Plan markdown files (003) are stored on the filesystem under `.loom/plans/`, which is an intentional design choice documented in 003. This is acceptable because plans are human-readable, git-trackable documents, not structured data.

#### B-8: No Implicit State Migration
- **Rule**: Settings migration must be explicit via `setPreference`. No silent upgrades of user configuration.
- **Verification**: Any new preference key is documented. Old keys are not removed without a migration path.

### 2.2 Frontend Invariants

#### F-1: Zustand Slice Pattern
- **Rule**: All shared state MUST use Zustand slices composed via `StateCreator` in `stores/index.ts`. No React Context for runtime state communication. No Redux.
- **Verification**: New store files export `createXxxSlice: StateCreator<XxxSlice>`. The AppStore type union in `stores/index.ts` includes the new slice. Zero cross-slice imports (a slice file MUST NOT import another slice file).
- **Slice count**: Currently 17 slices. After all 6 features, max 21 slices (new: plan, todo, selection-context, completion, write). Document the slice count delta.

#### F-2: contextBridge IPC
- **Rule**: All main-process communication MUST go through `contextBridge.exposeInMainWorld('loom', {...})`. No `nodeIntegration`, no direct `require('electron')` in the renderer.
- **Verification**: New IPC methods are added to both the `LoomApi` TypeScript interface AND the `exposeInMainWorld` call. Currently 30 exposed methods.

#### F-3: JSON-RPC for Backend Calls
- **Rule**: All backend data operations MUST use `loomRpc<T>(method, params)` from `services/jsonrpc.ts`. No direct `fetch()` calls to the backend.
- **Verification**: New RPC calls use the existing `loomRpc` function. Timeout behavior (30s default, 30min for long operations) is respected.

#### F-4: StreamBufferManager for Streaming
- **Rule**: All streaming LLM responses MUST flow through the existing `StreamBufferManager` singleton with RAF-based flush at 16ms intervals.
- **Verification**: New streaming paths (e.g., Write mode inline AI) reuse the existing stream buffer infrastructure, not a parallel implementation.

#### F-5: No React Router
- **Rule**: No `react-router` or similar routing library. View switching is done via conditional rendering based on store state.
- **Verification**: New views (Write mode, Plan panel) are conditionally rendered based on store values, not URL paths.

#### F-6: Tailwind + CSS Custom Properties
- **Rule**: Styling MUST use Tailwind CSS 4 utility classes OR CSS custom properties that reference the 9-theme system via `var(--xxx)` tokens. No hardcoded colors.
- **Verification**: New CSS contains no hex/rgb/rgba color values outside of `:root` theme definitions. Uses `var(--bg)`, `var(--text)`, `var(--accent)`, etc.

### 2.3 Unique Differentiator Preservation

These are Loom's unique strengths. Features must not degrade them:

| Differentiator | Verification Question |
|----------------|---------------------|
| **Knowledge Graph + Cognition** | Does the feature interact with KG data? If yes, does it go through `loom-memory` interfaces (not direct SQL)? |
| **Multi-Provider Inference** | Does the feature work with all 5 providers, or does it hardcode a specific provider? |
| **LSP Integration** | Does the feature interfere with LSP diagnostics/completion/hover handlers? |
| **Plugin/Skill/Marketplace** | Does the feature respect the 3-tier extensibility model? Does it work when plugins are active? |
| **Desktop Pet** | Does the feature overlap or conflict with the pet window? (e.g., Write mode takes full window -- does it obscure the pet?) |
| **Rust Backend Performance** | Does the feature add synchronous blocking calls to the Tokio runtime? Does it increase cold-start time? |
| **Zustand Sliced Store** | Does the feature add cross-slice dependencies? |

### 2.4 Anti-Patterns (Must NOT Appear)

| Anti-Pattern | DeepSeek-GUI Origin | Loom Replacement |
|-------------|---------------------|-----------------|
| React Context for runtime state | `AppContext` wrapping the entire app | Zustand store slices |
| JSONL event sourcing for storage | `messages.jsonl` per session | SQLite session.db |
| Bundling runtime inside Electron | Bundled Python/PyTorch | External `loom.exe` spawned as child process |
| Implicit settings migration | Automatic config upgrades | Explicit `setPreference` calls |
| Hardcoded Chinese strings | UI text in component code | i18n infrastructure (future; hardcoded English acceptable for now) |
| Over-engineering MCP | `mcp.json` config files, `mcp:*` HTTP endpoints | Direct service registration (current approach is sufficient) |

---

## 3. Review Process

### 3.1 Review Timeline

Each feature goes through up to 4 reviews:

```
DESIGN ──► PRE-IMPL REVIEW ──► IMPLEMENTATION ──► MID-PHASE REVIEW ──► POST-PHASE REVIEW
                                                                              │
                                                                              ▼
                                                                     CROSS-FEATURE REVIEW
                                                                     (after 2+ features done)
```

### 3.2 Pre-Implementation Review

**Timing**: After the design document is written, before any code is written.

**Artifacts Required**:
- Completed design document (markdown in `docs/design/`)
- Annotated architecture diagram showing how the feature integrates with existing Loom components

**Review Duration**: 30-60 minutes

**Checklist**:
- [ ] Design references specific Loom crates/files/types (not generic abstractions)
- [ ] New types are placed in the correct crate (loom-types, loom-context, or feature-local)
- [ ] JSON-RPC methods follow the existing naming convention (`{domain}.{action}`)
- [ ] New store slices follow the `StateCreator` pattern
- [ ] IPC additions go through `contextBridge`
- [ ] Anti-patterns from Section 2.4 are identified and mitigated
- [ ] Integration points with existing systems (SessionStore, EventBus, PrefixCache, etc.) are documented
- [ ] Feature flag strategy is defined (enable/disable without code changes)

### 3.3 Mid-Phase Review

**Timing**: At the midpoint of implementation, when ~50% of planned work is done.

**Artifacts Required**:
- Working branch with partial implementation
- List of completed vs. remaining implementation steps

**Review Duration**: 45-90 minutes

**Checklist**:
- [ ] Actual code structure matches the design document (no undocumented architectural drift)
- [ ] New crate files are in the expected locations
- [ ] New dispatch modules follow the `handle()` pattern and are registered in `mod.rs`
- [ ] New store slices are registered in `stores/index.ts` with correct TypeScript types
- [ ] IPC method count has not grown without documentation
- [ ] No new direct `fetch()` calls in renderer code
- [ ] Streaming paths use `StreamBufferManager`
- [ ] No cross-slice imports in Zustand stores
- [ ] CSS uses `var(--xxx)` theme tokens, not hardcoded colors
- [ ] Existing tests still pass (`cargo test`, `npm test`)
- [ ] No performance regressions (cold start time, memory usage)

### 3.4 Post-Phase Review

**Timing**: After a complete phase of implementation is done and all tests pass.

**Artifacts Required**:
- Completed implementation on the feature branch
- All unit tests passing
- Integration/E2E tests passing (or documented reasons for skipping)
- Updated design document with any deviations from the original plan noted

**Review Duration**: 60-120 minutes

**Checklist (all previous checklists apply, plus)**:
- [ ] Every public type in new code has doc comments listing its consumers
- [ ] No file in `loom-types/src/` exceeds 250 lines
- [ ] No new crate was created without documented justification
- [ ] `Cargo.toml` changes are minimal and justified (no unused dependencies)
- [ ] `package.json` changes are minimal and justified (no unused dependencies)
- [ ] The feature can be toggled off without affecting existing functionality
- [ ] Backward compatibility: existing sessions work, existing APIs unchanged
- [ ] The feature works with at least 2 providers (Anthropic + one other)
- [ ] Manual smoke test checklist items from the design document pass

### 3.5 Cross-Feature Review

**Timing**: After 2 or more features have completed their Post-Phase reviews.

**Artifacts Required**:
- All completed feature branches merged to a staging/review branch
- Updated `stores/index.ts` showing all new slices composed together
- Updated `dispatch/mod.rs` showing all new handlers registered

**Review Duration**: 90-180 minutes

**Checklist**:
- [ ] All store slices compose without TypeScript errors
- [ ] No two features added the same preference key
- [ ] No two features added conflicting JSON-RPC method names
- [ ] No two features modified the same file in incompatible ways
- [ ] No two features added IPC methods with the same name but different signatures
- [ ] Feature flags compose correctly (turning one feature off doesn't break another)
- [ ] Total IPC method count (after all features) is tracked
- [ ] Total store slice count (after all features) is tracked
- [ ] Total dispatch handler count is tracked
- [ ] Compile time has not increased by more than 20%
- [ ] Bundle size has not increased by more than 30%

### 3.6 Review Artifact: The Review Report

Every review produces a report following the template in Section 5. Reports are committed to `docs/reviews/` with the naming convention `{feature-number}-{review-type}-{date}.md` (e.g., `001-pre-impl-2026-06-10.md`).

---

## 4. Per-Feature Review Checklists

### 4.1 001 — Prompt-Cache Fingerprint

**Effort**: 1 day | **Scope**: Backend only | **Crates**: loom-context, loom-inference, loom-core, loom-types

#### 4.1.1 Loom-Rootedness Checklist

Verify these specific yes/no items:

- [ ] L-001 Is `PrefixDigest` defined in `loom-context/src/lib.rs` (not in loom-types and not in a new crate)?
  - *Expected*: Yes, because PrefixDigest is tightly coupled to ContextAssembler's computation logic.
- [ ] L-002 Is `CacheStatus` defined in `loom-inference/src/cache.rs` alongside the existing `PrefixCache`?
  - *Expected*: Yes.
- [ ] L-003 Does `CompletionRequest.prefix_digest` use `Option<PrefixDigest>` (not a required field)?
  - *Expected*: Yes, for backward compatibility with providers that don't implement the new flow.
- [ ] L-004 Are new `CloudClient` trait methods added as default stubs (not required abstract methods)?
  - *Expected*: Yes, so existing provider implementations compile without changes.
- [ ] L-005 Does `compute_prefix_digest()` use SHA256 from the `sha2` crate already in `[workspace.dependencies]`?
  - *Expected*: Yes, using `sha2.workspace = true`.
- [ ] L-006 Is the `DefaultHasher`-based `check()` method preserved for backward compatibility with internal LLM calls (summary generation)?
  - *Expected*: Yes, not removed.
- [ ] L-007 Does `AnthropicClient` gate `cache_control` injection behind `ModelBackend::Anthropic`?
  - *Expected*: Yes, to avoid sending invalid parameters to non-Anthropic models.
- [ ] L-008 Is per-component drift logged using `tracing::info!` (not `println!`)?
- [ ] L-009 Does the agent loop compute the digest in `run_agent_turn_inner()` and `run_agent_turn_streaming_inner()` at the same point (before the iteration loop)?
- [ ] L-010 Is `sha2` referenced as `workspace = true` in `loom-context/Cargo.toml` (not duplicated with its own version)?

#### 4.1.2 Anti-Pattern Watchlist

| Anti-Pattern | Why It's a Risk for 001 |
|-------------|------------------------|
| Hardcoded provider logic | Cache behavior differs per provider. If Anthropic-specific logic leaks into `loom-context` or `loom-core`, it violates the provider abstraction. |
| Over-hashing | Hashing the entire message array (including dynamic suffix) instead of only the stable prefix would make the fingerprint useless — every turn would be a miss. |
| New crate for "cache types" | Creating a `loom-cache` crate would fragment the inference layer. Cache types belong in `loom-inference` or `loom-context`. |

#### 4.1.3 Integration Points to Verify

| System | How to Verify No Breakage |
|--------|--------------------------|
| Existing PrefixCache users (orchestrator internal calls, summary engine, vision) | Run existing integration tests for summary generation. Legacy `check()` path must still work. |
| CloudClient trait implementors (Anthropic, OpenAI, DeepSeek, LM Studio, Ollama) | All 5 providers must compile and initialize without errors after the change. |
| AgentLoopConfig | Verify `Clone` derivation still works for all existing fields. |
| EventBus snapshot/restore flow | Existing `prefix_hash_snapshot()` / `prefix_hash_restore()` must not be broken. New `prefix_digest_snapshot()` / `prefix_digest_restore()` must coexist. |

#### 4.1.4 Acceptance Criteria

1. Two consecutive turns with the same system prompt + persona: Turn 1 shows ColdStart, Turn 2 shows Hit.
2. Editing persona between turns: next turn shows BreakingMiss with `["persona"]` in drift reasons.
3. All 5 providers start without errors.
4. Existing tests pass.
5. `tracing::info!` output shows "KV cache hit/miss" with token savings.

---

### 4.2 002 — Inline Selection Editor

**Effort**: 1 week | **Scope**: Frontend + minor IPC changes

#### 4.2.1 Loom-Rootedness Checklist

- [ ] L-011 Is `SelectionContextSlice` in `stores/selection-context.ts` using the `StateCreator<SelectionContextSlice>` pattern?
- [ ] L-012 Is `SelectionContextSlice` registered in `stores/index.ts` as part of `AppStore` type union and `useStore` creator?
- [ ] L-013 Does `SelectionContextSlice` import zero other slice files?
  - *Expected*: Yes. It must not import from `stores/chat.ts`, `stores/input.ts`, etc.
- [ ] L-014 Is the IPC change to `read-file` backward compatible: existing callers pass only `filePath` and get full file content?
- [ ] L-015 Is the `LoomApi` interface in `preload/index.ts` updated with the new `readFile` signature?
- [ ] L-016 Is `InlineInputOverlay` mounted in `App.tsx` alongside existing global components (`ToastContainer`, `ConfirmDialog`)?
- [ ] L-017 Does `InlineInput` use `ReactDOM.createPortal` to render to `document.body` (not inside the component tree)?
- [ ] L-018 Does `InputArea.tsx` read `quotedSelections` from the store and render `QuotedSelectionCard` components above the textarea (same pattern as `AttachedFiles`)?
- [ ] L-019 Does `sendMessage.ts` accept `quotedSelections` as an optional field in `SendMessageOptions` (not required)?
- [ ] L-020 Are `quoted_selection` blocks serialized in the message payload that goes to the backend via the existing `chat.send` RPC?
- [ ] L-021 Is `QuotedSelectionCard`'s `onRemove` prop made optional (hidden in chat history, shown in InputArea)?

#### 4.2.2 Anti-Pattern Watchlist

| Anti-Pattern | Why It's a Risk for 002 |
|-------------|------------------------|
| React Context for inline input state | The floating input's visibility and selection data must be in Zustand. Using React Context would mean it doesn't persist across component remounts. |
| Direct DOM manipulation for positioning | InlineInput position should be computed from `getBoundingClientRect()` + store state, not by injecting inline styles into the source DOM. |
| New IPC channel for selection data | Selection data flows through `chat.send` blocks, not a custom IPC channel. Avoid creating `inline-selection:*` IPC methods. |
| Hardcoded `Ctrl+Shift+I` without escape hatch | The hotkey must be configurable or at minimum documented as conflicting with DevTools. The design doc acknowledges this. |

#### 4.2.3 Integration Points to Verify

| System | How to Verify No Breakage |
|--------|--------------------------|
| InputArea textarea | Existing typing, Enter-to-send, Ctrl+Enter, file attachment, image paste must all still work. |
| sendMessage flow | Messages without quoted selections must serialize identically to before (no extra fields in JSON). |
| ChatWorkspace message rendering | UserMessage must handle `quoted_selection` blocks without crashing. AssistantMessage must ignore them. |
| QuotedSelectionCard existing usage | If QuotedSelectionCard was previously used anywhere, the `onRemove` change must not break it. |
| read-file IPC handler | Existing callers that pass only `filePath` must receive full file content (unchanged behavior). |

#### 4.2.4 Acceptance Criteria

1. Selecting text in a chat message and pressing Ctrl+Shift+I shows the floating input near the selection.
2. Typing an instruction and pressing Enter adds a QuotedSelectionCard to InputArea.
3. Sending the message includes `quoted_selection` blocks in the payload.
4. Chat history renders `quoted_selection` blocks read-only.
5. Pressing Escape dismisses the inline input without adding a card.
6. Pressing Ctrl+Shift+I with no text selection does NOT open the inline input.
7. DevTools still opens with Ctrl+Shift+I when no text is selected.

---

### 4.3 003 — Plan/SDD/Todo Workflow

**Effort**: 2 weeks | **Scope**: Full-stack

#### 4.3.1 Loom-Rootedness Checklist

- [ ] L-022 Are `PlanArtifact`, `TodoItem`, `ThreadGoal`, and all status enums defined in `loom-types/src/plan.rs`?
- [ ] L-023 Is `loom-types/src/plan.rs` under 250 lines?
- [ ] L-024 Are `plan.rs` and `goal.rs` dispatch handlers registered in `dispatch/mod.rs` following the `if let Some(result) = plan::handle(...)` pattern?
- [ ] L-025 Is `SlashRouter` extended via a `BuiltinCommands` layer in the orchestrator (not by modifying `SlashRouter` itself)?
- [ ] L-026 Is the `create_plan` tool added to `builtin_tools.rs` using the existing `ToolDef` pattern?
- [ ] L-027 Are new `AgentEvent` variants (`PlanCreated`, `PlanUpdated`, `GoalSet`, `TodoStatusChanged`) added to the existing `AgentEvent` enum in `loom-core/src/event_bus.rs`?
- [ ] L-028 Do the WebSocket push events use the existing EventBus-to-WS bridge (not a new WebSocket message type)?
- [ ] L-029 Are `plan.ts` and `todo.ts` Zustand slices using the `StateCreator` pattern?
- [ ] L-030 Do `plan.ts` and `todo.ts` import zero other slice files?
- [ ] L-031 Is `PlanPanel` conditionally rendered based on `planPanelOpen` store value (not a route)?
- [ ] L-032 Is `TodoPanel` conditionally rendered based on `todoPanelOpen` store value (not a route)?
- [ ] L-033 Are plan markdown files stored on the filesystem under `.loom/plans/` (not in SQLite)?
  - *Note*: This is an intentional deviation from B-7, documented in the 003 design. The reviewer must confirm the justification is in the design doc.
- [ ] L-034 Does `PlanPanel` autosave use debouncing (650ms) via `plan.update` RPC (not a new IPC channel)?
- [ ] L-035 Does `TodoPanel` toggle use `todo.update_status` RPC (not direct filesystem writes)?

#### 4.3.2 Anti-Pattern Watchlist

| Anti-Pattern | Why It's a Risk for 003 |
|-------------|------------------------|
| JSON-based plan metadata files | DeepSeek-GUI stores plan metadata in separate `.json` files. Loom's design uses YAML frontmatter in markdown. A parallel `.json` file per plan would be an anti-pattern. |
| Separate `plan` SQLite database | Creating a `plan.db` would violate the 3-DB split invariant. Plan metadata in SessionData and plan content on the filesystem is the correct approach. |
| React Router for panel navigation | PlanPanel and TodoPanel must be conditionally rendered components, not routes. |
| Hardcoded slash commands replacing the SlashRouter | The `BuiltinCommands` layer must complement (run before) the `SlashRouter`, not replace it. Registered skills with `/plan` prefix must still be intercepted correctly. |

#### 4.3.3 Integration Points to Verify

| System | How to Verify No Breakage |
|--------|--------------------------|
| SlashRouter | Registered skills with names starting with "plan", "goal", "review", "execute" must still work. BuiltinCommands takes priority, but if BuiltinCommands returns Passthrough, SlashRouter gets a chance. |
| SessionStore | SessionData extension must not break existing session save/load. New fields must be `Option` types. |
| dispatch/mod.rs | The dispatch chain order matters. `plan::handle` and `goal::handle` must be inserted at the right position. If placed after `cron::handle`, RPC calls for plan/goal methods might be incorrectly rejected if cron has a matching prefix. |
| ChatWorkspace layout | Opening PlanPanel must shrink ChatArea. Closing must restore it. Right panel must not overlap the left sidebar. |
| Existing message rendering | New block types (plan, goal) must be handled gracefully by UserMessage and AssistantMessage components. |
| FileEdit tool | The agent's `file_edit` tool must be able to edit plan markdown files. The resulting file change must trigger plan re-extraction. |

#### 4.3.4 Acceptance Criteria

1. `/plan "Add feature X"` creates a `.loom/plans/<uuid>.md` file with YAML frontmatter and checkbox items.
2. `plan.list` returns the created plan.
3. PlanPanel opens automatically after plan creation and shows the markdown content.
4. TodoPanel shows extracted todo items from the plan file.
5. Editing plan markdown in PlanPanel triggers autosave and re-extracts todos.
6. Toggling a todo in TodoPanel updates the plan file and the panel.
7. `/execute` makes the agent read and execute the plan checkboxes.
8. Agent marking a checkbox `- [x]` via FileEdit triggers WebSocket push that updates TodoPanel.
9. `/goal "Description"` sets a thread goal visible in TodoPanel.
10. All existing chat functionality works unchanged when PlanPanel is closed.

---

### 4.4 004 — FIM Code Completions

**Effort**: 2 weeks | **Scope**: Backend + Frontend (CodeMirror integration)

**Critical Note**: This feature contains the only explicit exception to invariant B-5 (CloudClient trait). The `/fim/completions` DeepSeek endpoint has a fundamentally different request/response shape than `/chat/completions`, so it cannot go through `CloudClient::complete_stream()`. The reviewer must verify that this exception is:
1. Documented in the design doc (it is).
2. Implemented as a narrowly-scoped bypass (only the HTTP call to DeepSeek's `/fim/completions` endpoint).
3. Not used as precedent for other feature bypasses.

#### 4.4.1 Loom-Rootedness Checklist

- [ ] L-036 Is `completion.fim` registered in `dispatch/mod.rs` as `completion::handle` following the standard dispatch pattern?
- [ ] L-037 Does `completion.fim` return errors as `{ ok: false, message: "..." }` in the result object (not as JSON-RPC error objects)?
- [ ] L-038 Is FIM provider resolution done by scanning existing model configs (`state.orchestrator.model_configs()`) rather than hardcoding DeepSeek credentials?
- [ ] L-039 Does `FimService` live in `loom-server/src/services/fim.rs` (or inline in `dispatch/completion.rs`), not in `loom-inference`?
  - *Expected*: Yes, because it bypasses the CloudClient trait. It should be in loom-server, not loom-inference, to make the exception visible.
- [ ] L-040 Is `CodeMirrorInput.tsx` a drop-in replacement for the `<textarea>` in `InputArea`, rendered conditionally based on a feature flag (not always-on)?
- [ ] L-041 Is `CompletionSlice` in `stores/completion.ts` using the `StateCreator<CompletionSlice>` pattern?
- [ ] L-042 Does `CompletionSlice` import zero other slice files?
- [ ] L-043 Is the existing `<textarea>` fallback preserved when the feature flag is off or CodeMirror fails to initialize?
- [ ] L-044 Does `sendMessage()` work identically regardless of whether the textarea or CodeMirror editor is active?
- [ ] L-045 Is ghost text rendered via a CodeMirror `ViewPlugin` + `DecorationSet` (not by appending text to the document)?
- [ ] L-046 Is Tab acceptance handled by a CodeMirror `keymap` binding with `Prec.highest` priority?
- [ ] L-047 Is the abort generation mechanism (`fimAbortGeneration` counter) used instead of modifying `loomRpc` to support `AbortSignal`?
  - *Expected*: Yes, per the v1 design decision. If `loomRpc` is later modified to support `AbortSignal`, that must be a separate change with its own review.

#### 4.4.2 Anti-Pattern Watchlist

| Anti-Pattern | Why It's a Risk for 004 |
|-------------|------------------------|
| Bundling a model runtime in Electron | The FIM completions go to DeepSeek's cloud API. No local model runtime bundled. Verify no new binary dependencies. |
| Replacing textarea wholesale without fallback | If CodeMirror fails to initialize (e.g., missing dependency), the textarea must still work. Verify conditional rendering with error boundary. |
| Direct `fetch()` from renderer to DeepSeek API | All external API calls must go through the backend. The renderer calls `loomRpc('completion.fim', ...)` which goes through the WebSocket to the backend. |
| Adding `@codemirror/autocomplete` only for ghost text | CodeMirror's built-in autocomplete popup is designed for symbol completion, not ghost text. Verify that the `CompletionSource` is used only for debounce orchestration, not for popup display. |
| Cross-slice imports for FIM state | `CompletionSlice` must not import `InputSlice` or `ChatSlice`. FIM state is independent. |

#### 4.4.3 Integration Points to Verify

| System | How to Verify No Breakage |
|--------|--------------------------|
| InputArea textarea mode | When FIM is disabled, typing, Enter, Ctrl+Enter, file attachment, paste all work as before. |
| sendMessage flow | Messages sent from CodeMirror input produce identical backend payloads to messages sent from textarea. |
| WebSocket connection | FIM requests must not congest the WebSocket. FIM payloads are small (<5KB). Verify no starvation of chat.stream_delta messages. |
| dispatch chain | `completion::handle` must not shadow any existing `chat.*`, `session.*`, or `model.*` methods. Its position in the dispatch chain matters. |
| @codemirror/* packages | Verify `@codemirror/autocomplete` is the only new npm dependency. Existing CodeMirror packages are sufficient. |

#### 4.4.4 Acceptance Criteria

1. Setting `fim.enabled = true` activates the CodeMirror editor with ghost text completions.
2. Short completion appears ~500ms after typing, disappears on next keystroke.
3. Long completion appears ~2.5s after pause.
4. Tab accepts the ghost text; Escape dismisses it.
5. Setting `fim.enabled = false` reverts to the plain textarea with no loss of functionality.
6. No DeepSeek model configured: `completion.fim` returns `{ ok: false, message: "..." }`.
7. Rapid typing causes stale completions to be discarded (abort generation counter).
8. WebSocket reconnects without issues; FIM gracefully degrades.

---

### 4.5 005 — Write Mode Workspace

**Effort**: 3 weeks | **Scope**: Frontend-heavy + minor backend additions

#### 4.5.1 Loom-Rootedness Checklist

- [ ] L-048 Is `WriteSlice` in `stores/write.ts` using the `StateCreator<WriteSlice>` pattern?
- [ ] L-049 Does `WriteSlice` import zero other slice files?
- [ ] L-050 Is `appMode` added to `UiSlice` (via `stores/ui.ts`) rather than creating a new slice for mode management?
- [ ] L-051 Is `ModeRouter.tsx` conditionally rendering `ChatWorkspace` or `WriteWorkspaceView` based on `appMode` store value (not a route)?
- [ ] L-052 Are VFS methods (`vfs.*`) registered in `dispatch/mod.rs` as `vfs::handle` following the standard dispatch pattern?
- [ ] L-053 Do VFS methods validate that resolved paths are within `workspace_root` (path traversal protection)?
- [ ] L-054 Are new IPC methods (`pickWorkspaceDirectory`, `readWorkspaceImage`, `exportWriteDocument`, `copyWriteDocumentAsRichText`, `watchFile`, `unwatchFile`) added to both the `LoomApi` interface AND the `exposeInMainWorld` call?
- [ ] L-055 Are `registerWriteIpc()` handlers in `frontend/src/main/ipc/write.ts` following the existing pattern (`ipcMain.handle(...)`)?
- [ ] L-056 Is `exportWriteDocument` registered in `frontend/src/main/ipc/index.ts` via `registerWriteIpc()`?
- [ ] L-057 Does `WriteMarkdownEditor` use the existing `codemirror` + `@codemirror/*` packages (no new editor dependency)?
- [ ] L-058 Are write threads stored as regular backend sessions with a `mode: 'write'` metadata tag (not a separate session type)?
- [ ] L-059 Is `WriteAssistantPanel` reusing existing message rendering components (`UserMessage`, `AssistantMessage`) rather than creating duplicates?
- [ ] L-060 Is `WriteMarkdownPreview` using the existing `markdown-it` + `highlight.js` packages (already in `package.json`)?
- [ ] L-061 Does the export pipeline use Electron's built-in `printToPDF()` (no Puppeteer dependency)?
- [ ] L-062 Are all Write mode preferences stored via `window.loom.setPreference()`?

#### 4.5.2 Anti-Pattern Watchlist

| Anti-Pattern | Why It's a Risk for 005 |
|-------------|------------------------|
| React Context for workspace state | Write mode has many sub-components (file tree, editor, preview, assistant). All shared state must be in the Zustand `WriteSlice`. |
| TipTap instead of CodeMirror | `@tiptap/*` is in `package.json` but unused. Using TipTap for the markdown editor would add maintenance burden and diverge from 004's CodeMirror investment. |
| Separate WebSocket connection for Write mode | Write mode and Chat mode must share the same WebSocket. Creating a parallel connection would double resource usage. |
| Server-side export rendering | Export happens in the Electron main process (headless BrowserWindow), not in the Rust backend. No new backend dependencies for export. |
| Hardcoded workspace root | The workspace root must be user-configurable and stored in preferences. No hardcoded `~/Documents/loom-workspace/`. |
| New `react-markdown` dependency without justification | If `react-markdown` is added to `package.json`, document why `markdown-it` (already available) is insufficient for the preview use case. |

#### 4.5.3 Integration Points to Verify

| System | How to Verify No Breakage |
|--------|--------------------------|
| ChatWorkspace | Switching to Write mode must preserve chat state (messages, streaming, input text). Switching back must restore it. |
| Sidebar / AppShell | The left sidebar and right Write panels must not overlap. Responsive widths must accommodate all three columns. |
| Session system | Write threads must appear in the session list alongside chat threads. They must be filterable but not hidden. |
| WebSocket | Both Chat and Write modes share one WebSocket. Stream events for a Write-mode chat.send must not interfere with Chat-mode messages. |
| Imported packages | No new npm dependencies beyond what's documented. Specifically: no puppeteer, no html-to-docx, no additional editor packages. |
| Theme system | All Write mode components must respond to theme changes (data-theme attribute) using CSS custom properties. |

#### 4.5.4 Acceptance Criteria

1. Ctrl+Shift+W switches to Write mode. Ctrl+Shift+C switches back to Chat mode.
2. Workspace picker allows selecting a directory. File tree renders directories and markdown files.
3. Opening a markdown file shows it in the CodeMirror editor with live preview.
4. Source/Live/Split/Preview modes all render correctly.
5. Autosave works: typing marks the file as dirty, 650ms of inactivity triggers save.
6. File watching: external edits to the open file are detected and content is reloaded.
7. Inline AI: selecting text and invoking the assistant inserts AI-generated content at the cursor.
8. Export produces valid HTML, PDF, and DOCX files.
9. Mode switching preserves state: chat messages survive the switch, editor content survives the switch back.
10. Feature flag OFF: Write mode tab is hidden or disabled.

---

### 4.6 006 — Session Compaction

**Effort**: 1.5 weeks | **Scope**: Backend only

#### 4.6.1 Loom-Rootedness Checklist

- [ ] L-063 Is `CompactionConfig` defined in `loom-types/src/config/compaction.rs` following the existing config module pattern?
- [ ] L-064 Is `CompactionConfig` registered in `loom-types/src/config/mod.rs` with `pub mod compaction;` and re-exported via `lib.rs`?
- [ ] L-065 Is `CompactionResult` defined in `loom-context/src/compaction.rs` (new module within loom-context), not in loom-types?
  - *Expected*: Yes, because CompactionResult is produced by the compaction logic and consumed by loom-core. It contains implementation details (strategy lists) that belong in the context crate.
- [ ] L-066 Is `compact()` implemented as a method on `ContextAssembler` (replacing the existing stub at `loom-context/src/lib.rs:145`)?
- [ ] L-067 Is the heuristic compaction logic (truncation, elision, loop collapse) isolated in `loom-context/src/compaction.rs`?
- [ ] L-068 Are `CompactionEvent` variants added to BOTH `EngineEvent` (in loom-types) AND `AgentEvent` (in loom-core)?
- [ ] L-069 Is the compaction step in the orchestrator inserted AFTER the summary check and BEFORE the system prompt assembly (not at an arbitrary point)?
- [ ] L-070 Is mid-turn compaction (in agent_loop.rs) heuristic-only (no LLM call)?
- [ ] L-071 Is `CompactionConfig` added to `AgentLoopConfig` as a field with a `Default` impl?
- [ ] L-072 Does `PrefixCache`'s `reset_prefix()` method properly force the next `check()` to be a miss?
- [ ] L-073 Is compaction gated behind `compaction_config.enabled` (feature flag)?
- [ ] L-074 Does LLM summarization use a separate auxiliary client (`build_auxiliary_client("summary")`) rather than the main model?
- [ ] L-075 Does the compaction LLM call use `temperature=0.0` and `reasoningEffort=off`?

#### 4.6.2 Anti-Pattern Watchlist

| Anti-Pattern | Why It's a Risk for 006 |
|-------------|------------------------|
| JSONL session compaction | DeepSeek-GUI uses JSONL event sourcing for session storage and might compact by rewriting the JSONL file. Loom uses SQLite. Compaction must operate on the in-memory message history, not on the database. |
| Implicit compaction (no user visibility) | The design explicitly requires `CompactionEvent` emission so the frontend can display what was compacted. Avoid silent compaction that confuses users about "lost" messages. |
| Compaction that modifies the database | Compacted history is for the LLM call only. The raw history written to session.db via `save_turn` must preserve full fidelity. Compaction is a prompt optimization, not a storage optimization. |
| Adding an LLM call per iteration | Mid-turn compaction is heuristic-only to avoid latency. LLM summarization only at inter-turn compaction. Verify that the mid-turn compaction path does not call `llm_summarize()`. |

#### 4.6.3 Integration Points to Verify

| System | How to Verify No Breakage |
|--------|--------------------------|
| Existing summary engine (`loom-memory/src/summary.rs`) | Summary generation must still work. Compaction complements (does not replace) the summary engine. Summary is additive context; compaction reduces history size. |
| `truncate_history()` in loom-context | Must still work. Compaction runs before truncation in the agent loop flow. |
| PrefixCache (feature 001) | After compaction, the prefix cache must be invalidated. Verify that `prefix_cache_reset()` or `reset_prefix()` is called. |
| `sanitize_message_sequence()` | After compaction, some tool-result messages may be orphaned. Verify that the existing sanitizer handles this. |
| Session save/load | Compacted history must not be written to the database. Full history must survive session save/load. |
| Token budget check | The existing budget check at `agent_loop.rs:627` must still fire. Compaction should reduce the chance of hitting the budget wall, not remove the safety check. |

#### 4.6.4 Acceptance Criteria

1. Session with 30+ tool-heavy iterations: compaction fires when tokens exceed 80% of budget.
2. After compaction, `tokens_after` is measurably lower than `tokens_before`.
3. `CompactionEvent` is emitted with correct token savings data.
4. Critical signals preserved: file paths from early turns remain in context. Error messages survive compaction.
5. Heuristic compaction alone works (turn off LLM summarization, verify tokens are still reduced).
6. LLM summarization produces valid JSON output that captures goals, decisions, errors, files, and state.
7. Mid-turn compaction runs heuristic-only and does not call the LLM.
8. Feature flag OFF: existing behavior unchanged, no compaction logic executed.
9. PrefixCache is correctly reset after compaction.
10. Session save/load preserves full history (not the compacted version).

---

## 5. Review Report Template

Every review report must follow this format:

```markdown
# Neutral Review Report

**Feature**: {001-006} — {Feature Name}
**Review Type**: {Pre-Implementation | Mid-Phase | Post-Phase | Cross-Feature}
**Date**: {YYYY-MM-DD}
**Reviewer**: {Name}
**Decision**: {Approve | Approve with Amendments | Reject | Suggest Alternative}

---

## 1. Summary

{Brief summary of what was reviewed and the overall assessment. 2-3 sentences.}

## 2. Architecture Compliance

### 2.1 Invariants Checked

| Invariant | Status | Evidence |
|-----------|--------|----------|
| B-1 Types in loom-types | {PASS/FAIL/N/A} | {File/line reference or reason N/A} |
| B-2 JSON-RPC 2.0 | {PASS/FAIL/N/A} | |
| B-3 Dispatch chain | {PASS/FAIL/N/A} | |
| B-4 Crate boundaries | {PASS/FAIL/N/A} | |
| B-5 CloudClient trait | {PASS/FAIL/EXCEPTION} | |
| B-6 EventBus | {PASS/FAIL/N/A} | |
| B-7 SQLite persistence | {PASS/FAIL/EXCEPTION} | |
| B-8 Explicit migration | {PASS/FAIL/N/A} | |
| F-1 Zustand slices | {PASS/FAIL/N/A} | |
| F-2 contextBridge | {PASS/FAIL/N/A} | |
| F-3 JSON-RPC frontend | {PASS/FAIL/N/A} | |
| F-4 StreamBufferManager | {PASS/FAIL/N/A} | |
| F-5 No React Router | {PASS/FAIL/N/A} | |
| F-6 Tailwind + CSS vars | {PASS/FAIL/N/A} | |

### 2.2 Loom-Rootedness Checklist

{List each L-xxx checklist item with status and evidence. Use the checklist items from Section 4 for the specific feature.}

## 3. Anti-Pattern Scan

| Anti-Pattern | Found? | File/Location | Severity |
|-------------|--------|---------------|----------|
| React Context for runtime state | {YES/NO} | | |
| JSONL event sourcing | {YES/NO} | | |
| Bundled runtime | {YES/NO} | | |
| Implicit migration | {YES/NO} | | |
| Hardcoded Chinese strings | {YES/NO} | | |
| Over-engineered MCP | {YES/NO} | | |
| Feature-specific anti-patterns | {YES/NO} | | |

## 4. Integration Verification

| System | Test Method | Result |
|--------|-----------|--------|
| {List each integration point from Section 4.x.3} | {How verified} | {PASS/FAIL} |

## 5. Cross-Feature Impact (Post-Phase and Cross-Feature reviews only)

| Concern | Status |
|---------|--------|
| Store slice count | Current: {N}. After feature: {N+delta}. Max allowed: 21. |
| IPC method count | Current: {N}. After feature: {N+delta}. |
| Dispatch handler count | Current: 12. After feature: {N}. |
| New npm dependencies | {List with sizes} |
| New Cargo dependencies | {List} |
| Compile time delta | {Before/After in seconds} |
| Bundle size delta | {Before/After in MB} |

## 6. Findings

### 6.1 Blocking Issues (must fix before proceeding)
- {Issue 1} — {File} — {Required fix}

### 6.2 Amendments (should fix within 48 hours)
- {Amendment 1} — {File} — {Suggested fix}

### 6.3 Suggestions (optional, at implementer's discretion)
- {Suggestion 1} — {Rationale}

## 7. Decision

{Approve | Approve with Amendments | Reject | Suggest Alternative}

{If Amendments: list the specific items that must be addressed before the next review.}
{If Reject: explain the fundamental architectural conflict and what redesign is needed.}
{If Suggest Alternative: describe the Loom-native alternative and why it's better.}

## 8. Sign-off

Reviewer: {Name}
Date: {YYYY-MM-DD}
Next Review: {Date or N/A}
```

---

## 6. Review Schedule

### 6.1 Feature Implementation Timeline (Estimated)

```
Week 1:  001 Pre-Impl ─► 001 Mid ─► 001 Post
Week 1:  002 Pre-Impl
Week 2:  002 Mid ─► 002 Post
Week 2:  003 Pre-Impl
Week 3:  003 Mid (backend)
Week 3:  006 Pre-Impl
Week 4:  003 Mid (frontend)
Week 4:  006 Mid ─► 006 Post
Week 5:  003 Post
Week 5:  004 Pre-Impl
Week 6:  004 Mid (backend)
Week 6:  005 Pre-Impl
Week 7:  004 Mid (frontend)
Week 7:  005 Mid (week 1 of 3)
Week 8:  004 Post
Week 8:  005 Mid (week 2 of 3)
Week 9:  005 Mid (week 3 of 3)
Week 9:  005 Post
Week 10: Cross-Feature Review (all 6 features)
```

### 6.2 Review Cadence Rules

1. **Pre-Implementation reviews** happen the day before implementation starts. The design doc must be finalized 24 hours before.
2. **Mid-Phase reviews** happen when the implementer declares "50% done" with concrete evidence (passing tests for completed steps, failing stubs for remaining steps).
3. **Post-Phase reviews** happen within 48 hours of the implementer declaring the phase complete.
4. **Cross-Feature reviews** happen after every 2 features reach Post-Phase, plus a final comprehensive review after all 6.

### 6.3 Review Time Budget

Total review time budget: **~20 hours** across all features.

| Review Type | Per Feature | Total (6 features) |
|------------|------------|-------------------|
| Pre-Implementation | 45 min | 4.5 hours |
| Mid-Phase | 60 min | 6 hours |
| Post-Phase | 90 min | 9 hours |
| Cross-Feature (3 rounds) | 120 min each | 6 hours |
| **Total** | | **~25.5 hours** |

---

## 7. Escalation Rules

### 7.1 When to Escalate

Escalate to the project lead when:

| Scenario | Action |
|----------|--------|
| **Reject decision issued** | Project lead must approve the rejection and schedule a redesign meeting within 3 business days. |
| **Amendments not addressed within 48 hours** | Project lead decides whether to grant an extension or escalate the amendment to blocking. |
| **Same anti-pattern found in 2+ features** | Project lead must conduct a root-cause analysis. The anti-pattern is added to Section 2.4 with stronger language. |
| **Disagreement between implementer and reviewer** | Both parties document their positions in 1 paragraph each. Project lead makes the final call within 24 hours. |
| **Cross-feature conflict discovered** | Both feature teams meet with the reviewer. Resolution plan documented within 2 business days. |

### 7.2 Dispute Resolution

1. **Implementer disagrees with a FAIL**: The implementer provides a written justification (1 paragraph + code reference). The reviewer re-evaluates. If still disagreeing, escalate.
2. **Reviewer disagrees with an EXCEPTION claim**: The implementer must provide evidence that the exception is documented in the design and that no Loom-native alternative exists. The reviewer can escalate to the project lead.
3. **Project lead overrides a review decision**: The override is documented in the review report with rationale. The override does not set precedent for future reviews.

### 7.3 Repeated Violations

If the same invariant violation appears across multiple features:

1. **First occurrence**: Flagged in review report as a finding. Implementer fixes.
2. **Second occurrence**: Flagged as a process issue. The invariant is re-emphasized in team documentation.
3. **Third occurrence**: Architectural review of the invariant itself — is the invariant too restrictive? If not, enforcement is escalated (review required before code commit for that invariant).

---

## 8. Cross-Feature Interaction Matrix

This matrix identifies potential conflicts between features. Reviewers must check these interactions during Cross-Feature reviews.

### 8.1 Immediate Interactions

| Feature Pair | Interaction | Risk |
|-------------|------------|------|
| 001 + 006 | Both touch `PrefixCache`. 001 upgrades it to SHA256-based. 006 adds `reset_prefix()`. Must compose. | **High** — if 006 implements before 001, the reset method must work with both old (DefaultHasher) and new (PrefixDigest) paths. |
| 001 + 005 | Write mode generates LLM calls for inline AI. These calls must carry `PrefixDigest` (from 001). | **Medium** — Write mode's chat.send calls are regular agent turns and should automatically get prefix digest from the agent loop. Verify. |
| 003 + 004 | Both add new Zustand slices. Both add new dispatch handlers. Order in `stores/index.ts` and `dispatch/mod.rs` must not cause conflicts. | **Low** — independent systems. |
| 004 + 005 | Both use CodeMirror 6. 004 uses `@codemirror/autocomplete` for FIM ghost text. 005 uses CodeMirror for markdown editing with live preview. They manage separate `EditorView` instances. | **Medium** — verify that CodeMirror extensions from 004 don't leak into 005's editor and vice versa. Separate component instances. |
| 005 + 003 | Write mode and Plan mode both use right-side panels. Layout must handle both being open simultaneously. | **Medium** — the right panel space is shared. Tab or stack design must be resolved. Design docs should agree. |

### 8.2 Aggregate Impact

| Metric | Before Features | After 6 Features | Limit |
|--------|----------------|-----------------|-------|
| Zustand slices | 17 | 22 (max) | 25 |
| contextBridge methods | 30 | 36 (estimated) | 45 |
| Dispatch sub-handlers | 12 | 15 (plan, goal, vfs, completion) | 20 |
| Backend crates | 15 | 15 (no new crates expected) | 18 |
| npm dependencies | current | +1 (`@codemirror/autocomplete`) | +5 |
| Cargo dependencies | current | +1 (`hex` for 001) | +3 |

### 8.3 Shared Files (Modified by Multiple Features)

| File | Features That Modify It | Risk |
|------|------------------------|------|
| `stores/index.ts` | 002, 003, 004, 005 | **High** — merge conflicts on AppStore type union. |
| `dispatch/mod.rs` | 003, 004, 005, 006 | **High** — merge conflicts on handler registration order. |
| `preload/index.ts` | 002, 005 | **Medium** — merge conflicts on LoomApi interface. |
| `main/ipc/index.ts` | 002, 005 | **Medium** — merge conflicts on registerIpcHandlers(). |
| `loom-types/src/lib.rs` | 001, 003, 006 | **Medium** — merge conflicts on re-exports. |
| `loom-core/src/agent_loop.rs` | 001, 006 | **High** — both add logic to the agent loop. |
| `loom-core/src/orchestrator.rs` | 003, 006 | **High** — both add orchestration steps. |
| `loom-inference/src/cache.rs` | 001, 006 | **High** — 001 rewrites PrefixCache; 006 adds reset_prefix(). |
| `InputArea.tsx` | 002, 004 | **Medium** — both modify the input area. |
| `AppShell.tsx` | 002, 003, 005 | **High** — layout changes from multiple features. |

### 8.4 Merge Order Recommendation

To minimize conflicts, implement features touching the same files in this order:

1. **001** (Prompt-Cache) — backend-only, touches agent loop and cache
2. **006** (Compaction) — builds on 001's PrefixCache changes
3. **003** (Plan/SDD/Todo) — full-stack, adds dispatch handlers
4. **002** (Inline Selection) — frontend, modifies InputArea
5. **004** (FIM) — modifies InputArea, adds dispatch handler
6. **005** (Write Mode) — largest surface area, should go last to absorb all other changes

---

*Document version: 1.0 — Last updated: 2026-06-08*

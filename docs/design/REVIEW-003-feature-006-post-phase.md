# Neutral Review Report — Post-Phase

**Feature**: 006 — Session Compaction
**Review Type**: Post-Phase
**Date**: 2026-06-08
**Reviewer**: Neutral Reviewer (Claude Opus)
**Decision**: APPROVE WITH AMENDMENTS (7 amendments required)

---

## 1. Summary

Feature 006's implementation is substantially complete for its heuristic tier. All four heuristic strategies (truncation, base64 elision, loop collapse, signal preservation) are implemented and tested. The feature is gated behind `CompactionConfig.enabled` (default `false`). Crate boundaries and type placement respect all architectural invariants. However, three critical gaps exist: (1) the `EngineEvent::CompactionPerformed` variant is defined but never emitted, (2) after-compaction prefix cache invalidation uses `reset_turn()` instead of the required `reset_prefix()`, and (3) compacted history overwrites the in-memory session cache, violating the design's commitment that DB preserves full history. These items plus four minor gaps require amendment before final merge.

---

## 2. Architecture Compliance

### 2.1 Invariants Checked

| Invariant | Status | Evidence |
|-----------|--------|----------|
| B-1 Types in loom-types | PASS | `CompactionConfig` at `loom-types/src/config/compaction.rs`. Registered in `config/mod.rs:5`, re-exported in `lib.rs:33`. |
| B-2 JSON-RPC 2.0 | N/A | No new JSON-RPC methods. Backend-internal feature. |
| B-3 Dispatch chain | N/A | No new dispatch handlers. |
| B-4 Crate boundaries | PASS | `CompactionResult` + `CompactionStrategy` in `loom-context/src/compaction.rs:13-35`. `CompactionConfig` in loom-types. Logic in loom-context, wiring in loom-core. |
| B-5 CloudClient trait | PASS (with note) | LLM summarization uses reserved CloudClient path; actual LLM call deferred. Heuristic compaction requires no LLM call. |
| B-6 EventBus | **FAIL** | `EngineEvent::CompactionPerformed` defined at `loom-types/src/event.rs:100-113` but **never emitted**. No `engine_events` broadcast channel exists in the orchestrator. See Amendment 1. |
| B-7 SQLite persistence | **FAIL** | Orchestrator saves compacted history to `session_histories` cache (lines 4172-4173, 4960-4961), overwriting full history. Design states "Compacted history is for the LLM call only." See Amendment 3. |
| B-8 Explicit migration | PASS | Feature flag via `compaction_config.enabled: false` (default). All compaction logic gated. |

### 2.2 Loom-Rootedness Checklist

| ID | Item | Status | Evidence |
|----|------|--------|----------|
| L-063 | CompactionConfig in loom-types/src/config/compaction.rs | PASS | File exists with proper consumers doc comment. |
| L-064 | Registered in config/mod.rs + lib.rs | PASS | `pub mod compaction;` at config/mod.rs:5. `pub use config::compaction::*;` at lib.rs:33. |
| L-065 | CompactionResult in loom-context/src/compaction.rs | PASS | Lines 13-35. Not in loom-types. |
| L-066 | compact() implemented on ContextAssembler | PASS (backward-compatible) | Legacy `compact()` at lib.rs:178-186 preserved; new `compact_with_config()` at lib.rs:189-195 returns `CompactionResult`. |
| L-067 | Heuristic logic in loom-context/src/compaction.rs | PASS | All 4 strategies isolated in compaction.rs. |
| L-068 | CompactionEvent in EngineEvent (no AgentEvent duplicate) | **PARTIAL** | `EngineEvent::CompactionPerformed` defined but never emitted. No AgentEvent duplicate exists (addressing A1). |
| L-069 | Orchestrator compaction step placement | PASS | Before agent loop call: lines 4150-4197 (non-streaming), lines 4938-4985 (streaming). |
| L-070 | Mid-turn compaction is heuristic-only | PASS | `None` passed as llm_client parameter at agent_loop.rs:706 and 1512. |
| L-071 | CompactionConfig in AgentLoopConfig with Default impl | PASS | Field at agent_loop.rs:122. Default at line 188: `CompactionConfig::default()`. |
| L-072 | reset_prefix() forces next check to be a miss | **FAIL** | `reset_prefix()` correctly clears both `last_digest` and `legacy_hash` (cache.rs:169-172). But orchestrator calls `client.prefix_cache_reset()` (lines 4178, 4966) which invokes `reset_turn()` **not** `reset_prefix()`. See Amendment 2. |
| L-073 | Feature flag gate | PASS | Gated at orchestrator.rs:4151, 4939; compaction.rs:50; agent_loop.rs:698, 1504. |
| L-074 | Auxiliary client for LLM summarization | N/A | LLM summarization deferred to future phase. Architecture prepared but not wired. |
| L-075 | temperature=0.0, reasoningEffort=off | N/A | LLM summarization not yet implemented. |

---

## 3. Amendment Verification (from Pre-Implementation Review)

### 3.1 A1: Dual event emission consolidated?
**Status**: PARTIALLY ADDRESSED

- The `AgentEvent` enum in `event_bus.rs` has NO compaction variant -- the duplicate was correctly removed.
- `EngineEvent::CompactionPerformed` is defined at `event.rs:100-113` with all required fields.
- **BUT**: The event is never emitted. The orchestrator has no `engine_events` broadcast channel, no `broadcast` or `send` call for EngineEvent. Compaction results are only logged via `tracing::info!` (orchestrator.rs:4163-4188, 4951-4975).

**Required fix**: Either (a) add an `engine_events` broadcast channel to the orchestrator and emit `EngineEvent::CompactionPerformed` after compaction, or (b) if the broadcast infrastructure is a future task, add a `// TODO: emit via engine_events when broadcast channel is added` comment with the exact event construction code.

### 3.2 A2: `enabled: bool` field present in CompactionConfig with default false?
**Status**: FULLY ADDRESSED

`loom-types/src/config/compaction.rs:11`: `pub enabled: bool,` with default `false` at line 29.

### 3.3 A3: `reset_prefix()` works with both PrefixCache paths?
**Status**: FULLY ADDRESSED

`loom-inference/src/cache.rs:169-172`:
```rust
pub fn reset_prefix(&self) {
    *self.last_digest.lock().unwrap() = None;
    *self.legacy_hash.lock().unwrap() = None;
}
```
Integration test at line 429: `test_reset_prefix_forces_miss()`.

### 3.4 A4: `compact()` callers audited before API change? Migration plan documented?
**Status**: FULLY ADDRESSED

Backward-compatible approach: legacy `compact()` preserved at lib.rs:178-186 (returns `Vec<Message>`). New `compact_with_config()` at lib.rs:189-195 (returns `CompactionResult`). Actual callers (orchestrator, agent_loop) use the free function `compact_history` directly, not the ContextAssembler methods.

### 3.5 A5: `collapse_repetitive_loops()` implemented (not todo!())?
**Status**: FULLY ADDRESSED

Fully implemented at `compaction.rs:244-325`. Uses window-based pair detection with tool name + arguments identity comparison. Tests at lines 451-477.

---

## 4. Anti-Pattern Scan

| Anti-Pattern | Found? | File/Location | Severity |
|-------------|--------|---------------|----------|
| JSONL session compaction | No | In-memory only. | N/A |
| Implicit compaction (no user visibility) | **YES** | EngineEvent never emitted; no frontend notification. | High — user cannot see compaction occurred. |
| Compaction modifying database | **PARTIAL** | Session cache overwritten (orchestrator.rs:4172-4173, 4960-4961). DB itself not modified. | Medium — cache contamination. |
| LLM call per mid-turn iteration | No | Mid-turn passes `None` as llm_client. | N/A |
| Hardcoded Chinese strings | No | All strings in English. | N/A |

---

## 5. Integration Verification

| System | Test Method | Result |
|--------|-----------|--------|
| Existing SummaryEngine | Static analysis -- compaction is additive, runs before agent loop, does not interfere with summary generation in orchestrator | PASS |
| truncate_history() | Compaction runs before the agent loop, history is compacted before ContextAssembler::build() assembles context | PASS |
| PrefixCache (Feature 001) | `reset_prefix()` is implemented but NOT called after compaction; `reset_turn()` is called instead | **FAIL** |
| sanitize_message_sequence() | Called at agent_loop.rs:519 and 1254 -- after compaction but before each LLM request within the agent loop | PASS |
| Session save/load | Compacted history saved to in-memory `session_histories` cache. DB write via `save_turn` preserves full `tool_messages` separately. On reload, `load_history()` reads from DB, so long-term persistence is intact. But inter-turn cache contamination exists. | **PARTIAL** |
| Token budget check | Preserved at agent_loop.rs:647 and 1453. Compaction runs BEFORE this check, reducing chance of hitting the wall | PASS |

---

## 6. Implementation Completeness

### 6.1 Heuristic Compaction Strategies

| Strategy | Implemented? | File:Line | Notes |
|----------|-------------|-----------|-------|
| Tool-output truncation | YES | compaction.rs:158-170 (Text), 190-201 (ToolResult) | keep_head=500, keep_tail=200 hardcoded |
| Base64 payload elision | YES | compaction.rs:149-155 (Text), 172-178 (Image), 179-187 (ToolResult) | Replaced with `[base64 image, N bytes]` |
| Repetitive loop collapse | YES | compaction.rs:244-325 | Threshold=3, consecutive identical (tool_call+tool_result) pairs |
| Signal preservation (errors) | YES | compaction.rs:211-218 | Checks for "error:", "error]", "failed", "panic" |
| Signal preservation (file paths) | **NO** | — | Design Section 2.1.4 says file paths should prevent truncation, but `has_signal_markers` only checks error patterns. |
| Signal preservation (file_read) | **NO** | — | Design says results from `file_read` should never be truncated. Not implemented. |

### 6.2 Tests

| Test | Implemented? | Location | Notes |
|------|-------------|----------|-------|
| test_elide_base64 | YES | compaction.rs:415 | Verifies base64 content is removed |
| test_truncation_respects_signals | YES | compaction.rs:424 | Error text prevents truncation |
| test_truncation_on_long_output | YES | compaction.rs:435 | Long text without signals gets truncated |
| test_collapse_repetitive_loops | YES | compaction.rs:451 | 3+ identical pairs collapsed |
| test_no_collapse_below_threshold | YES | compaction.rs:467 | 2 pairs = no collapse |
| test_compact_disabled | YES | compaction.rs:480 | Disabled flag returns identity |
| test_count_tokens_returns_zero_for_empty | NO | — | Missing |
| test_compaction_result_savings_pct | NO | — | Missing |
| test_partition_preserves_recent | NO | — | Missing |
| test_reset_prefix_forces_miss | YES | cache.rs:429 | Validates both digest and legacy hash cleared |
| Integration: orchestrator compacts over threshold | NO | — | Missing |
| Integration: compaction event emitted | NO | — | Missing (event never emitted) |
| Integration: prefix cache invalidated after compaction | NO | — | Missing (and broken -- see Amendment 2) |

### 6.3 LLM Summarization

**Status**: Deferred. The architecture is prepared (`CompactionConfig.use_llm_summarization`, `compact_history`'s `llm_client` parameter, comment at compaction.rs:351-364) but the actual `llm_summarize()` function is not implemented. The design document's rollout plan (Section 10) acknowledges this as Phase C. This is acceptable for the current implementation phase.

---

## 7. Code Quality

### 7.1 Warnings / Dead Code / TODO Markers
- **No TODO markers** found in compaction.rs, agent_loop.rs, or orchestrator.rs compaction sections.
- `EngineEvent::CompactionPerformed` variant at event.rs:100-113 is defined but never constructed anywhere -- it is effectively dead code.

### 7.2 Token Counting Accuracy
- `compaction.rs:count_tokens()` (line 367) uses tiktoken `CoreBPE::encode_with_special_tokens()` -- **accurate**.
- `orchestrator.rs:4153-4154` and `4941-4942`: Uses `text_content().len() / 4` (char/4 rough estimate) -- **inaccurate** but acceptable as a fast pre-check before the accurate count inside `compact_history()`.
- `agent_loop.rs:699-701` and `1505-1507`: Same char/4 rough estimate -- **inaccurate** but consistent.

### 7.3 Error Handling
- Compaction failures are gracefully handled: orchestrator logs a warning and continues with original history (orchestrator.rs:4191-4193, 4979-4981).
- Agent loop mid-turn compaction failures: logged as warning, messages unchanged (agent_loop.rs:717-719, 1523-1525).

---

## 8. Cross-Feature Impact

| Concern | Status |
|---------|--------|
| PrefixCache interaction (Feature 001) | `reset_prefix()` clears both `last_digest` and `legacy_hash`. Method exists and is tested. But orchestrator doesn't call it after compaction (calls `reset_turn()` instead). |
| Shared file: agent_loop.rs | Compaction check inserted inside iteration loop, after token budget check. Does not conflict with 001's prefix digest computation (which runs before the loop). |
| Shared file: orchestrator.rs | Compaction step inserted before the agent loop call in both process_message paths. Does not conflict with 003's BuiltinCommands layer. |
| Cargo dependencies | No new dependencies. Uses existing `tiktoken-rs`, `chrono`, `serde`, `anyhow`. |

---

## 9. Findings

### 9.1 Blocking Issues (must fix before proceeding)
None. No single issue warrants blocking the entire feature. The six amendments below cover all required fixes.

### 9.2 Amendments (should fix within 48 hours)

**Amendment 1: Emit EngineEvent::CompactionPerformed after compaction**
- **File**: `F:\openloom\backend\crates\loom-core\src\orchestrator.rs`
- **Location**: Lines 4162-4189 (non-streaming) and lines 4949-4976 (streaming)
- **Issue**: `EngineEvent::CompactionPerformed` (event.rs:100-113) is defined but never emitted. The orchestrator only logs with `tracing::info!`. No `engine_events` broadcast channel exists in the orchestrator struct.
- **Required fix**: Add an `engine_events: tokio::sync::broadcast::Sender<EngineEvent>` field to the Orchestrator struct, initialize it in `Orchestrator::new()`, and call `self.engine_events.send(EngineEvent::CompactionPerformed { ... }).ok();` after successful compaction in both code paths. The event fields should be populated from `CompactionResult` and the elapsed duration.

**Amendment 2: Call reset_prefix() after compaction, not just reset_turn()**
- **File**: `F:\openloom\backend\crates\loom-core\src\orchestrator.rs`
- **Location**: Lines 4178 and 4966
- **Issue**: `client.prefix_cache_reset()` calls `PrefixCache::reset_turn()` (engine.rs:561-563, openai.rs:636-638, anthropic.rs:570-572), which resets per-turn stats but does NOT clear `last_digest` or `legacy_hash`. After compaction, the message prefix has changed, so the stored hash is stale and the next `check()` may incorrectly report a cache hit.
- **Required fix**: After `client.prefix_cache_reset()`, also call `client.prefix_cache.reset_prefix()` if the CloudClient trait exposes it via a new method `fn reset_prefix_cache(&self) { self.prefix_cache.reset_prefix(); }`. Alternatively, add a new trait method `fn prefix_cache_reset_after_compaction(&self)` that calls both `reset_turn()` and `reset_prefix()`. The default stub in engine.rs:642 should remain a no-op.

**Amendment 3: Do not overwrite session_histories cache with compacted history**
- **File**: `F:\openloom\backend\crates\loom-core\src\orchestrator.rs`
- **Location**: Lines 4172-4173 and 4960-4961
- **Issue**: The orchestrator inserts compacted history into `session_histories` cache, replacing the full history. On subsequent inter-turn reads, the compacted version is used instead of the full history. The design (Section 6.1 goal point 6, B-7) explicitly states "Compacted history is for the LLM call only. Raw history preserved in DB."
- **Required fix**: Remove the `self.session_histories.write().await.insert(...)` lines. The local `history` variable already holds the compacted history for this turn. The session_histories cache should continue to hold the full (un-compacted) history.

**Amendment 4: Add file-path signal markers to has_signal_markers()**
- **File**: `F:\openloom\backend\crates\loom-context\src\compaction.rs`
- **Location**: Lines 211-218
- **Issue**: The design (Section 2.1.4) specifies three categories of signals that must prevent truncation: (1) error patterns, (2) `file_read` tool results, (3) results containing absolute file paths. The implementation only checks error patterns. Tool-results that are not from `file_read` but contain critical file paths may be truncated.
- **Required fix**: Add file-path detection to `has_signal_markers()`. The design references a regex: `(?mi)\b(error|Error|ERROR)\b` and `(?:[A-Za-z]:[/\\]|/)\S+\.\w{1,10}`. Add the file-path regex check. Additionally, the `apply_heuristic_compaction` function does not receive the tool name context to check for `file_read` specifically -- pass the message or role context to enable `file_read` tool name checking.

**Amendment 5: Add missing unit tests for compaction**
- **File**: `F:\openloom\backend\crates\loom-context\src\compaction.rs`
- **Location**: Test module (after line 487)
- **Issue**: Three tests from the design's Section 9.1 are missing: `test_count_tokens_returns_zero_for_empty`, `test_compaction_result_savings_pct`, `test_partition_preserves_recent`. These verify edge cases that are not covered by existing tests.
- **Required fix**: Add the three missing tests.

**Amendment 6: Ensure loop collapse correctly handles non-adjacent tool calls**
- **File**: `F:\openloom\backend\crates\loom-context\src\compaction.rs`
- **Location**: Lines 244-325 (`collapse_repetitive_loops`)
- **Issue**: The current implementation collects all (assistant+tool_call, tool+result) pairs into a `pairs` vector via `windows(2)`, then uses scattered index comparisons to detect repeats. This algorithm has a subtle bug: the `repeat_count` detection loop (lines 277-295) iterates through all pairs and checks `pair.0 >= i + repeat_count * 2`, which can incorrectly accumulate matches from non-consecutive positions. The design specifies "Scan consecutive pairs" -- the loop collapse should only detect **consecutive** identical pairs, not identical pairs scattered across the history.
- **Required fix**: Simplify the algorithm to scan linearly through messages, checking consecutive windows of size 2 without pre-collecting all pairs. Each time a tool_call+tool_result pair is found, check if the NEXT adjacent pair has the same tool name and args. This ensures only consecutive repeats are collapsed.

**Amendment 7: Define and document the `engine_events` channel lifecycle**
- **File**: `F:\openloom\backend\crates\loom-core\src\orchestrator.rs`
- **Location**: Orchestrator struct definition (around line 220) and constructor (around line 786)
- **Issue**: No `engine_events` broadcast channel exists. The existing EventBus only broadcasts `AgentEvent` variants. `EngineEvent` is a separate enum intended for infrastructure-level events. Without a broadcast channel, `EngineEvent::CompactionPerformed` is dead code.
- **Required fix**: Add `engine_events: tokio::sync::broadcast::Sender<EngineEvent>` to Orchestrator struct. Initialize with `tokio::sync::broadcast::channel(256)` in `new()`. Add a getter `pub fn engine_events(&self) -> tokio::sync::broadcast::Sender<EngineEvent>`. This channel is separate from the existing EventBus (which broadcasts AgentEvent) because EngineEvent covers infrastructure events (compaction, heartbeat, engine errors) while AgentEvent covers agent lifecycle events.

---

## 10. Decision

**APPROVE WITH AMENDMENTS**

The implementation correctly addresses 4 of 5 pre-implementation review amendments (A2-A5 fully; A1 partially). Architecture invariants B-1, B-4, and B-8 are respected. Crate boundaries and type placement are correct. All four heuristic strategies are implemented with tests.

However, 7 amendments must be addressed before this feature can proceed to cross-feature review:

1. **A1**: Emit `EngineEvent::CompactionPerformed` after compaction (event defined but never sent)
2. **A2**: Call `reset_prefix()` (not just `reset_turn()`) after compaction to invalidate prefix cache
3. **A3**: Do not overwrite `session_histories` cache with compacted history
4. **A4**: Add file-path signal markers to `has_signal_markers()`
5. **A5**: Add missing unit tests (empty history, savings_pct, partition preservation)
6. **A6**: Fix loop collapse algorithm to only detect consecutive (not scattered) repeats
7. **A7**: Add `engine_events` broadcast channel to Orchestrator

Amendments must be addressed within 48 hours per the review framework (Section 7.1). The reviewer will re-check only the amended items after the implementer confirms amendments are resolved.

---

## 11. Sign-off

**Reviewer**: Claude Opus (Neutral Reviewer)
**Date**: 2026-06-08
**Next Review**: Amendment re-check (after 7 amendments addressed), then Cross-Feature Review with Feature 001

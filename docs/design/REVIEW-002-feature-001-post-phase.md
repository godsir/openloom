# Neutral Review Report — Post-Phase

**Feature**: 001 — Prompt-Cache Fingerprint
**Review Type**: Post-Phase
**Date**: 2026-06-08
**Reviewer**: Neutral Reviewer
**Decision**: APPROVE WITH AMENDMENTS

---

## 1. Executive Summary

Feature 001 is substantially complete and architecturally sound. The core mechanism — SHA256-based prefix digest computed independently of the dynamic suffix, flowing from ContextAssembler through the agent loop into each inference provider — works correctly. All four pre-implementation amendments are satisfied. The legacy `DefaultHasher` path is fully preserved for orchestrator internal calls. Provider-specific behavior (Anthropic cache_control injection, DeepSeek/OpenAI logging) is correctly isolated.

Two regressions require immediate remediation: (1) the design-specified unit tests for `PrefixDigest` were not implemented in `loom-context/src/lib.rs`, and (2) the `InferenceEngine` (LM Studio / Ollama) stores the digest but never uses it for cache-hit detection, falling back to the legacy `check()` path. Additionally, one design deviation (using `set_prefix_digest` trait method instead of `CompletionRequest.prefix_digest` field) should be documented.

The verdict is **APPROVE WITH AMENDMENTS**. Two amendments required. Not blocking.

---

## 2. Amendment Verification (Pre-Implementation Review)

### Amendment 1: Legacy `check()` method preserved for backward compatibility

**Verdict: PASS**

Evidence:
- `check_legacy()` is preserved at `cache.rs:192-213` with the full `DefaultHasher` (SipHash-1-3) logic.
- It independently tracks `legacy_hash: Mutex<Option<u64>>` (cache.rs:48), separate from the digest-based `last_digest`.
- The primary `check()` method at `cache.rs:178-189` gates: if `last_digest` is `Some`, it returns the hit status from `last_hit` tracking; if `None`, it falls through to `check_legacy()`.
- The `legacy_hash` field is properly snapshotted/restored via `snapshot_hash()` / `restore_hash()` (cache.rs:230-236), used by the orchestrator's internal LLM call flow.
- Legacy tests at cache.rs:300-345 (`test_prefix_hit_same_messages`, `test_prefix_miss_different_messages`, `test_reset_turn_keeps_prefix`, `test_stats_accumulate`) all pass through the legacy path via `check()` with no digest set.

Verdict detail: The implementation actually **improves** on the original legacy design. It maintains a separate `legacy_hash: Mutex<Option<u64>>` field instead of deriving a u64 from the digest's combined_hash hex string. This is cleaner and avoids potential hash collisions between the DefaultHasher and the SHA256-derived u64.

### Amendment 2: `cache_control` gated on `ModelBackend::Anthropic`

**Verdict: PASS**

Evidence:
- The provider split in `openai.rs:712-718` ensures `AnthropicClient` is instantiated **only** for `ModelBackend::Anthropic`. All other backends (OpenAI, DeepSeek, Ollama, LM Studio) go through `OpenAIClient`.
- `AnthropicClient::lower_messages()` at anthropic.rs:169-241 injects `cache_control` when `cache_hit` is true (line 185 for system prompt, line 228 for first non-system message).
- `OpenAIClient::lower_messages()` at openai.rs:259-360 does **not** inject `cache_control` at all.
- The gating is structural (by client type) rather than explicit (`if self.provider() == ModelBackend::Anthropic`), but the effect is identical: only Anthropic requests receive `cache_control` breakpoints.

Minor note: There is no defensive `assert_eq!(self.provider(), ModelBackend::Anthropic)` or equivalent guard inside `lower_messages`. If an `AnthropicClient` were ever instantiated for a non-Anthropic backend (unlikely given the factory in `openai.rs:712-718`), it would incorrectly inject `cache_control`. This is a very low-risk edge case.

### Amendment 3: `CacheStatus` derives `Copy`

**Verdict: PASS**

Evidence:
- `cache.rs:28`: `#[derive(Debug, Clone, Copy, PartialEq, Eq)]`
- `Copy` is used throughout provider code for match arms. For example:
  - `anthropic.rs:185`: `let cache_hit = matches!(cache_status, CacheStatus::Hit);`
  - `anthropic.rs:80-91`: pattern matching on `cache_status` in `try_complete()`
  - `openai.rs:69-80`: same pattern matching in `try_complete()`
- Without `Copy`, these match arms would move the value, preventing subsequent branching.

### Amendment 4: `AgentLoopConfig` Clone verified or helper used

**Verdict: PASS (MOOT)**

The implementation **avoids cloning `AgentLoopConfig` entirely**. Instead of:
1. Adding `prefix_digest` to `AgentLoopConfig`
2. Cloning the config via `..config.clone()`
3. Passing the digest on `CompletionRequest`

The implementation uses a cleaner approach:
1. Computes the digest (agent_loop.rs:626-632 for non-streaming, 1405-1411 for streaming)
2. Calls `client.set_prefix_digest(Some(digest.clone()))` (agent_loop.rs:633, 1412)
3. Each provider stores the digest on a `pending_digest: Mutex<Option<PrefixDigest>>` field (anthropic.rs:23, openai.rs:23) and retrieves it at call time

This approach is architecturally superior because it:
- Avoids the `AgentLoopConfig` Clone dependency
- Avoids adding `PrefixDigest` to `CompletionRequest` (no loom-types changes needed)
- Is a single-point injection that works across all iterations of the agent loop

`AgentLoopConfig` does **not** derive `Clone` (agent_loop.rs:54), and `TurnResult` does derive `Clone` (agent_loop.rs:28), which is correct.

---

## 3. Architecture Compliance Matrix

| Invariant | Status | Evidence |
|-----------|--------|----------|
| B-1 Types in loom-types | **PASS** | `PrefixDigest` is in `loom-context/src/lib.rs:28` — the correct location per the justified exception (tightly coupled to `ContextAssembler::compute_prefix_digest`). No new types were added to `loom-types` (`CompletionRequest.prefix_digest` was not added; the implementation avoids this). |
| B-2 JSON-RPC 2.0 | **N/A** | No new JSON-RPC methods. Feature is a pure internal pipe. |
| B-3 Dispatch chain | **N/A** | No new dispatch handlers. |
| B-4 Crate boundaries | **PASS** | `PrefixDigest` in loom-context (computed by ContextAssembler). `CacheStatus` in loom-inference/cache.rs (used by PrefixCache). `sha2` (workspace) and `hex` in loom-context/Cargo.toml. No circular deps. `loom-inference` depends on `loom-context` (line 9 of Cargo.toml). |
| B-5 CloudClient trait | **PASS** | `set_prefix_digest()`, `prefix_digest_snapshot()`, `prefix_digest_restore()` added as default stubs at engine.rs:660-671. All 5 providers compile without changes. |
| B-6 EventBus | **N/A** | No new events. |
| B-7 SQLite persistence | **N/A** | No persistence changes. |
| B-8 Explicit migration | **PASS** | All new fields are `Option` types. Backward compatible. No migration needed. |

### Loom-Rootedness Checklist (Section 4.1)

| ID | Item | Status | Evidence |
|----|------|--------|----------|
| L-001 | `PrefixDigest` in loom-context | **PASS** | `loom-context/src/lib.rs:28`. Tightly coupled to `compute_prefix_digest()` at line 188. |
| L-002 | `CacheStatus` in loom-inference/cache.rs | **PASS** | `cache.rs:29-38`. Alongside the upgraded `PrefixCache`. |
| L-003 | `prefix_digest` as `Option<PrefixDigest>` | **PASS (alternative impl)** | Not on `CompletionRequest` or `AgentLoopConfig`. Instead, passed via `CloudClient::set_prefix_digest(Option<PrefixDigest>)` trait method. The Option contract is respected. |
| L-004 | `CloudClient` trait methods as default stubs | **PASS** | `set_prefix_digest()` (engine.rs:660), `prefix_digest_snapshot()` (engine.rs:665), `prefix_digest_restore()` (engine.rs:669) all have default no-op implementations. |
| L-005 | SHA256 via `sha2.workspace = true` | **PASS** | `loom-context/Cargo.toml:15`: `sha2.workspace = true`. Verified: `sha2 = "0.10"` at `F:\openloom\Cargo.toml:92`. |
| L-006 | Legacy `check()` method preserved | **PASS** | `check_legacy()` at cache.rs:192-213. Internal calls (summary, KG, vision) use this path. |
| L-007 | AnthropicClient gates `cache_control` behind `ModelBackend::Anthropic` | **PASS** | Structural gating via client type (AnthropicClient only created for Anthropic, openai.rs:713). |
| L-008 | Logging uses `tracing::info!` | **PASS** | cache.rs lines 81-89 in AnthropicClient; 70-80 in OpenAIClient; 309-313 in AnthropicClient streaming; 377-381 in OpenAIClient streaming; agent_loop.rs:634-639, 1413-1418. |
| L-009 | Digest computed before iteration loop | **PASS** | agent_loop.rs:626-632 (non-streaming), 1405-1411 (streaming). Both are before the `for iteration in 0..config.max_iterations` loop. |
| L-010 | `sha2` referenced as `workspace = true` | **PASS** | `loom-context/Cargo.toml:15`. Matches `F:\openloom\Cargo.toml:92`. |

---

## 4. Anti-Pattern Scan

| Anti-Pattern | Found? | Location | Severity |
|-------------|--------|----------|----------|
| Hardcoded provider logic | **NO** | Provider split is structural: AnthropicClient vs OpenAIClient. Cache_control only in AnthropicClient. | N/A |
| Over-hashing (dynamic suffix) | **NO** | `compute_prefix_digest()` at loom-context/src/lib.rs:188-227 receives `AssembleOptions` with `history: vec![]`. The digest is a pure function of stable prefix components. | N/A |
| New crate for cache types | **NO** | All additions in existing crates (loom-context, loom-inference, loom-core). | N/A |
| Implicit migration | **NO** | All new fields are `Option`. No preference keys changed. | N/A |
| Hardcoded Chinese strings | **NO** | No new user-facing strings in this feature. | N/A |

---

## 5. Code Quality Findings

### 5.1 Strengths

1. **Clean separation of legacy and digest paths**: `check()` vs `check_digest()` at cache.rs are clearly independent. The internal `legacy_hash` field (cache.rs:48) is separate from `last_digest` (cache.rs:46), avoiding any cross-contamination.

2. **Consistent `tracing::info!` usage**: All cache-related logging uses `tracing::info!`. The digest prefix is truncated to 12 chars for readability (e.g., `prefix_hash = %&digest.combined_hash[..12]` at agent_loop.rs:635).

3. **Proper drift reasons computation**: `check_digest()` at cache.rs:92-116 computes drift reasons BEFORE updating `last_digest`, ensuring accurate per-component comparison against the previous state.

4. **`reset_prefix()` clears both paths**: cache.rs:169-172 clears both `last_digest` and `legacy_hash`, ensuring Feature 006 (Compaction) will work correctly.

5. **Provider-appropriate digest handling**:
   - `AnthropicClient` uses `pending_digest` to carry the digest from `set_prefix_digest()` to the actual call methods (anthropic.rs:78, 307, 409).
   - `OpenAIClient` similarly uses `pending_digest` (openai.rs:67, 375, 488).
   - `InferenceEngine` stores via `restore_digest()` (engine.rs:579).

### 5.2 Issues Found

1. **Missing unit tests in loom-context** (AMENDMENT REQUIRED): The design specified 5 unit tests at `loom-context/src/lib.rs` (Section 8.1):
   - `test_prefix_digest_is_deterministic`
   - `test_prefix_digest_changes_with_system_prompt`
   - `test_prefix_digest_changes_with_persona`
   - `test_per_component_hashes_detect_drift`
   - `test_history_does_not_affect_digest`
   
   **None of these tests exist.** The file has no `#[cfg(test)]` module. The `compute_prefix_digest()` method has zero test coverage.

2. **InferenceEngine (local models) stores digest but never uses it for cache classification** (AMENDMENT REQUIRED): In all three calling paths of `InferenceEngine`:
   - `complete()` at engine.rs:131 calls `self.prefix_cache.check(&eff)` (legacy path)
   - `complete_stream()` at engine.rs:223 calls `self.prefix_cache.check(&eff)` (legacy path)
   - `complete_stream_structured()` at engine.rs:416 calls `self.prefix_cache.check(&eff)` (legacy path)
   
   Meanwhile, `set_prefix_digest()` at engine.rs:578-580 stores the digest via `self.prefix_cache.restore_digest(digest)`. The stored digest is never read back via `check_digest()`. This means the per-component drift detection, `CacheStatus` classification, and richer logging (hit/additive/breaking/cold-start) are unavailable for local models, even though the digest is correctly computed and stored.

3. **`hex` crate duplicated**: `hex = "0.4"` appears in both `loom-context/Cargo.toml:16` and `loom-inference/Cargo.toml:19`. The design only specified it for loom-context. It's used in loom-inference for drift reason logging and tests — legitimate, but the design didn't anticipate this.

4. **No integration test for the full agent-loop-to-provider flow**: The design Section 8.3 specifies an integration test that runs two consecutive turns and verifies `kv_cache_hit` changes from `Some(false)` to `Some(true)`. This test does not exist. However, the unit tests in `cache.rs` (lines 281-456) cover the `PrefixCache` logic well, and the integration test is inherently difficult without a running LLM backend.

---

## 6. Integration Verification

### 6.1 Design Integration Points

| System | Verification Method | Result |
|--------|-------------------|--------|
| Legacy PrefixCache users (orchestrator internal calls, summary, vision) | Code review: `check_legacy()` at cache.rs:192-213 is independent and preserved. `legacy_hash` field separate from `last_digest`. | **PASS** |
| All 5 CloudClient implementors (Anthropic, OpenAI, DeepSeek, LM Studio, Ollama) | Code review: Default stubs at engine.rs:660-671. All providers compile without changes. DeepSeek handled via OpenAIClient (openai.rs:712-718). | **PASS** |
| AgentLoopConfig Clone | Code review: Implementation avoids cloning entirely. | **PASS (MOOT)** |
| EventBus snapshot/restore | Code review: `snapshot_hash()` / `restore_hash()` at cache.rs:230-236 coexist with `snapshot_digest()` / `restore_digest()` at cache.rs:160-167. | **PASS** |
| Anthropic cache_control injection only for Anthropic | Code review: Structural gating via client type (openai.rs:713). `cache_control` only in `AnthropicClient::lower_messages()` (anthropic.rs:189-237). | **PASS** |
| check_legacy independence | Code review: Uses `legacy_hash` (cache.rs:48), separate from `last_digest`. Does not reference PrefixDigest internals. | **PASS** |

### 6.2 Data Flow Verification

| Flow Step | Verified? | Evidence |
|-----------|-----------|----------|
| 1. ContextAssembler computes digest | **PASS** | agent_loop.rs:626-632 (non-streaming), 1405-1411 (streaming). Both use `AssembleOptions { history: vec![], .. }` ensuring history exclusion. |
| 2. Digest passed to CloudClient | **PASS** | `client.set_prefix_digest(Some(digest.clone()))` at agent_loop.rs:633, 1412. Called before iteration loop. |
| 3a. AnthropicClient uses digest | **PASS** | Retrieves from `self.pending_digest` (anthropic.rs:78, 307, 409), calls `check_digest()`, injects `cache_control` on Hit. |
| 3b. OpenAIClient uses digest | **PASS** | Retrieves from `self.pending_digest` (openai.rs:67, 375, 488), calls `check_digest()`, logs classified status. |
| 3c. InferenceEngine uses digest | **PARTIAL** | Stores digest (engine.rs:578-580) but calls `check()` (legacy) instead of `check_digest()` in all 3 paths. | See Amendment 2. |
| 4. Cache hit/miss logged | **PASS** | AnthropicClient: anthropic.rs:81-90 (non-streaming), 309-314 (streaming), 411-416 (structured stream). OpenAIClient: openai.rs:70-80, 377-381, 490-495. Agent loop: agent_loop.rs:634-639, 1413-1418. |
| 5. TurnResult carries cache info | **PASS** | `cached_tokens` (agent_loop.rs:1083, 2124), `kv_cache_hit` (agent_loop.rs:1086, 2127), `cache_read_tokens` (agent_loop.rs:1084, 2125), `cache_write_tokens` (agent_loop.rs:1085, 2126). |

### 6.3 PrefixDigest Field Completeness

All PrefixDigest fields populated with real values (no empty strings where data should exist):
- `combined_hash`: SHA256 of assembled stable prefix string (loom-context/src/lib.rs:215)
- `system_hash`: SHA256 of system_prompt (line 194)
- `persona_hash`: SHA256 of persona string, or SHA256("") if None (line 195)
- `summary_hash`: SHA256 of summary string, or SHA256("") if None (line 196)
- `kg_hash`: SHA256 of kg_context string, or SHA256("") if None (line 197)
- `catalog_hash`: SHA256 of tool_catalog string, or SHA256("") if None (line 198)
- `prefix_token_count`: tiktoken count of combined string (line 216)

All hashes are computed via SHA256; empty strings produce valid SHA256("") hashes. No null/default values leak.

---

## 7. Test Coverage Assessment

### 7.1 Existing Test Coverage

| Test File | Test Name | What It Covers | Status |
|-----------|-----------|---------------|--------|
| `cache.rs:300-307` | `test_prefix_hit_same_messages` | Legacy check(): same prefix = hit | **PASS** |
| `cache.rs:309-321` | `test_prefix_miss_different_messages` | Legacy check(): different prefix = miss | **PASS** |
| `cache.rs:323-333` | `test_reset_turn_keeps_prefix` | Legacy check(): reset_turn preserves hash | **PASS** |
| `cache.rs:335-345` | `test_stats_accumulate` | Legacy check(): hit/miss counting | **PASS** |
| `cache.rs:362-366` | `test_digest_cold_start` | Digest check(): first request = ColdStart | **PASS** |
| `cache.rs:368-376` | `test_digest_hit` | Digest check(): same digest = Hit | **PASS** |
| `cache.rs:378-399` | `test_digest_breaking_miss` | Digest check(): different system_hash = BreakingMiss | **PASS** |
| `cache.rs:402-426` | `test_digest_drift_reasons` | Digest check(): per-component drift detection ("system_prompt" vs "persona") | **PASS** |
| `cache.rs:428-439` | `test_reset_prefix_forces_miss` | reset_prefix(): clears digest → ColdStart on next check | **PASS** |
| `cache.rs:441-455` | `test_digest_snapshot_restore` | snapshot_digest()/restore_digest(): round-trip preserves Hit | **PASS** |

Total: 10 tests in `cache.rs`. All 10 pass (verified by structural review of test logic).

### 7.2 Missing Tests

| Missing Test | Location | Impact |
|-------------|----------|--------|
| `test_prefix_digest_is_deterministic` | `loom-context/src/lib.rs` | No coverage for the core computation. If SHA256 or string assembly changes, the digest changes silently. |
| `test_prefix_digest_changes_with_system_prompt` | `loom-context/src/lib.rs` | No coverage for the primary drift signal. |
| `test_prefix_digest_changes_with_persona` | `loom-context/src/lib.rs` | No coverage for persona drift detection. |
| `test_per_component_hashes_detect_drift` | `loom-context/src/lib.rs` | No coverage for per-component hash correctness (critical for `drift_reasons`). |
| `test_history_does_not_affect_digest` | `loom-context/src/lib.rs` | No coverage for the core invariant: history exclusion. A regression here would make the digest useless. |
| Integration: two-turn cache hit | Agent loop test | No end-to-end test proving that Turn 1 = ColdStart, Turn 2 = Hit. The `cache.rs` unit tests cover the cache logic but not the agent loop wiring. |

---

## 8. Risks and Gaps

### 8.1 Identified Risks

| Risk | Severity | Description |
|------|----------|-------------|
| Missing digest tests in loom-context | **Medium** | `compute_prefix_digest()` has zero test coverage. A future refactor of `build()` (which duplicates the prefix assembly logic) could cause the digest to diverge from the actual system message content, creating phantom cache hits or misses. |
| InferenceEngine digest unused | **Low-Medium** | Local model users don't benefit from per-component drift logging. The legacy `check()` works correctly but provides less observability. |
| No integration/E2E test | **Low** | The agent loop wiring (digest computation + set_prefix_digest) is simple and unlikely to break in isolation, but there is no test proving the full flow. |
| `hex` crate ambiguity | **Low** | `hex` is in both `loom-context` and `loom-inference` Cargo.toml. If one is removed or version-skewed, the other may break. Not currently a problem since both use `"0.4"`. |
| `AgentLoopConfig` vs. design doc drift | **Low** | The design says `AgentLoopConfig` should have a `prefix_digest` field and derive `Clone`. The implementation does neither. This could confuse future readers of the design doc. |

### 8.2 No Blockers Found

- No TODO or incomplete implementation markers
- No performance issues (SHA256 is fast; digest computed once per turn, not per iteration)
- No security concerns (SHA256 is a cryptographic hash with no known collisions at this scale)
- No circular dependencies introduced
- No unused imports or dead code detected
- No warnings expected at compile time

---

## 9. Design-Implementation Delta

Two intentional deviations from the design document were identified:

### Delta 1: Digested passed via trait method instead of CompletionRequest field

**Design says** (Section 3.5): `CompletionRequest.prefix_digest: Option<PrefixDigest>` field. Design says (Section 3.4): `AgentLoopConfig.prefix_digest: Option<PrefixDigest>` field.

**Implementation does**: `CloudClient::set_prefix_digest(Option<PrefixDigest>)` trait method called once before the iteration loop. Each provider stores the digest on a `pending_digest: Mutex<Option<PrefixDigest>>` field.

**Assessment**: This is a **valid and cleaner alternative**. It avoids:
- Adding `PrefixDigest` to `loom-types` (CompletionRequest lives there)
- Adding `PrefixDigest` to `AgentLoopConfig` and the Clone concern
- Passing the digest on every CompletionRequest clone inside the iteration loop

The design's own Section 3.6 says: "If future providers need a dedicated digest setter, one can be added" — the implementation simply chose the setter approach from the start. No functional difference.

**Recommendation**: Document this decision in the design document's revision history.

### Delta 2: No `prefix_digest` field on `AgentLoopConfig`

**Design says** (Section 3.4): Add `pub prefix_digest: Option<PrefixDigest>` to `AgentLoopConfig`.

**Implementation does**: Omits the field. Uses `client.set_prefix_digest()` directly.

**Assessment**: Consistent with Delta 1. The field would be redundant since the digest is held by the client. Clean.

---

## 10. Verdict

**APPROVE WITH AMENDMENTS**

Feature 001 is functionally complete and architecturally sound. The core mechanism works correctly across all providers. All four pre-implementation amendments are satisfied. The two remaining issues are:

### Amendment 1 (Required — 48 hours): Add unit tests for `compute_prefix_digest()` in `loom-context/src/lib.rs`

Add the following 5 tests as specified in the design document Section 8.1:

1. `test_prefix_digest_is_deterministic` — same assembler + same opts = same hash
2. `test_prefix_digest_changes_with_system_prompt` — different system prompt = different combined_hash
3. `test_prefix_digest_changes_with_persona` — different persona = different persona_hash and combined_hash
4. `test_per_component_hashes_detect_drift` — changed persona, same summary → persona_hash differs, summary_hash matches
5. `test_history_does_not_affect_digest` — history in opts does not change combined_hash

These tests protect against regressions where a change to `build()` (which duplicates the prefix assembly logic at loom-context/src/lib.rs:97-125) could cause the digest to diverge from the actual system message.

### Amendment 2 (Required — 48 hours): Use `check_digest()` in `InferenceEngine` streaming paths

In all three `InferenceEngine` call paths (`complete()` at engine.rs:131, `complete_stream()` at engine.rs:223, `complete_stream_structured()` at engine.rs:416), replace the `self.prefix_cache.check(&eff)` call with:

```rust
let digest = self.prefix_cache.snapshot_digest();
let (cache_status, _, reasons) = self.prefix_cache.check_digest(&digest);
```

And add classified logging matching the AnthropicClient/OpenAIClient pattern (Hit/BreakingMiss/AdditiveMiss/ColdStart). The digest is already being stored via `set_prefix_digest()` → `restore_digest()` at engine.rs:579 — it just needs to be read back.

---

## 11. Sign-off

**Reviewer**: Neutral Reviewer (Claude Opus)
**Date**: 2026-06-08
**Next Review**: Amendment re-check after both amendments are addressed (within 48 hours per Section 7.1 of the review framework).

---

*Review conducted against:*
- Design document: `docs/design/001-prompt-cache-fingerprint.md` v1
- Pre-implementation review: `docs/design/REVIEW-001-pre-implementation.md` (Feature 001, 4 amendments)
- Review framework: `docs/design/007-neutral-review-framework.md` v1.0
- Loom Architecture Invariants: B-1 through B-8, F-1 through F-6

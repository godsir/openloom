# 001 — Prompt-Cache Fingerprinting

| Field     | Value                                                       |
|-----------|-------------------------------------------------------------|
| Author    | shenshuo                                                    |
| Status    | Draft                                                       |
| Created   | 2026-06-08                                                  |
| Scope     | Backend only                                                |
| Effort    | 1 day                                                       |
| Crates    | loom-context, loom-inference, loom-core, loom-types         |

---

## 1. Overview / Problem Statement

### 1.1 What we have today

The LLM request pipeline currently uses `PrefixCache` (in `loom-inference/src/cache.rs`)
to detect KV-cache reuse across requests.  It hashes the first 2 messages of the
assembled message array with Rust's `DefaultHasher` (not a cryptographic hash) and
compares to the previous value.

This works for simple "same prefix = full reuse" detection but has three serious
weaknesses:

1. **No separation of stable vs. dynamic**.  ContextAssembler already splits
   content into a stable prefix (system prompt + persona + summary + KG context +
   tool catalog) and a dynamic suffix (truncated conversation history), but the
   two are merged into one flat `Vec<Message>` before PrefixCache sees them.  If
   the first 2 messages happen to include history (because the system message was
   abnormally short, or because the splitting logic nudged a user message into the
   first slot), the hash changes even though the prefix is identical.

2. **Weak hasher**.  `DefaultHasher` uses SipHash-1-3, which is designed for
   hash-table DoS resistance, not content fingerprinting.  Hash collisions with
   security-relevant data (prompt content) are unlikely in practice but
   architecturally wrong.  The `sha2 = "0.10"` crate is already in the workspace
   `Cargo.toml` but unused by the inference layer.

3. **No cache-break classification**.  When the hash changes, the system has no
   way to say *why*.  Did the system prompt change?  Did the persona update?  Or
   was it just a new conversation that appended to the history suffix?  This
   matters because:
   - A persona update is a **breaking** change (full cache miss).
   - A history change is an **additive** change (prefix still reusable; only the
     suffix is new).
   - Knowing the difference lets us inject `cache_control` breakpoints correctly
     for Anthropic and provides accurate savings estimates for DeepSeek.

### 1.2 What we should build

A **SHA256-based prefix fingerprint** computed independently on the stable-prefix
components.  The fingerprint is passed from ContextAssembler into the agent loop
and through to each inference provider.  At request time the provider compares
the fingerprint to its stored value and determines whether the prefix is:
- **Identical** (full reuse — no change in system prompt, persona, summary, KG, or tools).
- **Broken** (prefix changed — persona edited, system prompt updated, etc.).
- **Not applicable** (first request in a session, or provider doesn't support caching).

The fingerprint also carries per-component hashes so the system can log *which*
component changed (e.g. `persona: drift` vs `system_prompt: stable`).

### 1.3 Goals

| Goal                                | How                                       |
|-------------------------------------|-------------------------------------------|
| SHA256-hash stable prefix           | New `compute_prefix_digest()` in ContextAssembler |
| Independently from dynamic suffix   | Digested before history is appended       |
| Track cache hit/miss across turns   | PrefixCache becomes fingerprint-aware     |
| Per-component drift detection       | Sub-hashes for each prefix section        |
| Inject cache_control breakpoints    | AnthropicClient sets `cache_control: { type: "ephemeral" }` at prefix boundary |
| Log savings                         | `tracing::info!` with token savings per turn |
| Backward-compatible API             | CloudClient trait extended with default stubs |

---

## 2. Architecture Diagram

```
                    ┌─────────────────────────────────┐
                    │         Agent Loop               │
                    │  (loom-core/src/agent_loop.rs)   │
                    └───────────┬─────────────────────┘
                                │ 1. Build context
                    ┌───────────▼─────────────────────┐
                    │       ContextAssembler           │
                    │  (loom-context/src/lib.rs)       │
                    │                                 │
                    │  ┌─ stable_prefix ──────────┐   │
                    │  │ system_prompt            │◄──┤ SHA256 of each
                    │  │ persona                  │   │ component
                    │  │ summary                  │   │ separately,
                    │  │ kg_context               │   │ then combine
                    │  │ tool_catalog             │   │ into PrefixDigest
                    │  └──────────────────────────┘   │
                    │  ┌─ dynamic_suffix ─────────┐   │
                    │  │ truncated history        │   │ NOT hashed
                    │  └──────────────────────────┘   │
                    └───────────┬─────────────────────┘
                                │ 2. Returns (Vec<Message>, PrefixDigest)
                    ┌───────────▼─────────────────────┐
                    │         Agent Loop               │
                    │  stores PrefixDigest in          │
                    │  AgentLoopConfig (new field)     │
                    │  passes to CloudClient methods   │
                    └───────────┬─────────────────────┘
                                │ 3. complete_stream_structured()
          ┌─────────────────────┼──────────────────────┐
          │                     │                      │
   ┌──────▼──────┐    ┌────────▼──────┐    ┌──────────▼──────────┐
   │ AnthropicClient│  │ OpenAIClient │    │ InferenceEngine     │
   │                │  │ (DeepSeek)   │    │ (LM Studio / Ollama)│
   │ compares digest│  │ compares     │    │ compares digest     │
   │ → injects      │  │ digest       │    │ (via PrefixCache)   │
   │ cache_control  │  │ → logs       │    │ → logs hit/miss     │
   │ breakpoint     │  │ hit/miss     │    │                     │
   └────────────────┘  └──────────────┘    └─────────────────────┘
```

### 2.1 Data flow (step-by-step)

```
Step 1: ContextAssembler::build() is called with AssembleOptions.
        It constructs the stable prefix string and computes:
          PrefixDigest {
            combined_hash: SHA256(stable_prefix_string),
            system_hash:   SHA256(system_prompt),
            persona_hash:  SHA256(persona),   // "" if None
            summary_hash:  SHA256(summary),   // "" if None
            kg_hash:       SHA256(kg_context),// "" if None
            catalog_hash:  SHA256(tool_catalog),// "" if None
            prefix_token_count: tiktoken count of stable prefix,
          }

Step 2: ContextAssembler::build() returns (Vec<Message>, PrefixDigest).
        The caller (agent loop) stores PrefixDigest in AgentLoopConfig.

Step 3: Each iteration of the agent loop clones the CompletionRequest
        with the current messages and PrefixDigest.

Step 4: AnthropicClient, OpenAIClient, and InferenceEngine each receive
        the PrefixDigest. They compare it to the last stored digest.

Step 5: If the digest matches and the provider supports caching:
          - Anthropic: add cache_control breakpoint at index 0 of the
            dynamic suffix (first message after the system message)
            and at the system message itself.
          - DeepSeek/OpenAI: the API's native KV-cache handles this;
            we log the hit for observability.
          - Local (LM Studio/Ollama): llama.cpp's built-in KV-cache
            handles this; we log the hit.

Step 6: After the LLM response, cache_read_tokens and cache_write_tokens
        are collected from the API response (already wired through
        StreamDelta::Usage) and logged alongside the PrefixDigest.
```

---

## 3. Data Structures

### 3.1 PrefixDigest (new — in `loom-context/src/lib.rs`)

```rust
/// Deterministic SHA256 fingerprint of the stable prompt prefix.
///
/// Computed by `ContextAssembler::compute_prefix_digest()` and carried
/// through the agent loop into each inference provider for cache-hit
/// detection and breakpoint injection.
///
/// The `combined_hash` covers the full assembled stable prefix string
/// (system_prompt + persona + summary + kg_context + tool_catalog)
/// in the exact order they appear in the system message.  Per-component
/// hashes enable drift attribution in logs — the system can say
/// "cache miss (system_prompt changed)" instead of just "cache miss".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrefixDigest {
    /// SHA256 hex of the assembled stable prefix string.
    ///
    /// This is the primary signal: if `combined_hash` matches the
    /// previous request, the entire stable prefix is reusable.
    pub combined_hash: String,

    /// SHA256 of the base system prompt only (before persona/summary/etc. are
    /// appended).
    pub system_hash: String,

    /// SHA256 of the persona block, or SHA256("") if no persona.
    pub persona_hash: String,

    /// SHA256 of the conversation summary block, or SHA256("") if no summary.
    pub summary_hash: String,

    /// SHA256 of the KG context block, or SHA256("") if no KG context.
    pub kg_hash: String,

    /// SHA256 of the tool catalog block (lazy-tools list), or SHA256("") if none.
    pub catalog_hash: String,

    /// Estimated token count of the stable prefix (via tiktoken cl100k_base).
    /// This is the number of tokens that a successful cache hit saves.
    pub prefix_token_count: usize,
}
```

### 3.2 DriftReport (new — in `loom-inference/src/cache.rs`)

```rust
/// Classification of a cache-check result against a previous digest.
///
/// Used to decide whether to reset the prefix cache entirely (breaking)
/// or keep the prefix and only append new suffix messages (additive).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheStatus {
    /// First request — no previous digest to compare against.
    ColdStart,
    /// Stable prefix is identical to the last request.
    /// Full KV-cache reuse expected.
    Hit,
    /// Stable prefix changed in a way that invalidates the entire KV cache.
    /// Example: system prompt edited, persona updated, summary regenerated.
    BreakingMiss,
    /// Stable prefix is identical but the dynamic suffix grew.
    /// The prefix prefix is still cached; only the new suffix tokens are charged.
    AdditiveMiss,
}
```

> **Note**: `Copy` is derived on `CacheStatus` because it is consumed in `match` arms
> throughout provider code (e.g., `matches!(cache_status, CacheStatus::Hit)`).
> Without `Copy`, the match would move the value, preventing subsequent use in
> branching or logging after the match.

### 3.3 PrefixCache upgrade (in `loom-inference/src/cache.rs`)

```rust
/// Tracks prefix fingerprints across requests to detect KV-cache reuse.
///
/// V2 upgrade: stores `PrefixDigest` instead of raw `u64` hashes.
/// Supports per-component drift detection and cache-break classification.
pub struct PrefixCache {
    /// SHA256 digest of the last known stable prefix.
    last_digest: Mutex<Option<PrefixDigest>>,
    stats: Mutex<PrefixCacheStats>,
    /// How many messages from the front of the array constitute the prefix.
    prefix_message_count: usize,
    last_hit: Mutex<Option<bool>>,
    last_prefix_tokens: Mutex<usize>,
    /// Whether the most-recent mismatch was additive (prefix same, suffix changed)
    /// rather than breaking (prefix itself changed).
    last_drift_additive: Mutex<bool>,
}
```

### 3.4 AgentLoopConfig new fields (in `loom-core/src/agent_loop.rs`)

```rust
pub struct AgentLoopConfig {
    // ... existing fields unchanged ...

    /// SHA256 fingerprint of the stable prefix for this turn.
    /// Set by the agent loop after `ContextAssembler::build()` returns.
    /// Carried through to inference providers for cache-hit detection.
    pub prefix_digest: Option<PrefixDigest>,
}
```

### 3.5 CompletionRequest new fields (in `loom-types/src/inference.rs`)

```rust
pub struct CompletionRequest {
    // ... existing fields unchanged ...

    /// Optional SHA256 fingerprint of the stable prompt prefix.
    /// Providers use this to detect KV-cache hits and inject
    /// cache_control breakpoints.
    pub prefix_digest: Option<PrefixDigest>,
}
```

### 3.6 CloudClient trait additions (in `loom-inference/src/engine.rs`)

No new methods are required.  The existing `prefix_cache_reset()`,
`prefix_cache_stats()`, `last_cache_hit()`, and `estimated_cache_tokens()`
methods already cover the surface area.  The `PrefixDigest` travels via
`CompletionRequest.prefix_digest` which is cheap to clone and doesn't need
a separate trait method.

If future providers need a dedicated digest setter, one can be added:

```rust
/// Accept a prefix digest for the upcoming request.
/// Default: no-op (providers that don't use PrefixCache ignore it).
fn set_prefix_digest(&self, _digest: PrefixDigest) {}
```

---

## 4. Implementation Steps

### Step 0: Verify AgentLoopConfig Clone (pre-implementation check)

**Before starting any code changes**, verify that all `AgentLoopConfig` fields
implement `Clone`.  The design relies on `AgentLoopConfig` deriving `Clone` so the
agent loop can cheaply create a per-turn copy with an updated `prefix_digest`.

- `ToolRegistry` is `Arc`-wrapped, so it clones trivially (bumps the refcount).
- `EventBus` is an enum that already derives `Clone`.
- All other fields (`model`, `system_prompt`, `max_tokens`, etc.) are scalar types
  that implement `Clone`.

**Mitigation if verification fails**: If any field does not implement `Clone`,
create a `clone_config_for_turn()` helper that clones only cache-relevant fields
(`model`, `system_prompt`, `prefix_digest`) and re-uses `Arc` handles for the rest.

### Step 1: loom-context/Cargo.toml — add dependencies

**File**: `backend/crates/loom-context/Cargo.toml`

Add `sha2` and `hex` to `[dependencies]`:

```toml
[dependencies]
loom-types = { path = "../loom-types" }
anyhow = "1"
tracing = "0.1"
chrono = { version = "0.4", features = ["serde"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tiktoken-rs = "0.6"
sha2.workspace = true       # added — workspace already declares "0.10"
hex = "0.4"                  # added — hex encoding for SHA256 output
```

### Step 2: loom-context/src/lib.rs — add PrefixDigest and compute_prefix_digest()

**File**: `backend/crates/loom-context/src/lib.rs`

**a)** Add a new method `compute_prefix_digest()` on `ContextAssembler` that
   returns a `PrefixDigest` based on the *current* `AssembleOptions` without
   truncating history.  This is called by the agent loop *before* `build()`
   so the digest can be stored on the config before iteration starts.

```rust
use sha2::{Sha256, Digest};

/// Deterministic SHA256 fingerprint of the stable prompt prefix.
///
/// Computed by `ContextAssembler::compute_prefix_digest()` and carried
/// through the agent loop into each inference provider for cache-hit
/// detection and breakpoint injection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrefixDigest {
    pub combined_hash: String,
    pub system_hash: String,
    pub persona_hash: String,
    pub summary_hash: String,
    pub kg_hash: String,
    pub catalog_hash: String,
    pub prefix_token_count: usize,
}

impl ContextAssembler {
    // ... existing methods unchanged ...

    /// Compute a SHA256 fingerprint of the stable prefix **without** building
    /// the full message array.
    ///
    /// This is intentionally a pure function of the prefix components (not the
    /// history) so it can be used for cache-hit detection independently of the
    /// dynamic suffix.
    pub fn compute_prefix_digest(&self, opts: &AssembleOptions) -> PrefixDigest {
        let persona  = opts.persona.as_deref().unwrap_or("");
        let summary  = opts.summary.as_deref().unwrap_or("");
        let kg       = opts.kg_context.as_deref().unwrap_or("");
        let catalog  = opts.tool_catalog.as_deref().unwrap_or("");

        let system_hash   = hex::encode(Sha256::digest(self.system_prompt.as_bytes()));
        let persona_hash  = hex::encode(Sha256::digest(persona.as_bytes()));
        let summary_hash  = hex::encode(Sha256::digest(summary.as_bytes()));
        let kg_hash       = hex::encode(Sha256::digest(kg.as_bytes()));
        let catalog_hash  = hex::encode(Sha256::digest(catalog.as_bytes()));

        // Build the same stable prefix string that build() would produce.
        let mut combined = self.system_prompt.clone();
        if !persona.is_empty() {
            combined.push_str(&format!("\n\n## User Profile\n{}", persona));
        }
        if !summary.is_empty() {
            combined.push_str(&format!("\n\n## Conversation Summary\n{}", summary));
        }
        if !kg.is_empty() {
            combined.push_str(&format!("\n\n{}", kg));
        }
        if !catalog.is_empty() {
            combined.push_str(&format!("\n\n## Available Tools\n{}", catalog));
        }

        let combined_hash = hex::encode(Sha256::digest(combined.as_bytes()));

        // Estimate token count for the combined prefix (cl100k_base).
        let prefix_token_count = bpe().encode_with_special_tokens(&combined).len();

        PrefixDigest {
            combined_hash,
            system_hash,
            persona_hash,
            summary_hash,
            kg_hash,
            catalog_hash,
            prefix_token_count,
        }
    }
}
```

### Step 3: loom-inference/src/cache.rs — upgrade PrefixCache to fingerprint-based

**File**: `backend/crates/loom-inference/src/cache.rs`

Replace `last_hash: Mutex<Option<u64>>` with `last_digest: Mutex<Option<PrefixDigest>>`
and add per-component drift detection.

```rust
use loom_context::PrefixDigest;
use loom_types::Message;

#[derive(Debug, Clone, Default)]
pub struct PrefixCacheStats {
    pub hits: u64,
    pub misses: u64,
    /// Count of additive misses (prefix same, suffix grew → prefix reusable).
    pub additive_misses: u64,
    /// Count of breaking misses (prefix changed → full KV-cache flush).
    pub breaking_misses: u64,
}

pub struct PrefixCache {
    last_digest: Mutex<Option<PrefixDigest>>,
    stats: Mutex<PrefixCacheStats>,
    prefix_message_count: usize,
    last_hit: Mutex<Option<bool>>,
    last_prefix_tokens: Mutex<usize>,
    /// True when the most-recent miss was additive (prefix unchanged, only suffix grew).
    last_drift_additive: Mutex<bool>,
}

impl PrefixCache {
    pub fn new(prefix_message_count: usize) -> Self {
        Self {
            last_digest: Mutex::new(None),
            stats: Mutex::new(PrefixCacheStats::default()),
            prefix_message_count,
            last_hit: Mutex::new(None),
            last_prefix_tokens: Mutex::new(0),
            last_drift_additive: Mutex::new(false),
        }
    }

    /// Check whether the incoming `digest` matches the last known prefix.
    ///
    /// Returns `(CacheStatus, &PrefixDigest)` so callers can log which
    /// components drifted.
    pub fn check_digest(&self, digest: &Option<PrefixDigest>) -> (CacheStatus, Option<PrefixDigest>) {
        let Some(incoming) = digest else {
            // No digest provided → can't check (legacy path or bug).
            *self.last_hit.lock().unwrap() = Some(false);
            *self.last_prefix_tokens.lock().unwrap() = 0;
            *self.last_drift_additive.lock().unwrap() = false;
            return (CacheStatus::ColdStart, None);
        };

        let mut last = self.last_digest.lock().unwrap();
        let prev = last.clone();

        let result = match &prev {
            None => {
                // First request — record the digest and report cold start.
                *last = Some(incoming.clone());
                *self.last_hit.lock().unwrap() = Some(false);
                *self.last_prefix_tokens.lock().unwrap() = 0;
                *self.last_drift_additive.lock().unwrap() = false;
                let mut stats = self.stats.lock().unwrap();
                stats.misses += 1;
                CacheStatus::ColdStart
            }
            Some(prev_digest) => {
                if prev_digest.combined_hash == incoming.combined_hash {
                    // Full hit — prefix is identical.
                    *last = Some(incoming.clone());
                    *self.last_hit.lock().unwrap() = Some(true);
                    *self.last_prefix_tokens.lock().unwrap() = incoming.prefix_token_count;
                    *self.last_drift_additive.lock().unwrap() = false;
                    let mut stats = self.stats.lock().unwrap();
                    stats.hits += 1;
                    CacheStatus::Hit
                } else {
                    // Mismatch.  Determine whether it's breaking or additive
                    // by comparing per-component hashes.
                    let breaking = prev_digest.system_hash != incoming.system_hash
                        || prev_digest.persona_hash != incoming.persona_hash
                        || prev_digest.summary_hash != incoming.summary_hash
                        || prev_digest.kg_hash != incoming.kg_hash
                        || prev_digest.catalog_hash != incoming.catalog_hash;

                    *last = Some(incoming.clone());
                    *self.last_hit.lock().unwrap() = Some(false);
                    *self.last_prefix_tokens.lock().unwrap() = 0;
                    let mut stats = self.stats.lock().unwrap();
                    stats.misses += 1;
                    if breaking {
                        *self.last_drift_additive.lock().unwrap() = false;
                        stats.breaking_misses += 1;
                        CacheStatus::BreakingMiss
                    } else {
                        *self.last_drift_additive.lock().unwrap() = true;
                        stats.additive_misses += 1;
                        CacheStatus::AdditiveMiss
                    }
                }
            }
        };

        (result, Some(incoming.clone()))
    }

    /// Which component(s) changed between the last digest and `incoming`?
    /// Returns a human-readable list for logging.
    pub fn drift_reasons(
        &self,
        incoming: &Option<PrefixDigest>,
    ) -> Vec<&'static str> {
        let Some(incoming) = incoming else {
            return vec!["no digest provided"];
        };
        let last = self.last_digest.lock().unwrap();
        let Some(prev) = last.as_ref() else {
            return vec!["cold start (no previous digest)"];
        };
        let mut reasons = Vec::new();
        if prev.system_hash  != incoming.system_hash  { reasons.push("system_prompt"); }
        if prev.persona_hash != incoming.persona_hash { reasons.push("persona"); }
        if prev.summary_hash != incoming.summary_hash { reasons.push("summary"); }
        if prev.kg_hash      != incoming.kg_hash      { reasons.push("kg_context"); }
        if prev.catalog_hash != incoming.catalog_hash { reasons.push("tool_catalog"); }
        if reasons.is_empty() && prev.combined_hash != incoming.combined_hash {
            reasons.push("unknown (combined hash mismatch but no component drift?)");
        }
        reasons
    }

    // --- remaining methods updated for PrefixDigest ---

    pub fn last_cached_tokens(&self) -> usize {
        if self.last_hit.lock().unwrap().unwrap_or(false) {
            *self.last_prefix_tokens.lock().unwrap()
        } else {
            0
        }
    }

    pub fn reset_turn(&self) {
        *self.stats.lock().unwrap() = PrefixCacheStats::default();
        *self.last_hit.lock().unwrap() = None;
    }

    /// Snapshot the current digest for save/restore around internal LLM calls
    /// (e.g. summary generation).
    pub fn snapshot_digest(&self) -> Option<PrefixDigest> {
        self.last_digest.lock().unwrap().clone()
    }

    pub fn restore_digest(&self, saved: Option<PrefixDigest>) {
        *self.last_digest.lock().unwrap() = saved;
    }

    // --- backward-compatible hash methods (for internal calls) ---

    /// Check prefix via the new SHA256 digest path.
    ///
    /// This is the **primary** check method. It attempts to use the stored
    /// `PrefixDigest` (set by the agent loop) to determine cache-hit status.
    /// When a digest is available and matches, this returns `(true, hash)`
    /// where `hash` is a u64 fingerprint derived from the digest.
    ///
    /// If no digest has been stored (legacy orchestrator internal calls like
    /// summary generation, KG extraction, vision auxiliary), it falls through
    /// to `check_legacy()` which uses the original `DefaultHasher` logic.
    pub fn check(&self, all_messages: &[Message]) -> (bool, u64) {
        let last = self.last_digest.lock().unwrap();
        if last.is_some() {
            // Digest-based path: use the stored PrefixDigest to decide.
            let prefix_end = self.prefix_message_count.min(all_messages.len());
            let prefix = &all_messages[..prefix_end];
            let hash = hash_prefix(prefix);

            // Determine hit status from the digest comparison that was already
            // performed by a prior check_digest() call (agent loop path), or
            // fall back to last_hit tracking.
            let is_hit = self.last_hit.lock().unwrap().unwrap_or(false);
            (is_hit, hash)
        } else {
            // No stored digest — use the legacy DefaultHasher path.
            drop(last);
            self.check_legacy(all_messages)
        }
    }

    /// Legacy hash check using `DefaultHasher` (SipHash-1-3).
    ///
    /// Preserves the **original** `PrefixCache` behavior for orchestrator
    /// internal calls (summary generation, KG extraction, vision auxiliary)
    /// where `PrefixDigest` is not available.
    ///
    /// This method is kept for **backward compatibility only**. New code paths
    /// (agent loop → inference providers) should use `check_digest()` with a
    /// full `PrefixDigest` for per-component drift detection and accurate
    /// cache-break classification.
    pub fn check_legacy(&self, all_messages: &[Message]) -> (bool, u64) {
        let prefix_end = self.prefix_message_count.min(all_messages.len());
        let prefix = &all_messages[..prefix_end];
        let hash = hash_prefix(prefix);

        // Derive a comparable u64 from the last stored digest (if any).
        // If no digest is stored, last_hash stays 0 (cold start).
        let last_hash: u64 = {
            let last = self.last_digest.lock().unwrap();
            last.as_ref().map_or(0, |d| {
                let bytes = hex::decode(&d.combined_hash).unwrap_or_default();
                if bytes.len() >= 8 {
                    u64::from_be_bytes(bytes[..8].try_into().unwrap())
                } else {
                    0
                }
            })
        };

        let is_hit = last_hash != 0 && hash == last_hash;

        let mut last_hit = self.last_hit.lock().unwrap();
        *last_hit = Some(is_hit);
        if is_hit {
            self.stats.lock().unwrap().hits += 1;
        } else {
            self.stats.lock().unwrap().misses += 1;
        }

        (is_hit, hash)
    }

    pub fn stats(&self) -> PrefixCacheStats {
        self.stats.lock().unwrap().clone()
    }

    pub fn last_check_was_hit(&self) -> Option<bool> {
        *self.last_hit.lock().unwrap()
    }
}
```

### Step 4: loom-core/src/agent_loop.rs — wire PrefixDigest into the agent loop

**File**: `backend/crates/loom-core/src/agent_loop.rs`

**a)** Add the `prefix_digest` field to `AgentLoopConfig`:

```rust
pub struct AgentLoopConfig {
    // ... existing fields ...

    /// SHA256 fingerprint of the stable prefix for this turn.
    /// Computed by ContextAssembler and used by inference providers
    /// for cache-hit detection and Anthropic cache_control injection.
    pub prefix_digest: Option<loom_context::PrefixDigest>,
}
```

Add to `Default` impl:
```rust
            prefix_digest: None,
```

**b)** In `run_agent_turn_inner()` and `run_agent_turn_streaming_inner()`, compute
   the digest from ContextAssembler before entering the iteration loop:

```rust
// AFTER: let assembler = ContextAssembler::new(&config.system_prompt, 8192);
// BEFORE: let mut messages = assembler.build(opts)?;

// Compute stable-prefix fingerprint for cache-hit detection.
let digest = assembler.compute_prefix_digest(&AssembleOptions {
    persona: config.persona.clone(),
    summary: config.summary.clone(),
    kg_context: config.kg_context.clone(),
    tool_catalog: None,  // lazy_tools: tool catalog not in system msg
    history: vec![],      // not needed for prefix digest
});
let mut local_config = AgentLoopConfig {
    prefix_digest: Some(digest.clone()),
    ..config.clone()  // NOTE: AgentLoopConfig needs Clone
};
// Use `local_config` in place of `config` for the remainder.
```

**c)** Pass `prefix_digest` into `CompletionRequest` at each iteration:

```rust
let request = CompletionRequest {
    messages: messages.clone(),
    tools: effective_tools,
    tool_choice: None,
    prompt: String::new(),
    max_tokens: config.max_tokens,
    temperature: config.temperature,
    top_p: 1.0,
    stop: Vec::new(),
    stream: true,
    thinking_budget: config.thinking_budget,
    prefix_digest: local_config.prefix_digest.clone(),  // NEW
};
```

**d)** After the turn completes, log cache hit rate:

```rust
tracing::info!(
    prefix_hash = ?digest.combined_hash,
    prefix_tokens = digest.prefix_token_count,
    cache_read_tokens = total_cache_read,
    cache_write_tokens = total_cache_write,
    "turn complete — prefix digest={}, estimated cache savings={} tokens",
    &digest.combined_hash[..12],
    digest.prefix_token_count,
);
```

### Step 5: loom-inference/src/anthropic.rs — inject cache_control breakpoint

**File**: `backend/crates/loom-inference/src/anthropic.rs`

**a)** Compare `PrefixDigest` from the request against the stored digest in
   `PrefixCache`.  On a hit, annotate the system message and the first
   non-system message with `cache_control: { type: "ephemeral" }`.

**b)** In `lower_messages()`, accept the digest as a parameter and add
   `cache_control` if appropriate:

```rust
impl AnthropicClient {
    // lower_messages now takes an optional PrefixDigest.
    fn lower_messages(
        &self,
        messages: &[Message],
        prefix_digest: &Option<PrefixDigest>,
    ) -> (Option<serde_json::Value>, Vec<serde_json::Value>) {
        let (cache_status, _) = self.prefix_cache.check_digest(prefix_digest);

        let system_text = /* ... same as before ... */;
        let system_with_cache = if self.provider() == ModelBackend::Anthropic
            && matches!(cache_status, CacheStatus::Hit)
        {
            serde_json::json!([
                {
                    "type": "text",
                    "text": system_text.clone().unwrap_or_default(),
                    "cache_control": { "type": "ephemeral" }
                }
            ])
        } else {
            serde_json::json!(system_text.clone().unwrap_or_default())
        };

        let msgs: Vec<serde_json::Value> = /* ... same as before ... */;

        // On cache hit with an Anthropic model, mark the first non-system message
        // with cache_control to define the prefix boundary.
        // NOTE: Other providers (OpenAI, etc.) may support cache_control in the
        // future. The provider gate can be relaxed at that point.
        if self.provider() == ModelBackend::Anthropic
            && matches!(cache_status, CacheStatus::Hit)
            && let Some(first) = msgs.first_mut()
        {
            if let Some(content) = first.get_mut("content").and_then(|c| c.as_array_mut()) {
                if let Some(last_block) = content.last_mut() {
                    last_block["cache_control"] = serde_json::json!({ "type": "ephemeral" });
                }
            }
        }

        // Log drift reasons on miss
        if matches!(cache_status, CacheStatus::BreakingMiss) {
            let reasons = self.prefix_cache.drift_reasons(prefix_digest);
            tracing::info!(
                ?reasons,
                "Anthropic cache miss — prefix components changed"
            );
        }

        (system_with_cache, msgs)
    }
}
```

**c)** Update all three call sites (`try_complete`, `complete_stream`, `complete_stream_structured`)
   to pass `req.prefix_digest` into `lower_messages()`:

```rust
let (system_prompt, messages) = self.lower_messages(&eff, &req.prefix_digest);
```

### Step 6: loom-inference/src/openai.rs and engine.rs — digest-aware logging

**File**: `backend/crates/loom-inference/src/openai.rs` and `backend/crates/loom-inference/src/engine.rs`

Both providers call `self.prefix_cache.check(&eff)` today.  Upgrade to use
`check_digest()` and log richer information:

```rust
// For OpenAIClient (same pattern for InferenceEngine):
let (cache_status, digest) = self.prefix_cache.check_digest(&req.prefix_digest);
match cache_status {
    CacheStatus::Hit => {
        tracing::info!(
            prefix_tokens = digest.as_ref().map(|d| d.prefix_token_count),
            "KV cache hit — prefix tokens saved"
        );
    }
    CacheStatus::AdditiveMiss => {
        tracing::info!("KV cache miss (additive) — prefix unchanged, suffix grew");
    }
    CacheStatus::BreakingMiss => {
        let reasons = self.prefix_cache.drift_reasons(&req.prefix_digest);
        tracing::info!(?reasons, "KV cache miss (breaking) — prefix changed");
    }
    CacheStatus::ColdStart => {
        tracing::info!("KV cache cold start — first request in sequence");
    }
}
```

### Step 7: loom-inference/src/engine.rs — CloudClient trait additions

**File**: `backend/crates/loom-inference/src/engine.rs`

Add backward-compatible stubs to `CloudClient` trait for providers that don't
use PrefixDigest:

```rust
/// Set the prefix digest for the upcoming request.
/// Default: no-op.
fn set_prefix_digest(&self, _digest: Option<loom_context::PrefixDigest>) {}
```

Also update `prefix_hash_snapshot` / `prefix_hash_restore` to work with
`PrefixDigest` instead of `u64`:

```rust
fn prefix_digest_snapshot(&self) -> Option<loom_context::PrefixDigest> { None }
fn prefix_digest_restore(&self, _saved: Option<loom_context::PrefixDigest>) {}
```

The existing `u64`-based snapshot/restore methods remain for backward compat
with the orchestrator's internal LLM calls (summary generation).

---

## 5. API Changes Summary

| Crate           | Public API change                                        | Breaking? |
|-----------------|----------------------------------------------------------|-----------|
| `loom-context`  | New `PrefixDigest` struct (exported)                     | No        |
| `loom-context`  | New method `ContextAssembler::compute_prefix_digest()`   | No        |
| `loom-context`  | `AssembleOptions` unchanged                              | No        |
| `loom-inference`| `PrefixCacheStats`: new fields `additive_misses`, `breaking_misses` | No |
| `loom-inference`| `PrefixCache::check_digest()` new method                 | No        |
| `loom-inference`| `PrefixCache::drift_reasons()` new method                | No        |
| `loom-inference`| `CloudClient::set_prefix_digest()` default stub added    | No        |
| `loom-inference`| `CloudClient::prefix_digest_snapshot/restore` default stubs added | No |
| `loom-core`     | `AgentLoopConfig.prefix_digest` new field                | No        |
| `loom-core`     | `AgentLoopConfig` derives `Clone` (was manual)           | No        |
| `loom-types`    | `CompletionRequest.prefix_digest` new field              | No        |

No breaking changes.  All new fields have `Option` / default types and are ignored
by providers that don't implement the new flow.

---

## 6. Cache Hit Flow (Step-by-Step)

### 6.1 Turn 1: First request (Cold Start)

```
1. Agent loop builds context:
   - assembler.compute_prefix_digest() → PrefixDigest { combined_hash: "a1b2...", ... }
   - PrefixDigest stored in AgentLoopConfig.prefix_digest

2. Iteration 1: CompletionRequest sent to AnthropicClient.
   - req.prefix_digest = Some("a1b2...")
   - prefix_cache.check_digest(&req.prefix_digest)
   - last_digest is None → CacheStatus::ColdStart
   - No cache_control breakpoints injected
   - Digest recorded: last_digest = Some("a1b2...")
   - Log: "KV cache cold start — first request in sequence"

3. API call proceeds normally.
   - Providers may still use their own internal caching (e.g. DeepSeek's native
     disk KV-cache), but we report it as a miss on our side.

4. Turn completes. TurnResult.cache_read_tokens = 0, cache_write_tokens = N.
```

### 6.2 Turn 2: Same prefix, new user message (Hit)

```
1. Agent loop builds context with same system_prompt, persona, summary, KG, tools.
   - compute_prefix_digest() → "a1b2..." (same as Turn 1)

2. Iteration 1: CompletionRequest sent.
   - prefix_cache.check_digest(&Some("a1b2...")) → CacheStatus::Hit
   - PrefixCache.last_hit = Some(true)
   - PrefixCache.last_prefix_tokens = 2850 (example)

3. AnthropicClient sees CacheStatus::Hit → injects cache_control:
   - System message: "cache_control": {"type": "ephemeral"}
   - First non-system message (last text block): "cache_control": {"type": "ephemeral"}

4. Anthropic API response includes:
   - "cache_read_input_tokens": 2850  (our prefix was cached)
   - "cache_creation_input_tokens": 0 (no new cache entries, prefix already cached)

5. Turn completes.
   - TurnResult.cache_read_tokens = 2850
   - TurnResult.kv_cache_hit = Some(true)
   - Log: "KV cache hit — prefix tokens saved: 2850"
```

### 6.3 Turn 3: Persona updated (Breaking Miss)

```
1. User edits their persona. Agent loop re-builds context:
   - persona = "New persona text"
   - compute_prefix_digest() → "c3d4..." (different from "a1b2...")

2. prefix_cache.check_digest(&Some("c3d4...")) → CacheStatus::BreakingMiss
   - drift_reasons() → ["persona"]
   - Log: "KV cache miss (breaking) — prefix changed: [persona]"

3. No cache_control breakpoints injected.
   - Anthropic will NOT reuse the previous prefix.
   - The system message is sent in full.

4. Turn completes. TurnResult.cache_read_tokens = 0.
```

### 6.4 Turn 4: Same prefix as Turn 3 (Hit again)

```
1. Persona unchanged from Turn 3. PrefixDigest = "c3d4..." (same as Turn 3).

2. prefix_cache.check_digest(&Some("c3d4...")) → CacheStatus::Hit
   - Cache hit on the *new* prefix established in Turn 3.

3. cache_control breakpoints injected.
```

### 6.5 Turn 5: More history, same prefix (Additive Miss -- for local only)

```
Note: AdditiveMiss is detected when the combined_hash doesn't match but
all per-component hashes DO match.  This can happen when the hash computation
includes something outside the stable prefix (edge case), or when the digest
was computed with different tool_catalog content.

In practice, the combined_hash is computed ONLY from the stable prefix, so
AdditiveMiss should rarely occur in the current architecture.  It exists
as a safety valve for future use cases (e.g. prefix digest includes
tool definitions for the non-lazy-tools path).
```

---

## 7. Provider-Specific Behavior

| Provider           | Native Cache Support    | Our Layer                    | cache_control annotation |
|--------------------|------------------------|------------------------------|--------------------------|
| Anthropic          | Prompt Caching (ephemeral) | PrefixDigest → cache_control breakpoints | Yes (system + first non-system msg) |
| DeepSeek           | Disk KV-cache (automatic) | PrefixDigest → log hit/miss  | No                       |
| OpenAI             | Prompt Caching (manual, not used yet) | PrefixDigest → log hit/miss | No (future: could add)   |
| LM Studio / Ollama | llama.cpp KV-cache (automatic) | PrefixDigest → log hit/miss | No                       |

### 7.1 Anthropic cache_control placement

```
Request messages:

  [0] System  ← cache_control: { type: "ephemeral" }
  [1] User    ← cache_control: { type: "ephemeral" } on last content block
  [2] Assistant
  ...
  [N] User (current message)

Anthropic's prompt caching counts all tokens from [0] through the
cache_control breakpoint.  The dynamic suffix (history messages after
the breakpoint) is not cached.
```

### 7.2 DeepSeek and local models

DeepSeek's API has a built-in context caching mechanism that is triggered
by default (no special header needed).  The `cached_tokens` field in the
response comes from `prompt_tokens_details.cached_tokens`.  We log this
for observability but don't inject any markers.

Local models via LM Studio/Ollama benefit from llama.cpp's automatic
KV-cache: if the prefix bytes are identical at the raw HTTP level,
llama.cpp reuses the cached keys/values.

---

## 8. Testing Strategy

### 8.1 Unit tests (in loon-context)

**File**: `backend/crates/loom-context/src/lib.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prefix_digest_is_deterministic() {
        let a = ContextAssembler::new("sys", 1024);
        let opts = AssembleOptions::default();
        let d1 = a.compute_prefix_digest(&opts);
        let d2 = a.compute_prefix_digest(&opts);
        assert_eq!(d1.combined_hash, d2.combined_hash);
    }

    #[test]
    fn test_prefix_digest_changes_with_system_prompt() {
        let a = ContextAssembler::new("sys A", 1024);
        let b = ContextAssembler::new("sys B", 1024);
        let opts = AssembleOptions::default();
        assert_ne!(
            a.compute_prefix_digest(&opts).combined_hash,
            b.compute_prefix_digest(&opts).combined_hash,
        );
    }

    #[test]
    fn test_prefix_digest_changes_with_persona() {
        let a = ContextAssembler::new("sys", 1024);
        let opts1 = AssembleOptions { persona: Some("engineer".into()), ..Default::default() };
        let opts2 = AssembleOptions { persona: Some("designer".into()), ..Default::default() };
        assert_ne!(
            a.compute_prefix_digest(&opts1).combined_hash,
            a.compute_prefix_digest(&opts2).combined_hash,
        );
    }

    #[test]
    fn test_per_component_hashes_detect_drift() {
        let a = ContextAssembler::new("sys", 1024);
        let opts1 = AssembleOptions {
            persona: Some("engineer".into()),
            summary: Some("prev summary".into()),
            ..Default::default()
        };
        let opts2 = AssembleOptions {
            persona: Some("designer".into()),  // changed
            summary: Some("prev summary".into()), // same
            ..Default::default()
        };
        let d1 = a.compute_prefix_digest(&opts1);
        let d2 = a.compute_prefix_digest(&opts2);
        assert_ne!(d1.persona_hash, d2.persona_hash, "persona hash should differ");
        assert_eq!(d1.summary_hash, d2.summary_hash, "summary hash should be same");
        assert_ne!(d1.combined_hash, d2.combined_hash);
    }

    #[test]
    fn test_history_does_not_affect_digest() {
        let a = ContextAssembler::new("sys", 1024);
        let opts = AssembleOptions {
            history: vec![Message::user("hello")],
            ..Default::default()
        };
        let opts_empty = AssembleOptions {
            history: vec![],
            ..Default::default()
        };
        assert_eq!(
            a.compute_prefix_digest(&opts).combined_hash,
            a.compute_prefix_digest(&opts_empty).combined_hash,
            "history should not affect prefix digest"
        );
    }
}
```

### 8.2 Unit tests (in loon-inference)

**File**: `backend/crates/loom-inference/src/cache.rs`

```rust
#[test]
fn test_digest_hit_and_miss() {
    let cache = PrefixCache::new(2);
    let digest = Some(dummy_digest("abc"));
    let (status, _) = cache.check_digest(&digest);
    assert_eq!(status, CacheStatus::ColdStart);

    let (status, _) = cache.check_digest(&digest);
    assert_eq!(status, CacheStatus::Hit);
    assert_eq!(cache.last_check_was_hit(), Some(true));
}

#[test]
fn test_breaking_miss_on_persona_change() {
    let cache = PrefixCache::new(2);
    let d1 = Some(dummy_digest("abc"));
    let d2 = Some(dummy_digest_with_persona("abc", "persona_B"));

    cache.check_digest(&d1);  // cold start
    let (status, _) = cache.check_digest(&d2);  // different persona → breaking
    assert_eq!(status, CacheStatus::BreakingMiss);
    let reasons = cache.drift_reasons(&d2);
    assert!(reasons.contains(&"persona"));
}

#[test]
fn test_drift_reasons_on_multiple_changes() {
    let cache = PrefixCache::new(2);
    let d1 = Some(dummy_digest_full("sysA", "persA", "summA", "kgA", "catA"));
    let d2 = Some(dummy_digest_full("sysB", "persA", "summB", "kgA", "catA"));

    cache.check_digest(&d1);
    let reasons = cache.drift_reasons(&d2);
    assert_eq!(reasons.len(), 2);
    assert!(reasons.contains(&"system_prompt"));
    assert!(reasons.contains(&"summary"));
}

// Helper:
fn dummy_digest(combined: &str) -> PrefixDigest {
    let h = |s: &str| hex::encode(Sha256::digest(s.as_bytes()));
    PrefixDigest {
        combined_hash: h(combined),
        system_hash: h("sys"),
        persona_hash: h(""),
        summary_hash: h(""),
        kg_hash: h(""),
        catalog_hash: h(""),
        prefix_token_count: 100,
    }
}
```

### 8.3 Integration test (via existing orchestrator test)

**File**: `backend/crates/loom-core/tests/` (or equivalent)

Run two consecutive agent turns with the same system prompt and persona.
Verify:
1. Turn 1: `TurnResult.kv_cache_hit = Some(false)` (cold start).
2. Turn 2: `TurnResult.kv_cache_hit = Some(true)` (hit).
3. Turn 2: `TurnResult.cached_tokens > 0` (estimated savings populated).

Then change the persona and verify Turn 3 is a miss.

### 8.4 Manual verification checklist

- [ ] Start the app with Anthropic provider. Send two messages. Verify that
      the second request includes `"cache_control": {"type": "ephemeral"}` in
      the HTTP body (check debug logging).
- [ ] Check `tracing::info!` output for "KV cache hit — prefix tokens saved: N".
- [ ] Edit persona mid-session. Verify next request shows "KV cache miss
      (breaking) — prefix changed: [persona]".
- [ ] Switch to DeepSeek provider. Verify that cache hit/miss is logged
      but no cache_control is injected.
- [ ] Verify that the existing `DefaultHasher` path still works for internal
      LLM calls (summary generation) by checking the orchestrator snapshot/restore
      flow.

---

## 9. Rollout Plan

### Phase 1: Core types (30 min)
1. Add `sha2` (workspace), `hex` to `loom-context/Cargo.toml`.
2. Define `PrefixDigest` in `loom-context/src/lib.rs`.
3. Implement `ContextAssembler::compute_prefix_digest()`.
4. Add unit tests for `PrefixDigest`.

### Phase 2: Cache layer (45 min)
1. Add `CacheStatus` enum and `PrefixCacheStats` new fields.
2. Implement `PrefixCache::check_digest()` and `PrefixCache::drift_reasons()`.
3. Add `snapshot_digest` / `restore_digest` methods.
4. Keep existing `check()` for backward compat with orchestrator internal calls.
5. Add unit tests for cache hit/miss/drift classification.

### Phase 3: Agent loop wiring (30 min)
1. Add `prefix_digest` to `AgentLoopConfig` and its `Default`.
2. Derive `Clone` for `AgentLoopConfig` (or make a clone fn if fields are not Clone).
3. Compute digest in `run_agent_turn_inner()` and `run_agent_turn_streaming_inner()`.
4. Pass digest into `CompletionRequest`.
5. Log cache hit rate at turn completion.

### Phase 4: Provider integration (45 min)
1. Update `AnthropicClient::lower_messages()` to accept digest and inject `cache_control`.
2. Update `OpenAIClient` call sites to use `check_digest()` with richer logging.
3. Update `InferenceEngine` call sites similarly.
4. Add `CompletionRequest.prefix_digest` field.
5. Add default stubs to `CloudClient` trait.

### Phase 5: Integration test and manual verification (30 min)
1. Write integration test for two-turn cache hit scenario.
2. Manual smoke test with Anthropic and DeepSeek.
3. Verify backward compatibility (internal LLM calls, snapshot/restore).

### Phase 6: Observability (15 min)
1. Add `tracing::info!` for cache hit/miss with token savings.
2. Add cache stats to the existing metrics/logging endpoint if applicable.

**Total: ~3 hours (with tests)**

---

## 10. Risks and Mitigations

| Risk                                    | Mitigation                                                |
|-----------------------------------------|-----------------------------------------------------------|
| `AgentLoopConfig` has fields that don't implement `Clone` (e.g. `EventBus`) | Wrap non-Clone fields in `Arc` or use `config.clone_turn_config()` that clones only cache-relevant fields.  `EventBus` already has `Clone` (it's an enum); verify all fields. |
| `sha2` already in workspace but at wrong version | Verify workspace version `0.10` is compatible.  If older, update workspace `Cargo.toml`. Already confirmed: `sha2 = "0.10"`. |
| `tiktoken-rs` token count changes between versions | Token count is used only for savings *estimation*, not for correctness.  Small drift doesn't affect cache behavior. |
| Anthropic API rejects `cache_control` on non-Claude models | Only inject `cache_control` when `ModelBackend::Anthropic`.  Gate behind `self.model.starts_with("claude-")` if needed. |
| Internal orchestrator LLM calls (summary, KG) break because they don't carry digest | They continue using the legacy `check()` path via `snapshot_hash`/`restore_hash`.  No change needed. |

---

## 11. Revision History

| Date       | Rev | Description                        |
|------------|-----|------------------------------------|
| 2026-06-08 | 1   | Initial design document            |

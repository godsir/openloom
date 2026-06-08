# Session Compaction with Context Retention — Technical Design

**Document**: 006-session-compaction
**Status**: Draft
**Date**: 2026-06-08
**Scope**: Backend only
**Estimated Effort**: 1.5 weeks

---

## 1. Overview / Problem Statement

### 1.1 Current State (v2.0)

The openLoom agent loop currently handles long conversations through two independent mechanisms, neither of which actually compacts the message history:

| Mechanism | Location | What It Does | Gap |
|---|---|---|---|
| `truncate_history()` | `loom-context/src/lib.rs:111` | Scans newest-to-oldest, skips Tool-role messages, stops at max_tokens via tiktoken. Drops everything older. | No summarization — old context is **lost**. |
| `SummaryEngine` | `loom-memory/src/summary.rs` | Generates a summary paragraph via LLM when history > 12 messages or > 6 hours since last summary. Saves to DB. | Summary is **additive only** — injected into the system prompt prefix (`AgentLoopConfig.summary`). Old messages **stay** in the history array. |
| `ContextAssembler::compact()` | `loom-context/src/lib.rs:145` | Stub — always returns `Ok(Vec::new())`. | Not implemented. |
| Token budget check | `loom-core/src/agent_loop.rs:627` | When `total_prompt > max_prompt_budget`, stops the loop with a "task in progress" message. | No compaction before stopping. The user must manually type "continue" to resume. |

### 1.2 The Real Problem

Long tool-using conversations grow unbounded. Each iteration appends `assistant(tool_call) + tool(result)` pairs. After 20-30 iterations with verbose tool outputs (file reads, shell dumps, web fetches), the context window fills and the LLM either:

1. Hits the token budget wall — the loop stops defensively and the user sees "task in progress, type continue" (a bad UX cliff).
2. Loses early context — old tool outputs dominate the prompt budget, starving recent turns.
3. Suffers KV-cache invalidation — the stable prefix (system prompt + summary) is pushed beyond the prefix boundary by the growing history.

### 1.3 Goal

When a conversation approaches the token budget (default: 80% of `max_prompt_budget`), perform **intelligent compaction** that:

- Summarizes old tool outputs with the LLM (semantic compression)
- Retains critical decisions, errors, and file paths (heuristic signal preservation)
- Elides base64 image payloads (token-cost neutralization)
- Detects and collapses repetitive tool-call loops (pattern suppression)
- Emits `CompactionEvent` so the frontend can display what was compacted and how many tokens were saved.
- Replaces old history with the compacted form — not additive injection.

---

## 2. Compaction Strategies

The system uses a **two-tier strategy**: a fast, no-cost heuristic pass followed by an optional LLM-based summarization pass.

### 2.1 Tier 1: Heuristic Compaction (always on, zero LLM cost)

Applied per-message in a single pass over the history. These transformations are deterministic, require no API calls, and guarantee token reduction.

#### 2.1.1 Long Tool-Output Truncation
Tool-result messages exceeding `max_tool_output_chars` (default 2000 chars) are truncated to `keep_head(500) + "...[truncated N chars]..." + keep_tail(200)`. This preserves error messages (usually at the tail) and the start-of-output context while slashing bulk content (e.g., 50,000-char shell dumps of repo listings).

#### 2.1.2 Base64 Payload Elision
`ContentPart::Image` and inline base64 data URIs in Text parts are replaced with placeholders: `[base64 image, N bytes]`. A single 800x600 JPEG in base64 consumes ~1600 tokens; elision reduces this to ~5 tokens.

#### 2.1.3 Repetitive Tool-Loop Collapse
When the same tool is called with the same arguments >= 3 consecutive times, subsequent calls are replaced with a single summary message: `[Repetitive calls to <tool>(<args>) suppressed — called N times. Last result: <last_result truncated>]`. This handles agent-loop stalls where the LLM retries the same failing operation in a loop.

#### 2.1.4 Tool-Result Retention Markers
Certain tool results are **never** truncated because they carry critical context:
- Results containing `"Error:"` or `"error:"` patterns (failure signals)
- Results from `file_read` (user's source code)
- Results containing absolute file paths (detected via regex)

### 2.2 Tier 2: LLM Summarization (optional, configurable)

When `use_llm_summarization` is true and the heuristic pass alone fails to bring the token count below the trigger threshold, the system calls a separate LLM (temperature=0, `reasoningEffort=off`) to produce a structured summary of the oldest messages.

#### LLM Prompt Template
```
You are a conversation compressor. Given a conversation between a user and an AI
assistant with tools, produce a structured summary that preserves:

1. KEY DECISIONS: Any irreversible choices, design decisions, or architectural
   tradeoffs the user and assistant agreed on.
2. CRITICAL ERRORS: Error messages that blocked progress, with their resolutions.
3. FILE PATHS: Every absolute file path mentioned, categorized by read/write.
4. CONTEXT: What the user is trying to accomplish (their goal).
5. STATE: Current state of the work (e.g., "implemented X, Y remains").

Output format (JSON inside ```json fence):
{
  "goal": "one sentence",
  "decisions": ["decision 1", "decision 2"],
  "errors": [{"context": "...", "resolution": "..."}],
  "files": {"read": ["/path/a", "/path/b"], "written": ["/path/c"]},
  "state": "current state description"
}

Conversation to summarize:
---
{history_text}
---
```

The JSON output is stored and reformatted as a compact text block injected at the start of the dynamic suffix. This costs ~300-500 prompt tokens + ~150 completion tokens per compaction, but can release ~4000-8000 tokens of old history.

### 2.3 Strategy Selection Algorithm

```
fn select_strategies(history, config, token_budget) -> Vec<Strategy>:
    let current = count_tokens(history)
    if current <= budget * 0.5: return []  // No compaction needed

    let strategies = [Heuristic]
    let after_heuristic = apply_heuristic(history, config)
    if count_tokens(after_heuristic) > budget * 0.5:
        strategies.push(LLM)

    return strategies
```

---

## 3. Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────────┐
│                     Orchestrator (orchestrator.rs)                   │
│                                                                     │
│  process_message_streaming()                                        │
│    │                                                                │
│    ├─ 1. Load history from session_histories                        │
│    │                                                                │
│    ├─ 2. SummaryEngine::should_summarize() — existing check        │
│    │      (kept for backward compatibility)                         │
│    │                                                                │
│    ├─ 3. [NEW] CompactionDecisionEngine::evaluate()                 │
│    │      ┌──────────────────────────────────────────┐              │
│    │      │ If token_usage > 80% of max_prompt_budget │              │
│    │      │   OR history messages > keep_recent_msgs  │              │
│    │      │   → Trigger compaction                    │              │
│    │      └──────────────────────────────────────────┘              │
│    │                                                                │
│    ├─ 4. [NEW] ContextAssembler::compact()                          │
│    │      ┌──────────────────────────────────────────┐              │
│    │      │ Phase 1: Heuristic compaction             │              │
│    │      │   - Truncate long tool outputs            │              │
│    │      │   - Elide base64 payloads                 │              │
│    │      │   - Collapse repetitive loops             │              │
│    │      │   - Preserve error/file-path signals      │              │
│    │      │                                           │              │
│    │      │ Phase 2: LLM summarization (if needed)    │              │
│    │      │   - Separate LLM call (temp=0)            │              │
│    │      │   - Summarize oldest messages             │              │
│    │      │   - Produce structured JSON               │              │
│    │      │                                           │              │
│    │      │ Returns: CompactionResult {               │              │
│    │      │   compacted_history,                      │              │
│    │      │   tokens_before, tokens_after,             │              │
│    │      │   items_compacted, strategy_used            │              │
│    │      │ }                                          │              │
│    │      └──────────────────────────────────────────┘              │
│    │                                                                │
│    ├─ 5. [NEW] Emit CompactionEvent via EventBus                    │
│    │                                                                │
│    ├─ 6. [NEW] Save compacted history back to session_histories     │
│    │      (and eventual DB persistence via save_turn)               │
│    │                                                                │
│    ├─ 7. PrefixCache: snapshot hash before compaction               │
│    │      restore after (compaction invalidates prefix)             │
│    │                                                                │
│    └─ 8. Proceed with normal agent loop (run_agent_turn_streaming)  │
│                                                                     │
├─────────────────────────────────────────────────────────────────────┤
│                    Agent Loop (agent_loop.rs)                        │
│                                                                     │
│  run_agent_turn_inner() / run_agent_turn_streaming_inner()          │
│    │                                                                │
│    ├─ [NEW] Mid-turn compaction check (after each iteration):       │
│    │      if count_tokens(messages) > budget * 0.8:                 │
│    │          compacted = assembler.compact(&messages).await        │
│    │          messages = compacted.compacted_history                │
│    │          emit CompactionEvent via StreamDelta                   │
│    │                                                                │
│    └─ Token budget check (existing): stops loop when exceeded       │
│        [NEW] Before stopping, try compaction first                  │
│                                                                     │
├─────────────────────────────────────────────────────────────────────┤
│                    ContextAssembler (loom-context/src/lib.rs)        │
│                                                                     │
│  pub async fn compact(&self, history, config) -> CompactionResult   │
│    │                                                                │
│    ├─ Step 1: Partition history                                     │
│    │      recent = history[-(keep_recent_msgs):]  // untouched      │
│    │      old = history[:-(keep_recent_msgs)]       // to compact   │
│    │                                                                │
│    ├─ Step 2: Heuristic compaction on 'old'                         │
│    │      For each message in old:                                  │
│    │        - truncate_long_tool_outputs()                          │
│    │        - elide_base64_payloads()                               │
│    │        - detect_repetitive_loops()                             │
│    │        - preserve_signals()                                    │
│    │                                                                │
│    ├─ Step 3: LLM summarization on 'old' (if enabled & needed)     │
│    │      summary = llm_summarize(old).await                        │
│    │      compacted_prefix = [System(summary_text)]                 │
│    │                                                                │
│    ├─ Step 4: Reassemble                                            │
│    │      compacted_history = compacted_prefix + recent             │
│    │                                                                │
│    └─ Step 5: Return CompactionResult                               │
│                                                                     │
├─────────────────────────────────────────────────────────────────────┤
│                    PrefixCache (loom-inference/src/cache.rs)         │
│                                                                     │
│  snapshot_hash() → save before compaction auxiliary LLM call        │
│  restore_hash()  → restore after compaction auxiliary LLM call      │
│                                                                     │
│  After compaction, the message prefix has changed, so reset:        │
│  - In orchestrator: snapshot before → restore after compaction      │
│  - In agent loop: call prefix_cache_reset() after compaction        │
│    (the CloudClient trait already provides this)                    │
└─────────────────────────────────────────────────────────────────────┘
```

---

## 4. Data Structures

### 4.1 CompactionConfig

Location: `loom-types/src/config/compaction.rs` (new file, re-exported via `lib.rs`)

```rust
/// Configuration for session compaction behavior.
///
/// Consumers: loom-context (ContextAssembler::compact), loom-core (orchestrator)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionConfig {
    /// Master on/off switch. Default false during initial rollout; set to true
    /// after validation.
    pub enabled: bool,

    /// Fraction of max_prompt_budget at which compaction triggers (0.0-1.0).
    /// Default: 0.8 (80%).
    pub trigger_threshold_pct: f32,

    /// Maximum character count for a single tool output before truncation kicks in.
    /// Tool outputs longer than this will be truncated to keep_head + keep_tail.
    /// Default: 2000.
    pub max_tool_output_chars: usize,

    /// Number of most recent messages to always keep uncompacted.
    /// Default: 6.
    pub keep_recent_messages: usize,

    /// Whether to use LLM-based summarization as a second-tier strategy.
    /// Default: true.
    pub use_llm_summarization: bool,

    /// Model to use for LLM summarization. If None, uses the active model.
    /// Default: None.
    pub summarization_model: Option<String>,

    /// Timeout in milliseconds for the LLM summarization call.
    /// Default: 15000.
    pub summarization_timeout_ms: u64,
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            trigger_threshold_pct: 0.8,
            max_tool_output_chars: 2000,
            keep_recent_messages: 6,
            use_llm_summarization: true,
            summarization_model: None,
            summarization_timeout_ms: 15000,
        }
    }
}
```

### 4.2 CompactionResult

Location: `loom-context/src/compaction.rs` (new module in loom-context)

```rust
/// The result of a compaction pass.
///
/// Consumers: loom-core (orchestrator, agent_loop)
#[derive(Debug, Clone)]
pub struct CompactionResult {
    /// The compacted message history, ready for LLM consumption.
    pub compacted_history: Vec<Message>,

    /// Estimated token count of the history before compaction.
    pub tokens_before: usize,

    /// Estimated token count of the history after compaction.
    pub tokens_after: usize,

    /// Fraction of tokens saved: (before - after) / before.
    /// 0.0 = no savings; 1.0 = all tokens saved (impossible, but the range is [0, 1]).
    pub savings_pct: f64,

    /// Number of individual messages that were modified or removed.
    pub items_compacted: usize,

    /// Which strategies were applied.
    pub strategies_used: Vec<CompactionStrategy>,

    /// Number of tool outputs that were truncated.
    pub tool_outputs_truncated: usize,

    /// Number of base64 payloads that were elided.
    pub base64_payloads_elided: usize,

    /// Number of repetitive tool-call loops that were collapsed.
    pub repetitive_loops_collapsed: usize,

    /// Whether LLM summarization was performed.
    pub llm_summarization_performed: bool,

    /// The LLM-generated summary text (empty if not performed).
    pub summary_text: String,
}

/// Enum of compaction strategies applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompactionStrategy {
    /// Heuristic truncation of long tool outputs.
    HeuristicTruncation,
    /// Heuristic elision of base64 image/data payloads.
    Base64Elision,
    /// Heuristic detection and collapse of repetitive tool-call loops.
    RepetitiveLoopCollapse,
    /// LLM-based summarization of the oldest message block.
    LLMSummarization,
}
```

### 4.3 EngineEvent::CompactionPerformed (Canonical Event)

Location: `loom-types/src/event.rs` (added to existing `EngineEvent` enum)

Compaction is an infrastructure event, not a user-facing agent event. It is emitted as
an EngineEvent which the WS bridge forwards to the frontend for optional display in
the token usage panel. There is no corresponding AgentEvent variant — EngineEvent
is the single source of truth for compaction notifications.

```rust
// Added to EngineEvent enum:
pub enum EngineEvent {
    // ... existing variants ...

    /// Session compaction was performed.
    CompactionPerformed {
        session_id: String,
        tokens_before: usize,
        tokens_after: usize,
        savings_pct: f64,
        items_compacted: usize,
        strategies: Vec<String>,  // serialized CompactionStrategy values
        tool_outputs_truncated: usize,
        base64_elided: usize,
        loops_collapsed: usize,
        llm_summarization_used: bool,
        duration_ms: u64,
    },
}
```

### 4.4 AgentEvent — No Duplicate

No `AgentEvent` variant is created for compaction. The original design included an
`AgentEvent::CompactionEvent` variant, but this was removed during review because
it duplicated `EngineEvent::CompactionPerformed`. EngineEvent variants reach the
frontend via the same broadcast channel as AgentEvent variants (the WS bridge in
loom-server subscribes to both), so a second event type is unnecessary.

If a caller inside loom-core needs to react to compaction completion, it can receive
the `CompactionResult` directly from `ContextAssembler::compact()` (the return value)
instead of listening on the event bus. The `CompactionResult` carries richer fields
(tokens_before, tokens_after, savings_pct, items_compacted, strategy_used,
summary_generated) than any event struct would.

---

## 5. Implementation Steps

### 5.1 Step 1: Add CompactionConfig to loom-types (1 day)

**Files**:
- `backend/crates/loom-types/src/config/compaction.rs` (new)
- `backend/crates/loom-types/src/config/mod.rs` (add `pub mod compaction;`)
- `backend/crates/loom-types/src/lib.rs` (re-export `pub use config::compaction::*;`)

**Approach**: Follow the existing pattern from `loom-types/src/config/model_config.rs`. The config module already exists, so this is straightforward.

```rust
// backend/crates/loom-types/src/config/compaction.rs
// Full definition in Section 4.1 above.

// backend/crates/loom-types/src/config/mod.rs (add line):
pub mod compaction;

// backend/crates/loom-types/src/lib.rs (add after existing config exports):
pub use config::compaction::*;
```

### 5.2 Step 2: Implement ContextAssembler::compact() (2 days)

**Files**:
- `backend/crates/loom-context/src/compaction.rs` (new module)
- `backend/crates/loom-context/src/lib.rs` (replace stub, add module)

**Approach**: Replace the stub with a real implementation. The logic is split into a new `compaction.rs` module to keep `lib.rs` from growing beyond 250 lines (per the type-crate anti-dumping-ground rules).

```rust
// backend/crates/loom-context/src/compaction.rs

use anyhow::Result;
use loom_types::{CompactionConfig, ContentPart, Message};
use regex::Regex;
use tiktoken_rs::CoreBPE;

use crate::CompactionResult;
use crate::CompactionStrategy;

/// Compact conversation history using the configured strategies.
///
/// Partitioning: the most recent `config.keep_recent_messages` messages are
/// preserved as-is.  The older block is passed through heuristic compaction
/// and optionally LLM summarization.
pub async fn compact_history(
    history: &[Message],
    config: &CompactionConfig,
    bpe: &CoreBPE,
    llm_client: Option<&dyn CloudClient>,  // for LLM summarization
) -> Result<CompactionResult> {
    let tokens_before = count_tokens(history, bpe);

    // Step 1: Partition
    let split_idx = history.len().saturating_sub(config.keep_recent_messages);
    let (old, recent) = history.split_at(split_idx);
    let old: Vec<Message> = old.to_vec();
    let recent: Vec<Message> = recent.to_vec();

    let mut items_compacted = 0usize;
    let mut tool_outputs_truncated = 0usize;
    let mut base64_payloads_elided = 0usize;
    let mut repetitive_loops_collapsed = 0usize;

    // Step 2: Heuristic compaction on 'old' block
    let mut old_compacted: Vec<Message> = Vec::new();
    for msg in &old {
        let (compacted_msg, truncations, elisions) =
            apply_heuristic_compaction(msg, config);
        old_compacted.push(compacted_msg);
        tool_outputs_truncated += truncations;
        base64_payloads_elided += elisions;
        items_compacted += 1;
    }

    // Step 3: Detect and collapse repetitive loops (threshold hardcoded at 3)
    let (old_compacted, loops_collapsed) =
        collapse_repetitive_loops(&old_compacted, 3);
    repetitive_loops_collapsed += loops_collapsed;

    // Step 4: LLM summarization (if enabled and still over threshold)
    let mut summary_text = String::new();
    let mut llm_performed = false;
    if config.use_llm_summarization
        && let Some(client) = llm_client
    {
        let after_heuristic = count_tokens(&old_compacted, bpe);
        // Only call LLM if heuristic pass wasn't enough
        if after_heuristic > 0 && after_heuristic > count_tokens(&recent, bpe) {
            let summary = llm_summarize(
                &old_compacted,
                30,  // max messages to summarize (hardcoded)
                client,
            ).await?;
            summary_text = summary;
            llm_performed = true;
            // Replace entire 'old' block with a single summary message
            old_compacted = vec![Message {
                role: loom_types::Role::System,
                content: vec![ContentPart::Text {
                    text: format!("## Compaction Summary\n{}", summary_text),
                }],
                timestamp: chrono::Utc::now(),
                usage: None,
            }];
        }
    }

    // Step 5: Reassemble
    let mut compacted_history = std::mem::take(&mut old_compacted);
    compacted_history.extend(recent);

    let tokens_after = count_tokens(&compacted_history, bpe);
    let savings_pct = if tokens_before > 0 {
        (tokens_before - tokens_after) as f64 / tokens_before as f64
    } else {
        0.0
    };

    let mut strategies = vec![CompactionStrategy::HeuristicTruncation];
    if base64_payloads_elided > 0 {
        strategies.push(CompactionStrategy::Base64Elision);
    }
    if repetitive_loops_collapsed > 0 {
        strategies.push(CompactionStrategy::RepetitiveLoopCollapse);
    }
    if llm_performed {
        strategies.push(CompactionStrategy::LLMSummarization);
    }

    Ok(CompactionResult {
        compacted_history,
        tokens_before,
        tokens_after,
        savings_pct,
        items_compacted,
        strategies_used: strategies,
        tool_outputs_truncated,
        base64_payloads_elided,
        repetitive_loops_collapsed,
        llm_summarization_performed: llm_performed,
        summary_text,
    })
}

/// Apply per-message heuristic compaction.
/// Returns (compacted_message, tool_outputs_truncated, base64_payloads_elided).
fn apply_heuristic_compaction(
    msg: &Message,
    config: &CompactionConfig,
) -> (Message, usize, usize) {
    let mut new_msg = msg.clone();
    let mut truncations = 0usize;
    let mut elisions = 0usize;

    for part in &mut new_msg.content {
        match part {
            ContentPart::ToolResult { result, .. } => {
                if result.len() > config.max_tool_output_chars
                    && !contains_critical_signal(result)
                {
                    let head: String = result
                        .chars()
                        .take(500)  // keep_head_chars (hardcoded)
                        .collect();
                    let tail: String = result
                        .chars()
                        .rev()
                        .take(200)  // keep_tail_chars (hardcoded)
                        .collect::<String>()
                        .chars()
                        .rev()
                        .collect();
                    let removed = result.len() - head.len() - tail.len();
                    *result = format!(
                        "{}\n...[{} chars truncated]...\n{}",
                        head, removed, tail
                    );
                    truncations += 1;
                }
            }
            ContentPart::Image { data, .. } => {
                let byte_count = (data.len() * 3) / 4; // approximate decoded size
                *part = ContentPart::Text {
                    text: format!("[base64 image, {} bytes]", byte_count),
                };
                elisions += 1;
            }
            _ => {}
        }
    }

    (new_msg, truncations, elisions)
}

/// Check if a tool result contains signals that should prevent truncation.
fn contains_critical_signal(text: &str) -> bool {
    let Error = Regex::new(r"(?mi)\b(error|Error|ERROR)\b").unwrap();
    let FilePath = Regex::new(r"(?:[A-Za-z]:[/\\]|/)\S+\.\w{1,10}").unwrap();

    Error.is_match(text) || FilePath.is_match(text)
}

/// Detect consecutive identical tool-calls and collapse them.
///
/// Algorithm:
/// 1. Scan consecutive pairs of (ToolCall, ToolResult) messages.
/// 2. If two consecutive pairs have identical tool_name AND identical arguments
///    (canonicalized JSON — sorted keys, normalized whitespace):
///    - Keep the FIRST pair
///    - Replace subsequent repeats with a single synthetic Assistant message:
///      "Tool '{name}' was called again with the same arguments and produced
///       a similar result. [N repeats suppressed]"
/// 3. Increment the repeat counter for each additional repeat.
/// 4. Return the compacted message list.
/// 5. Track `items_compacted` count.
///
/// Argument comparison:
/// - Canonicalize JSON via `serde_json::to_string(&args)` — serde_json sorts
///   keys by default for structs. For `serde_json::Value`, insert entries into
///   a sorted map before comparison.
/// - If arguments cannot be compared as JSON (different structure), fall back
///   to comparing the string representation.
/// - Max 3 repeats collapsed per detection pass.
fn collapse_repetitive_loops(
    messages: &[Message],
    threshold: usize,
) -> (Vec<Message>, usize) {
    use std::collections::HashMap;

    if messages.len() < (threshold * 2) {
        return (messages.to_vec(), 0);
    }

    let mut result: Vec<Message> = Vec::with_capacity(messages.len());
    let mut loops_collapsed = 0usize;
    let mut i = 0;

    while i < messages.len() {
        // Look ahead for a run of identical (tool_call, tool_result) pairs.
        if i + 1 < messages.len()
            && is_tool_call(&messages[i])
            && is_tool_result(&messages[i + 1])
        {
            let tool_name = get_tool_name(&messages[i]);
            let tool_args_canon = canonicalize_args(&messages[i]);
            let mut run_len = 1usize;

            // Count identical consecutive pairs
            let mut j = i + 2;
            while j + 1 < messages.len()
                && run_len < 3  // max 3 repeats collapsed per pass
                && is_tool_call(&messages[j])
                && is_tool_result(&messages[j + 1])
            {
                if get_tool_name(&messages[j]) == tool_name
                    && canonicalize_args(&messages[j]) == tool_args_canon
                {
                    run_len += 1;
                    j += 2;
                } else {
                    break;
                }
            }

            if run_len >= threshold {
                // Keep the first pair
                result.push(messages[i].clone());
                result.push(messages[i + 1].clone());
                loops_collapsed += run_len - 1;

                // Replace the repeats with a synthetic message
                result.push(Message {
                    role: loom_types::Role::Assistant,
                    content: vec![ContentPart::Text {
                        text: format!(
                            "Tool '{}' was called again with the same arguments \
                             and produced a similar result. [{} repeat(s) suppressed]",
                            tool_name,
                            run_len - 1,
                        ),
                    }],
                    timestamp: chrono::Utc::now(),
                    usage: None,
                });

                i = j; // skip past the collapsed run
            } else {
                result.push(messages[i].clone());
                i += 1;
            }
        } else {
            result.push(messages[i].clone());
            i += 1;
        }
    }

    (result, loops_collapsed)
}

/// Canonicalize tool call arguments for comparison.
/// Uses serde_json serialization which sorts keys for structs.
fn canonicalize_args(msg: &Message) -> String {
    if let Some(tool_calls) = msg.tool_calls() {
        if let Some(tc) = tool_calls.first() {
            return serde_json::to_string(&tc.function.arguments)
                .unwrap_or_else(|_| format!("{:?}", tc.function.arguments));
        }
    }
    String::new()
}

fn is_tool_call(msg: &Message) -> bool {
    msg.role == loom_types::Role::Assistant && msg.tool_calls().map_or(false, |tc| !tc.is_empty())
}

fn is_tool_result(msg: &Message) -> bool {
    msg.role == loom_types::Role::Tool
}

fn get_tool_name(msg: &Message) -> String {
    msg.tool_calls()
        .and_then(|tc| tc.first().map(|t| t.function.name.clone()))
        .unwrap_or_default()
}

/// Count tokens in a message slice using the shared tiktoken BPE.
fn count_tokens(messages: &[Message], bpe: &CoreBPE) -> usize {
    messages
        .iter()
        .map(|m| {
            m.content
                .iter()
                .filter_map(|p| match p {
                    ContentPart::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("")
        })
        .map(|t| bpe.encode_with_special_tokens(&t).len())
        .sum()
}

/// Call the LLM to produce a structured summary of old messages.
async fn llm_summarize(
    messages: &[Message],
    max_msgs: usize,
    client: &dyn CloudClient,
) -> Result<String> {
    // Take at most max_msgs from the front (oldest).
    let to_summarize: Vec<&Message> = messages
        .iter()
        .take(max_msgs)
        .collect();

    let history_text: String = to_summarize
        .iter()
        .map(|m| format!("[{}]: {}", m.role.as_str(), m.text_content()))
        .collect::<Vec<_>>()
        .join("\n");

    let prompt = format!(r#"You are a conversation compressor. Given a conversation between a user and an AI assistant with tools, produce a structured summary that preserves:

1. KEY DECISIONS: Any irreversible choices, design decisions, or architectural tradeoffs the user and assistant agreed on.
2. CRITICAL ERRORS: Error messages that blocked progress, with their resolutions.
3. FILE PATHS: Every absolute file path mentioned, categorized by read/write.
4. CONTEXT: What the user is trying to accomplish (their goal).
5. STATE: Current state of the work.

Output ONLY valid JSON inside a ```json fence. Do not add commentary.

Conversation:
---
{}
---"#, history_text);

    let request = CompletionRequest {
        messages: vec![Message::user(&prompt)],
        max_tokens: 512,
        temperature: 0.0,
        ..Default::default()
    };

    let response = client.complete(request).await?;
    Ok(response.text)
}
```

### 5.3 Step 3: Update ContextAssembler::compact() stub (0.5 day)

**File**: `backend/crates/loom-context/src/lib.rs`

Replace the stub at line 145 with:

```rust
/// Compact conversation history using the configured strategies.
///
/// Partitions history into an "old" block (to compact) and a "recent" block
/// (preserved as-is, size = `config.keep_recent_messages`). Applies heuristic
/// compaction to the old block, then optionally performs an LLM summarization
/// pass if the heuristic pass alone doesn't bring the token count low enough.
///
/// Callers should pass `llm_client` as the auxiliary "summary" client to avoid
/// consuming the main model's token budget.
pub async fn compact(
    &self,
    history: &[Message],
    config: &CompactionConfig,
    llm_client: Option<&dyn CloudClient>,
) -> Result<CompactionResult> {
    compaction::compact_history(history, config, bpe(), llm_client).await
}
```

And add the module declaration at the top of `lib.rs`:

```rust
mod compaction;
pub use compaction::{CompactionResult, CompactionStrategy};
```

#### 5.3.1 Migration Plan: compact() Signature Change

The `compact()` stub signature changes from `Vec<Message>` to `CompactionResult`.
Existing callers must be updated.

1. **Audit callers** (before changing the signature):
   - Grep `loom-core/src/orchestrator.rs` for `compact(`
   - Grep `loom-core/src/agent_loop.rs` for `compact(`
   - Grep test files for `compact(`
2. If callers exist: update them to destructure `CompactionResult` and use
   `result.compacted_history` for the message list.
3. If zero callers exist (the stub was never called): note this and proceed
   with the signature change.
4. **Compatibility wrapper** (optional, remove after all callers migrated):
   ```rust
   // Temporary compat wrapper — remove after all callers migrated
   pub async fn compact_legacy(&self, history: &[Message]) -> Result<Vec<Message>> {
       let config = CompactionConfig::default();
       let result = self.compact(history, &config, None).await?;
       Ok(result.compacted_history)
   }
   ```

### 5.4 Step 4: Add CompactionEvent to EngineEvent (0.5 day)

**File**: `backend/crates/loom-types/src/event.rs` — add `CompactionPerformed` variant

The `CompactionPerformed` variant is described in Section 4.3 above. No corresponding
`AgentEvent` variant is needed (see Section 4.4 for rationale).

### 5.5 Step 5: Integrate into Orchestrator (1.5 days)

**File**: `backend/crates/loom-core/src/orchestrator.rs`

#### 5.5.1 Add `compaction_config` field to Orchestrator

```rust
// New field in Orchestrator struct:
compaction_config: Arc<RwLock<CompactionConfig>>,

// In Orchestrator::new():
compaction_config: Arc::new(RwLock::new(CompactionConfig::default())),

// New public accessors:
pub async fn compaction_config(&self) -> CompactionConfig {
    self.compaction_config.read().await.clone()
}

pub async fn set_compaction_config(&self, config: CompactionConfig) {
    *self.compaction_config.write().await = config;
}
```

#### 5.5.2 Insert compaction step in process_message_streaming()

After the summary check block (after line 4759, before the system prompt assembly), insert:

```rust
// ── Session Compaction (P1) ──
// Check if history token count exceeds the compaction threshold BEFORE
// running the agent turn. Compact if needed to keep the context window
// manageable and preserve KV-cache prefix.
let compaction_config = self.compaction_config().await;
let history_for_compaction = history.clone();
let max_budget = *self.default_max_prompt_budget.read().await;
let (compacted_history, compaction_event) = if max_budget > 0 {
    let assembler = ContextAssembler::new(&system_prompt, max_budget);
    let current_tokens = {
        let bpe = tiktoken_rs::cl100k_base().unwrap();
        history_for_compaction.iter()
            .map(|m| bpe.encode_with_special_tokens(&m.text_content()).len())
            .sum::<usize>()
    };
    let threshold_tokens = (max_budget as f64 * compaction_config.trigger_threshold_pct as f64) as usize;

    if current_tokens > threshold_tokens {
        tracing::info!(
            session_id = %session_id,
            current_tokens,
            threshold_tokens,
            "compaction triggered"
        );
        let llm_client = self.build_auxiliary_client("summary").await;
        match assembler.compact(
            &history_for_compaction,
            &compaction_config,
            llm_client.as_deref(),
        ).await {
            Ok(result) => {
                // Reset prefix cache since compaction changed the history
                if let Some(ref sc) = llm_client {
                    sc.prefix_cache_reset();
                }
                let event = EngineEvent::CompactionPerformed {
                    session_id: session_id.to_string(),
                    tokens_before: result.tokens_before,
                    tokens_after: result.tokens_after,
                    savings_pct: result.savings_pct,
                    items_compacted: result.items_compacted,
                    strategies: result.strategies_used
                        .iter()
                        .map(|s| format!("{:?}", s))
                        .collect(),
                    tool_outputs_truncated: result.tool_outputs_truncated,
                    base64_elided: result.base64_payloads_elided,
                    loops_collapsed: result.repetitive_loops_collapsed,
                    llm_summarization_used: result.llm_summarization_performed,
                    duration_ms: 0, // filled by the broadcast layer
                };
                (result.compacted_history, Some(event))
            }
            Err(e) => {
                tracing::warn!(error = %e, "compaction failed, using original history");
                (history_for_compaction, None)
            }
        }
    } else {
        (history_for_compaction, None)
    }
} else {
    (history_for_compaction, None)
};

// Emit compaction event via EngineEvent broadcast
// The WS bridge in loom-server subscribes to EngineEvent variants and
// forwards them to the frontend for optional display in the token usage panel.
if let Some(event) = compaction_event {
    self.engine_events.send(event).ok();
}

// Use compacted_history for the agent turn
let history = compacted_history;

// Save compacted history back to session cache
{
    let mut cache = self.session_histories.write().await;
    cache.insert(session_id.to_string(), history.clone());
}
```

### 5.6 Step 6: Add mid-turn compaction to agent loop (1.5 days)

**File**: `backend/crates/loom-core/src/agent_loop.rs`

#### 5.6.1 Add compaction check after each iteration

In both `run_agent_turn_inner` (line 625, inside the iteration loop) and `run_agent_turn_streaming_inner` (line 1388, inside the iteration loop), after the token budget check at the top of the loop body, insert a compaction check:

```rust
// ── Mid-turn compaction check ──
// If the accumulated messages are approaching the budget but we haven't
// yet exceeded it, compact in-place to make room for another iteration.
if config.max_prompt_budget > 0 {
    let compaction_config = CompactionConfig::default(); // or pass via AgentLoopConfig
    let current_tokens = {
        let bpe = tiktoken_rs::cl100k_base().unwrap();
        messages.iter()
            .map(|m| bpe.encode_with_special_tokens(&m.text_content()).len())
            .sum::<usize>()
    };
    let threshold = (config.max_prompt_budget as f64
        * compaction_config.trigger_threshold_pct as f64) as usize;

    if current_tokens > threshold && iteration > 0 {
        // Only compact messages before the current turn (preserve the
        // current iteration's tool-call context).
        let current_turn_start = messages.len()
            .saturating_sub(2 + pending_tool_calls.len() * 2); // user msg + tool pairs
        let (old, recent) = messages.split_at(current_turn_start);

        // Apply heuristics only — don't call LLM mid-iteration
        let compacted = compaction::apply_heuristic_compaction_batch(
            old, &compaction_config,
        );

        // Reassemble
        messages = compacted;
        messages.extend_from_slice(recent);

        // Notify via stream delta
        let _ = delta_tx.send(StreamDelta::Text(format!(
            "\x02COMPACTION\x02{};{};{}",
            current_tokens,
            count_tokens(&messages, bpe()),
            (current_tokens - count_tokens(&messages, bpe())),
        ))).await;
    }
}
```

Note: mid-turn compaction uses **heuristic-only** strategies. LLM summarization is too slow for mid-iteration use (the LLM is already doing work). The LLM summarization path is reserved for the inter-turn compaction in the orchestrator.

#### 5.6.2 Add CompactionConfig to AgentLoopConfig

```rust
// New field in AgentLoopConfig:
pub struct AgentLoopConfig {
    // ... existing fields ...
    /// Compaction configuration for mid-turn compaction.
    pub compaction_config: CompactionConfig,
}

// In Default impl:
compaction_config: CompactionConfig::default(),
```

Propagate this through the orchestrator's config construction (both `process_message_streaming` and `process_message_with_config`).

### 5.7 Step 7: PrefixCache Integration (0.5 day)

**Files**: `backend/crates/loom-inference/src/cache.rs`, `backend/crates/loom-core/src/orchestrator.rs`

The `PrefixCache` already has `snapshot_hash()` and `restore_hash()` used by the orchestrator around auxiliary LLM calls (summarization, vision). After compaction, the message prefix has changed, so the old hash is invalid.

Changes:
1. In the orchestrator's compaction step (Step 5.5.2 above), call `client.prefix_hash_snapshot()` before compaction and `client.prefix_hash_restore(None)` after compaction (passing `None` forces a miss on the next check, which is correct since the history changed).
2. Alternatively, call `client.prefix_cache_reset()` — this is what the existing agent loop already does on line 484 (`client.prefix_cache_reset()`). The `reset_turn()` method on `PrefixCache` resets stats but keeps the prefix hash, which works for normal turns but not after compaction. We should add a `reset_prefix()` method:

```rust
// Add to PrefixCache (loom-inference/src/cache.rs):
/// Reset the stored prefix hash, forcing the next check() to be a miss.
/// Call this after compaction or any operation that changes the message prefix.
///
/// Integration requirement for Feature 001: clears BOTH the SHA256 digest
/// and the legacy DefaultHasher hash, so the next call is a miss regardless
/// of which check path is active.
pub fn reset_prefix(&self) {
    let mut digest = self.last_digest.lock().unwrap();
    *digest = None;
    let mut hash = self.last_hash.lock().unwrap();
    *hash = None;
}
```

Feature 006 must merge AFTER Feature 001 so that `last_digest` is already
present. An integration test must verify that `reset_prefix()` results in
`check()` returning `(false, _)` on the next call.

```rust
// Add to test module (loom-inference/src/cache.rs tests):
#[test]
fn test_reset_prefix_forces_cache_miss() {
    let cache = PrefixCache::new();
    let msgs = vec![Message::user("hello")];
    // Prime the cache with a hit
    let (hit1, _) = cache.check(&msgs);
    // Reset
    cache.reset_prefix();
    // Next check must be a miss
    let (hit2, _) = cache.check(&msgs);
    assert!(!hit2, "reset_prefix() must force the next check() to be a miss");
}
```

And expose it through the `CloudClient` trait:

```rust
// In CloudClient trait (loom-inference/src/engine.rs or wherever defined):
fn prefix_cache_reset(&self) {
    // existing: resets turn stats
    // Call prefix_cache.reset_prefix() here as well
}
```

---

## 6. Compaction Decision Algorithm

### 6.1 When to Trigger

The decision is made at two points:

**Point A: Between Turns (Orchestrator)**
```
Inputs:
  - history: Vec<Message>          // raw history for the session
  - max_prompt_budget: usize       // from global config
  - config.trigger_threshold_pct: f64  // e.g., 0.8
  - config.keep_recent_messages: usize  // e.g., 6

Decision:
  if max_prompt_budget == 0:
      return NO_COMPACTION  // budget tracking disabled

  current_tokens = count_tokens(history)
  threshold_tokens = max_prompt_budget * trigger_threshold_pct

  if current_tokens <= threshold_tokens:
      return NO_COMPACTION  // under threshold

  // Also check: is there anything to compact?
  if history.len() <= keep_recent_messages:
      return NO_COMPACTION  // everything is in the "recent" window

  return COMPACTION_NEEDED
```

**Point B: Mid-Turn (Agent Loop)**
```
Inputs:
  - messages: Vec<Message>      // in-progress conversation
  - max_prompt_budget: usize    // from AgentLoopConfig
  - iteration: usize             // current iteration index
  - trigger_threshold_pct: f64

Decision:
  if max_prompt_budget == 0:
      return NO_COMPACTION

  if iteration < 4:              // don't compact early iterations
      return NO_COMPACTION

  current_tokens = count_tokens(messages)
  threshold_tokens = max_prompt_budget * trigger_threshold_pct

  if current_tokens <= threshold_tokens:
      return NO_COMPACTION

  return HEURISTIC_COMPACTION_ONLY  // No LLM call mid-iteration
```

### 6.2 What to Compact

The compaction targets are prioritized by token cost:

1. **Tool outputs** (`ContentPart::ToolResult`): These are the largest single contributors to token growth (file reads, shell dumps, web fetches). Heuristic truncation targets these first.
2. **Base64 images** (`ContentPart::Image`): One image can consume 500-2000 tokens. Elision replaces with a ~5-token placeholder.
3. **Repetitive tool loops**: Consecutive identical calls waste tokens linearly. Collapse replaces N messages with 1.
4. **Old assistant reasoning** (`ContentPart::Thinking`): Already stripped by `compact_for_llm()` during `truncate_history()`. No additional action needed.
5. **Old user messages**: Only compacted via LLM summarization when the heuristic pass isn't enough. User messages are the most critical to preserve.

---

## 7. LLM Summarization Prompt Template

```
You are a conversation compressor. Given a conversation between a user and an AI
assistant with tools, produce a structured summary that preserves meaningful context.

The summary MUST capture the following (do not omit any category):

1. GOAL: The user's overall objective in one sentence.
2. DECISIONS: Any irreversible choices, design decisions, or architectural
   tradeoffs that were agreed upon.
3. ERRORS: Critical error messages that blocked progress, with how they were
   resolved (or if still unresolved).
4. FILES: Every absolute file path mentioned, categorized as read or written.
   Include what each file contains if known.
5. STATE: The current state of the work — what has been completed, what remains.

Output ONLY valid JSON inside a ```json code fence. Use empty arrays/lists for
categories where nothing was found. Do not add any other commentary or preamble.

Example output:
```json
{
  "goal": "Implement a session compaction feature for the openLoom codebase.",
  "decisions": [
    "Use two-tier strategy: heuristic + LLM summarization.",
    "Keep 6 most recent messages uncompacted."
  ],
  "errors": [
    {"context": "Failed to compile compaction.rs due to missing import", "resolution": "Added use loom_types::CompactionConfig;"}
  ],
  "files": {
    "read": ["/home/user/openloom/backend/crates/loom-context/src/lib.rs"],
    "written": ["/home/user/openloom/backend/crates/loom-types/src/config/compaction.rs"]
  },
  "state": "Ported the CompactionConfig type definition. Next: implement compact() in ContextAssembler."
}
```

=== CONVERSATION TO SUMMARIZE ===
{conversation_text}
=== END CONVERSATION ===
```

Key properties:
- Temperature = 0.0 (deterministic output)
- Max tokens = 512 (the summary is structured, not prose)
- Reasoning effort = off (no thinking budget — keep it fast)
- The caller parses the JSON and formats it as a compact block for injection

---

## 8. PrefixCache Hash Handling After Compaction

### Problem

`PrefixCache` tracks a hash of the first N messages to detect KV-cache reuse. After compaction:
- The message count changes (old messages are summarized/collapsed)
- Message content changes (tool outputs truncated, base64 elided)
- Therefore the old prefix hash is stale

### Solution: Two-Point Reset

1. **Before compaction**: Call `client.prefix_hash_snapshot()` to save the current hash. The existing `snapshot_hash()` / `restore_hash()` pattern is already used around auxiliary LLM calls.

2. **After compaction**: Call `client.prefix_cache_reset()` to force the next `check()` call to be a miss. This is already the pattern used at the start of each agent turn (line 484 of `agent_loop.rs`).

3. **After auxiliary LLM call within compaction**: Restore the hardware hash so other auxiliary calls (vision, etc.) are not affected by the summarization request.

```rust
// Pseudocode for orchestrator compaction step:
let saved = cloud_client.prefix_hash_snapshot();  // save main prefix

// ... run compaction (which may call auxiliary LLM) ...

cloud_client.prefix_hash_restore(saved);  // restore main hash
cloud_client.prefix_cache_reset();         // force miss — content changed
```

---

## 9. Testing Strategy

### 9.1 Unit Tests (loom-context)

**File**: `backend/crates/loom-context/src/compaction.rs` (test module)

| Test | Description |
|---|---|
| `test_truncate_long_tool_output` | ToolResult > 2000 chars -> truncated to 500+200 with count marker |
| `test_preserve_critical_signal` | ToolResult with "Error:" -> NOT truncated |
| `test_preserve_file_path` | ToolResult with `/home/user/project/main.rs` -> NOT truncated |
| `test_elide_base64_image` | ContentPart::Image -> replaced with "[base64 image, N bytes]" |
| `test_collapse_repetitive_loop` | 4 consecutive identical tool-call + result pairs -> 1 pair + 1 summary |
| `test_no_collapse_below_threshold` | 2 consecutive identical calls -> no collapse |
| `test_partition_preserves_recent` | With keep_recent=6, last 6 messages are identical after compaction |
| `test_count_tokens_returns_zero_for_empty` | Empty vec -> 0 tokens |
| `test_compaction_result_savings_pct` | Verify savings_pct calculation: (100 - 30) / 100 = 0.7 |
| `test_no_compaction_needed_below_threshold` | All messages under threshold -> empty result |

### 9.2 Integration Tests (loom-core)

| Test | Description |
|---|---|
| `test_orchestrator_compacts_when_over_threshold` | Seed history with large tool outputs, verify compaction fires |
| `test_orchestrator_skips_compaction_when_under_threshold` | Small history -> no compaction |
| `test_mid_turn_compaction_preserves_current_iteration` | Compaction runs mid-turn, recent tool calls survive |
| `test_compaction_event_emitted` | Compaction fires -> EngineEvent::CompactionPerformed is broadcast |
| `test_prefix_cache_reset_after_compaction` | After compaction, next check() is a miss |

### 9.3 Manual Verification

1. Start a session with a large codebase, run 20+ tool-heavy iterations.
2. Observe the compaction event in the frontend (or log output).
3. Verify that the agent continues working without losing critical context.
4. Check that file paths from early turns are preserved in the compaction summary.
5. Verify that error messages from early failed tool calls are not lost.

---

## 10. Rollout Plan (Behind Feature Flag)

### 10.1 Feature Flag

Add to `CompactionConfig`:
```rust
pub enabled: bool,  // default: false during rollout, true after validation
```

In the orchestrator and agent loop, gate all compaction logic behind `compaction_config.enabled`. When disabled, the existing behavior is unchanged.

### 10.2 Rollout Phases

**Phase A (Week 1)**:
- `enabled = false` by default
- Ship the new types (`CompactionConfig`, `CompactionResult`, events) — they're inert without the logic being called
- Ship the heuristic compaction implementation in `loom-context`

**Phase B (Week 2)**:
- Enable heuristic-only compaction (`use_llm_summarization = false`)
- Dogfood on internal sessions, monitor token savings and context retention
- Collect feedback on whether critical context is being lost

**Phase C (Week 3)**:
- Enable LLM summarization (`use_llm_summarization = true`)
- Monitor summarization quality — is the LLM preserving critical decisions?
- Tune prompt template based on observed output quality

**Phase D (Week 4)**:
- Set `enabled = true` by default
- Add to CompactionConfig to settings UI (frontend)
- Remove feature flag entirely after 2 weeks of stable default behavior

### 10.3 Config Override via Settings

Users can configure compaction behavior through the model/settings interface:
```json
{
  "compaction": {
    "enabled": true,
    "trigger_threshold_pct": 0.8,
    "keep_recent_messages": 10,
    "use_llm_summarization": true
  }
}
```

---

## 11. File Change Summary

| File | Change | Lines (est.) |
|---|---|---|
| `loom-types/src/config/compaction.rs` | **New** — CompactionConfig type | ~50 |
| `loom-types/src/config/mod.rs` | Add `pub mod compaction;` | +1 |
| `loom-types/src/lib.rs` | Add `pub use config::compaction::*;` | +1 |
| `loom-types/src/event.rs` | Add `CompactionPerformed` to EngineEvent | ~15 |
| `loom-context/src/compaction.rs` | **New** — compact_history, heuristics, LLM summarization | ~350 |
| `loom-context/src/lib.rs` | Replace `compact()` stub, add `mod compaction` | ~15 |
| `loom-core/src/event_bus.rs` | (removed — no AgentEvent variant needed; see Section 4.4) | — |
| `loom-core/src/orchestrator.rs` | Add compaction_config field + accessors + compaction step in process_message_streaming | ~80 |
| `loom-core/src/agent_loop.rs` | Add CompactionConfig to AgentLoopConfig + mid-turn compaction check | ~60 |
| `loom-inference/src/cache.rs` | Add `reset_prefix()` method (clears both digest and legacy hash) | ~10 |
| **Total** | | **~582** |

---

## 12. Open Questions

1. **Should compaction persist to the DB?** Currently, compacted history is used for the LLM call but the raw history is still written to the DB via `save_turn`. This means the frontend always shows the full, pre-compaction message list. This is the correct UX but means compaction only benefits the LLM call, not DB storage.

2. **Which model for LLM summarization?** The current code uses `build_auxiliary_client("summary")` which resolves from the model configs. We should ensure the summary model is fast and cheap (e.g., a flash model). The temperature=0 + reasoningEffort=off settings ensure minimal cost.

3. **Streaming compaction progress?** Should the frontend see a "Compacting session..." indicator? The compaction is fast (< 1s for heuristics, < 5s for LLM summarization) so a separate indicator may not be needed, but the `CompactionEvent` provides the data if the frontend wants to display it.

4. **Compaction on session load?** When restoring a session from DB with a large history, should we compact immediately before the first prompt? Current design only compacts when approaching the budget — but a loaded session may already be over the budget. We may need to run compaction eagerly during `load_history()`.

5. **Interaction with `sanitize_message_sequence()`**: After compaction, some tool-result messages may become orphaned (their corresponding assistant+tool_call was summarized away). The existing `sanitize_message_sequence()` function already handles this — it is called at the start of every agent loop turn. No additional changes needed.

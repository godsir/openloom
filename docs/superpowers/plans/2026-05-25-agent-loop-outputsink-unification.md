# Agent Loop OutputSink Unification — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix Electron desktop app hallucinating tool calls by unifying the two divergent code paths (TUI `agent_loop_inner` and Electron `complete_with_model_streaming_meta`) through an `OutputSink` abstraction, so the Electron path also runs the full agent loop with tool definitions.

**Architecture:** Introduce an `OutputSink` enum in `agent_loop.rs` that abstracts over TUI (mpsc channel) and Electron (event_bus). Add optional parameters to `agent_loop_inner` for system prompt override and per-request model client override. Modify `complete_with_model_streaming_meta` to delegate to `agent_loop_inner` with `OutputSink::Electron`, preserving its own system prompt assembly (permission directives, identity, ishiki) and model resolution.

**Tech Stack:** Rust (axum, tokio, serde_json), existing engine/inference/skills crates

---

## File Structure

| File | Responsibility |
|------|---------------|
| `crates/engine/src/agent_loop.rs` | Add `OutputSink` enum; modify `agent_loop_inner` signature and body; modify `invoke_model_native`/`invoke_model_streaming` to accept model override |
| `crates/engine/src/lib.rs` | Modify `complete_with_model_streaming_meta` to delegate to `agent_loop_inner`; update `agent_loop()` and `agent_loop_streaming()` signatures |
| `crates/engine/src/stream.rs` | Update `handle_message_streaming` caller for new `agent_loop_streaming` signature |
| `crates/server/src/dispatch.rs` | Update `chat.send` handler to pass `OutputSink::Electron` variant; fire `StreamEnd` after `agent_loop_inner` returns |

---

### Task 1: Add `OutputSink` enum and modify `agent_loop_inner` signature

**Files:**
- Modify: `crates/engine/src/agent_loop.rs`

- [ ] **Step 1: Add `OutputSink` enum to agent_loop.rs**

After the `use` statements at the top of the file (after line 7), add:

```rust
/// Abstracts over TUI (mpsc channel) and Electron (event_bus) output mechanisms.
pub(crate) enum OutputSink {
    /// No streaming output (used by handle_message non-streaming path)
    None,
    /// TUI path: sends text tokens and tool markers via mpsc channel
    Tui(tokio::sync::mpsc::Sender<String>),
    /// Electron path: fires StreamDelta events via event_bus
    Electron {
        event_bus: tokio::sync::broadcast::Sender<EngineEvent>,
        session_id: String,
    },
}
```

- [ ] **Step 2: Modify `agent_loop` public method signature**

Replace the `agent_loop` function (lines 11-26) to pass `OutputSink::None`:

```rust
pub(crate) async fn agent_loop(
    &self,
    msg: &ChatMessage,
    session_id: &str,
    mode: openloom_models::Mode,
    model_pref: openloom_models::ModelPreference,
) -> Result<ChatResponse> {
    self.agent_loop_inner(
        msg,
        session_id,
        OutputSink::None,
        None,           // system_prompt_override
        None,           // model_override
        &[],            // images
        mode,
        model_pref,
    )
    .await
}
```

- [ ] **Step 3: Modify `agent_loop_streaming` public method signature**

Replace the `agent_loop_streaming` function (lines 28-38) to pass `OutputSink::Tui`:

```rust
pub(crate) async fn agent_loop_streaming(
    &self,
    msg: &ChatMessage,
    session_id: &str,
    tx: mpsc::Sender<String>,
    mode: openloom_models::Mode,
    model_pref: openloom_models::ModelPreference,
) -> Result<ChatResponse> {
    self.agent_loop_inner(
        msg,
        session_id,
        OutputSink::Tui(tx),
        None,
        None,
        &[],
        mode,
        model_pref,
    )
    .await
}
```

- [ ] **Step 4: Modify `agent_loop_inner` signature**

Replace the function signature (lines 40-47):

```rust
async fn agent_loop_inner(
    &self,
    msg: &ChatMessage,
    session_id: &str,
    sink: OutputSink,
    system_prompt_override: Option<String>,
    model_override: Option<std::sync::Arc<dyn openloom_inference::CloudClient>>,
    images: &[openloom_models::ImagePart],
    mode: openloom_models::Mode,
    model_pref: openloom_models::ModelPreference,
) -> Result<ChatResponse> {
```

- [ ] **Step 5: Build and verify it compiles (will have errors from old tx usage)**

Run: `cargo build 2>&1`
Expected: compilation errors in agent_loop_inner body where `tx` is used

- [ ] **Step 6: Commit**

```bash
git add crates/engine/src/agent_loop.rs crates/engine/src/lib.rs crates/engine/src/stream.rs
git commit -m "feat: add OutputSink enum and update agent_loop signatures"
```

---

### Task 2: Update `agent_loop_inner` body for OutputSink

**Files:**
- Modify: `crates/engine/src/agent_loop.rs`

- [ ] **Step 1: Replace tx usage in streaming logic (lines 161-170)**

Replace the first_iteration streaming block:

```rust
// When a streaming sink is available, use invoke_model_streaming on the
// FIRST iteration only — so reasoning tokens flow to the UI in real time.
// Subsequent tool-call rounds use non-streaming for clean tool_call parsing.
let response = if first_iteration {
    match &sink {
        OutputSink::Tui(stream_tx) => {
            first_iteration = false;
            self.invoke_model_streaming(
                completion_req,
                stream_tx.clone(),
                model_override.clone(),
                model_pref,
            )
            .await?
        }
        OutputSink::Electron { .. } => {
            first_iteration = false;
            // Electron path: use non-streaming for tool-call parsing,
            // then stream the final text via OutputSink after the loop.
            // Streaming with tools is not supported — invoke_model_streaming
            // falls back to non-streaming when tools are present anyway.
            self.invoke_model_native(&completion_req, model_override.clone(), model_pref).await?
        }
        OutputSink::None => {
            first_iteration = false;
            self.invoke_model_native(&completion_req, model_override.clone(), model_pref).await?
        }
    }
} else {
    first_iteration = false;
    self.invoke_model_native(&completion_req, model_override.clone(), model_pref).await?
};
```

- [ ] **Step 2: Replace tool call marker streaming (lines 174-181)**

Replace the tool call marker block:

```rust
if !response.tool_calls.is_empty() {
    // Stream tool call markers to UI
    match &sink {
        OutputSink::Tui(tx) => {
            for tc in &response.tool_calls {
                let call_json = serde_json::to_string(tc).unwrap_or_default();
                let _ = tx.send(format!("\x01CALL\x02{}", call_json)).await;
            }
        }
        OutputSink::Electron { .. } => {
            // ToolCallStarted events are already fired below via event_bus.
            // No additional streaming needed — the frontend handles tool.start events.
        }
        OutputSink::None => {}
    }
    // ... rest of tool execution (state change, event firing) stays the same
```

- [ ] **Step 3: Replace tool result streaming (lines 218-220)**

Replace the tool result tx.send:

```rust
match &sink {
    OutputSink::Tui(tx) => {
        let _ = tx.send(format!("\x01RESULT\x02{}", result)).await;
    }
    OutputSink::Electron { .. } => {
        // ToolCallEnded events are already fired above via event_bus.
        // No additional streaming needed.
    }
    OutputSink::None => {}
}
```

- [ ] **Step 4: Replace final text streaming for non-first iterations (lines 261-269)**

Replace the final text word-splitting block:

```rust
if !first_iteration {
    match &sink {
        OutputSink::Tui(tx) => {
            for word in response.text.split_inclusive(' ') {
                if tx.send(word.to_string()).await.is_err() {
                    break;
                }
            }
        }
        OutputSink::Electron { event_bus, session_id } => {
            // Stream the final text as StreamDelta events
            for word in response.text.split_inclusive(' ') {
                let _ = event_bus.send(EngineEvent::StreamDelta {
                    session_id: session_id.clone(),
                    delta: word.to_string(),
                });
            }
        }
        OutputSink::None => {}
    }
}
```

- [ ] **Step 5: Replace post-loop fallback streaming (lines 312-318)**

Replace the post-loop summarization streaming block (inside the `if last_response.is_empty()` handler, where it sends words after a forced text response):

```rust
if let Some(ref stream_tx) = tx {  // ← DELETE this pattern
```

Replace all remaining `if let Some(ref stream_tx) = tx` with the OutputSink match pattern. There are three occurrences:
- Line ~264: final text from tool-use follow-up (handled in step 4)
- Line ~313: forced summarization response
- Line ~338: last-resort tool output synthesis

For the forced summarization (around line 312):
```rust
match &sink {
    OutputSink::Tui(tx) => {
        for word in response.text.split_inclusive(' ') {
            if tx.send(word.to_string()).await.is_err() { break; }
        }
    }
    OutputSink::Electron { event_bus, session_id } => {
        for word in response.text.split_inclusive(' ') {
            let _ = event_bus.send(EngineEvent::StreamDelta {
                session_id: session_id.clone(),
                delta: word.to_string(),
            });
        }
    }
    OutputSink::None => {}
}
```

For the last-resort synthesis (around line 338):
```rust
match &sink {
    OutputSink::Tui(tx) => { let _ = tx.send(summary).await; }
    OutputSink::Electron { event_bus, session_id } => {
        let _ = event_bus.send(EngineEvent::StreamDelta {
            session_id: session_id.clone(),
            delta: summary,
        });
    }
    OutputSink::None => {}
}
```

- [ ] **Step 6: Use system_prompt_override in weaver assembly (lines 126-146)**

Replace the system instruction construction:

```rust
let persona_summary = self.persona.summarize().await.unwrap_or_default();
let system_with_mode = if let Some(ref override_prompt) = system_prompt_override {
    override_prompt.clone()
} else {
    let base = crate::system_instruction();
    if mode_cfg.system_suffix.is_empty() {
        base
    } else {
        format!("{}\n\n{}", base, mode_cfg.system_suffix)
    }
};
```

- [ ] **Step 7: Handle images in the user message (around line 58)**

When `images` is non-empty, the user message pushed to history should include image metadata:

```rust
let mut msg_with_images = msg.clone();
if !images.is_empty() {
    let imgs: Vec<serde_json::Value> = images.iter().map(|img| serde_json::json!({
        "data": img.data,
        "mimeType": img.mime_type,
    })).collect();
    msg_with_images.metadata = Some(serde_json::json!({"images": imgs}).to_string());
}
history.push(msg_with_images);
```

But this is complex because the model needs `Message::user_with_images()` for multimodal. For now, since agent_loop_inner's weaver assembly uses `Message::user(user_message)` (text-only), we skip native multimodal and rely on the vision auxiliary path in `complete_with_model_streaming_meta` for image analysis. The vision auxiliary converts images to text descriptions before calling agent_loop_inner.

- [ ] **Step 8: Build and verify it compiles**

Run: `cargo build 2>&1`
Expected: compilation succeeds (may have warnings about unused variables in downstream callers)

- [ ] **Step 9: Commit**

```bash
git add crates/engine/src/agent_loop.rs
git commit -m "feat: update agent_loop_inner body to use OutputSink for all streaming paths"
```

---

### Task 3: Update `invoke_model_native` and `invoke_model_streaming` for model override

**Files:**
- Modify: `crates/engine/src/agent_loop.rs`

- [ ] **Step 1: Modify `invoke_model_native` signature (line 404)**

Add `model_override` parameter:

```rust
pub(crate) async fn invoke_model_native(
    &self,
    req: &CompletionRequest,
    model_override: Option<std::sync::Arc<dyn openloom_inference::CloudClient>>,
    model_pref: openloom_models::ModelPreference,
) -> Result<CompletionResponse> {
```

- [ ] **Step 2: Add model_override fast path at the top of invoke_model_native**

Before the existing model_pref match (before line 410), add:

```rust
// If a per-request model override is provided, use it directly
if let Some(ref client) = model_override {
    if client.provider() == ModelBackend::LmStudio {
        let _ = openloom_inference::ensure_lm_studio_model(
            "http://localhost:1234/v1",
            client.model_name(),
            32000,
        )
        .await;
    }
    return client.complete(req.clone()).await;
}
```

- [ ] **Step 3: Modify `invoke_model_streaming` signature (line 466)**

Add `model_override` parameter:

```rust
pub(crate) async fn invoke_model_streaming(
    &self,
    req: CompletionRequest,
    tx: mpsc::Sender<String>,
    model_override: Option<std::sync::Arc<dyn openloom_inference::CloudClient>>,
    model_pref: openloom_models::ModelPreference,
) -> Result<CompletionResponse> {
```

- [ ] **Step 4: Add model_override fast path in invoke_model_streaming**

Before the `has_tools` check (before line 483), add:

```rust
// If a per-request model override is provided, use it directly
if let Some(ref client) = model_override {
    let has_tools = !req.tools.is_empty();
    if has_tools {
        // Tool-use path: non-streaming
        return client.complete(req).await;
    }
    // Text-only path: stream
    // ... (reuse the existing streaming collector pattern but with client instead of self.cloud)
    // This is complex; for now, fall through to the existing path.
    // The model_override non-streaming case is already covered above.
}
```

Actually, for simplicity: in `invoke_model_streaming`, if `model_override` is set, use `client.complete()` (non-streaming) regardless. The TUI path never uses model_override, and the Electron path uses non-streaming for tool rounds anyway.

```rust
if let Some(ref client) = model_override {
    return client.complete(req).await;
}
```

- [ ] **Step 5: Build and verify**

Run: `cargo build 2>&1`
Expected: compilation errors in callers that still pass the old signature

- [ ] **Step 6: Commit**

```bash
git add crates/engine/src/agent_loop.rs
git commit -m "feat: add model_override parameter to invoke_model_native and invoke_model_streaming"
```

---

### Task 4: Update callers of changed functions

**Files:**
- Modify: `crates/engine/src/lib.rs` (handle_message simple path)
- Modify: `crates/engine/src/stream.rs` (handle_message_streaming)

- [ ] **Step 1: Update `handle_message` simple path callers of `invoke_model_native`**

In `crates/engine/src/lib.rs`, search for all calls to `self.invoke_model_native()` in the simple path (around lines 714-784). These calls use `self.cloud` / `self.local_client` directly with `.complete()`, not `invoke_model_native`. So no change needed here — they don't call `invoke_model_native`.

Actually, let me verify: the simple path in `handle_message` calls `local.complete()` and `cloud.complete()` directly. It does NOT use `invoke_model_native`. So no changes needed in lib.rs for invoke_model_native callers.

- [ ] **Step 2: Update `agent_loop` caller in `handle_message` (lib.rs line 643)**

The call `self.agent_loop(&msg, session_id, mode, model_pref).await` — this now has the same signature (agent_loop's public signature didn't change), so no change needed.

- [ ] **Step 3: Update `handle_message_streaming` in stream.rs (line 65)**

The call `self.agent_loop_streaming(&msg, session_id, tx_clone, mode, model_pref).await` — agent_loop_streaming signature didn't change (still takes tx, wraps it in OutputSink::Tui internally). No change needed.

- [ ] **Step 4: Verify `cargo build` passes**

Run: `cargo build 2>&1`
Expected: compilation succeeds

- [ ] **Step 5: Commit**

```bash
git add crates/engine/src/lib.rs crates/engine/src/stream.rs
git commit -m "chore: verify all callers compile with new agent_loop signatures"
```

---

### Task 5: Modify `complete_with_model_streaming_meta` to delegate to `agent_loop_inner`

**Files:**
- Modify: `crates/engine/src/lib.rs` (lines 1112-1422)

This is the core fix. `complete_with_model_streaming_meta` currently:
1. Builds enhanced system prompt (lines 1121-1159)
2. Handles vision auxiliary (lines 1182-1203)
3. Resolves model, creates client (lines 1161-1250)
4. Builds messages manually (lines 1216-1229)
5. Calls `complete_stream()` once — NO tools, no loop

We change it to:
1. Build enhanced system prompt (keep)
2. Handle vision auxiliary (keep)
3. Resolve model, create client (keep)
4. **Classify intent with router** — simple queries skip the agent loop (fast path, no tools); complex/skill-match queries enter the agent loop
5. Build the user message with vision text if applicable (keep)
6. For simple queries: use the existing non-streaming `client.complete()` (no tools)
7. For complex queries: call `agent_loop_inner` with `OutputSink::Electron`, `system_prompt_override`, `model_override`
8. Fire `StreamEnd` with the response

- [ ] **Step 1: Add skill context to the enhanced system prompt**

After the existing system prompt assembly (around line 1159), inject skill context so the LLM knows about available tools in the system prompt too:

```rust
// Inject skill context into the system prompt
let skill_infos = self.skills.list_all();
let skill_context: String = skill_infos
    .iter()
    .map(|s| format!("- {}: {}", s.name, s.description))
    .collect::<Vec<_>>()
    .join("\n");
let agent_system = if skill_context.is_empty() {
    agent_system
} else {
    format!("{}\n\n## Available Tools\n{}", agent_system, skill_context)
};
```

- [ ] **Step 2: Add router intent classification before the branching logic**

After building the model client (after the `model_client` block), classify the user's intent to decide whether to use the agent loop or a fast path:

```rust
// Classify intent: simple queries skip the agent loop (no tools needed)
let router_out = self.router.classify_sync(prompt);
let use_agent_loop = router_out.complexity >= 0.8 || router_out.skill_match.is_some();

// Feed cognition extraction pipeline (non-blocking)
let _ = self.memory_tx.send(memory_thread::ProcessRequest {
    session_id: sid.clone(),
    text: prompt.to_string(),
    context: router_out.intent.to_string(),
});
```

- [ ] **Step 3: Replace the streaming request + collection logic (lines 1252-1422)**

Delete the `CompletionRequest` construction (lines 1252-1256), the token collector spawn (lines 1269-1300), the stream_result match (lines 1315-1359), the collect_handle await (line 1362), and the save/logic (lines 1364-1420).

Replace with branching logic — simple path (no tools) or agent loop (with tools):

```rust
// Build the user ChatMessage
let user_content = if let Some(ref vtext) = vision_text {
    format!("{}\n\n{}", vtext, prompt)
} else {
    prompt.to_string()
};

let user_msg = openloom_models::ChatMessage {
    role: "user".into(),
    content: user_content.clone(),
    timestamp: chrono::Utc::now(),
    id: None,
    seq: None,
    metadata: metadata.map(|s| s.to_string()),
};

// Feed user message into memory pipeline for cognition extraction
self.feed_memory_pipeline(&sid, &user_content, "user_message");

if use_agent_loop {
    // ── Complex/skill-match: run the full agent loop with tools ──

    let sink = OutputSink::Electron {
        event_bus: event_bus.clone(),
        session_id: sid.clone(),
    };

    let model_pref = openloom_models::ModelPreference::Auto;

    let result = self.agent_loop_inner(
        &user_msg,
        &sid,
        sink,
        Some(agent_system),  // enhanced system prompt override
        model_client.clone(),
        mode,
        model_pref,
    ).await;

    // Fire StreamEnd event
    match result {
        Ok(response) => {
            let _ = event_bus.send(EngineEvent::StreamEnd {
                session_id: sid.clone(),
                full_response: response.response.clone(),
            });
            if !response.response.is_empty() {
                self.feed_memory_pipeline(&sid, &response.response, "assistant_response");
            }
            Ok(())
        }
        Err(e) => {
            let _ = event_bus.send(EngineEvent::StreamEnd {
                session_id: sid.clone(),
                full_response: format!("[error: {}]", e),
            });
            Err(e)
        }
    }
} else {
    // ── Simple query: fast path without tools (preserves current behavior) ──

    // Build messages for a simple completion (no tools)
    let messages: Vec<Message> = std::iter::once(Message {
            role: Role::System,
            content: vec![openloom_models::ContentPart::Text { text: agent_system }],
            timestamp: chrono::Utc::now(),
        })
        .chain(history.iter().map(|m| {
            if m.role == "user" {
                Message::user(&m.content)
            } else {
                Message::assistant(&m.content)
            }
        }))
        .chain(std::iter::once(Message::user(&user_content)))
        .collect();

    let req = CompletionRequest {
        messages,
        max_tokens: self.max_output_tokens,
        temperature: 0.0,
        ..Default::default()
    };

    let (token_tx, mut token_rx) = tokio::sync::mpsc::channel::<String>(256);

    // Spawn token collector that forwards each token as StreamDelta event
    let sid_clone = sid.clone();
    let bus_clone = event_bus.clone();
    let collect_handle = tokio::spawn(async move {
        let mut full = String::new();
        while let Some(token) = token_rx.recv().await {
            if token.starts_with('\x00') { continue; } // suppress usage signals
            full.push_str(&token);
            let _ = bus_clone.send(EngineEvent::StreamDelta {
                session_id: sid_clone.clone(),
                delta: token,
            });
        }
        full
    });

    // Use the resolved model client for streaming
    let stream_result = if let Some(ref client) = model_client {
        client.complete_stream(req, token_tx).await
    } else {
        // Fallback: try pre-configured clients
        if let Some(ref cloud) = self.cloud {
            cloud.complete_stream(CompletionRequest {
                prompt: user_content.clone(),
                stream: true,
                ..Default::default()
            }, token_tx).await
        } else if let Some(ref local) = self.local_client {
            local.complete_stream(CompletionRequest {
                prompt: user_content.clone(),
                stream: true,
                ..Default::default()
            }, token_tx).await
        } else {
            Err(anyhow::anyhow!("no model client available"))
        }
    };

    let full_response = collect_handle.await.unwrap_or_default();

    // Save messages
    let _ = self.save_messages(&sid, &user_msg, &full_response);

    // Feed assistant response into memory pipeline
    if !full_response.is_empty() {
        self.feed_memory_pipeline(&sid, &full_response, "assistant_response");
    }

    // Fire StreamEnd
    if let Err(e) = stream_result {
        let _ = event_bus.send(EngineEvent::StreamEnd {
            session_id: sid.clone(),
            full_response: format!("[error: {}]", e),
        });
        return Err(e);
    }

    let _ = event_bus.send(EngineEvent::StreamEnd {
        session_id: sid.clone(),
        full_response,
    });

    Ok(())
}
```

- [ ] **Step 3: Make `OutputSink` visible to lib.rs**

In `crates/engine/src/lib.rs`, ensure `OutputSink` is importable. Since it's defined in `agent_loop.rs` as `pub(crate)`, it's accessible within the engine crate. Add the import at the top of the function or use the full path `super::agent_loop::OutputSink`.

Actually, since both are in the same crate and `OutputSink` is `pub(crate)`, we can reference it directly. But we need to make sure it's re-exported or the path is correct. In lib.rs, we can use `crate::agent_loop::OutputSink` or add a `use` statement.

At the top of `complete_with_model_streaming_meta`, add:
```rust
use crate::agent_loop::OutputSink;
```

- [ ] **Step 4: Handle the `agent_loop_inner` visibility**

`agent_loop_inner` is currently `async fn agent_loop_inner(...)` — private. We need to make it `pub(crate)` so `complete_with_model_streaming_meta` can call it.

In agent_loop.rs line 40, change:
```rust
async fn agent_loop_inner(
```
to:
```rust
pub(crate) async fn agent_loop_inner(
```

- [ ] **Step 5: Build and verify**

Run: `cargo build 2>&1`
Expected: compilation succeeds

- [ ] **Step 6: Commit**

```bash
git add crates/engine/src/lib.rs crates/engine/src/agent_loop.rs
git commit -m "feat: route complete_with_model_streaming_meta through agent_loop_inner with OutputSink::Electron"
```

---

### Task 6: Update dispatch.rs to finalize the Electron streaming path

**Files:**
- Modify: `crates/server/src/dispatch.rs`

- [ ] **Step 1: Remove the separate `chat.send` streaming path (lines 119-166)**

The current code at lines 119-166 handles the case where `model_id` and `provider` are non-empty. Since `complete_with_model_streaming_meta` now delegates to `agent_loop_inner` (which has tools), this path is fixed. However, we should also handle the case where `model_id`/`provider` are empty by routing to `handle_message`.

The current `if !model_id.is_empty() && !provider.is_empty()` branch at line 119 now calls the fixed `complete_with_model_streaming_meta`. The else branch at line 167 already calls `handle_message`. Both paths now support tool calling.

No structural change needed in dispatch.rs — but let's verify the flow is correct end-to-end.

- [ ] **Step 2: Remove redundant AgentStateChanged events from dispatch.rs**

The dispatch.rs `chat.send` handler fires AgentStateChanged events (lines 91-94, 122-125, 135-138). `agent_loop_inner` now fires its own (lines 51-54, 184-187, 350-354). Remove the dispatch.rs events to avoid duplicate/conflicting state transitions seen by the frontend.

Delete lines 89-94:
```rust
// DELETE these lines:
let event_bus = engine.event_bus().clone();
let _ = event_bus.send(EngineEvent::AgentStateChanged {
    old_state: openloom_models::AgentState::Idle,
    new_state: openloom_models::AgentState::Thinking,
});
```

Delete lines 122-125:
```rust
// DELETE these lines:
let _ = event_bus.send(EngineEvent::AgentStateChanged {
    old_state: openloom_models::AgentState::Thinking,
    new_state: openloom_models::AgentState::Acting,
});
```

Delete lines 135-138 and 156-159:
```rust
// DELETE these lines (in both Ok and Err branches):
let _ = event_bus.send(EngineEvent::AgentStateChanged {
    old_state: openloom_models::AgentState::Acting,
    new_state: openloom_models::AgentState::Idle,
});
```

- [ ] **Step 3: Extract mode from params instead of hardcoding Mode::Code**

In the `chat.send` handler, parse the `mode` field from params (already done at line 177-181 in the else branch). Move it before the branch so both paths can use it:

```rust
let mode = p
    .get("mode")
    .and_then(|v| v.as_str())
    .and_then(openloom_models::Mode::from_key)
    .unwrap_or(openloom_models::Mode::Code);  // Default to Code for agent_loop support
```

Pass `mode` to `complete_with_model_streaming_meta` as a new parameter (add to its signature).

- [ ] **Step 4: Add `mode` parameter to `complete_with_model_streaming_meta`**

Change the signature at lib.rs line 1113:

```rust
pub async fn complete_with_model_streaming_meta(
    &self,
    session_id: &str,
    prompt: &str,
    images: &[openloom_models::ImagePart],
    metadata: Option<&str>,
    model_id: &str,
    provider: &str,
    mode: openloom_models::Mode,  // NEW
) -> Result<()> {
```

Use `mode` instead of the hardcoded `Mode::Code` when calling `agent_loop_inner`.

Update the caller in `complete_with_model_streaming` (line 1109) to pass `Mode::Code` as default.

- [ ] **Step 5: Update dispatch.rs call site for new signature**

At line 133, pass the parsed `mode`:
```rust
engine.complete_with_model_streaming_meta(&session_id, content, &images, metadata.as_deref(), model_id, provider, mode).await
```

- [ ] **Step 6: Build and verify**

```bash
cargo build 2>&1
cargo clippy -- -D warnings 2>&1
```

- [ ] **Step 7: Commit**

```bash
git add crates/server/src/dispatch.rs crates/engine/src/lib.rs
git commit -m "fix: remove redundant AgentStateChanged events, pass mode from params to agent loop"
```

---

### Task 7: Fix gaps found in review — persona duplication, metadata, abort, images

**Files:**
- Modify: `crates/engine/src/agent_loop.rs`
- Modify: `crates/engine/src/lib.rs`

- [ ] **Step 1: Fix persona summary duplication (GAP 1)**

In `agent_loop_inner`, when `system_prompt_override` is set, pass an empty string as persona_summary to the weaver since the override already includes it:

In the weaver assembly block (around line 139), change:
```rust
let messages = self.weaver.assemble_messages(
    &system_with_mode,
    "",
    &persona_summary,  // ← includes persona
    None,
    assembly_history,
    self.context_max_chars,
);
```

To:
```rust
let effective_persona = if system_prompt_override.is_some() {
    ""  // persona is already embedded in the override
} else {
    &persona_summary
};
let messages = self.weaver.assemble_messages(
    &system_with_mode,
    "",
    effective_persona,
    None,
    assembly_history,
    self.context_max_chars,
);
```

- [ ] **Step 2: Fix metadata loss in save_all_messages (GAP 2)**

In `crates/engine/src/lib.rs`, find `save_all_messages` (around line 874). Change the user message insert to preserve metadata:

```rust
// Before:
let _ = store.insert(session_id, seq, "user", &user_msg.content);

// After:
let _ = store.insert_with_metadata(session_id, seq, "user", &user_msg.content, user_msg.metadata.as_deref());
```

- [ ] **Step 3: Add abort flag check in agent_loop_inner loop (GAP 4)**

In `agent_loop_inner`, at the start of each iteration (after line 124 `for _iteration in 0..max_iterations {`), add:

```rust
// Check per-session abort flag (set by chat.abort)
{
    let flags = self.abort_flags.lock().unwrap();
    if let Some(flag) = flags.get(session_id) {
        if flag.load(Ordering::SeqCst) {
            tracing::info!(session_id, "agent loop aborted by user");
            break;
        }
    }
}
```

Add `use std::sync::atomic::Ordering;` at the top of agent_loop.rs if not already present (it should be — line 7 already imports it).

- [ ] **Step 4: Remove unused `images` parameter from agent_loop_inner (GAP 7)**

The vision auxiliary path in `complete_with_model_streaming_meta` converts images to text descriptions before calling `agent_loop_inner`. The `images` parameter in `agent_loop_inner` is never consumed for multimodal messages. Remove it:

From `agent_loop_inner` signature (Task 1 Step 4): remove `images: &[openloom_models::ImagePart],`
From `agent_loop` (Task 1 Step 2): remove `&[],`
From `agent_loop_streaming` (Task 1 Step 3): remove `&[],`
From `complete_with_model_streaming_meta` call site (Task 5 Step 2): remove `images,`

- [ ] **Step 5: Build and verify**

```bash
cargo build 2>&1
cargo clippy -- -D warnings 2>&1
```

- [ ] **Step 6: Commit**

```bash
git add crates/engine/src/agent_loop.rs crates/engine/src/lib.rs
git commit -m "fix: persona duplication, metadata loss, abort support, remove unused images param"
```

---

### Task 8: Integration test — verify tool calling works end-to-end

**Files:**
- No code changes — manual verification only

```bash
cargo build 2>&1
cargo clippy -- -D warnings 2>&1
```

Expected: both pass with zero warnings.

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/dispatch.rs
git commit -m "chore: verify dispatch.rs chat.send flow after agent_loop unification"
```

---

### Task 7: Integration test — verify tool calling works end-to-end

**Files:**
- Create: (test manually via Electron app, or add integration test)

- [ ] **Step 1: Run existing tests to check for regressions**

```bash
cargo test 2>&1
```

Expected: all existing tests pass (180+ pass).

- [ ] **Step 2: Manual verification steps**

1. Start the openLoom server: `cargo run -- serve`
2. Open the Electron app
3. Send a message that requires tool use, e.g., "list files in the current directory" or "read CLAUDE.md"
4. Verify:
   - The LLM calls `file_search` or `file_read` tools (visible as tool.start/tool.end in UI)
   - Tool results are shown
   - The LLM produces a final text response based on tool results
   - StreamDelta events show text tokens in real time
   - No hallucinated tool calls (text that looks like a tool call but isn't executed)

- [ ] **Step 3: Commit (if any test fixes)**

```bash
git add -A
git commit -m "test: verify tool calling works end-to-end in Electron path"
```

---

## Verification Checklist

After all tasks complete, verify:

1. `cargo build` — zero errors
2. `cargo clippy -- -D warnings` — zero warnings
3. `cargo test` — all existing tests pass
4. Electron app: tool calls actually execute (not hallucinated)
5. Electron app: streaming text tokens appear in real time
6. TUI (`cargo run -- serve` + CLI): agent loop still works correctly
7. Model switching in Electron UI: new model is used for subsequent requests

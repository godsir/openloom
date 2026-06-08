# 004 — FIM Code Completions

> **Status**: Draft  
> **Effort**: 2 weeks  
> **Scope**: Backend (new inference path) + Frontend (CodeMirror integration)  
> **Depends On**: `004-inline-edit-session-scope` (for hotkey/accept UX patterns)

---

## 1. Overview / Problem Statement

### 1.1 Current State

The openLoom chat input is a plain HTML `<textarea>` (see `InputArea.tsx`, ref `textareaRef`). Every message traverses the full agent loop via `loomRpc('chat.send', ...)` -- minimum latency 500ms-2s even for trivial replies. There is zero typing assistance: no autocomplete, no ghost text, no next-token prediction.

Meanwhile, `@codemirror/autocomplete` is absent from `package.json`, but four CodeMirror v6 packages (`@codemirror/lang-markdown`, `@codemirror/language`, `@codemirror/state`, `@codemirror/view`, and the `codemirror` facade) are already installed -- they simply are not wired to any renderer component.

The inference layer (`loom-inference`) supports chat completions (`POST /chat/completions`) via `OpenAIClient` and `InferenceEngine`, both implementing the `CloudClient` trait. DeepSeek is one of the supported backends, but its native FIM endpoint (`POST /fim/completions`) is not called anywhere.

### 1.2 Problem

Users writing code, configuration, or structured prose in the chat input get no predictive assistance. The existing agent loop is far too slow for keystroke-level completions. For a ghost-text completion to feel instantaneous the end-to-end cycle (keystroke -> network -> render) must complete in < 500 ms. A dedicated lightweight RPC path that bypasses the agent loop is required.

### 1.3 Goal

Add **two-tier ghost-text code completions** to the chat input:

| Tier | Trigger | Debounce | Max Tokens | Provider | Endpoint |
|------|---------|----------|------------|----------|----------|
| **Short** | Any keystroke (after min prefix length) | 300 ms | 64 | DeepSeek | `POST /fim/completions` |
| **Long** | 2 s idle after last keystroke | 2000 ms | 256 | DeepSeek | `POST /fim/completions` (+ optional BM25 context) |

Both tiers call DeepSeek's native FIM API **directly**, bypassing the agent loop entirely. The completion appears as **ghost text** (dimmed, non-committed) in the CodeMirror editor. The user accepts with **Tab** or dismisses by continuing to type.

---

## 2. Two-Tier Completion Architecture

### 2.1 Short Completions (keystroke-assist)

- **Purpose**: Predict the next few tokens while the user is typing.
- **Trigger**: Any keystroke after the prefix reaches >= 3 characters.
- **Debounce**: 300 ms from last `docChanged` event. Each new keystroke resets the timer.
- **Prefix**: Last 512 characters before cursor (char-windowed from the editor).
- **Suffix**: Up to 128 characters after cursor (empty if at end-of-document).
- **Max tokens**: 64.
- **Thinking**: Disabled (`thinking: { type: 'disabled' }` -- DeepSeek v4 supports this; older versions ignore it).
- **Cancellation**: If the user types again before the RPC resolves, the in-flight request is cancelled (via AbortController) and a new debounce cycle begins.

### 2.2 Long Completions (inspiration)

- **Purpose**: Suggest a larger block of code when the user pauses.
- **Trigger**: 2000 ms idle period (no keystrokes).
- **Prefix**: Last 2048 characters before cursor, optionally including BM25-retrieved context from the current workspace.
- **Suffix**: Up to 512 characters after cursor.
- **Max tokens**: 256.
- **Thinking**: Same disabled policy as short completions.
- **Retrieval (optional)**: If the current session has an active workspace (`sessionWorkspace`), BM25 search retrieves up to 3 relevant chunks from `.loom/retrieval/chunks/` (or equivalent index). These chunks are prepended to the prefix as context comments.

### 2.3 Why Not OpenAI / Anthropic for FIM?

Neither OpenAI nor Anthropic expose a public FIM API. Their chat completions API can be coerced into FIM via prompt engineering, but:
- Latency is 2-5x worse than DeepSeek's FIM endpoint.
- Cost is significantly higher for per-keystroke usage.
- Coerced prompts produce lower-quality fills.

DeepSeek is the **only** cloud provider with a native, documented FIM endpoint suitable for sub-second completions. Local models (LM Studio / Ollama) that support FIM can be added later via the same `completion.fim` RPC method; the design reserves space for a `provider` field in the request.

---

## 3. Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────────┐
│                         RENDERER (Electron)                         │
│                                                                     │
│  ┌─────────────────────┐        ┌──────────────────────────────┐   │
│  │   CodeMirrorInput   │───────▶│   completion store slice      │   │
│  │   (EditorView)      │  state │   - loading: short|long|idle  │   │
│  │                     │◀───────│   - pendingAbort: AbortCtrl   │   │
│  │   - docChanged      │        │   - lastCompletion: string    │   │
│  │   - ViewPlugin      │        │   - mode: 'off'|'short'|     │   │
│  │   - ghost text      │        │     'long'|'both'            │   │
│  │     decoration set   │        └──────────┬───────────────────┘   │
│  └─────────────────────┘                   │                       │
│                                            │ loomRpc()              │
│  ┌─────────────────────┐                   │                       │
│  │   completion.ts     │◀──────────────────┘                       │
│  │   CompletionSource  │                                           │
│  │   (autocomplete ext)│  debounce 300ms / idle 2s                 │
│  └─────────────────────┘                                           │
└─────────────────────────────────────────────────────────────────────┘
                                    │
                              WebSocket
                                    │
┌───────────────────────────────────┼─────────────────────────────────┐
│                          BACKEND (loom-server)                      │
│                                   │                                 │
│  ┌────────────────────┐           ▼                                 │
│  │ dispatch/mod.rs    │  match "completion.fim"                     │
│  │ dispatch/completion│  new sub-handler                            │
│  └────────┬───────────┘                                             │
│           │                                                         │
│  ┌────────▼───────────┐                                             │
│  │ FimService         │  resolve model config                       │
│  │ - resolve_model()  │  route to DeepSeek FIM client               │
│  │ - build_prompt()   │  format prefix/suffix                       │
│  │ - parse_response() │  extract completion text                    │
│  └────────┬───────────┘                                             │
│           │                                                         │
│  ┌────────▼───────────┐     ┌──────────────────────────┐            │
│  │ loom-inference     │     │ loom-memory (optional)    │            │
│  │ DeepSeekFimClient  │     │ Bm25Retriever            │            │
│  │ POST /fim/         │     │ search workspace chunks  │            │
│  │   completions      │     └──────────────────────────┘            │
│  └────────────────────┘                                             │
└─────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
                     ┌──────────────────────────┐
                     │  DeepSeek API            │
                     │  POST /v1/fim/completions│
                     │  { prompt, suffix,       │
                     │    max_tokens, ... }     │
                     └──────────────────────────┘
```

---

## 4. Backend Design

### 4.0 Pre-Implementation Verification

Before implementing the `FimService`, verify the following:

1. **Check if `Orchestrator` exposes a public `model_configs()` method** or equivalent accessor. Search the `loom-core` crate for the `Orchestrator` struct and its public API surface.

2. If the method exists, **document its return type** and how to filter for DeepSeek backend models:
   ```rust
   // Expected usage pattern:
   let configs: Vec<&ModelConfig> = state.orchestrator.get_model_configs();
   let deepseek_model = configs.iter().find(|c| c.backend == ModelBackend::DeepSeek);
   ```

3. If no such method exists, **add one** to `Orchestrator`:
   ```rust
   /// Returns all configured models with their backends.
   pub fn get_model_configs(&self) -> Vec<&ModelConfig> {
       self.model_configs.values().collect()
   }
   ```

4. The `FimService` uses this to find a DeepSeek model:
   ```rust
   let configs = state.orchestrator.get_model_configs();
   let deepseek_config = configs.iter().find(|c| c.backend == ModelBackend::DeepSeek);
   ```

### 4.1 New JSON-RPC Method: `completion.fim`

#### Request

```json
{
  "jsonrpc": "2.0",
  "method": "completion.fim",
  "params": {
    "prefix": "fn main() {\n    println!(\"",
    "suffix": "\");\n}",
    "mode": "short | long",
    "max_tokens": 64,
    "temperature": 0.0,
    "language": "rust",
    "file_path": "/home/user/project/src/main.rs",
    "retrieval_context": [
      { "file": "lib.rs", "chunk": "pub fn helper() { ... }", "score": 0.87 }
    ],
    "provider": "deepseek | lmstudio | auto"
  }
}
```

| Param | Type | Required | Default | Description |
|-------|------|:--------:|---------|-------------|
| `prefix` | string | yes | -- | Text before cursor (last N chars) |
| `suffix` | string | yes | -- | Text after cursor (first N chars) |
| `mode` | string | no | `"short"` | `"short"` or `"long"` |
| `max_tokens` | number | no | 64 | Max completion tokens |
| `temperature` | number | no | 0.0 | Sampling temperature (0 = greedy) |
| `language` | string | no | `""` | File language hint |
| `file_path` | string | no | `""` | Absolute path for context |  
| `retrieval_context` | array | no | `[]` | BM25 chunks from frontend |
| `provider` | string | no | `"auto"` | `"deepseek"`, `"lmstudio"`, `"auto"` |

`provider: "auto"` logic: scan `models[]` for the first `backend: "DeepSeek"` model. If none found, scan for any model with `api_format: "fim"` capability flag. If still none, return error.

#### Response (Success)

```json
{
  "jsonrpc": "2.0",
  "result": {
    "ok": true,
    "completion": "\"Hello, world!\");",
    "model": "deepseek-chat",
    "mode": "short",
    "usage": {
      "prompt_tokens": 12,
      "completion_tokens": 5
    },
    "latency_ms": 187
  },
  "id": 1
}
```

#### Response (No Completion / Error)

```json
{
  "jsonrpc": "2.0",
  "result": {
    "ok": false,
    "completion": null,
    "message": "No FIM-capable model configured. Add a DeepSeek model in Settings.",
    "mode": "short"
  },
  "id": 1
}
```

Errors are **never** returned as JSON-RPC errors for this method; they are always returned as `{ ok: false, message }` to avoid tripping global error handling in the frontend. The only JSON-RPC error case is `MethodNotFound` if the dispatch module is not registered.

### 4.2 New Dispatch Handler: `dispatch/completion.rs`

File: `backend/crates/loom-server/src/dispatch/completion.rs`

```rust
pub async fn handle(
    state: &AppState,
    method: &str,
    p: &Value,
) -> Option<Result<Value, JsonRpcError>> {
    match method {
        "completion.fim" => Some(handle_completion_fim(state, p).await),
        _ => None,
    }
}
```

Registered in `dispatch/mod.rs` after the `chat` handler (since it is the second most latency-sensitive path):

```rust
if let Some(result) = completion::handle(state, method, &p).await {
    return result;
}
```

### 4.3 FimService (New)

File: `backend/crates/loom-server/src/services/fim.rs` (or inline in `dispatch/completion.rs` for v1)

Responsibilities:
1. **Resolve model**: Scan `state.orchestrator.model_configs()` for a DeepSeek model.
2. **Build prompt**: Format `prefix` + `suffix` according to DeepSeek's FIM spec (see Section 4.4).
3. **Call FIM endpoint**: Send `POST {base_url}/fim/completions` with the `reqwest` HTTP client (NOT through `CloudClient` -- the FIM endpoint has a different payload shape than chat completions).
4. **Parse response**: Extract completion text, filter artifacts.
5. **Return**: `{ ok, completion, model, mode, usage, latency_ms }`.

#### DeepSeek FIM API Contract

**Endpoint**: `POST https://api.deepseek.com/v1/fim/completions`

**Request**:
```json
{
  "model": "deepseek-chat",
  "prompt": "fn main() {\n    println!(\"",
  "suffix": "\");\n}",
  "max_tokens": 64,
  "temperature": 0.0,
  "stream": false,
  "stop": ["<|fim_end|>", "<|endoftext|>"]
}
```

**Response** (from DeepSeek docs):
```json
{
  "id": "cmpl-xxx",
  "object": "text_completion",
  "created": 1710000000,
  "model": "deepseek-chat",
  "choices": [
    {
      "index": 0,
      "text": "\"Hello, world!\")",
      "finish_reason": "stop"
    }
  ],
  "usage": {
    "prompt_tokens": 12,
    "completion_tokens": 5,
    "total_tokens": 17
  }
}
```

We extract `choices[0].text` as the completion. The `stop` sequences prevent the model from emitting content beyond the fill region.

#### Streaming (Future)

Streaming FIM completions (`"stream": true`) would use SSE chunks with `choices[0].text` deltas, exactly like chat completions streaming. We defer streaming to v2 since ghost text is all-or-nothing - the completion is rendered only when fully received.

### 4.4 API Key Resolution

The `FimService` needs to extract the API key from the matched model config or the key store.

- **Context**: The `FimService` receives `AppState` which has access to the key store.
- **Primary resolution path**: `state.key_store.read().get("DEEPSEEK_API_KEY")` or from environment variable.
- **If the model config stores its own API key**: use `model_config.api_key.clone()`.
- **Fallback chain**:
  1. `model_config.api_key` — if the matched model config has an API key field
  2. `key_store["DEEPSEEK_API_KEY"]` — from the application key store
  3. `env::var("DEEPSEEK_API_KEY")` — from the environment
- **If no API key found**: return `{ ok: false, message: "No DeepSeek API key configured" }`.

Implementation sketch in `FimService`:

```rust
fn resolve_api_key(state: &AppState, model_config: &ModelConfig) -> Option<String> {
    // 1. Try model config's own API key
    if let Some(key) = &model_config.api_key {
        if !key.is_empty() {
            return Some(key.clone());
        }
    }
    // 2. Try key store
    if let Some(key) = state.key_store.read().get("DEEPSEEK_API_KEY") {
        if !key.is_empty() {
            return Some(key.clone());
        }
    }
    // 3. Try environment variable
    if let Ok(key) = std::env::var("DEEPSEEK_API_KEY") {
        if !key.is_empty() {
            return Some(key);
        }
    }
    None
}
```

### 4.5 FIM Prompt Format

We use a simple TextIDE-style format that DeepSeek's FIM models expect:

```
<|fim_prefix|>fn main() {
    println!("<|fim_suffix|>");
}<|fim_middle|>
```

DeepSeek's FIM endpoint uses special tokens:
- `<|fim_prefix|>` — marks the beginning of prefix context
- `<|fim_suffix|>` — separates prefix from suffix
- `<|fim_middle|>` — marks where the model should generate

However, DeepSeek's public `/fim/completions` API **accepts `prompt` and `suffix` as separate fields** in the JSON body (see Section 4.3). The internal tokenization is handled server-side. Our Rust code sends `prompt` and `suffix` as separate JSON string fields -- no manual token insertion required.

For `language` hints, we prepend a language comment to the prompt:
```
// language: rust
```

### 4.6 BM25 Retrieval Service (Optional, Phase 2)

For long completions, the frontend may pre-fetch workspace context via the existing KG search (already at `loomRpc('kg.search', ...)`) and pass results as `retrieval_context`. The backend's FIM handler simply injects these chunks as `// context:` comments in the prefix.

In Phase 2, a dedicated `retrieval.search` RPC method could be added that wraps:
- BM25 tokenizer over workspace `.loom/chunks/` directory
- TF-IDF ranking with file-path boosting
- Deduplication by chunk hash

The frontend calls this method during the 2000 ms idle window (in parallel with the completion request), and if results arrive before the RPC timeout, they are appended to the next request.

---

## 5. Frontend Design

### 5.1 New Dependency

Add to `frontend/package.json`:
```json
"@codemirror/autocomplete": "^6.18.0"
```

The `@codemirror/autocomplete` package provides `CompletionSource`, `acceptCompletion`, `currentCompletions`, and the `autocompletion()` extension. We use it for the underlying machinery but render ghost text via our own `ViewPlugin` (not via CodeMirror's pop-up completion list).

### 5.2 CodeMirrorInput Component

File: `frontend/src/renderer/src/components/input/CodeMirrorInput.tsx`

This is a **drop-in replacement** for the current `<textarea>` in `InputArea.tsx`. It wraps a `@codemirror/view` `EditorView` inside a React component using `useRef` + `useEffect`.

```tsx
// Key structure (pseudo-code)
function CodeMirrorInput({ value, onChange, onSend, ... }) {
  const editorRef = useRef<HTMLDivElement>(null)
  const viewRef = useRef<EditorView | null>(null)
  const completionPlugin = useFimCompletion({ viewRef })

  useEffect(() => {
    // 1. Create EditorView with extensions:
    //    - markdown() or minimal setup
    //    - completionPlugin (includes CompletionSource + ViewPlugin)
    //    - EditorView.updateListener.of(...) for syncing React state
    // 2. Attach to editorRef.current
    // 3. Return () => view.destroy()
  }, [])

  return <div ref={editorRef} className={styles.cmEditor} />
}
```

**Key design decisions**:

1. **Not replacing InputArea.tsx wholesale**: v1 adds CodeMirrorInput as a new component. InputArea renders either the textarea or the CodeMirror editor based on a feature flag. This minimizes regression risk.

2. **State sync**: The CodeMirror editor's `doc.toString()` is synced to React `text` state via `EditorView.updateListener` on every transaction. React state still drives `sendMessage()`. This avoids touching the send path at all.

3. **Keyboard handling**: Existing `handleKeyDown` logic (Enter/Ctrl+Enter/Shift+Enter for send) is moved to a CodeMirror `keymap` binding that maps to the same `handleSend` callback.

4. **Paste handling**: CodeMirror handles paste natively; we retain the `handlePaste` logic for image pasting by intercepting `EditorView.domEventHandlers`.

5. **Auto-resize**: CodeMirror's `EditorView` can auto-grow via CSS `max-height` + `overflow-y: auto` on the `.cm-editor` wrapper.

### 5.3 Completion Source

File: `frontend/src/renderer/src/services/completion.ts`

The completion source is a `CompletionSource` function registered with `@codemirror/autocomplete`:

```ts
async function fimCompletionSource(context: CompletionContext): Promise<CompletionResult | null> {
  const { state, pos } = context
  const store = useStore.getState()

  // Guard: FIM only active in chat mode (not write mode)
  const appMode = useStore.getState().appMode
  if (appMode !== 'chat') return null

  // 1. Guard: feature flag and model availability
  if (!store.fimEnabled) return null
  if (store.fimLoading === 'off') return null

  // 2. Extract prefix and suffix
  const prefix = state.doc.slice(Math.max(0, pos - 2048), pos).toString()
  const suffix = state.doc.slice(pos, Math.min(state.doc.length, pos + 512)).toString()

  // 3. Determine mode from timers
  const mode = store.fimPendingMode // 'short' | 'long'

  // 4. Prepare request
  const controller = new AbortController()
  store.setFimAbortController(controller)

  // 5. Call RPC
  try {
    const result = await loomRpc<FimCompletionResponse>('completion.fim', {
      prefix, suffix, mode,
      max_tokens: mode === 'short' ? 64 : 256,
      temperature: 0,
      language: store.fimLanguage ?? '',
      file_path: store.fimFilePath ?? '',
      retrieval_context: store.fimRetrievalContext ?? [],
    }, { signal: controller.signal }) // Note: would require extending loomRpc to support AbortSignal
  } catch (e) {
    // Aborted or error => return null (no completion)
    return null
  }

  // 6. Parse and return ghost text
  if (!result.ok || !result.completion) return null

  return {
    from: pos,
    to: pos, // ghost text starts at cursor
    options: [{
      label: result.completion,
      displayLabel: result.completion, // rendered as ghost text
      type: 'text',
      boost: 0,
    }],
    filter: false,
  }
}
```

**Key design decisions**:

1. **We do NOT use CodeMirror's built-in autocomplete popup**. The popup is designed for symbol completion, not ghost text. Instead we register a `ViewPlugin` that renders the completion as a dimmed decoration inline.

2. **AbortController integration**: The existing `loomRpc` function does not support `AbortSignal`. We have two options:
   - (Recommended) Add an optional `signal?: AbortSignal` parameter to `loomRpc`. When `signal.aborted`, reject the promise with `AbortError`. When the WebSocket is queued (not yet open), attach the signal listener to abort the queued send.
   - (Simpler) Track `pendingAbort` in the completion store. When a new keystroke arrives, set `pendingAbort = true`. The RPC handler checks this flag after the response arrives and discards the result.

   We choose the second option for v1 because modifying the RPC transport is high-risk. The completion store tracks an `abortGeneration` counter: each new keystroke increments it, and the RPC handler only processes responses whose generation matches the current counter.

### 5.4 Ghost Text Rendering (ViewPlugin)

File: `frontend/src/renderer/src/services/completion-ghost.ts`

A CodeMirror `ViewPlugin` that manages a `DecorationSet` for ghost text:

```ts
const ghostTextMark = Decoration.mark({
  class: 'cm-ghost-text',
  inclusiveEnd: false,
})

class GhostTextPlugin {
  decorations: DecorationSet

  constructor(view: EditorView) {
    this.decorations = Decoration.none
  }

  update(update: ViewUpdate) {
    // On any doc change, clear ghost text
    if (update.docChanged) {
      this.decorations = Decoration.none
      return
    }

    // Check store for new completion
    const store = useStore.getState()
    if (store.fimGhostText && store.fimGhostPos === update.view.state.selection.main.head) {
      const { from, text } = store.fimGhostText
      // Create a widget decoration that renders dimmed text after the cursor
      const widget = Decoration.widget({
        widget: new GhostTextWidget(text),
        side: 1, // after cursor
      })
      this.decorations = Decoration.set([widget.range(from)])
    } else {
      this.decorations = Decoration.none
    }
  }
}
```

**GhostTextWidget class implementation** (`GhostTextWidget` used in the `Decoration.widget` above):

```typescript
class GhostTextWidget extends WidgetType {
  constructor(readonly text: string) { super() }
  
  toDOM(): HTMLElement {
    const span = document.createElement('span')
    span.className = 'cm-ghost-text'
    span.textContent = this.text
    span.style.cssText = `
      color: var(--color-text-muted, #888);
      font-style: italic;
      opacity: 0.6;
      pointer-events: none;
      user-select: none;
    `
    return span
  }
  
  eq(other: GhostTextWidget): boolean {
    return this.text === other.text
  }
  
  // Optional: make ghost text unselectable
  ignoreEvent(): boolean { return true }
}
```

Note: CSS uses `var(--color-text-muted)` for theme compatibility. `pointer-events: none` and `user-select: none` ensure ghost text does not interfere with cursor positioning or text selection.

**Ghost text styling** (CSS):
```css
.cm-ghost-text {
  color: var(--color-text-muted);
  opacity: 0.45;
  font-style: italic;
  pointer-events: none;
  user-select: none;
}
```

**Acceptance**: When the user presses **Tab** while ghost text is visible, the completion is committed into the document (via `view.dispatch({ changes: { from, insert: text } })`). This is handled by a `keymap` binding with `Prec.highest` priority so it fires before any other Tab handler.

**Dismissal**: Ghost text is cleared automatically on any document change. It is also cleared explicitly when:
- The user presses Escape
- The cursor moves (selection changes)
- 10 seconds elapse with no acceptance (TTL timer)

### 5.5 Completion Store Slice

File: `frontend/src/renderer/src/stores/completion.ts`

```ts
import { StateCreator } from 'zustand'

export type FimMode = 'off' | 'short' | 'long' | 'both'
export type FimLoading = 'idle' | 'short' | 'long'

export interface FimRetrievalChunk {
  file: string
  chunk: string
  score: number
}

export interface CompletionSlice {
  // Settings
  fimEnabled: boolean
  fimMode: FimMode
  fimLanguage: string | null
  fimFilePath: string | null

  // Runtime state
  fimLoading: FimLoading
  fimAbortGeneration: number
  fimRetrievalContext: FimRetrievalChunk[]
  fimGhostText: { from: number; text: string; generation: number } | null
  fimGhostPos: number | null

  // Actions
  setFimEnabled: (v: boolean) => void
  setFimMode: (v: FimMode) => void
  setFimLoading: (v: FimLoading) => void
  incrementFimAbortGeneration: () => number
  setFimRetrievalContext: (ctx: FimRetrievalChunk[]) => void
  setFimGhostText: (gt: { from: number; text: string; generation: number } | null) => void
  clearFimGhost: () => void
}

export const createCompletionSlice: StateCreator<CompletionSlice> = (set) => ({
  fimEnabled: false,
  fimMode: 'both',
  fimLanguage: null,
  fimFilePath: null,
  fimLoading: 'idle',
  fimAbortGeneration: 0,
  fimRetrievalContext: [],
  fimGhostText: null,
  fimGhostPos: null,

  setFimEnabled: (v) => set({ fimEnabled: v }),
  setFimMode: (v) => set({ fimMode: v }),
  setFimLoading: (v) => set({ fimLoading: v }),
  incrementFimAbortGeneration: () => {
    const gen = Date.now()
    set({ fimAbortGeneration: gen })
    return gen
  },
  setFimRetrievalContext: (ctx) => set({ fimRetrievalContext: ctx }),
  setFimGhostText: (gt) => set({ fimGhostText: gt }),
  clearFimGhost: () => set({ fimGhostText: null, fimGhostPos: null }),
})
```

Registered in `stores/index.ts` alongside the other slices.

---

## 6. Debounce and Trigger Logic (State Machine)

```
                            ┌──────────────────┐
                            │      IDLE        │
                            │  (no ghost text) │
                            └───┬────────┬─────┘
                    keystroke  │        │  2s idle timer fires
                    (min 3     │        │  (no keystroke since
                     char      │        │   last debounce)
                     prefix)   │        │
                      ┌────────▼──┐  ┌──▼──────────────┐
                      │ SHORT_WAIT│  │  LONG_WAIT      │
                      │ (300ms    │  │  (immediate     │
                      │  timer)   │  │   send)         │
                      └───┬───┬───┘  └──┬──────────────┘
            keystroke     │   │300ms   │
            (reset timer) │   │fires   │
                          │   │        │
                          │  ┌▼────────▼──────┐
                          │  │    LOADING      │
                          │  │  (RPC in flight) │
                          │  └───┬─────────┬───┘
                          │      │ success  │ error/timeout
                          │      │          │
                          │  ┌───▼────┐  ┌──▼───┐
                          │  │ GHOST  │  │ IDLE │
                          │  │ ACTIVE │  └──────┘
                          │  └───┬────┘
                          │      │ Tab → accept
                          │      │ Esc → dismiss
                          │      │ keystroke → dismiss + restart SHORT_WAIT
                          │      │
                          └──────┘ (restart cycle)
```

### Debounce Timer Implementation

The debounce is implemented via the `CompletionSource`'s built-in `delay` option in `@codemirror/autocomplete`. However, since we have two tiers with different delays, we manage this in user-space:

```ts
// In the CompletionSource:
let shortTimer: ReturnType<typeof setTimeout> | null = null
let longTimer: ReturnType<typeof setTimeout> | null = null

async function fimCompletionSource(context: CompletionContext) {
  const store = useStore.getState()

  // Short completion: debounce 300ms
  clearTimeout(shortTimer!)
  shortTimer = setTimeout(() => {
    store.setFimLoading('short')
    triggerCompletion('short')
  }, 300)

  // Long completion: debounce 2000ms, only if not already loading
  clearTimeout(longTimer!)
  if (store.fimLoading === 'idle' || store.fimLoading === 'short') {
    longTimer = setTimeout(() => {
      store.setFimLoading('long')
      triggerCompletion('long')
    }, 2000)
  }

  // Signal to CodeMirror that we're waiting (prevents default autocomplete)
  return null
}
```

**Note**: The `CompletionSource` in `@codemirror/autocomplete` expects synchronous return of `CompletionResult | null`. Our actual RPC calls are async and happen outside the `CompletionSource`. The source function is purely for debounce orchestration. The actual RPC call is triggered from the setTimeout callbacks, which update the store, which the ViewPlugin observes.

---

## 7. Policy System

### 7.1 Acceptance/Rejection Criteria

The policy system is kept **simple** in v1. Complex policies (instruction-based filtering, score thresholds) are deferred to v2.

**v1 Policy** (hardcoded, configurable via settings.json blob):

```json
{
  "fim": {
    "min_prefix_length": 3,
    "max_prefix_tokens_short": 512,
    "max_prefix_tokens_long": 2048,
    "max_suffix_tokens_short": 128,
    "max_suffix_tokens_long": 512,
    "reject_patterns": [
      "<|fim_end|>",
      "<|endoftext|>",
      "I apologize",
      "I'm sorry",
      "As an AI"
    ],
    "reject_exact_matches": true,
    "max_completion_repetition": 3,
    "min_accept_score": 0
  }
}
```

| Policy | Description |
|--------|-------------|
| `min_prefix_length` | Minimum characters of prefix before triggering |
| `max_prefix_tokens_*` | Character limits for prefix window |
| `max_suffix_tokens_*` | Character limits for suffix window |
| `reject_patterns` | Substrings that cause instant rejection |
| `reject_exact_matches` | Reject if completion == existing suffix prefix |
| `max_completion_repetition` | Reject if completion contains the same token >= N times |
| `min_accept_score` | Reserved for v2 scoring |

**Post-processing on the backend** (in `FimService::parse_response`):

1. Strip leading/trailing whitespace.
2. Check `reject_patterns`. If any match, return `ok: false`.
3. Check for repetition (> `max_completion_repetition` consecutive identical tokens).
4. Truncate at `max_tokens` token boundary (approximated via character count / 4 for non-streaming).
5. Return.

### 7.2 Feature Flag

Feature flag stored in `AppConfig.settings.fim` (the flexible JSON blob):

```json
{
  "settings": {
    "fim": {
      "enabled": false,
      "mode": "both",
      "short_debounce_ms": 300,
      "long_debounce_ms": 2000
    }
  }
}
```

The `enabled` flag is read by:
- Frontend: `useStore.getState().fimEnabled` controls whether the `CompletionSource` activates.
- Backend: `state.config().settings.fim.enabled` controls whether `completion.fim` returns `ok: false` immediately (soft-disable at server level).

The flag can be toggled at runtime via the Settings UI without restarting the app.

---

## 8. Data Flow: Keystroke to Ghost Text

```
1. User types character in CodeMirrorInput
   │
2. EditorView fires `docChanged` transaction
   │
3. ViewPlugin.update() clears any existing ghost text decoration
   │
4. CompletionSource is re-evaluated by @codemirror/autocomplete
   │
5. CompletionSource sets 300ms timer (short) and 2000ms timer (long)
   │
6a. 300ms timer fires (no intervening keystroke):
   │   ├─ Extract prefix (last 512 chars before cursor)
   │   ├─ Extract suffix (first 128 chars after cursor)
   │   ├─ Set fimLoading = 'short', increment abortGeneration
   │   ├─ Call loomRpc('completion.fim', { prefix, suffix, mode: 'short', max_tokens: 64 })
   │   │
   │   │   ┌─ WebSocket ─▶ loam-server ─▶ dispatch/completion ─▶ FimService
   │   │   │                                                        │
   │   │   │   POST https://api.deepseek.com/v1/fim/completions     │
   │   │   │   { prompt, suffix, max_tokens: 64, temperature: 0 }   │
   │   │   │                                                        │
   │   │   └─ response: { ok, completion, model, mode, usage }   ◀──┘
   │   │
   │   ├─ Check abortGeneration matches current (discard if stale)
   │   ├─ Parse completion, apply reject_patterns
   │   ├─ If valid: set fimGhostText = { from: cursorPos, text: completion, generation }
   │   ├─ If invalid/no-op: set fimLoading = 'idle'
   │   │
   │   └─ ViewPlugin update cycle detects fimGhostText change
   │      └─ Creates Decoration.widget at cursor position with GhostTextWidget
   │         └─ Renders dimmed, italic text inline after cursor
   │
6b. 2000ms timer fires (no intervening keystroke for 2s):
   │   └─ Same flow but mode: 'long', max_tokens: 256,
   │      optional BM25 context prepended
   │
7. User either:
   ├─ Presses Tab → keymap accepts ghost text
   │   └─ view.dispatch({ changes: { from: pos, insert: text } })
   │      └─ clearFimGhost()
   │
   ├─ Types another character → GhostTextPlugin clears on docChanged
   │   └─ Cycle restarts at step 2
   │
   └─ Presses Escape → explicit clearFimGhost()
```

---

## 9. File Manifest

### 9.1 New Files

| File | Language | Purpose |
|------|----------|---------|
| `backend/crates/loom-server/src/dispatch/completion.rs` | Rust | `completion.fim` RPC handler |
| `backend/crates/loom-server/src/services/fim.rs` | Rust | FimService: model resolution, FIM prompt building, DeepSeek API call, response parsing |
| `backend/crates/loom-inference/src/fim.rs` | Rust | (Optional, v2) `DeepSeekFimClient` as a separate struct implementing a `FimClient` trait |
| `frontend/src/renderer/src/components/input/CodeMirrorInput.tsx` | TSX | CodeMirror editor wrapper component |
| `frontend/src/renderer/src/components/input/CodeMirrorInput.module.css` | CSS | Styles for CodeMirror editor and ghost text |
| `frontend/src/renderer/src/services/completion.ts` | TS | CompletionSource + debounce orchestration + RPC call logic |
| `frontend/src/renderer/src/services/completion-ghost.ts` | TS | GhostTextPlugin (ViewPlugin + Decoration.widget) |
| `frontend/src/renderer/src/stores/completion.ts` | TS | Zustand store slice for FIM state |

### 9.2 Modified Files

| File | Change |
|------|--------|
| `frontend/package.json` | Add `@codemirror/autocomplete` dependency |
| `frontend/src/renderer/src/stores/index.ts` | Register `CompletionSlice` in `AppStore` type and `createCompletionSlice` in store factory |
| `frontend/src/renderer/src/components/input/InputArea.tsx` | Conditionally render `CodeMirrorInput` vs `<textarea>` based on feature flag |
| `frontend/src/renderer/src/services/jsonrpc.ts` | (Optional) Add `AbortSignal` support to `loomRpc` |
| `backend/crates/loom-server/src/dispatch/mod.rs` | Register `completion::handle` |
| `backend/crates/loom-server/src/lib.rs` | (If needed) Register FimService in AppState |
| `backend/crates/loom-types/src/config/mod.rs` | (Optional) Add `FimConfig` struct to `AppConfig` |

---

## 10. Testing Strategy

### 10.1 Unit Tests

| Layer | Test | Method |
|-------|------|--------|
| `fim.rs` (backend) | Prompt formatting with empty/full prefix+suffix | `#[test]` |
| `fim.rs` | Response parsing: valid completion, empty, error | `#[test]` |
| `fim.rs` | Reject pattern matching | `#[test]` |
| `fim.rs` | Repetition detection (3+ identical tokens) | `#[test]` |
| `completion.ts` (frontend) | Debounce timer orchestration (mock timers) | `vitest` |
| `completion.ts` | Abort generation discarding stale responses | `vitest` |
| `completion-ghost.ts` | Ghost text decoration creation/clearing | `vitest` |
| `completion store` | State transitions: idle -> loading -> ghost | `vitest` |

### 10.2 Integration Tests

| Test | Description |
|------|-------------|
| `completion.fim` round-trip | Mock DeepSeek endpoint with WireMock (or `reqwest` test server), verify full RPC cycle |
| CodeMirrorInput + completion | Render `CodeMirrorInput` in jsdom, simulate keystrokes, verify ghost text appears |
| Websocket reconnect | Kill backend, verify completion gracefully degrades (returns `ok: false` instead of crashing) |

### 10.3 Manual Testing Checklist

- [ ] Short completion appears ~500ms after typing, disappears on next keystroke
- [ ] Long completion appears ~2.5s after pause
- [ ] Tab accepts completion, text is committed to editor and React state
- [ ] Escape dismisses ghost text
- [ ] Feature flag OFF: no completions triggered, no RPCs sent
- [ ] No DeepSeek model configured: `ok: false` returned with helpful message
- [ ] Rapid typing: abort generation prevents stale completions from rendering
- [ ] Offline: graceful `ok: false` or timeout, no error toast

---

## 11. Rollout Plan

### Phase 1 — Core (Week 1)

**Days 1-2: Backend**
- [ ] Add `dispatch/completion.rs` with `completion.fim` handler
- [ ] Implement `FimService` (model resolution, DeepSeek FIM API call, response parsing)
- [ ] Add reject pattern logic to `FimService::parse_response`
- [ ] Register handler in `dispatch/mod.rs`
- [ ] Add unit tests for FimService

**Days 3-4: Frontend Core**
- [ ] Install `@codemirror/autocomplete`
- [ ] Create `CodeMirrorInput.tsx` with basic editor, React state sync, keyboard bindings
- [ ] Create `completion.ts` with debounce logic and RPC call
- [ ] Create `completion-ghost.ts` with ViewPlugin for ghost text rendering
- [ ] Create `completion.ts` store slice
- [ ] Allow disabling via `AppConfig.settings.fim.enabled`

**Day 5: Integration**
- [ ] Wire InputArea to conditionally render CodeMirrorInput
- [ ] End-to-end test: keystroke -> ghost text -> Tab accept
- [ ] Add `use-esc` keybinding for dismissal
- [ ] Add TTL timer (10s) for ghost text auto-dismiss

### Phase 2 — Polish (Week 2)

**Days 6-8: Long Completions + Retrieval**
- [ ] Implement 2000ms idle trigger
- [ ] Add BM25 context retrieval from `kg.search` results
- [ ] Add `max_tokens: 256` path

**Days 9-10: UX Polish**
- [ ] Ghost text animation (fade in)
- [ ] Settings UI toggle for FIM (mode: off/short/long/both)
- [ ] Reject pattern configurability via AppConfig.settings.fim
- [ ] Edge case handling: cursor at boundaries, empty document, multi-byte characters
- [ ] Add streaming mode (SSE) for very long completions
- [ ] Revert to textarea gracefully if CodeMirror fails to initialize

### Phase 3 — Future (Post v1)

- [ ] Local model FIM support (LM Studio/Ollama models with FIM capability flag)
- [ ] Multi-line ghost text (block widget instead of inline)
- [ ] Completion history / analytics (acceptance rate)
- [ ] Context-aware completions based on active file type
- [ ] Extend to the full code editor (not just chat input) via the existing LSP integration
- [ ] Adaptive debounce (adjust delay based on acceptance rate)

---

## 12. Cross-Feature Integration

### 12.1 FIM and App Modes

FIM completions are exclusive to **Chat mode**. In **Write mode** (`appMode === 'write'`), the `WriteInlineAgent` handles AI-assisted editing via `005-inline-edit-session-scope`, and FIM completions are disabled.

The FIM `CompletionSource` guards against write mode at the top of the function:

```typescript
const appMode = useStore.getState().appMode
if (appMode !== 'chat') return null
```

### 12.2 FIM and WriteMarkdownEditor

The 004 FIM ViewPlugin must NOT activate in 005's `WriteMarkdownEditor`. The `CompletionSource` function checks `appMode` before proceeding; in write mode it returns `null` immediately, preventing any ghost text decorations from being created.

---

## Appendix A: Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| DeepSeek FIM API rate limiting | Medium | High | Client-side rate limit: max 1 request per 500ms regardless of debounce |
| CodeMirror performance with large documents | Low | Medium | Prefix window capped at 2048 chars; avoid full-doc sync |
| React state sync lag with CodeMirror | Medium | Medium | Use `EditorView.updateListener` with batched `requestAnimationFrame` |
| WebSocket congestion from completion traffic | Low | Low | FIM requests are small (<5KB payload); separate from chat.send stream |
| Ghost text rendering glitches with IME | Medium | Medium | Disable FIM during IME composition (check `view.composing`) |
| User confusion about ghost text | High | Low | Tooltip on first appearance; clear visual distinction (dimmed + italic) |

## Appendix B: DeepSeek FIM API Reference

- **Endpoint**: `POST https://api.deepseek.com/v1/fim/completions`
- **Auth**: `Authorization: Bearer {api_key}`
- **Request body**: `{ model, prompt, suffix, max_tokens, temperature, stream, stop }`
- **Response**: `{ id, object, created, model, choices: [{ index, text, finish_reason }], usage }`
- **FIM special tokens**: `<|fim_prefix|>`, `<|fim_suffix|>`, `<|fim_middle|>`, `<|fim_end|>`
- **Thinking suppression**: `{ thinking: { type: 'disabled' } }` (v4+ only)

# Neutral Review Report — Post-Phase (Features 002, 004, 005)

**Review Type**: Post-Phase Review (3 features)
**Date**: 2026-06-08
**Reviewer**: Neutral Reviewer (Claude Opus)
**Aggregate Decision**: ALL THREE features are APPROVED WITH AMENDMENTS

---

## 1. Executive Summary

This review audits the post-phase implementation of Features 002 (Inline Selection Editor), 004 (FIM Completions), and 005 (Write Mode) against the pre-implementation amendments (REVIEW-001) and the Loom Architecture Invariants from the 007 Neutral Review Framework.

**Feature 002** is the most complete -- all 4 pre-implementation amendments are addressed, Zustand slice pattern is correct, portal rendering and keyboard handling are sound, and integration with InputArea/sendMessage is clean. One gap: quotedSelections are not passed to the backend `chat.send` RPC call.

**Feature 004** is partial -- the backend JSON-RPC handler exists and is correct, but all 4 pre-implementation amendments have issues. The API key comes from RPC params (not model configs), and the entire frontend CodeMirror/FIM integration (CompletionSource, GhostTextWidget, CodeMirrorInput) is absent. AppMode guard is frontend work and not present.

**Feature 005** is the least complete -- 3 of 5 amendments fail outright. The Write IPC layer (`registerWriteIpc`) is completely missing, `createWriteSession` is undefined, binary image upload is unhandled, and the right-panel gating (PlanPanel/TodoPanel behind `appMode === 'chat'`) is only a TODO comment. The core store slice, VFS handlers, mode switching, and autosave are correct.

**Verdict**: APPROVE WITH AMENDMENTS for all three. Feature 002 has 1 gap to resolve. Feature 004 has 4 amendments (2 frontend missing, 2 backend) to address. Feature 005 has 5 amendments (3 missing pieces, 2 unimplemented requirements).

---

## 2. Feature 002 -- Inline Selection Editor

### 2.1 Amendment Compliance

#### Amendment 1: openInlineInput Signature Alignment
**Verdict**: PASS

- Store signature (`selectionContext.ts:48`): `openInlineInput(rect: DOMRect, filePath?: string, startLine?: number, endLine?: number) => void`
- Call site (`App.tsx:180`): `useStore.getState().openInlineInput(rect, filePath, startLine, endLine)`
- 4 parameters match the call. Optional params default correctly. Signature is aligned.

#### Amendment 2: Data-Attribute Injection
**Verdict**: PASS (conditional)

- The amendment required data-attribute injection on code-block DOM elements so the inline selection editor can resolve file paths.
- **Evidence of injection**: `markdown.ts:112` injects `data-file-path` on code blocks. `FileDiffCard.tsx:161-175` injects `data-file-path`, `data-start-line`, `data-end-line`. `markdown-sanitizer.ts:20` allows `data-file-path` through the sanitizer.
- **Evidence of consumption**: `App.tsx:166-178` walks up the DOM looking for these attributes and passes them to `openInlineInput`.
- **Caveat**: Only code blocks rendered via `markdown.ts` and `FileDiffCard` carry these attributes. If message text or quoted replies render as plain HTML without these attributes, selections within them will have `filePath: ''`. This is acceptable for the current phase.

#### Amendment 3: QuotedSelection ID Generation
**Verdict**: PASS

- `addQuotedSelection` in `selectionContext.ts:38`: `const id = crypto.randomUUID()`
- The store action generates IDs internally. The caller (`InlineInput.tsx:37`) calls `addQuotedSelection` without providing an ID.
- This resolves the ambiguity from the pre-implementation review.

#### Amendment 4: setInlineInstructionText Implementation
**Verdict**: PASS

- `setInlineInputText` is implemented in `selectionContext.ts:61`: `setInlineInputText: (text) => set({ inlineInputText: text })`
- The `InlineInput` component uses local React state (`useState('')` at line 19) for the transient textarea value instead of the store's `inlineInputText`.
- This is acceptable. The amendment said "Either implement it or remove it." It is implemented. The component choosing local state for the textarea is architecturally valid -- the store still holds `inlineInputText` as a reset point when `openInlineInput` clears it to `''`.

### 2.2 Architecture Compliance Summary

| Check | Status | Evidence |
|-------|--------|----------|
| Zustand StateCreator pattern | PASS | `selectionContext.ts:28` -- `createSelectionContextSlice: StateCreator<SelectionContextSlice>` |
| Registered in stores/index.ts | PASS | `stores/index.ts:21, 43, 66` |
| Zero cross-slice imports | PASS | SelectionContextSlice only imports `StateCreator` from zustand |
| Portal rendering (createPortal) | PASS | `InlineInput.tsx:66` -- `createPortal(...)` to `document.body` |
| Keyboard handling (Escape/Enter/Shift+Enter) | PASS | `InlineInput.tsx:53-62` -- Escape cancels, Enter confirms, Shift+Enter newline |
| Global Ctrl+Shift+I listener | PASS | `App.tsx:150-185` -- capture phase, checks non-empty selection, DevTools fallback when no selection |
| QuotedSelectionCard in InputArea | PASS | `InputArea.tsx:372-383` -- reads `quotedSelections` from store, renders cards with remove handler |
| sendMessage accepts quotedSelections | PASS | `sendMessage.ts:19, 26, 34-42` -- optional param, defaults to `[]`, serialized as `quoted_selection` blocks |
| QuotedSelectionCard onRemove optional | PASS | `QuotedSelectionCard.tsx:26-33` -- button only rendered if `onRemove` prop provided |
| read-file IPC backward compatible | PASS | `files.ts:24` -- when no options provided, returns full content unchanged |
| InlineInput mounted in App.tsx | PASS | `App.tsx:259` -- `<InlineInput />` alongside ToastContainer, ConfirmDialog |
| No React Router | PASS | Conditionally rendered via store state |
| CSS uses var() tokens | PASS | QuotedSelectionCard uses `var(--bg-card)`, `var(--border)`, `var(--text-muted)`, `var(--text-light)` |

### 2.3 Gaps Found

**Gap F2-1: quotedSelections not passed to backend chat.send RPC**

- File: `sendMessage.ts:108-123`
- The `chat.send` RPC params object does NOT include `quoted_selections`. The blocks are added to the frontend user message (lines 34-42) but are not explicitly included in the RPC payload.
- Impact: The backend cannot process quoted selections as distinct data. It receives them only insofar as they appear in the message history.
- Severity: Medium. The blocks are in the frontend message which is part of the session. However, if the backend needs to do context assembly or token counting on quoted selections specifically, it cannot without this data.

### 2.4 Verdict

**APPROVE WITH AMENDMENTS**

4 of 4 pre-implementation amendments PASS. 1 gap identified (F2-1). Feature 002 is substantially complete.

---

## 3. Feature 004 -- FIM Code Completions

### 3.1 Amendment Compliance

#### Amendment 1: model_configs Verification
**Verdict**: FAIL

- The pre-implementation review required: "Verify that `Orchestrator` exposes a `model_configs()` method, or add a method to retrieve configured models with their backends."
- **Actual implementation** (`completion.rs:19`): `handle_completion_fim` takes `_state: &AppState` -- the state parameter is unused (underscore prefix).
- The backend does NOT scan model configs. Instead, the frontend is expected to pass `api_key`, `model`, and `base_url` as RPC parameters.
- Required fix: Either (a) wire `_state` to actually scan model configs, or (b) document that the frontend is responsible for model resolution and the backend just acts as a proxy.

#### Amendment 2: AppMode Guard in CompletionSource
**Verdict**: GAP (frontend not implemented)

- The pre-implementation review required: "Add an explicit guard in the CompletionSource: `if (useStore.getState().appMode !== 'chat') return null`."
- **Actual implementation**: No `CompletionSource`, `CodeMirrorInput`, or `GhostTextWidget` exists anywhere in the frontend codebase.
- The entire frontend FIM integration layer (CodeMirror EditorView, CompletionSource, GhostTextWidget ViewPlugin, Tab acceptance keymap) is absent.
- This is acknowledged by the user as "frontend work" and the review scope notes: "This is backend-only; guard: appMode check in CompletionSource is frontend work -- note this gap."

#### Amendment 3: DeepSeek API Key Resolution
**Verdict**: FAIL

- The pre-implementation review required: "Specify how the API key is retrieved -- from `ModelConfig.api_key` or equivalent."
- **Actual implementation** (`completion.rs:32-34`): API key extracted from RPC params: `p.get("api_key").and_then(|v| v.as_str()).unwrap_or("")`
- This means the API key is sent from the frontend to the backend with every FIM request, then forwarded to DeepSeek.
- Security concern: The API key travels through the WebSocket in plain JSON. While the WS is localhost, this violates the principle that credentials should be resolved server-side from stored configs.
- Required fix: Resolve API key from the orchestrator's stored model configs, not from RPC params.

#### Amendment 4: GhostTextWidget DOM Implementation
**Verdict**: GAP (frontend not implemented)

- The pre-implementation review required: "Provide the GhostTextWidget class implementation (at minimum: the `toDOM()` method that creates a `<span>` with the ghost text CSS class)."
- **Actual implementation**: No `GhostTextWidget` exists in the codebase. This is part of the unimplemented frontend FIM layer.
- Acknowledged as frontend work.

### 3.2 Architecture Compliance Summary

| Check | Status | Evidence |
|-------|--------|----------|
| completion.fim method correct | PASS | `completion.rs:14` -- `"completion.fim"` matches design spec |
| Registered in dispatch/mod.rs | PASS | `mod.rs:17` (mod declaration), `mod.rs:65-67` (dispatch chain, 2nd position after chat) |
| B-3 Dispatch chain pattern | PASS | `pub async fn handle(state: &AppState, method: &str, p: &Value) -> Option<Result<Value, JsonRpcError>>` at `completion.rs:8-17` |
| B-4 Crate boundary: in loom-server | PASS | `loom-server/src/dispatch/completion.rs` -- NOT in loom-inference |
| B-5 CloudClient exception | PASS (documented) | DeepSeek `/fim/completions` endpoint called directly via reqwest. Exception documented in design doc. |
| Error handling: in-band ok:false | PASS | `completion.rs:46, 82-84, 89` -- all errors return `{ ok: false, message: "..." }`, not JSON-RPC error objects |
| DeepSeek FIM endpoint URL correct | PASS | `completion.rs:59`: `{base_url}/fim/completions` |
| Request body shape correct | PASS | `completion.rs:50-57`: `model`, `prompt` (prefix), `suffix`, `max_tokens`, `temperature: 0.0`, `stream: false` |
| Response parsing correct | PASS | `completion.rs:72`: `json["choices"][0]["text"].as_str()` |
| Non-streaming (v1 design) | PASS | `stream: false` at line 57. No stream handling needed. |
| Timeout handling | PASS | `completion.rs:66` -- 10-second timeout via reqwest |
| Tracing::warn for errors | PASS | `completion.rs:81, 88` -- uses `tracing::warn!`, not println |
| F-3 JSON-RPC frontend | N/A | Frontend FIM layer not implemented. When it is, should use `loomRpc('completion.fim', ...)`. |

### 3.3 Gaps Found

**Gap F4-1: API key resolution from RPC params instead of model configs**
- File: `completion.rs:32-35`
- Severity: High. Security concern. API key travels in WebSocket JSON. Backend should resolve credentials from stored configs.
- Required fix: Use `state.orchestrator` to look up the DeepSeek model config and extract its API key.

**Gap F4-2: Frontend FIM integration layer entirely absent**
- Files: No `CompletionSource`, `CodeMirrorInput`, `GhostTextWidget`, FIM-related store (`completion.ts`) found anywhere in the frontend.
- Severity: High. The backend endpoint exists but the frontend integration (the actual UX -- CodeMirror editor, ghost text, Tab acceptance, debounce orchestrator) is not implemented.
- Required: Create `CompletionSlice` in stores, `CompletionSource` for debounce orchestrator, `GhostTextWidget` ViewPlugin, `CodeMirrorInput` component, `keymap` for Tab acceptance, abort generation counter.

**Gap F4-3: No AppMode guard (frontend concern)**
- The CompletionSource (when implemented) must check `useStore.getState().appMode !== 'chat'` and return null to prevent FIM from activating in Write mode.
- Acknowledged as frontend work.

### 3.4 Verdict

**APPROVE WITH AMENDMENTS**

Backend core is correct and minimal. 0 of 4 pre-implementation amendments fully PASS. 2 FAIL (model_configs, API key resolution). 2 GAP (frontend not implemented). The backend `completion.fim` handler itself is well-structured, error handling is correct, and JSON-RPC integration follows the dispatch chain pattern.

---

## 4. Feature 005 -- Write Mode Workspace

### 4.1 Amendment Compliance

#### Amendment 1: React-Markdown Avoidance
**Verdict**: PASS

- No `react-markdown` found in frontend `package.json` or any source file.
- `WriteWorkspaceView.tsx` uses a plain `<textarea>` (line 58-67) for the editor.
- No preview component using react-markdown. The preview mode selector is present in the UI but the actual preview rendering is deferred.
- This matches the pre-implementation review's recommendation: "Use `dangerouslySetInnerHTML` with `markdown-it` (already a dependency) + the existing `utils/markdown-sanitizer.ts`."

#### Amendment 2: Right-Panel Ownership (Mode-Exclusive Visibility)
**Verdict**: FAIL

- The pre-implementation review required: "Write mode hides PlanPanel/TodoPanel; Chat mode hides WriteAssistantPanel. The right panel is mode-exclusive."
- **Actual implementation** (`AppShell.tsx:133-136`):
  ```tsx
  {/* TODO: gate behind appMode === 'chat' when Feature 005 is merged */}
  <PlanPanel />
  {/* TODO: gate behind appMode === 'chat' when Feature 005 is merged */}
  <TodoPanel />
  ```
- PlanPanel and TodoPanel render unconditionally -- they are visible in Write mode.
- Required fix: Add `{appMode === 'chat' && <PlanPanel />}` and `{appMode === 'chat' && <TodoPanel />}` guards. Remove the TODO comments.

#### Amendment 3: QuotedSelection Type Unification
**Verdict**: PASS

- The pre-implementation review required: "Use a SINGLE `QuotedSelection` type definition. Place it in a shared types file. Both Feature 002 and Feature 005 should import the same type."
- **Actual implementation**: The `WriteSlice` (`write.ts`) does NOT define its own `QuotedSelection` type. The only definition is in `selectionContext.ts:3-10`, which Feature 002 uses.
- `sendMessage.ts:5` imports QuotedSelection from `../stores/selectionContext` -- same source.
- While placing it in the store file is not ideal (the pre-implementation review said "in a shared types file, not in a slice file"), there is only ONE definition, which avoids the type conflict.

#### Amendment 4: createWriteSession Helper
**Verdict**: FAIL

- The pre-implementation review required: "Specify the `createWriteSession` helper: it should call `loomRpc('session.create', { title: 'Write Assistant', metadata: { mode: 'write', workspace_root: root } })`."
- **Actual implementation**: `createWriteSession` is completely absent from the codebase. The `WriteSlice` has no session initialization logic.
- `WriteWorkspaceView.tsx` imports `useStore` but never calls any session creation.
- Required fix: Add `createWriteSession` to the store or as a helper function. This is essential for the Write Assistant thread.

#### Amendment 5: Binary Image Upload
**Verdict**: FAIL

- The pre-implementation review required: "Either (a) add a binary-capable IPC method for image upload, or (b) use the main process IPC directly."
- **Actual implementation**: No image upload handling exists. The `vfs.rs` `write_file` method accepts `content: string` (UTF-8), which cannot handle binary data.
- No `writeWorkspaceImage` or similar IPC method exists anywhere in the codebase.
- Required fix: Add a binary image upload path. The simplest approach: add an IPC handler in the main process that accepts a base64 string + file path and writes the file with `fs.writeFile(filePath, Buffer.from(base64, 'base64'))`.

### 4.2 Architecture Compliance Summary

| Check | Status | Evidence |
|-------|--------|----------|
| Zustand StateCreator pattern | PASS | `write.ts:32` -- `createWriteSlice: StateCreator<WriteSlice>` |
| Zero cross-slice imports | PASS | `write.ts` only imports `StateCreator` from zustand |
| VFS path traversal protection | PASS | `vfs.rs:26-29` -- `canonical.starts_with(&ws_canonical)`, falls back to raw path if canonicalize fails |
| All 6 VFS methods | PASS | `vfs.rs:11-18` -- `vfs.read_file`, `vfs.write_file`, `vfs.list_directory`, `vfs.create_directory`, `vfs.rename`, `vfs.delete` |
| VFS registered in dispatch/mod.rs | PASS | `mod.rs:23` (mod declaration), `mod.rs:95-97` (dispatch chain, position 12 of 15) |
| B-3 Dispatch chain pattern | PASS | `vfs.rs:9-19` follows standard pattern |
| JSON-RPC 2.0 error handling | PASS | All errors use `err(ErrorCode::...)` from the shared helper |
| Mode switching via appMode | PASS | `AppShell.tsx:88` -- `{appMode === 'write' ? <WriteWorkspaceView /> : <ChatWorkspace />}` |
| Write-to-Chat back button | PASS | `WriteWorkspaceView.tsx:31` -- `onClick={() => setAppMode('chat')}` |
| Autosave 650ms debounce | PASS | `WriteWorkspaceView.tsx:11-21` -- 650ms setTimeout, clears on effect cleanup |
| Dirty tracking | PASS | `write.ts:51` -- `setFileContent` sets `saveStatus: 'dirty'`. `markSaved` resets to `saved`. |
| Save status state machine | PASS | `saved -> dirty -> saving -> saved` or `saved -> dirty -> saving -> error` |
| F-5 No React Router | PASS | Mode switching via conditional rendering, not routes |
| CSS uses var() tokens | PASS | WriteWorkspaceView uses `var(--bg-canvas)`, `var(--border)`, `var(--text)`, `var(--text-muted)`, `var(--bg-card)`, `var(--success)`, `var(--warning-soft)`, `var(--accent-soft)`, `var(--font-mono)`, `var(--bg)` |
| appMode location (L-050) | PARTIAL | The pre-implementation review (L-050) said `appMode` should be in `UiSlice`. It is in `WriteSlice` instead (`write.ts:5`). This works because WriteSlice is composed into the store, and AppShell accesses it via `useStore(s => s.appMode)`. Low impact but a deviation from the design. |

### 4.3 Gaps Found

**Gap F5-1: writeIpc handlers entirely missing**
- The design calls for 6 IPC methods: `pickWorkspaceDirectory`, `readWorkspaceImage`, `exportWriteDocument`, `copyWriteDocumentAsRichText`, `watchFile`, `unwatchFile`.
- `registerWriteIpc()` is not defined. `main/ipc/write.ts` does not exist. `main/ipc/index.ts:6` only registers `registerFileIpc()`.
- These IPC methods are essential for: workspace directory selection, image paste, export to PDF/HTML/DOCX, copy as rich text, and file change watching.
- Required fix: Create `frontend/src/main/ipc/write.ts` with the 6 handlers and register them in `main/ipc/index.ts`.

**Gap F5-2: CreateWriteSession undefined**
- See Amendment 4 FAIL above.

**Gap F5-3: Binary image upload unhandled**
- See Amendment 5 FAIL above. `vfs.writeFile` accepts string content only.

**Gap F5-4: Right-panel gating not implemented**
- See Amendment 2 FAIL above. PlanPanel/TodoPanel render unconditionally.

**Gap F5-5: WriteWorkspaceView uses textarea, not CodeMirror 6**
- The design (L-057) specifies: "Does `WriteMarkdownEditor` use the existing `codemirror` + `@codemirror/*` packages (no new editor dependency)?"
- The actual implementation uses a plain `<textarea>` (`WriteWorkspaceView.tsx:58`). A CodeMirror markdown editor with live preview was expected.
- Severity: Medium. A textarea is functional but lacks syntax highlighting, line numbers, and the live preview split view. However, this aligns with Amendment 1 (no react-markdown, using markdown-it) and avoids the CodeMirror dependency conflicts with Feature 004.

**Gap F5-6: No file tree, workspace picker UI**
- The design specifies a file tree that renders directories and markdown files, plus a workspace picker dialog.
- `WriteWorkspaceView.tsx` has no file tree component, no directory listing UI, and no workspace picker button.
- The `workspaceRoot` state exists in the store but there is no UI to populate it.

**Gap F5-7: No preview rendering**
- The `previewMode` selector is present in the toolbar, but only "Source" mode is actually implemented (the textarea).
- There is no live preview (markdown-it rendering), no split view, and no preview-only view.

### 4.4 Verdict

**APPROVE WITH AMENDMENTS**

2 of 5 pre-implementation amendments PASS. 3 FAIL. The core architecture is sound -- Zustand slice, VFS handlers with path traversal protection, mode switching, autosave, and dirt tracking all pass. But significant implementation gaps remain: the entire IPC layer, workspace initialization, image handling, right-panel gating, the file tree, workspace picker, and preview rendering are all incomplete.

---

## 5. Cross-Feature Impact

### 5.1 Shared File Modifications

| File | Modified By | Status |
|------|-------------|--------|
| `stores/index.ts` | 002, 005 | PASS -- SelectionContextSlice and WriteSlice both registered (lines 21-22, 43-44, 66-67). No merge conflicts. |
| `dispatch/mod.rs` | 004, 005 | PASS -- completion (line 65-67) and vfs (line 95-97) both registered. Order is correct: completion before vfs. |
| `main/ipc/index.ts` | 002, 005 | PARTIAL -- 002's read-file extension is present. 005's registerWriteIpc is missing. |
| `App.tsx` | 002 | PASS -- InlineInput mounted, Ctrl+Shift+I listener added. Non-overlapping with 005. |
| `AppShell.tsx` | 005 | PARTIAL -- Mode switching is implemented. Right-panel gating is TODO. |
| `InputArea.tsx` | 002 | PASS -- QuotedSelectionCard rendering added above textarea. No conflict with 004's future CodeMirrorInput. |

### 5.2 Store Slice Count

| Metric | Before | After 002+005 | Limit |
|--------|--------|---------------|-------|
| Zustand slices | 17 | 19 (selection-context + write) | 25 |
| CompletionSlice (004) | -- | Not yet created | -- |

### 5.3 Dispatch Handler Count

| Metric | Before | After 004+005 | Limit |
|--------|--------|---------------|-------|
| Sub-handlers | 12 | 14 (completion + vfs) | 20 |

### 5.4 TypeScript Compilation

All reviewed files are syntactically valid TypeScript. No compilation errors from the reviewed files.

---

## 6. Findings Summary

### 6.1 Blocking Issues (must fix before proceeding)

None. No fundamental architectural conflicts.

### 6.2 Amendments (should fix within 48 hours)

**Feature 002 (1 amendment)**:
- **F2-A1**: Pass `quoted_selections` in the `chat.send` RPC params, or document why the frontend user-message blocks are sufficient. File: `sendMessage.ts:108-123`.

**Feature 004 (2 amendments)**:
- **F4-A1**: Resolve DeepSeek API key from `state.orchestrator` model configs, not from RPC params. File: `completion.rs:19-35`. Remove `api_key` and `base_url` from RPC params; extract from stored config.
- **F4-A2**: Wire `_state` parameter in `handle_completion_fim` to actually use the orchestrator for model config scanning. File: `completion.rs:19`.

**Feature 005 (3 amendments)**:
- **F5-A1**: Implement right-panel mode-exclusive gating. Add `{appMode === 'chat' && <PlanPanel />}` and `{appMode === 'chat' && <TodoPanel />}` guards. Remove TODO comments. File: `AppShell.tsx:133-136`.
- **F5-A2**: Implement `createWriteSession` helper. Call `loomRpc('session.create', { title: 'Write Assistant', metadata: { mode: 'write', workspace_root: root } })` when workspace is initialized. File: `write.ts` or new helper function.
- **F5-A3**: Create `main/ipc/write.ts` with at minimum 3 IPC handlers: `pickWorkspaceDirectory`, `writeWorkspaceImage` (binary), `watchFile`/`unwatchFile`. Register via `registerWriteIpc()` in `main/ipc/index.ts`. Files: `frontend/src/main/ipc/write.ts` (new), `frontend/src/main/ipc/index.ts`.

### 6.3 Suggestions (optional, at implementer's discretion)

- **S-002-1**: Extract `QuotedSelection` type to a shared types file (e.g., `types/quoted-selection.ts`) instead of keeping it in `stores/selectionContext.ts`. Both features 002 and 005 reference the type; a shared location is cleaner.
- **S-004-1**: The frontend FIM integration (CompletionSource, GhostTextWidget, CodeMirrorInput, CompletionSlice) should be implemented in a separate branch and undergo its own mid-phase review. The backend is ready.
- **S-005-1**: Consider adding `appMode` to `UiSlice` as originally designed (L-050) rather than keeping it in `WriteSlice`. Mode management is a UI concern that other features (003, 004) may need to query.
- **S-005-2**: Replace the `<textarea>` in WriteWorkspaceView with a CodeMirror 6 markdown editor (for syntax highlighting, line numbers). Use the existing `@codemirror/*` packages, not a new editor dependency. Ensure CompletionSource from Feature 004 does not activate on this editor instance.

---

## 7. Decision

| Feature | Decision |
|---------|----------|
| 002 -- Inline Selection Editor | **APPROVE WITH AMENDMENTS** (1 amendment) |
| 004 -- FIM Code Completions | **APPROVE WITH AMENDMENTS** (2 amendments + frontend not implemented) |
| 005 -- Write Mode Workspace | **APPROVE WITH AMENDMENTS** (3 amendments; significant implementation gaps) |

## 8. Sign-off

**Reviewer**: Claude Opus (Neutral Reviewer)
**Date**: 2026-06-08
**Next Review**: Mid-Phase or Post-Phase re-review after amendments are addressed. Suggested order: 005 (largest gaps) first, then 004 (frontend layer), then 002 (minor gap).

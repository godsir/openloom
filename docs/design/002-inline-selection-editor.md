# Inline Selection Editor -- Technical Design

**Document ID**: 002-inline-selection-editor
**Status**: Draft
**Date**: 2026-06-08
**Scope**: Frontend + minor IPC changes
**Estimated Effort**: 1 week

---

## 1. Overview

### 1.1 Problem

Users frequently want to ask the agent about a specific piece of text -- a code snippet in a chat message, a paragraph in a document, or a selection in the editor. Today the user must copy-paste the text, manually annotate it with file/line info, and type the instruction into the chat input. This is high-friction and breaks flow.

### 1.2 Solution

The user selects text in any renderer context (chat messages initially; future editor), presses **Ctrl+Shift+I**, and a **floating inline input** appears near the selection. The user types a short instruction (e.g. "refine this", "explain", "translate to English"), hits Enter, and the system sends `{ filePath, startLine, endLine, quotedText }` as structured context to the agent.

### 1.3 User Journey

```
1. User scrolls through a chat message containing a code block or long text.
2. User clicks and drags to select 3-4 lines of code.
3. User presses Ctrl+Shift+I.
4. A floating textarea appears just below/above the selection.
5. User types: "explain this function" and presses Enter.
6. The floating input dismisses. A QuotedSelectionCard appears in the main InputArea.
7. User clicks Send (or presses Enter). The message is sent with the quoted selection
   as a structured context block.
8. The agent receives file+line+text context and responds with targeted analysis.
```

---

## 2. Component Tree

```
App
 +-- AppShell
      +-- ChatWorkspace
      |    +-- MessageList (chat messages)
      |    |    +-- UserMessage          <-- DOM selection source
      |    |    +-- AssistantMessage     <-- DOM selection source
      |    +-- InputArea                 <-- modified: renders QuotedSelectionCard[]
      |         +-- QuotedSelectionCard  (wired, previously unused)
      |         +-- AttachedFiles
      |         +-- <textarea>
      |         +-- toolbar ...
      +-- InlineInputOverlay            <-- NEW: portal, positioned absolute
           +-- InlineInput              <-- floating textarea + backdrop
```

**Key**: `InlineInputOverlay` is rendered at the `App` level (via a React portal to `document.body`) so it floats above all content and is not clipped by `overflow: hidden` on any ancestor.

---

## 3. Data Structures

### 3.1 QuotedSelection (new)

```typescript
// stores/selection-context.ts

export interface SelectionRange {
  /** The DOM selection anchor/range serialized for positioning the floating input */
  anchorNodePath: number[]   // childNode index path from document body
  anchorOffset: number
  focusNodePath: number[]
  focusOffset: number
  /** The selected plain-text content */
  text: string
}

export interface QuotedSelection {
  /** Display text (truncated in UI, full in payload) */
  text: string
  /** Absolute file path (empty for chat-message selections) */
  filePath: string
  /** 1-based start line number (from file or message block metadata) */
  startLine: number
  /** 1-based end line number (inclusive) */
  endLine: number
  /** Character count of the selected text */
  charCount: number
  /** Unique ID for removal tracking */
  id: string
}

export interface InlineInputState {
  /** The floating input is visible */
  visible: boolean
  /** The DOM selection range captured when Ctrl+Shift+I was pressed */
  selectionRange: SelectionRange | null
  /** Current text in the floating input */
  instructionText: string
}
```

### 3.2 Extended ContentBlock Types

```typescript
// stores/chat.ts -- addition to the existing ContentBlock union

/**
 * A quoted_selection block is appended to the user message alongside
 * the existing text / image / file blocks.
 */
export interface QuotedSelectionBlock {
  type: 'quoted_selection'
  text: string           // full quoted text (server gets all of it)
  filePath: string       // '' for inline/chat selections
  startLine: number      // 1-based
  endLine: number        // 1-based (inclusive)
}
```

### 3.3 Extended SendMessageOptions

```typescript
// services/sendMessage.ts -- addition to existing interface

export interface SendMessageOptions {
  // ... existing fields ...
  /**
   * Quoted selections captured via Ctrl+Shift+I.
   * Rendered as QuotedSelectionCard chips in InputArea.
   * Serialized as quoted_selection blocks in the user message.
   */
  quotedSelections?: QuotedSelection[]
}
```

---

## 4. Store Slice Design: SelectionContextSlice

A **new** Zustand slice (`stores/selection-context.ts`) tracks the inline-input lifecycle and the list of captured quoted selections. This is separate from `stores/selection.ts` (which tracks chat message checkbox selection).

```typescript
// stores/selection-context.ts

import { StateCreator } from 'zustand'

export interface SelectionContextSlice {
  // --- Inline Input State ---
  inlineInputVisible: boolean
  inlineInputSelection: SelectionRange | null
  inlineInputAnchorRect: DOMRect | null   // pixel position for floating UI
  inlineInputFilePath: string             // resolved from DOM data attributes
  inlineInputStartLine: number            // resolved from DOM data attributes
  inlineInputEndLine: number              // resolved from DOM data attributes

  // --- Captured Selections (pinned to InputArea) ---
  quotedSelections: QuotedSelection[]

  // --- Actions ---
  openInlineInput: (
    sel: SelectionRange,
    rect: DOMRect,
    filePath?: string,
    startLine?: number,
    endLine?: number
  ) => void
  closeInlineInput: () => void

  addQuotedSelection: (qs: Omit<QuotedSelection, 'id'>) => void
  removeQuotedSelection: (id: string) => void
  clearQuotedSelections: () => void
}

export const createSelectionContextSlice: StateCreator<SelectionContextSlice> = (set, get) => ({
  inlineInputVisible: false,
  inlineInputSelection: null,
  inlineInputAnchorRect: null,
  inlineInputFilePath: '',
  inlineInputStartLine: 1,
  inlineInputEndLine: 1,
  quotedSelections: [],

  openInlineInput: (sel, rect, filePath, startLine, endLine) => set({
    inlineInputVisible: true,
    inlineInputSelection: sel,
    inlineInputAnchorRect: rect,
    inlineInputFilePath: filePath || '',
    inlineInputStartLine: startLine ?? 1,
    inlineInputEndLine: endLine ?? 1,
  }),

  closeInlineInput: () => set({
    inlineInputVisible: false,
    inlineInputSelection: null,
    inlineInputAnchorRect: null,
    inlineInputFilePath: '',
    inlineInputStartLine: 1,
    inlineInputEndLine: 1,
  }),

  addQuotedSelection: (qs) => set(state => ({
    quotedSelections: [...state.quotedSelections, { ...qs, id: crypto.randomUUID() }],
  })),

  removeQuotedSelection: (id) => set(state => ({
    quotedSelections: state.quotedSelections.filter(q => q.id !== id),
  })),

  clearQuotedSelections: () => set({ quotedSelections: [] }),
})
```

**Registration** in `stores/index.ts`:

```typescript
// Add to AppStore type union
export type AppStore = ... & SelectionContextSlice

// Add to create() call
export const useStore = create<AppStore>()((...a) => ({
  ...
  ...createSelectionContextSlice(...a),
}))
```

---

## 5. IPC Changes

### 5.1 Extended `read-file` Handler (Main Process)

`frontend/src/main/ipc/files.ts` -- extend the existing `read-file` handler to accept an optional line-range object. The `readFileSync` approach of reading the whole file is kept for small files; we add line slicing in the handler.

```typescript
// files.ts -- modified read-file handler

ipcMain.handle('read-file', async (_event, filePath: string, options?: {
  startLine?: number   // 1-based, inclusive
  endLine?: number     // 1-based, inclusive
}) => {
  try {
    const full = readFileSync(filePath, 'utf-8')
    if (!options || (options.startLine == null && options.endLine == null)) {
      return full
    }
    const lines = full.split('\n')
    const start = Math.max(0, (options.startLine ?? 1) - 1)
    const end = Math.min(lines.length, (options.endLine ?? lines.length))
    return lines.slice(start, end).join('\n')
  } catch {
    return null
  }
})
```

**Decision**: Extend the existing `read-file` handler rather than creating a new `read-file-range` channel. The options object is optional, so existing callers are unaffected. The preload API signature changes to accept the optional second argument.

### 5.2 Preload API Extensions

`frontend/src/preload/index.ts` -- extend the `LoomApi` interface and `contextBridge.exposeInMainWorld`:

```typescript
// LoomApi additions

export interface LoomApi {
  // ... existing methods ...

  /** Read a file, optionally restricting to a line range. */
  readFile: (filePath: string, options?: {
    startLine?: number
    endLine?: number
  }) => Promise<string | null>
}
```

```typescript
// contextBridge.exposeInMainWorld -- modified

readFile: (filePath: string, options?: { startLine?: number; endLine?: number }) =>
  ipcRenderer.invoke('read-file', filePath, options),
```

**Note**: `ipcRenderer.invoke` passes all arguments through to the handler. The existing callers pass only `filePath`, so they continue to get the full file content. No breaking change.

---

## 6. Component Specs

### 6.1 InlineInput.tsx

**Path**: `frontend/src/renderer/src/components/input/InlineInput.tsx`

A floating textarea rendered via React portal to `document.body`. Appears when `inlineInputVisible` is true. Positioned based on `inlineInputAnchorRect`.

```typescript
// InlineInput component contract

Props: none (reads from store)

Internal state:
  - instructionText: string  (local state, not persisted)
  - position: { top: number; left: number } | null

Behavior:
  - Renders inside ReactDOM.createPortal to document.body
  - On mount / rect change: compute position from anchorRect:
      * Prefer below the selection, with a 12px gap
      * If insufficient space below (< 200px), position above
      * Horizontally centered on the anchor rect midpoint
      * Clamped to viewport edges (16px margin)
  - Shows a semi-transparent backdrop (backdrop-filter: blur)
    covering the full viewport to focus attention
  - Textarea: auto-resizing, min 2 rows, max 6 rows, 320px min-width, 480px max-width
  - Placeholder: "输入指令，例如: 解释这段代码..."
  - Enter (no modifier): submits instruction → calls onConfirm
  - Escape: dismisses → calls onCancel
  - Shift+Enter: newline in textarea
  - Button row: "发送" (primary) + "取消" (secondary)
  - Auto-focuses textarea on mount

Lifecycle:
  onConfirm():
    1. Get selection range, filePath, and line range from store
       (openInlineInput already resolved data-file-path/data-start-line/data-end-line
       from DOM attributes in the keyboard handler and stored them in the slice)
    2. Build QuotedSelection object (without id; addQuotedSelection generates it)
    3. Call store.addQuotedSelection(qs)
    4. Call store.closeInlineInput()

  Note: instructionText is managed as local React state (useState) in InlineInput
  rather than in the store. This is correct because the instruction text is ephemeral
  and only relevant while the overlay is open. The text is consumed on confirm and
  does not need to persist in the global store.

  onCancel():
    1. Call store.closeInlineInput()
    2. Restore focus to previously focused element
```

**DOM Structure**:
```
<div class="inline-input-overlay">           <!-- fixed, inset:0, z-index:1000 -->
  <div class="inline-input-backdrop" />      <!-- full-viewport blur overlay -->
  <div class="inline-input-container" style="top; left;">  <!-- absolutely positioned -->
    <textarea
      class="inline-input-textarea"
      placeholder="输入指令..."
      autoFocus
    />
    <div class="inline-input-actions">
      <span class="inline-input-hint">Enter 发送 · Esc 取消</span>
      <button class="inline-input-cancel">取消</button>
      <button class="inline-input-send" disabled={!text.trim()}>发送</button>
    </div>
  </div>
</div>
```

### 6.2 InlineInputOverlay.tsx

**Path**: `frontend/src/renderer/src/components/input/InlineInputOverlay.tsx`

Thin wrapper that:
1. Reads `inlineInputVisible` from store.
2. When true, renders `<InlineInput />` via portal.
3. When false, renders nothing (null).
4. Provides the top-level keyboard listener for **Ctrl+Shift+I** (see Section 7).

This component is mounted in `App.tsx` alongside `<ToastContainer />` and `<ConfirmDialog />`.

### 6.3 QuotedSelectionCard Integration

**Existing file**: `frontend/src/renderer/src/components/input/QuotedSelectionCard.tsx`

The component already exists and is well-formed but unused. We wire it into `InputArea.tsx` above the textarea, exactly like `AttachedFiles`:

```tsx
// Inside InputArea.tsx, above <textarea>:
{quotedSelections.length > 0 && (
  <div className={styles.attachmentsArea}>
    {quotedSelections.map(qs => (
      <QuotedSelectionCard
        key={qs.id}
        text={qs.text}
        filePath={qs.filePath}
        onRemove={() => removeQuotedSelection(qs.id)}
      />
    ))}
  </div>
)}
```

Props already match: `text`, `filePath?`, `onRemove`.

---

## 7. Keyboard Flow

### 7.1 Hotkey Definition

| Keystroke | Context | Action |
|-----------|---------|--------|
| `Ctrl+Shift+I` | Anywhere in renderer, when text is selected | Capture selection, open InlineInput |
| `Enter` | InlineInput textarea focused | Confirm instruction, add QuotedSelection to InputArea |
| `Escape` | InlineInput open | Dismiss inline input |
| `Shift+Enter` | InlineInput textarea focused | Insert newline |

### 7.2 Listener Placement

The `Ctrl+Shift+I` listener is registered in `InlineInputOverlay` via a `useEffect` on `document`. Rationale:

- The overlay component is always mounted (rendering null when invisible), so the listener is always active.
- It's co-located with the component it controls, keeping concerns together.
- It fires **before** the browser's default behavior on the selection (we call `e.preventDefault()`).

```typescript
// InlineInputOverlay.tsx -- useEffect

useEffect(() => {
  const handler = (e: KeyboardEvent) => {
    if (e.key === 'I' && e.ctrlKey && e.shiftKey && !e.metaKey && !e.altKey) {
      // Avoid conflict with browser devtools Ctrl+Shift+I (which also uses this combo)
      // Strategy: only intercept when a text selection exists in the document
      const sel = window.getSelection()
      if (!sel || sel.isCollapsed || !sel.toString().trim()) return

      e.preventDefault()
      e.stopPropagation()

      // Extract selection metadata
      const range = sel.getRangeAt(0)
      const rect = range.getBoundingClientRect()

      // Try to find file/line metadata from ancestor elements
      const container = range.commonAncestorContainer
      const messageEl = (container as Element).closest?.('[data-file-path]') as HTMLElement | null

      const filePath = messageEl?.dataset?.filePath || ''
      const startLine = parseInt(messageEl?.dataset?.startLine || '1', 10)
      const endLine = parseInt(messageEl?.dataset?.endLine || String(startLine), 10)

      store.openInlineInput({
        anchorNodePath: [], // simplified; compute from range
        anchorOffset: range.startOffset,
        focusNodePath: [],
        focusOffset: range.endOffset,
        text: sel.toString(),
      }, rect, filePath, startLine, endLine)
    }
  }
  document.addEventListener('keydown', handler, true) // capture phase to beat devtools
  return () => document.removeEventListener('keydown', handler, true)
}, [])
```

### 7.3 DevTools Conflict Mitigation

`Ctrl+Shift+I` is also the default Chrome DevTools shortcut. Mitigations (in priority order):

1. **Capture phase + selection guard**: The listener fires on `keydown` in capture phase and only proceeds when `window.getSelection()` is non-empty. DevTools opens when no text is selected -- this is the common case.
2. **Preference for custom hotkey**: Future iteration can make the hotkey configurable via `setPreference('inlineInputHotkey', 'ctrl+shift+i')`, allowing users to remap to e.g. `Ctrl+Shift+J` or `Ctrl+Shift+K`.

---

## 8. sendMessage Integration

### 8.1 Changes to sendMessage.ts

The `sendMessage` function already builds `blocks` from content and attachedFiles. We add quoted selections as `quoted_selection` blocks:

```typescript
// sendMessage.ts -- modified section (in the block-building block)

export async function sendMessage({ sessionId, content, attachedFiles = [], skills, skipUserMessage, quotedSelections = [] }: SendMessageOptions): Promise<void> {
  // ... existing setup ...

  const blocks: any[] = []

  // 1. Quoted selections first (context before instruction)
  for (const qs of quotedSelections) {
    blocks.push({
      type: 'quoted_selection',
      text: qs.text,
      filePath: qs.filePath,
      startLine: qs.startLine,
      endLine: qs.endLine,
    })
  }

  // 2. Text content
  if (content) {
    blocks.push({ type: 'text', html: escapeHtml(content).replace(/\n/g, '<br>'), source: content })
  }

  // 3. Attached files/images
  for (const f of attachedFiles) {
    // ... existing logic ...
  }

  // ... rest of function (appendMessage, send to backend, etc.) ...
}
```

### 8.2 Changes to InputArea.tsx handleSend

```typescript
// InputArea.tsx -- handleSend modified

const handleSend = async () => {
  const content = text.trim()
  const { quotedSelections } = useStore.getState()
  const hasContent = content || attachedFiles.length > 0 || quotedSelections.length > 0

  if (!hasContent || sendingRef.current || (sessionId && streamingSessionIds.has(sessionId))) return

  sendingRef.current = true
  setText('')
  const filesToSend = attachedFiles
  setAttachedFiles([])
  const selectionsToSend = [...quotedSelections]
  useStore.getState().clearQuotedSelections()

  try {
    const sid = await ensureSession()
    if (!sid) { /* rollback */ return }
    await sendMessage({
      sessionId: sid,
      content,
      attachedFiles: filesToSend,
      skills: selectedSkills.length > 0 ? selectedSkills : undefined,
      quotedSelections: selectionsToSend,
    })
  } finally {
    sendingRef.current = false
  }
}
```

### 8.3 Backend Wire Format

The `quoted_selection` blocks are serialized in the `chat.send` RPC payload. The backend already receives `content` (plain text) and `attached_files`; it will also receive the blocks array via the stream buffer. No backend changes are strictly required for the initial implementation -- the `quoted_selection` blocks flow through the existing message structure.

However, for optimal prompt construction, the backend should later be updated to format quoted selections distinctly. Example prompt template:

```
[引用片段 — /path/to/file.ts L10-L15]
```ts
function example() {
  return true;
}
```

用户指令: explain this function
```

This formatting happens in the backend's prompt builder and is **out of scope** for this design.

---

## 9. Rendering QuotedSelections in Chat History

### 9.1 UserMessage Changes

`UserMessage.tsx` currently renders `text`, `image`, and `file` blocks. We add a `quoted_selection` block renderer:

```tsx
// UserMessage.tsx -- added to block iteration

const quotedSelectionBlocks = message.blocks.filter(b => b.type === 'quoted_selection')

{quotedSelectionBlocks.length > 0 && (
  <div className={styles.quotedSelections}>
    {quotedSelectionBlocks.map((block, i) => (
      <QuotedSelectionCard
        key={i}
        text={block.text as string}
        filePath={block.filePath as string}
        onRemove={() => {}} // no-op in history view; read-only
      />
    ))}
  </div>
)}
```

**Display decision**: Quoted selections in chat history are **display-only** (no remove button, or a hidden/disabled one). The `onRemove` prop on `QuotedSelectionCard` is made optional.

### 9.2 QuotedSelectionCard Prop Change

```typescript
// QuotedSelectionCard.tsx -- make onRemove optional

interface Props {
  text: string
  filePath?: string
  onRemove?: () => void   // optional: hidden in history, shown in InputArea
}
```

Render the remove button conditionally: `{onRemove && <button onClick={onRemove}>...</button>}`.

---

## 10. File Metadata on Message Blocks

For inline selections to carry `filePath` and line range, the chat message DOM elements must expose this information via `data-*` attributes.

### 10.1 Message List Attributes

In the code-block rendering path (currently inside `TextBlock` or similar), each code fence or source-reference block should carry:

```html
<div data-file-path="/path/to/file.ts" data-start-line="10" data-end-line="25">
  <pre><code>...</code></pre>
</div>
```

For inline-selection-on-chat-text (not a specific file-reference), the attributes are omitted and `filePath` will be empty, `startLine`/`endLine` will be `1`/estimated or `0`/`0` indicating "inline chat text".

### 10.2 Data-Attribute Injection (Required — Not Follow-Up)

The inline selection feature **depends** on `data-file-path`, `data-start-line`, and `data-end-line` DOM attributes to resolve file context when a user selects text in a rendered code block or diff. This must be implemented as part of the initial implementation, not deferred.

#### 10.2.1 Injection Points

**In `TextBlock.tsx`** — code fence elements:
- When a code fence references a known file (e.g., via an existing file-path header or source annotation), the outermost `<div>` or `<pre>` wrapper around the code block receives:
  ```html
  <div data-file-path="/absolute/path/to/file.ts">
    <pre><code>...</code></pre>
  </div>
  ```
- If no file path is known (plain chat message code blocks), the attribute is omitted and `filePath` defaults to `''`.

**In `FileDiffCard.tsx`** — diff lines:
- The container element for each diff hunk receives:
  ```html
  <div data-file-path="/absolute/path/to/file.ts"
       data-start-line="10"
       data-end-line="25">
    <!-- diff lines rendered here -->
  </div>
  ```
- `data-start-line` and `data-end-line` reflect the 1-based line numbers of the visible diff range.

#### 10.2.2 Attribute Resolution

The selection handler (in `InlineInputOverlay`'s keyboard listener) resolves file metadata via:
```typescript
const container = event.target instanceof Element
  ? event.target.closest('[data-file-path]')
  : null
const filePath = (container as HTMLElement)?.dataset?.filePath || ''
const startLine = parseInt((container as HTMLElement)?.dataset?.startLine || '1', 10)
const endLine = parseInt((container as HTMLElement)?.dataset?.endLine || String(startLine), 10)
```

This uses `closest()` to walk up the DOM tree, so the selected element does not need to be the container itself — any descendant within the attributed region works.

#### 10.2.3 Timeline Impact

Data-attribute injection in `TextBlock.tsx` and `FileDiffCard.tsx` is a **blocking prerequisite** for the inline selection feature. It must be completed in Phase 1 alongside the store + IPC work, not deferred to a later phase.

For inline selections on plain chat text (no code block, no file reference), the attributes are absent and `filePath` will be `''`, `startLine`/`endLine` will be `1`. This is the fallback and works without file-specific attributes.

---

## 11. CSS Module: InlineInput.module.css

A new CSS module with these key rules:

```css
/* InlineInput.module.css */

.overlay {
  position: fixed;
  inset: 0;
  z-index: 10000;
  display: flex;
  align-items: flex-start;
  justify-content: center;
}

.backdrop {
  position: absolute;
  inset: 0;
  background: rgba(0, 0, 0, 0.3);
  backdrop-filter: blur(2px);
}

.container {
  position: absolute;
  min-width: 320px;
  max-width: 480px;
  background: var(--bg-card);
  border: 1px solid var(--border);
  border-radius: var(--r-md);
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.4);
  padding: 12px;
  display: flex;
  flex-direction: column;
  gap: 8px;
  animation: inline-slide-in 0.15s ease-out;
}

.textarea {
  width: 100%;
  min-height: 48px;
  max-height: 144px;
  resize: none;
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: var(--r-sm);
  color: var(--text);
  font-size: 14px;
  line-height: 1.5;
  padding: 8px 10px;
  outline: none;
}

.textarea:focus {
  border-color: var(--accent);
}

.actions {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 8px;
}

.hint {
  flex: 1;
  font-size: 11px;
  color: var(--text-muted);
}

@keyframes inline-slide-in {
  from { opacity: 0; transform: translateY(-4px) scale(0.98); }
  to   { opacity: 1; transform: translateY(0) scale(1); }
}
```

---

## 12. Files to Create / Modify

### 12.1 New Files

| File | Purpose |
|------|---------|
| `frontend/src/renderer/src/stores/selection-context.ts` | New Zustand slice: SelectionContextSlice |
| `frontend/src/renderer/src/components/input/InlineInput.tsx` | Floating textarea component |
| `frontend/src/renderer/src/components/input/InlineInputOverlay.tsx` | Portal wrapper + keyboard listener |
| `frontend/src/renderer/src/components/input/InlineInput.module.css` | Styles for InlineInput |

### 12.2 Modified Files

| File | Change |
|------|--------|
| `frontend/src/renderer/src/stores/index.ts` | Register SelectionContextSlice in AppStore |
| `frontend/src/renderer/src/components/input/InputArea.tsx` | Read quotedSelections from store; render QuotedSelectionCard list; pass quotedSelections to sendMessage; clear on send |
| `frontend/src/renderer/src/components/input/QuotedSelectionCard.tsx` | Make `onRemove` optional; hide button when undefined |
| `frontend/src/renderer/src/services/sendMessage.ts` | Add `quotedSelections` to SendMessageOptions; build `quoted_selection` blocks |
| `frontend/src/renderer/src/App.tsx` | Mount `<InlineInputOverlay />` |
| `frontend/src/renderer/src/components/chat/UserMessage.tsx` | Render `quoted_selection` blocks in chat history |
| `frontend/src/main/ipc/files.ts` | Extend `read-file` to accept optional `{ startLine, endLine }` |
| `frontend/src/preload/index.ts` | Extend `LoomApi.readFile` signature with optional line-range options |

### 12.3 Files NOT Modified

| File | Reason |
|------|--------|
| `stores/selection.ts` | This slice is for chat message checkbox selection; unrelated |
| `stores/input.ts` | Quoted selections are separate from drafts; persist via store state, not draft |
| `components/chat/AssistantMessage.tsx` | No change needed; quoted_selection is a user-side block |
| `services/bootstrap.ts` | No change needed |

---

## 13. Testing Strategy

### 13.1 Unit Tests (Vitest)

**SelectionContextSlice** (`stores/selection-context.test.ts`):
- `openInlineInput` sets `visible`, `selectionRange`, `anchorRect`
- `closeInlineInput` resets all three
- `addQuotedSelection` appends with unique ID
- `removeQuotedSelection` removes by ID, leaves others intact
- `clearQuotedSelections` empties the array

**sendMessage** (`services/sendMessage.test.ts`):
- When `quotedSelections` is provided, `quoted_selection` blocks appear in the message
- Block order: quoted_selections before text content
- Empty `quotedSelections` (default) produces no quoted_selection blocks
- Backward compatibility: existing callers without `quotedSelections` work unchanged

### 13.2 Integration Tests (Playwright / Spectron)

1. **Hotkey fires InlineInput**:
   - Load a chat with a code block
   - Select text in the code block
   - Press Ctrl+Shift+I
   - Assert: InlineInput overlay is visible, positioned near selection
   - Assert: backdrop is rendered

2. **Full flow**:
   - Select text, press Ctrl+Shift+I
   - Type "explain this", press Enter
   - Assert: InlineInput dismissed
   - Assert: QuotedSelectionCard appears in InputArea
   - Click Send
   - Assert: user message contains `quoted_selection` block

3. **Escape dismisses**:
   - Open InlineInput via hotkey
   - Press Escape
   - Assert: overlay gone, no QuotedSelectionCard added

4. **No selection = no trigger**:
   - Press Ctrl+Shift+I with collapsed selection
   - Assert: InlineInput does NOT open

5. **Empty instruction**:
   - Open InlineInput, leave textarea empty, press Enter
   - Assert: send button is disabled; nothing happens

### 13.3 Manual Smoke Test Checklist

- [ ] Hotkey works on Windows (Ctrl+Shift+I)
- [ ] Hotkey works on macOS (Cmd+Shift+I -- needs platform detection)
- [ ] Floating input appears near selection, not off-screen
- [ ] Multiple quoted selections accumulate in InputArea
- [ ] Remove button on QuotedSelectionCard works
- [ ] Sending with quoted selections + normal text works
- [ ] Sending with only quoted selections (no text) works
- [ ] Chat history renders quoted_selection blocks correctly
- [ ] DevTools still opens when no text is selected (Ctrl+Shift+I passes through)

---

## 14. Open Questions & Future Iterations

1. **Editor integration**: When a file editor (Monaco / CodeMirror) is added, the same Ctrl+Shift+I flow should work there. The file path and line range come directly from the editor model.

2. **Staleness guard**: The DeepSeek-GUI reference implements a staleness check (verifying the file content at selection scope hasn't changed before applying edits). This is relevant when the agent is asked to edit the selected code. Out of scope for v1.

3. **Configurable hotkey**: The user should be able to change the hotkey from Ctrl+Shift+I to something else. This requires a new preference `inlineInputHotkey` and a keyboard shortcut management UI component.

4. **Multi-selection**: Browser-native multi-selection (Ctrl+click to select disjoint ranges) could be supported to quote multiple non-contiguous snippets in one instruction.

5. **Backend prompt formatting**: The backend should be updated to render `quoted_selection` blocks with file path, line range, and syntax-highlighted code in the LLM prompt.

---

## 15. Appendix: Timeline Estimate

| Phase | Work | Est. |
|-------|------|------|
| 1. Store + IPC + Data Attributes | SelectionContextSlice, read-file extension, preload signature, data-attribute injection in TextBlock.tsx + FileDiffCard.tsx | 1.5 days |
| 2. InlineInput component | Floating textarea, portal, positioning, keyboard handling | 1.5 days |
| 3. InputArea + sendMessage wiring | QuotedSelectionCard integration, send flow, clear on send | 1 day |
| 4. Chat history rendering | UserMessage block rendering, QuotedSelectionCard optional onRemove | 0.5 day |
| 5. Styling + polish | CSS module, animations, edge cases | 0.5 day |
| 6. Testing | Unit tests + integration tests + smoke tests | 1 day |
| **Total** | | **~6 days** |

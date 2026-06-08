# 005 — Write Mode Workspace

**Status**: Draft
**Created**: 2026-06-08
**Effort**: ~3 weeks (1 developer)
**Scope**: Frontend-heavy + minor backend additions

---

## Table of Contents

1. [Overview / User Journey](#1-overview--user-journey)
2. [Architecture Overview](#2-architecture-overview)
3. [Mode Switching Design](#3-mode-switching-design)
4. [Component Tree](#4-component-tree)
5. [Store Design](#5-store-design)
6. [VFS Backend Design](#6-vfs-backend-design)
7. [IPC / Preload Additions](#7-ipc--preload-additions)
8. [CodeMirror Integration](#8-codemirror-integration)
9. [Export Pipeline Design](#9-export-pipeline-design)
10. [File Manifest](#10-file-manifest)
11. [Implementation Phases](#11-implementation-phases)
12. [Testing Strategy](#12-testing-strategy)

---

## 1. Overview / User Journey

### Problem

openLoom is a chat-centric AI assistant. Users have conversations with agents and ask them to write documents, articles, READMEs, or technical specs. The agent responds with markdown in a chat bubble. But there is no way to:

- Edit the output collaboratively with the AI
- Preview the rendered document side-by-side
- Organize documents in a project-like file tree
- Export the final document to HTML/PDF/DOCX
- Have the AI assist inline at a specific cursor position

### Solution

Write Mode is a second workspace view that coexists with Chat Mode. It provides a markdown editor with live preview, file-tree-based document management, inline AI assistance, and one-click export.

### User Journeys

**J1 — Quick Draft with AI**
1. User clicks "Write" tab or presses a keyboard shortcut
2. User creates a new markdown file in a workspace directory
3. User types a heading, then invokes inline AI: "Continue writing a tutorial about..."
4. AI fills in content; user edits inline
5. User switches to Split View to see the rendered preview
6. User exports to PDF

**J2 — Multi-file Document Project**
1. User creates a workspace rooted at `~/Documents/loom-workspace/`
2. Using the file tree, user creates `chapter-01.md`, `chapter-02.md`, `README.md`
3. User opens each file in tabs, edits, invokes AI assistance
4. File tree reflects external changes (user edited chapter-01.md in VS Code)
5. User exports README.md as HTML

**J3 — AI-Generated Content Refinement**
1. In Chat Mode, the agent generates a long markdown document
2. User clicks "Open in Write" on a tool result or message
3. Content is copied into a new Write workspace file
4. User edits, previews, and refines with inline AI

---

## 2. Architecture Overview

### 2.1 System-Level Diagram

```
┌──────────────────────────────────────────────────────────────────┐
│                        Electron Main Process                      │
│  ┌──────────────┐  ┌──────────────┐  ┌─────────────────────────┐ │
│  │  window.ts    │  │  ipc/files.ts │  │  export-write.ts (new)  │ │
│  │  (tray, etc.) │  │  +vfs ops    │  │  headless BrowserWin    │ │
│  └──────────────┘  └──────────────┘  └─────────────────────────┘ │
└──────────────────────────────┬───────────────────────────────────┘
                               │ IPC (invoke / on)
┌──────────────────────────────┴───────────────────────────────────┐
│                     Preload (contextBridge)                        │
│  loom.vfs.*        loom.exportWrite.*        loom.watchFile.*     │
└──────────────────────────────┬───────────────────────────────────┘
                               │
┌──────────────────────────────┴───────────────────────────────────┐
│                       Renderer Process                            │
│  ┌─────────────────────────────────────────────────────────────┐ │
│  │  AppShell                                                    │ │
│  │  ┌──────────┐  ┌──────────────────────────────────────────┐ │ │
│  │  │ Sidebar   │  │  ModeRouter (new)                         │ │ │
│  │  │ (sessions)│  │  ┌─────────────────┬───────────────────┐ │ │ │
│  │  │           │  │  │ ChatWorkspace    │ WriteWorkspace    │ │ │ │
│  │  │           │  │  │ (existing)       │ (new)             │ │ │ │
│  │  └──────────┘  │  └─────────────────┴───────────────────┘ │ │ │
│  │                └──────────────────────────────────────────┘ │ │
│  └─────────────────────────────────────────────────────────────┘ │
│                                                                   │
│  Stores (Zustand): ...existing 17 slices + write slice (new)      │
│  Services: jsonrpc.ts (loomRpc → WS), websocket.ts               │
└──────────────────────────────────┬────────────────────────────────┘
                                   │ WebSocket (JSON-RPC 2.0)
┌──────────────────────────────────┴────────────────────────────────┐
│                      Rust Backend (loom-server)                    │
│  ┌─────────────────────────────────────────────────────────────┐ │
│  │  dispatch/mod.rs                                              │ │
│  │  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌───────────────┐  │ │
│  │  │ chat.rs   │ │session.rs│ │ tool.rs   │ │ vfs.rs (new)  │  │ │
│  │  └──────────┘ └──────────┘ └──────────┘ └───────────────┘  │ │
│  └─────────────────────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────────────────┘
```

### 2.2 Data Flow Patterns

**Pattern A — VFS Operations (synchronous, short-lived)**

```
WriteWorkspaceDocumentPane
  → useStore.getState().openFile(path)
    → loomRpc('vfs.readFile', { path })
      → WS → backend: vfs::handle_vfs_read_file()
        → tokio::fs::read(path)
      ← WS ← result: { content, size, mime_type }
    ← store set: { activeFilePath, fileContent, fileLoading: false }
  → CodeMirror receives new doc value
```

**Pattern B — Autosave (debounced, 650ms)**

```
CodeMirror onDocChanged
  → (debounce 650ms)
    → useStore.getState().markDirty()
    → loomRpc('vfs.writeFile', { path, content })
      → WS → backend: vfs::handle_vfs_write_file()
        → tokio::fs::write(path, content)
      ← WS ← result: { ok: true }
    ← store set: { saveStatus: 'saved' }
```

**Pattern C — AI Inline Completion**

```
WriteMarkdownEditor → user opens WriteAssistantPanel
  → user types prompt in FloatingComposer
    → loomRpc('chat.send', { content: composedPrompt, session_id: writeSessionId })
      → WS → backend (spawned on tokio)
        → stream events back: chat.stream_delta
    ← WS notifications → loomSubscribe handler
      → useStore.getState().appendWriteInlineDelta(delta)
  → CodeMirror receives appended/replaced text
```

**Pattern D — Export**

```
WriteWorkspaceToolbar → user clicks "Export" → pick format
  → window.loom.exportWriteDocument({ filePath, format: 'pdf' })
    → IPC invoke → main: export-write.ts
      → render markdown → HTML
      → create hidden BrowserWindow → load HTML
      → webContents.printToPDF()
      → dialog.showSaveDialog()
      → write file
    ← IPC result: { ok: true, outputPath }
  → store: addToast('导出完成')
```

### 2.3 How Write Mode Coexists with Chat Mode

Write Mode and Chat Mode share the same:
- WebSocket connection (single port)
- Zustand store (same `useStore` instance)
- Session system (write threads are sessions with `mode: 'write'`)
- Theme, font settings, preference persistence

They differ in:
- **Workspace UI**: Chat has message timeline + textarea input; Write has editor + file tree + inline assistant
- **Active session**: Write mode binds to a "Write Assistant" thread (auto-created)
- **Keyboard shortcuts**: Ctrl+Enter sends message in Chat, but inserts newline in Write editor

### 2.4 Thread Model

Write mode sessions are regular backend sessions with a metadata tag `mode: 'write'`. The `write-thread-registry` (port from DeepSeek-GUI pattern) maintains a mapping:

```
workspaceRoot → activeThreadId
```

When the user switches workspaces, the active thread changes. The WriteAssistantPanel loads the corresponding thread's message history.

Thread hydration on startup:
1. `hydrateWriteThreads()` scans all sessions for those with `title: "Write Assistant"` or `[写作上下文]` pattern
2. For each session found, extract the workspace root from session metadata
3. Register in the in-memory registry

---

## 3. Mode Switching Design

### 3.1 State Machine

```
┌──────────┐  switchMode('write')  ┌───────────┐
│  Chat    │ ──────────────────────→│  Write    │
│  Mode    │←──────────────────────│  Mode     │
└──────────┘  switchMode('chat')   └───────────┘
```

### 3.2 Implementation

A new field in the UI store:
```typescript
// stores/ui.ts (additions)
export type AppMode = 'chat' | 'write'

export interface UiSlice {
  // ...existing fields...
  appMode: AppMode
  switchMode: (mode: AppMode) => void
}
```

### 3.3 Triggers

| Trigger | Action |
|---------|--------|
| Tab in titlebar (Write / Chat) | `switchMode('write')` |
| Keyboard shortcut Ctrl+Shift+W | `switchMode('write')` |
| Ctrl+Shift+C or Escape (from Write) | `switchMode('chat')` |
| "Open in Write" context action on message | `switchMode('write')` + open file with content |
| Start typing in chat when Write is open | Auto-switch to Chat (if no dirty file) or prompt |

### 3.4 ModeRouter Component

```typescript
// components/app/ModeRouter.tsx (new)
export default function ModeRouter() {
  const appMode = useStore(s => s.appMode)

  if (appMode === 'write') {
    return <WriteWorkspaceView />
  }
  return <ChatWorkspace />
}
```

This replaces the direct `<ChatWorkspace />` in `AppShell.tsx`:
```tsx
// AppShell.tsx (change)
- <ChatWorkspace />
+ <ModeRouter />
```

### 3.5 State Preservation Across Mode Switches

- Chat state (messages, streaming) is **preserved** — not cleared when switching to Write
- Write state (active file, editor content, dirty flag) is **preserved** — not cleared when switching to Chat
- If the user switches back to Write, the editor is restored to its last state
- File watching continues in the background regardless of mode

### 3.6 Tab Visibility

When in Write mode, the sidebar's session list can optionally hide chat-only sessions or show a filtered view. The default behavior is to show all sessions (both chat and write threads) to avoid confusion.

### 3.7 Right Panel Ownership

WriteAssistantPanel (this feature) and PlanPanel/TodoPanel (Feature 003) both want right sidebar space. These are **mode-exclusive**:

- **Chat mode**: Right panel owned by Feature 003 (PlanPanel/TodoPanel, tabbed). WriteAssistantPanel is not rendered.
- **Write mode**: Right panel owned by WriteAssistantPanel. PlanPanel/TodoPanel are not rendered.

**State preservation**: When switching modes, the hidden panel's Zustand state is preserved (not reset). The panel is conditionally rendered, not unmounted (or uses the `keepMounted` pattern to avoid losing in-progress AI streaming, editing state, etc.).

**ModeRouter implementation**:
```tsx
function ModeRouter() {
  const appMode = useStore(s => s.appMode)
  return (
    <div class="app-layout">
      <Sidebar />
      <main>{appMode === 'chat' ? <ChatWorkspace /> : <WriteWorkspaceView />}</main>
      <aside class="right-panel">
        {appMode === 'chat' && <><PlanPanel /><TodoPanel /></>}
        {appMode === 'write' && <WriteAssistantPanel />}
      </aside>
    </div>
  )
}
```
Reference Feature 003 Amendment 3 for the complementary change (PlanPanel/TodoPanel side).

---

## 4. Component Tree

```
AppShell
├── Sidebar (existing — no change)
│   └── SessionItem (existing)
└── main[data-content]
    └── ModeRouter (new)
        ├── ChatWorkspace (existing)
        │   ├── ChatArea
        │   │   ├── UserMessage
        │   │   ├── AssistantMessage
        │   │   ├── ThinkingBlock
        │   │   ├── ToolGroupBlock
        │   │   └── ...
        │   └── InputArea
        │       ├── textarea
        │       ├── ContextRing
        │       ├── ModelSelector
        │       └── ...
        │
        └── WriteWorkspaceView (new)
            ├── WriteSidebar (new)
            │   ├── WorkspaceSwitcher
            │   │   ├── <select> workspace roots
            │   │   └── "Browse..." button
            │   ├── WriteFileTree
            │   │   ├── FileTreeItem (recursive)
            │   │   └── ContextMenu (new/open/rename/delete)
            │   └── CreateDialog / RenameDialog / DeleteConfirmDialog
            │
            ├── WriteWorkspaceMain (new)
            │   ├── WriteWorkspaceToolbar (new)
            │   │   ├── FileNameDisplay + saveStatus indicator
            │   │   ├── PreviewModePicker (Source | Live | Split | Preview)
            │   │   ├── SaveButton
            │   │   └── ExportMenu (HTML / PDF / DOCX / Rich Text Copy)
            │   │
            │   ├── WriteWorkspaceDocumentPane (new)
            │   │   ├── [Source mode] WriteMarkdownEditor (new)
            │   │   │   └── CodeMirror 6 instance
            │   │   │       ├── MarkdownLivePreview extension (new)
            │   │   │       ├── WriteInlineCompletion extension (new)
            │   │   │       ├── ImagePasteHandler extension (new)
            │   │   │       └── Autosave hook (650ms debounce)
            │   │   │
            │   │   ├── [Live mode] WriteMarkdownEditor (same CM, live preview active)
            │   │   │
            │   │   ├── [Split mode]
            │   │   │   ├── WriteMarkdownEditor (left pane, resizable split)
            │   │   │   └── WriteMarkdownPreview (right pane)
            │   │   │
            │   │   └── [Preview mode] WriteMarkdownPreview (new)
            │   │       └── HTML via renderMarkdown() from utils/markdown.ts (markdown-it + highlight.js + katex + mermaid)
            │   │
            │   ├── [non-markdown] WriteImagePreview (new)
            │   │   └── <img> for image files
            │   │
            │   └── WriteInlineAgent (new)
            │       └── FloatingComposer at cursor position
            │
            └── WriteAssistantPanel (new, collapsible right sidebar)
                ├── MessageTimeline (reuse existing pattern)
                │   ├── UserMessage
                │   └── AssistantMessage
                └── FloatingComposer (compact input for inline prompting)
```

### 4.1 Component Responsibilities

| Component | Responsibility |
|-----------|---------------|
| `WriteWorkspaceView` | Top-level shell for Write mode; orchestrates sidebar, main area, and assistant panel |
| `WriteSidebar` | Workspace selection + file tree; reuses existing sidebar CSS patterns |
| `WriteFileTree` | Recursive directory tree with expand/collapse; right-click context menu |
| `WriteWorkspaceToolbar` | File name, save status, preview mode toggle, export dropdown |
| `WriteWorkspaceDocumentPane` | Layout container for editor + preview; handles Source/Live/Split/Preview modes with CSS flex/grid |
| `WriteMarkdownEditor` | CodeMirror 6 wrapper; manages extensions, autosave, file watching |
| `WriteMarkdownPreview` | Rendered HTML via `renderMarkdown()` from `utils/markdown.ts` (markdown-it + highlight.js + katex + mermaid); scroll-synced with editor in Split mode |
| `WriteAssistantPanel` | Chat sidebar for Write threads; shares message rendering components with Chat mode |
| `WriteInlineAgent` | Compact prompt input that appears at the cursor position; sends context + selection |

---

## 5. Store Design

### 5.1 New Store Slice: `write.ts`

```typescript
// stores/write.ts (new file)

import { StateCreator } from 'zustand'

// ── Types ──────────────────────────────────────────────────────

export type PreviewMode = 'source' | 'live' | 'split' | 'preview'
export type SaveStatus = 'saved' | 'dirty' | 'saving' | 'error'
export type FileKind = 'text' | 'image'

export interface FileEntry {
  name: string
  path: string           // absolute path
  kind: 'file' | 'directory'
  mimeType?: string
  size?: number
  modified?: string      // ISO 8601
}

// QuotedSelection — canonical type shared with Feature 002
// Defined in: frontend/src/renderer/src/types/quotedSelection.ts
// Both 002's SelectionContextSlice and 005's WriteSlice import from this shared type.
export interface QuotedSelection {
  id: string          // Unique ID for removal tracking (crypto.randomUUID())
  text: string        // The selected text
  filePath?: string   // Path to the source file (if applicable)
  startLine?: number  // 1-based start line
  endLine?: number    // 1-based end line
  charCount: number   // Character count of selection
}

export interface RecentEdit {
  filePath: string
  oldText: string        // clipped to 900 chars
  newText: string
  timestamp: number
  source: 'user' | 'ai' | 'inline'
}

export interface WriteSlice {
  // ── Workspace Settings ──
  defaultWorkspaceRoot: string | null
  recentWorkspaceRoots: string[]

  // ── Active Workspace ──
  workspaceRoot: string | null
  entriesByDir: Record<string, FileEntry[]>
  expandedDirs: Set<string>
  loadingDirs: Set<string>
  treeError: string | null

  // ── Active File ──
  activeFilePath: string | null
  activeFileKind: FileKind
  fileContent: string
  imageDataUrl: string | null
  imageMimeType: string | null
  fileSize: number
  fileTruncated: boolean
  fileError: string | null
  fileLoading: boolean
  saveStatus: SaveStatus

  // ── UI State ──
  previewMode: PreviewMode
  assistantOpen: boolean
  assistantModel: string | null
  selection: QuotedSelection | null
  quotedSelections: QuotedSelection[]
  recentEdits: RecentEdit[]

  // ── Thread Management ──
  writeThreadByWorkspace: Record<string, string>  // workspaceRoot → sessionId
  activeWriteThreadId: string | null

  // ── Actions: Settings ──
  setDefaultWorkspaceRoot: (root: string | null) => void
  addRecentWorkspaceRoot: (root: string) => void

  // ── Actions: Workspace ──
  initializeWorkspace: (root: string) => Promise<void>
  loadDirectory: (dirPath: string) => Promise<void>
  toggleDirectory: (dirPath: string) => void
  refreshWorkspace: () => Promise<void>

  // ── Actions: File ──
  openFile: (filePath: string) => Promise<void>
  createFile: (parentDir: string, name: string) => Promise<void>
  createDirectory: (parentDir: string, name: string) => Promise<void>
  renameEntry: (oldPath: string, newName: string) => Promise<void>
  deleteEntry: (path: string) => Promise<void>

  // ── Actions: Editor ──
  setFileContent: (content: string) => void
  markDirty: () => void
  setSaveStatus: (status: SaveStatus) => void
  saveFile: () => Promise<void>

  // ── Actions: UI ──
  setPreviewMode: (mode: PreviewMode) => void
  toggleAssistant: () => void
  addQuotedSelection: (sel: QuotedSelection) => void
  removeQuotedSelection: (index: number) => void
  clearQuotedSelections: () => void

  // ── Actions: Recent Edits ──
  recordEdit: (edit: Omit<RecentEdit, 'timestamp'>) => void

  // ── Actions: Thread ──
  bindWriteThread: (workspaceRoot: string, sessionId: string) => void
  unbindWriteThread: (workspaceRoot: string) => void
  hydrateWriteThreads: () => Promise<void>
}
```

### 5.2 Store Implementation Pattern

Following the existing codebase convention, `createWriteSlice` is a `StateCreator`:

```typescript
export const createWriteSlice: StateCreator<WriteSlice> = (set, get) => ({
  // Initial state
  defaultWorkspaceRoot: null,
  recentWorkspaceRoots: [],
  workspaceRoot: null,
  entriesByDir: {},
  expandedDirs: new Set(),
  loadingDirs: new Set(),
  treeError: null,
  activeFilePath: null,
  activeFileKind: 'text',
  fileContent: '',
  imageDataUrl: null,
  imageMimeType: null,
  fileSize: 0,
  fileTruncated: false,
  fileError: null,
  fileLoading: false,
  saveStatus: 'saved',
  previewMode: 'live',
  assistantOpen: false,
  assistantModel: null,
  selection: null,
  quotedSelections: [],
  recentEdits: [],
  writeThreadByWorkspace: {},
  activeWriteThreadId: null,

  // Action implementations (detailed below)

  setDefaultWorkspaceRoot: (root) => {
    window.loom.setPreference('defaultWorkspaceRoot', root)
    set({ defaultWorkspaceRoot: root })
  },

  addRecentWorkspaceRoot: (root) => {
    const recent = get().recentWorkspaceRoots
    const next = [root, ...recent.filter(r => r !== root)].slice(0, 10)
    window.loom.setPreference('recentWorkspaceRoots', next)
    set({ recentWorkspaceRoots: next })
  },

  initializeWorkspace: async (root) => {
    set({ workspaceRoot: root, treeError: null })
    get().addRecentWorkspaceRoot(root)
    await get().loadDirectory(root)
    // Hydrate or create Write thread
    await get().hydrateWriteThreads()
    const existingThread = get().writeThreadByWorkspace[root]
    if (!existingThread) {
      // Create a new session for this workspace
      const sessionId = await createWriteSession(root)
      get().bindWriteThread(root, sessionId)
    }
  },

  loadDirectory: async (dirPath) => {
    const loading = new Set(get().loadingDirs)
    loading.add(dirPath)
    set({ loadingDirs: loading })
    try {
      const result = await loomRpc<{ entries: FileEntry[] }>('vfs.listDirectory', { path: dirPath })
      set(s => ({
        entriesByDir: { ...s.entriesByDir, [dirPath]: result.entries },
        loadingDirs: removeFromSet(s.loadingDirs, dirPath),
      }))
    } catch (e: any) {
      set(s => ({
        treeError: e.message || 'Failed to load directory',
        loadingDirs: removeFromSet(s.loadingDirs, dirPath),
      }))
    }
  },

  toggleDirectory: (dirPath) => {
    const next = new Set(get().expandedDirs)
    if (next.has(dirPath)) {
      next.delete(dirPath)
    } else {
      next.add(dirPath)
      // Load if not yet loaded
      if (!get().entriesByDir[dirPath]) {
        get().loadDirectory(dirPath)
      }
    }
    set({ expandedDirs: next })
  },

  refreshWorkspace: async () => {
    const root = get().workspaceRoot
    if (!root) return
    // Clear cached entries and reload from root
    const expanded = get().expandedDirs
    for (const dir of expanded) {
      await get().loadDirectory(dir)
    }
  },

  openFile: async (filePath) => {
    set({ fileLoading: true, fileError: null, activeFilePath: filePath })
    try {
      const result = await loomRpc<{
        content: string | null
        size: number
        mime_type: string
        truncated: boolean
      }>('vfs.readFile', { path: filePath })

      const isImage = result.mime_type.startsWith('image/') && result.mime_type !== 'image/svg+xml'
      if (isImage) {
        // For images, read via IPC (which can handle binary/buffer)
        const imgData = await window.loom.readWorkspaceImage(filePath)
        set({
          activeFileKind: 'image',
          imageDataUrl: imgData.dataUrl,
          imageMimeType: result.mime_type,
          fileSize: result.size,
          fileTruncated: false,
          fileLoading: false,
          fileContent: '',
        })
      } else {
        set({
          activeFileKind: 'text',
          fileContent: result.content ?? '',
          fileSize: result.size,
          fileTruncated: result.truncated,
          fileLoading: false,
          previewMode: 'live', // default to live preview for markdown
        })
      }
    } catch (e: any) {
      set({ fileError: e.message || 'Failed to open file', fileLoading: false })
    }
  },

  createFile: async (parentDir, name) => {
    const fullPath = `${parentDir}/${name}`
    await loomRpc('vfs.createFile', { path: fullPath })
    await get().loadDirectory(parentDir)
    // Auto-open the new file
    await get().openFile(fullPath)
  },

  createDirectory: async (parentDir, name) => {
    const fullPath = `${parentDir}/${name}`
    await loomRpc('vfs.createDirectory', { path: fullPath })
    await get().loadDirectory(parentDir)
  },

  renameEntry: async (oldPath, newName) => {
    const parentDir = oldPath.substring(0, oldPath.lastIndexOf('/'))
    await loomRpc('vfs.rename', { path: oldPath, new_name: newName })
    // If this was the active file, update the active path
    const active = get().activeFilePath
    if (active === oldPath) {
      const newPath = `${parentDir}/${newName}`
      set({ activeFilePath: newPath })
    }
    await get().loadDirectory(parentDir)
  },

  deleteEntry: async (path) => {
    await loomRpc('vfs.delete', { path })
    const parentDir = path.substring(0, path.lastIndexOf('/'))
    const active = get().activeFilePath
    if (active === path || active?.startsWith(path + '/')) {
      set({ activeFilePath: null, fileContent: '', activeFileKind: 'text' })
    }
    await get().loadDirectory(parentDir)
  },

  setFileContent: (content) => {
    set({ fileContent: content })
  },

  markDirty: () => {
    if (get().saveStatus !== 'dirty') {
      set({ saveStatus: 'dirty' })
    }
  },

  setSaveStatus: (saveStatus) => set({ saveStatus }),

  saveFile: async () => {
    const { activeFilePath, fileContent } = get()
    if (!activeFilePath) return
    set({ saveStatus: 'saving' })
    try {
      await loomRpc('vfs.writeFile', { path: activeFilePath, content: fileContent })
      set({ saveStatus: 'saved' })
    } catch {
      set({ saveStatus: 'error' })
    }
  },

  setPreviewMode: (previewMode) => set({ previewMode }),
  toggleAssistant: () => set(s => ({ assistantOpen: !s.assistantOpen })),

  addQuotedSelection: (sel) => {
    set(s => ({
      quotedSelections: [...s.quotedSelections, sel].slice(-5),
    }))
  },

  removeQuotedSelection: (index) => {
    set(s => ({
      quotedSelections: s.quotedSelections.filter((_, i) => i !== index),
    }))
  },

  clearQuotedSelections: () => set({ quotedSelections: [] }),

  recordEdit: (edit) => {
    const now = Date.now()
    const existing = get().recentEdits
    // Merge adjacent typing edits within 3s on same file
    const last = existing[existing.length - 1]
    if (
      last &&
      last.filePath === edit.filePath &&
      last.source === edit.source &&
      edit.source === 'user' &&
      now - last.timestamp < 3000
    ) {
      const merged = { ...last, newText: edit.newText, timestamp: now }
      set({ recentEdits: [...existing.slice(0, -1), merged] })
      return
    }
    const capped = [{ ...edit, timestamp: now }, ...existing].slice(0, 48)
    // Purge edits older than 2 minutes
    const fresh = capped.filter(e => now - e.timestamp < 120_000)
    set({ recentEdits: fresh })
  },

  bindWriteThread: (workspaceRoot, sessionId) => {
    set(s => ({
      writeThreadByWorkspace: { ...s.writeThreadByWorkspace, [workspaceRoot]: sessionId },
      activeWriteThreadId: sessionId,
    }))
  },

  unbindWriteThread: (workspaceRoot) => {
    set(s => {
      const next = { ...s.writeThreadByWorkspace }
      delete next[workspaceRoot]
      return {
        writeThreadByWorkspace: next,
        activeWriteThreadId: s.activeWriteThreadId === s.writeThreadByWorkspace[workspaceRoot]
          ? null
          : s.activeWriteThreadId,
      }
    })
  },

  hydrateWriteThreads: async () => {
    // Scan sessions for Write Assistant threads
    try {
      const result = await loomRpc<{ sessions: any[] }>('session.list')
      const threads: Record<string, string> = {}
      for (const s of (result.sessions || [])) {
        const title = (s.title || '').toLowerCase()
        if (title.includes('write assistant') || title.includes('写作上下文') || s.write_workspace) {
          const workspace = s.write_workspace || extractWorkspaceFromTitle(s.title)
          if (workspace) {
            threads[workspace] = s.id || s.path
          }
        }
      }
      set({ writeThreadByWorkspace: threads })
    } catch {
      // Silently ignore — threads will be created on first use
    }
  },
})

// Helper
function removeFromSet<T>(set: Set<T>, item: T): Set<T> {
  const next = new Set(set)
  next.delete(item)
  return next
}

// Extract workspace root from thread title (heuristic)
function extractWorkspaceFromTitle(title: string): string | null {
  // Pattern: "Write Assistant [/home/user/docs]"
  const match = title.match(/\[(.+?)\]/)
  return match ? match[1] : null
}
```

### 5.3 Integration with AppStore

```typescript
// stores/index.ts (additions)
import { createWriteSlice, WriteSlice } from './write'

export type AppStore = ConnectionSlice &
  // ...existing slices...
  WriteSlice

export const useStore = create<AppStore>()((...a) => ({
  // ...existing slices...
  ...createWriteSlice(...a),
}))
```

### 5.4 Helper: createWriteSession

The `initializeWorkspace` action calls `createWriteSession(root)` to create a new Write Assistant session for a workspace. This helper is defined outside the slice:

```typescript
async function createWriteSession(workspaceRoot: string): Promise<string> {
  const result = await loomRpc<{ session_id: string }>('session.create', {
    title: 'Write Assistant',
    metadata: { 
      mode: 'write', 
      workspace_root: workspaceRoot 
    }
  })
  return result.session_id
}
```

Error handling: if session creation fails, show a toast and set `workspaceRoot` to `null` (no workspace loaded). The caller in `initializeWorkspace` should wrap the call in try/catch and handle this case.

---

## 6. VFS Backend Design

### 6.1 Design Rationale

The existing codebase has no VFS. File operations are done through agent tool calls (`file_read`, `file_write`, `file_list`, `file_delete`, `content_search`) within `dispatch/tool.rs`. Write Mode needs a **direct** VFS layer that the UI can call without going through the agent tool system.

We add a new dispatch module `vfs.rs` with JSON-RPC methods prefixed `vfs.*`.

### 6.2 New Backend Module

**File**: `backend/crates/loom-server/src/dispatch/vfs.rs` (new)

```rust
//! VFS dispatch handlers — direct filesystem operations for Write mode.

use loom_types::JsonRpcError;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

use super::err;
use crate::AppState;

const MAX_FILE_SIZE: u64 = 5 * 1024 * 1024; // 5 MB for text files
const MAX_DIR_ENTRIES: usize = 5000;
const SAFE_EXTENSIONS: &[&str] = &[
    "md", "txt", "json", "yaml", "yml", "toml", "xml", "csv",
    "html", "css", "js", "ts", "jsx", "tsx", "vue", "svelte",
    "py", "rs", "go", "java", "c", "cpp", "h", "hpp",
    "svg", "tex", "rst", "adoc", "mmd", "rmd", "qmd",
];

pub async fn handle(
    state: &AppState,
    method: &str,
    p: &Value,
) -> Option<Result<Value, JsonRpcError>> {
    match method {
        "vfs.listDirectory"   => Some(handle_list_directory(p).await),
        "vfs.readFile"        => Some(handle_read_file(p).await),
        "vfs.writeFile"       => Some(handle_write_file(p).await),
        "vfs.createFile"      => Some(handle_create_file(p).await),
        "vfs.createDirectory" => Some(handle_create_directory(p).await),
        "vfs.rename"          => Some(handle_rename(p).await),
        "vfs.delete"          => Some(handle_delete(p).await),
        _ => None,
    }
}
```

### 6.3 VFS Method Specifications

#### `vfs.listDirectory`

```
Params:  { path: string }
Result:  { entries: [{ name, path, kind: 'file'|'directory', mime_type?, size?, modified? }] }
```

- Resolves path; rejects if outside workspace root (path traversal check)
- Sorts: directories first, then files, alphabetical
- Hides dotfiles by default (configurable via `show_hidden: bool` param)
- Caps at 5000 entries; returns `truncated: true` if exceeded

#### `vfs.readFile`

```
Params:  { path: string }
Result:  { content: string | null, size: number, mime_type: string, truncated: boolean }
```

- Reads file as UTF-8
- Files >5MB: returns `content: null, truncated: true, size: N`
- Binary files: detected via null byte check (first 512 bytes); returns `content: null, mime_type: "application/octet-stream"`
- SVG files: treated as text (returned as content)

#### `vfs.writeFile`

```
Params:  { path: string, content: string }
Result:  { ok: true, size: number }
```

- Writes content to path (UTF-8)
- Creates parent directories if they do not exist
- Atomic write: write to temp file, then rename

#### `vfs.createFile`

```
Params:  { path: string }
Result:  { ok: true, path: string }
```

- Creates empty file at path
- Fails if file already exists

#### `vfs.createDirectory`

```
Params:  { path: string }
Result:  { ok: true, path: string }
```

- Creates directory (including parents, like `mkdir -p`)
- No-op if directory already exists

#### `vfs.rename`

```
Params:  { path: string, new_name: string }
Result:  { ok: true, new_path: string }
```

- Renames file or directory within the same parent
- `new_name` is the basename only (not a full path)
- Fails if target already exists

#### `vfs.delete`

```
Params:  { path: string }
Result:  { ok: true }
```

- Deletes file or directory
- Directories are deleted recursively

### 6.4 Security Considerations

| Concern | Mitigation |
|---------|-----------|
| Path traversal (`../../../etc/passwd`) | Canonicalize path; verify resolved path is within the workspace root |
| Symlink attacks | Reject symlinks (follow them for canonicalization, but reject if target is outside workspace) |
| Large file DoS | Max 5MB for text reads; no limit for writes (but backend is local) |
| Hidden file enumeration | Dotfiles hidden by default; `show_hidden: true` required |
| Arbitrary file write | Only writes allowed within workspace root (via path canonicalization) |

### 6.5 Registration in Dispatch

```rust
// dispatch/mod.rs (add)
mod vfs;

// In dispatch_method():
if let Some(result) = vfs::handle(state, method, &p).await {
    return result;
}
```

Placement order: insert `vfs` before `cron` (since VFS is likely to be called more frequently during Write mode usage):

```rust
pub async fn dispatch_method(...) -> Result<Value, JsonRpcError> {
    // ...existing handlers...
    if let Some(result) = vfs::handle(state, method, &p).await {
        return result;
    }
    if let Some(result) = cron::handle(state, method, &p).await {
        return result;
    }
    // ...
}
```

### 6.6 Workspace Root Tracking

The VFS module needs to validate that paths are within the user's workspace root. The workspace root is communicated in each request context:

**Option A (chosen)**: Include `workspace_root` in every VFS request. The backend validates that the resolved path is within `workspace_root`.

```
Params: { path: string, workspace_root: string }
```

This avoids server-side state management and works with multiple concurrent workspace roots.

---

## 7. IPC / Preload Additions

### 7.1 New Preload API Surface

```typescript
// preload/index.ts (additions to LoomApi)

export interface LoomApi {
  // ...existing methods...

  // ── Workspace selection ──
  pickWorkspaceDirectory: () => Promise<string | null>

  // ── Image reading (binary → data URL) ──
  readWorkspaceImage: (filePath: string) => Promise<{ dataUrl: string; mimeType: string }>

  // ── Binary image write (main process IPC, bypasses JSON-RPC) ──
  writeWorkspaceImage: (workspaceRoot: string, relativePath: string, base64Data: string) => Promise<{ ok: boolean; path?: string; message?: string }>

  // ── Export ──
  exportWriteDocument: (opts: {
    filePath: string
    format: 'html' | 'pdf' | 'docx'
    outputPath?: string
  }) => Promise<{ ok: boolean; outputPath?: string; error?: string }>

  copyWriteDocumentAsRichText: (filePath: string) => Promise<{ ok: boolean; error?: string }>

  // ── File Watching ──
  watchFile: (filePath: string) => Promise<void>
  unwatchFile: (filePath: string) => Promise<void>
  onFileChanged: (cb: (payload: { filePath: string }) => void) => void
}
```

### 7.2 Context Bridge Bindings

```typescript
// preload/index.ts (additions in exposeInMainWorld)

contextBridge.exposeInMainWorld('loom', {
  // ...existing...

  pickWorkspaceDirectory: () => ipcRenderer.invoke('pick-workspace-directory'),
  readWorkspaceImage: (filePath: string) =>
    ipcRenderer.invoke('read-workspace-image', filePath),
  writeWorkspaceImage: (workspaceRoot: string, relativePath: string, base64Data: string) =>
    ipcRenderer.invoke('write-workspace-image', workspaceRoot, relativePath, base64Data),
  exportWriteDocument: (opts) =>
    ipcRenderer.invoke('export-write-document', opts),
  copyWriteDocumentAsRichText: (filePath: string) =>
    ipcRenderer.invoke('copy-write-document-as-rich-text', filePath),

  watchFile: (filePath: string) =>
    ipcRenderer.invoke('watch-file', filePath),
  unwatchFile: (filePath: string) =>
    ipcRenderer.invoke('unwatch-file', filePath),
  onFileChanged: (cb) =>
    ipcRenderer.on('file-changed', (_e, payload) => cb(payload)),
})
```

### 7.3 Main Process IPC Handlers

**File**: `frontend/src/main/ipc/write.ts` (new)

```typescript
import { ipcMain, dialog, BrowserWindow, clipboard } from 'electron'
import { readFileSync, writeFileSync, watch, FSWatcher } from 'fs'
import { basename } from 'path'

const watchers = new Map<string, FSWatcher>()

export function registerWriteIpc(): void {
  // ── Workspace selection ──
  ipcMain.handle('pick-workspace-directory', async () => {
    const result = await dialog.showOpenDialog({ properties: ['openDirectory'] })
    return result.canceled ? null : result.filePaths[0]
  })

  // ── Image reading ──
  ipcMain.handle('read-workspace-image', async (_, filePath: string) => {
    try {
      const buf = readFileSync(filePath)
      const ext = basename(filePath).split('.').pop()?.toLowerCase() || 'png'
      const mimeMap: Record<string, string> = {
        png: 'image/png', jpg: 'image/jpeg', jpeg: 'image/jpeg',
        gif: 'image/gif', webp: 'image/webp', bmp: 'image/bmp',
        svg: 'image/svg+xml', ico: 'image/x-icon',
      }
      const mimeType = mimeMap[ext] || 'application/octet-stream'
      const dataUrl = `data:${mimeType};base64,${buf.toString('base64')}`
      return { dataUrl, mimeType }
    } catch {
      return { dataUrl: '', mimeType: '' }
    }
  })

  // ── Binary image write (bypasses JSON-RPC WebSocket) ──
  ipcMain.handle('write-workspace-image', async (_, workspaceRoot: string, relativePath: string, base64Data: string) => {
    const { resolve, join } = await import('path')
    const fs = await import('fs')
    try {
      // Resolve full path within workspace root (path traversal protection)
      const resolved = resolve(join(workspaceRoot, relativePath))
      if (!resolved.startsWith(resolve(workspaceRoot))) {
        return { ok: false, message: 'Path traversal detected' }
      }
      // Ensure parent directory exists
      const dir = resolve(resolved, '..')
      fs.mkdirSync(dir, { recursive: true })
      // Decode base64 and write binary file
      const buf = Buffer.from(base64Data, 'base64')
      fs.writeFileSync(resolved, buf)
      return { ok: true, path: relativePath }
    } catch (e: any) {
      return { ok: false, message: e.message }
    }
  })

  // ── Export (delegates to export-write.ts) ──
  ipcMain.handle('export-write-document', async (_, opts) => {
    const { exportWriteDocument } = await import('./export-write')
    return exportWriteDocument(opts)
  })

  // ── Rich text clipboard ──
  ipcMain.handle('copy-write-document-as-rich-text', async (_, filePath: string) => {
    const { copyAsRichText } = await import('./export-write')
    return copyAsRichText(filePath)
  })

  // ── File watching ──
  ipcMain.handle('watch-file', async (_, filePath: string) => {
    if (watchers.has(filePath)) return
    try {
      const watcher = watch(filePath, (eventType) => {
        if (eventType === 'change') {
          const win = BrowserWindow.getAllWindows()[0]
          if (win) win.webContents.send('file-changed', { filePath })
        }
      })
      watchers.set(filePath, watcher)
    } catch { /* file may not exist yet */ }
  })

  ipcMain.handle('unwatch-file', async (_, filePath: string) => {
    const w = watchers.get(filePath)
    if (w) { w.close(); watchers.delete(filePath) }
  })
}
```

### 7.4 Registration

```typescript
// main/ipc/index.ts (add)
import { registerWriteIpc } from './write'

export function registerIpcHandlers(): void {
  registerFileIpc()
  registerShellIpc()
  registerAppIpc()
  registerWriteIpc()  // new
}
```

---

## 8. CodeMirror Integration

### 8.1 Editor Architecture

The existing codebase has `@tiptap/react` installed (for possible rich text editing) and `@codemirror/*` packages already in `package.json` but **unused**. The design uses CodeMirror 6 (not TipTap) for markdown editing because:

1. CodeMirror is already a dependency (no new dependency)
2. CodeMirror has a robust extension system for live preview, inline completions, and customization
3. DeepSeek-GUI reference implementation uses CodeMirror 6 successfully

### 8.2 Editor Component

**File**: `frontend/src/renderer/src/components/write/WriteMarkdownEditor.tsx` (new)

```typescript
import { useRef, useEffect, useCallback } from 'react'
import { EditorView, keymap, placeholder } from '@codemirror/view'
import { EditorState, StateEffect } from '@codemirror/state'
import { markdown } from '@codemirror/lang-markdown'
import { markdownLivePreview, markdownLivePreviewConfig } from './extensions/markdown-live-preview'
import { writeInlinePlugin } from './extensions/write-inline-plugin'
import { imageDropHandler } from './extensions/image-drop-handler'
import { autosaveExtension } from './extensions/autosave'
import { useStore } from '../../stores'

interface Props {
  readOnly?: boolean
  showLivePreview?: boolean
}

export default function WriteMarkdownEditor({ readOnly = false, showLivePreview = true }: Props) {
  const containerRef = useRef<HTMLDivElement>(null)
  const viewRef = useRef<EditorView | null>(null)
  const setFileContent = useStore(s => s.setFileContent)
  const markDirty = useStore(s => s.markDirty)
  const recordEdit = useStore(s => s.recordEdit)
  const fileContent = useStore(s => s.fileContent)
  const activeFilePath = useStore(s => s.activeFilePath)
  const previewMode = useStore(s => s.previewMode)

  // Initialize or recreate editor when file changes
  useEffect(() => {
    if (!containerRef.current) return

    // Destroy previous editor
    viewRef.current?.destroy()

    const extensions = [
      markdown(),
      keymap.of([
        // Ctrl+Enter → insert newline (not send)
        { key: 'Ctrl-Enter', run: (view) => { view.dispatch(view.state.replaceSelection('\n')); return true } },
        // Ctrl+S → manual save
        { key: 'Ctrl-s', run: () => { useStore.getState().saveFile(); return true } },
      ]),
      placeholder('开始写作...'),
      EditorView.lineWrapping,
      ...(readOnly ? [EditorState.readOnly.of(true)] : []),
      ...(showLivePreview ? [markdownLivePreview, markdownLivePreviewConfig.of({})] : []),
      writeInlinePlugin,
      imageDropHandler,
      autosaveExtension,
      EditorView.updateListener.of((update) => {
        if (update.docChanged) {
          const content = update.state.doc.toString()
          setFileContent(content)
          markDirty()
          // Track recent edit
          const oldText = update.startState.doc.toString()
          recordEdit({
            filePath: activeFilePath || '',
            oldText: oldText.slice(0, 900),
            newText: content.slice(0, 900),
            source: 'user',
          })
        }
      }),
      EditorView.theme({
        '&': { height: '100%' },
        '.cm-scroller': { overflow: 'auto', height: '100%' },
        '.cm-content': { padding: '16px', fontFamily: 'var(--font, inherit)', fontSize: '15px' },
      }),
    ]

    const view = new EditorView({
      state: EditorState.create({ doc: fileContent, extensions }),
      parent: containerRef.current,
    })

    viewRef.current = view
    return () => view.destroy()
  }, [activeFilePath]) // Recreate on file switch

  // Sync external content changes (file watched)
  useEffect(() => {
    const view = viewRef.current
    if (!view) return
    const currentContent = view.state.doc.toString()
    // Only sync if content differs and editor hasn't been modified locally
    if (fileContent !== currentContent && useStore.getState().saveStatus === 'saved') {
      view.dispatch({
        changes: { from: 0, to: currentContent.length, insert: fileContent },
      })
    }
  }, [fileContent, activeFilePath])

  return <div ref={containerRef} style={{ height: '100%', width: '100%' }} />
}
```

### 8.3 Extensions

#### 8.3.1 Markdown Live Preview Extension

**File**: `components/write/extensions/markdown-live-preview.ts` (new)

This is a port from DeepSeek-GUI's `markdown-live-preview.ts`. Key behaviors:

- **CodeMirror 6 StateField**: Replaces code blocks and tables with rendered HTML widgets when the active line is not inside them
- **CodeMirror 6 ViewPlugin (Decoration)**: Hides markdown syntax markers (`**`, `_`, `#`, `[text](url)`) on non-active lines; replaces images with `<img>` widgets; styles bullets and checkboxes
- **Active Line Detection**: Uses `collectActiveLinesFromState(state)` — only hides decorators on lines where the cursor is NOT
- **Render Safety**: Disabled for files >300K chars, non-markdown files, or truncated files

```typescript
// Key exports
export const markdownLivePreview = ViewPlugin.fromClass(MarkdownLivePreviewPlugin)
export const markdownLivePreviewConfig = Facet.define<{ enabled?: boolean }>({})
```

#### 8.3.2 Inline Completion Extension

**File**: `components/write/extensions/write-inline-plugin.ts` (new)

Handles inserting AI-generated text at the cursor position:

```typescript
import { ViewPlugin, Decoration, WidgetType } from '@codemirror/view'
import { StateField, StateEffect } from '@codemirror/state'

// Effect dispatched when AI returns inline completion text
export const insertInlineCompletion = StateEffect.define<{ from: number; to: number; text: string }>()

// StateField: tracks pending inline ghost text
export const inlineGhostField = StateField.define<{ from: number; text: string } | null>({ ... })

// ViewPlugin: renders ghost text as a decoration (faded, non-editable)
export const writeInlinePlugin = ViewPlugin.fromClass(class { ... })
```

#### 8.3.3 Autosave Extension

**File**: `components/write/extensions/autosave.ts` (new)

```typescript
import { ViewPlugin } from '@codemirror/view'

// Debounced autosave: after 650ms of no typing, trigger save
export const autosaveExtension = ViewPlugin.fromClass(class {
  private timer: ReturnType<typeof setTimeout> | null = null

  constructor(view: EditorView) {
    // Listen for document changes via update listener
    // (handled in WriteMarkdownEditor component)
  }

  // The actual debounce logic is in WriteWorkspaceView via a useEffect
  // that watches fileContent and saveStatus, triggering saveFile after 650ms
})
```

#### 8.3.4 Image Paste Handler

**File**: `components/write/extensions/image-drop-handler.ts` (new)

```typescript
import { EditorView } from '@codemirror/view'

// Handles paste events for images:
// 1. Reads pasted image blob
// 2. Converts to base64
// 3. Saves to <workspaceRoot>/images/pasted-<timestamp>.png via window.loom.writeWorkspaceImage()
// 4. Inserts ![](images/pasted-<timestamp>.png) at cursor
//
// Binary files (images, binaries) go through the main process IPC directly.
// Text files go through the JSON-RPC vfs.* backend.
// This split is necessary because JSON-RPC over WebSocket is text-only.
export const imageDropHandler = EditorView.domEventHandlers({
  paste(event, view) { ... },
  drop(event, view) { ... },
})
```

### 8.4 Dependencies (already in package.json)

```
"codemirror": "^6.0.2"
"@codemirror/lang-markdown": "^6.5.0"
"@codemirror/language": "^6.12.3"
"@codemirror/state": "^6.6.0"
"@codemirror/view": "^6.43.0"
```

No new npm dependencies required for the editor.

---

## 9. Export Pipeline Design

### 9.1 Architecture

```
WriteWorkspaceToolbar → ExportMenu
  ├── Export HTML  → window.loom.exportWriteDocument({ format: 'html' })
  ├── Export PDF   → window.loom.exportWriteDocument({ format: 'pdf' })
  ├── Export DOCX  → window.loom.exportWriteDocument({ format: 'docx' })
  └── Copy Rich Text → window.loom.copyWriteDocumentAsRichText(filePath)
                              ↓
                         IPC invoke → main process
                              ↓
                    ┌─────────┴──────────┐
                    │  export-write.ts    │
                    │  (main process)     │
                    └─────────┬──────────┘
                              ↓
              ┌───────────────┼───────────────┐
              ↓               ↓                ↓
         HTML Export     PDF Export        DOCX Export
              ↓               ↓                ↓
    markdown-it → HTML  markdown-it → HTML  markdown-it → HTML
              ↓               ↓                ↓
    inline images     create hidden       html-to-docx
    as base64         BrowserWindow       conversion
              ↓               ↓                ↓
    writeFile to      webContents.       writeFile to
    output path       printToPDF()       output path
              ↓               ↓
    dialog.show      dialog.show
    SaveDialog        SaveDialog
```

### 9.2 Main Process Export Module

**File**: `frontend/src/main/ipc/export-write.ts` (new)

Key functions:

```typescript
import { BrowserWindow, dialog, clipboard, nativeImage } from 'electron'
import { readFileSync, writeFileSync } from 'fs'
import { basename, extname } from 'path'

// ── Markdown → HTML rendering ──
// Uses the existing `renderMarkdown()` from `utils/markdown.ts`
// which wraps markdown-it + highlight.js + katex + mermaid (all already dependencies).
// No new rendering library needed.

function renderMarkdownToHtml(markdown: string, filePath: string): string {
  // Call the shared renderMarkdown() → resolve relative image paths → base64 data URLs
  // Wrap in complete HTML document with CSS for print
}

// ── PDF Export ──
async function exportPdf(html: string, defaultName: string): Promise<string | null> {
  // 1. Create hidden BrowserWindow
  // 2. Load HTML content via data URL
  // 3. Wait for render
  // 4. webContents.printToPDF({ printBackground: true, margins: {...} })
  // 5. Close window
  // 6. dialog.showSaveDialog({ defaultPath: defaultName, filters: [{ name: 'PDF', extensions: ['pdf'] }] })
  // 7. writeFileSync
}

// ── HTML Export ──
async function exportHtml(html: string, defaultName: string): Promise<string | null> {
  // dialog.showSaveDialog → writeFileSync
}

// ── DOCX Export ──
async function exportDocx(markdown: string, defaultName: string): Promise<string | null> {
  // Convert markdown → HTML → DOCX using html-to-docx algorithm
  // (Can implement with a lightweight OOXML builder without adding html-to-docx dependency)
}

// ── Rich Text Clipboard ──
function copyAsRichText(filePath: string): { ok: boolean; error?: string } {
  // Read file, render to HTML
  // clipboard.write({ text: plainText, html: renderedHtml })
}
```

### 9.3 Renderer-Side Export Trigger

```typescript
// In WriteWorkspaceToolbar.tsx
async function handleExport(format: 'html' | 'pdf' | 'docx') {
  const filePath = useStore.getState().activeFilePath
  if (!filePath) return

  try {
    const result = await window.loom.exportWriteDocument({ filePath, format })
    if (result.ok) {
      useStore.getState().addToast({ type: 'success', message: `已导出到 ${result.outputPath}` })
    }
  } catch (e: any) {
    useStore.getState().addToast({ type: 'error', message: `导出失败: ${e.message}` })
  }
}
```

### 9.4 Capability Comparison with DeepSeek-GUI

| Feature | DeepSeek-GUI | openLoom Write Mode | Notes |
|---------|-------------|---------------------|-------|
| Export HTML | Yes (server-side) | Yes (main process) | Use markdown-it (already installed) |
| Export PDF | Yes (Puppeteer) | Yes (Electron printToPDF) | No extra dependency; Electron provides headless printing |
| Export DOCX | Yes (html-to-docx) | Yes (lightweight OOXML builder) | Avoid adding html-to-docx dependency; build minimal OOXML |
| Rich text clipboard | Yes | Yes | Electron clipboard API supports text/html |
| Live Preview | Yes (CM6) | Yes (CM6, port) | Port from reference |
| Inline AI | Yes | Yes | Via existing chat.send RPC |

---

## 10. File Manifest

### New Files

| # | File Path | Purpose |
|---|-----------|---------|
| 1 | `frontend/src/renderer/src/stores/write.ts` | Write mode Zustand slice (~300 lines) |
| 2 | `frontend/src/renderer/src/components/app/ModeRouter.tsx` | Route between Chat/Write workspaces (~20 lines) |
| 3 | `frontend/src/renderer/src/components/write/WriteWorkspaceView.tsx` | Top-level Write shell (~80 lines) |
| 4 | `frontend/src/renderer/src/components/write/WriteWorkspaceView.module.css` | Styles for Write shell (~60 lines) |
| 5 | `frontend/src/renderer/src/components/write/WriteSidebar.tsx` | Workspace switcher + file tree (~200 lines) |
| 6 | `frontend/src/renderer/src/components/write/WriteSidebar.module.css` | Sidebar styles (~80 lines) |
| 7 | `frontend/src/renderer/src/components/write/WriteFileTree.tsx` | Recursive file tree (~120 lines) |
| 8 | `frontend/src/renderer/src/components/write/WriteFileTree.module.css` | File tree styles (~60 lines) |
| 9 | `frontend/src/renderer/src/components/write/WriteWorkspaceToolbar.tsx` | Toolbar with save/preview/export (~100 lines) |
| 10 | `frontend/src/renderer/src/components/write/WriteWorkspaceToolbar.module.css` | Toolbar styles (~50 lines) |
| 11 | `frontend/src/renderer/src/components/write/WriteWorkspaceDocumentPane.tsx` | Editor + preview layout (~80 lines) |
| 12 | `frontend/src/renderer/src/components/write/WriteWorkspaceDocumentPane.module.css` | Layout styles (~40 lines) |
| 13 | `frontend/src/renderer/src/components/write/WriteMarkdownEditor.tsx` | CodeMirror 6 wrapper (~100 lines) |
| 14 | `frontend/src/renderer/src/components/write/WriteMarkdownPreview.tsx` | Rendered markdown preview (~60 lines) |
| 15 | `frontend/src/renderer/src/components/write/WriteImagePreview.tsx` | Image display for non-text (~30 lines) |
| 16 | `frontend/src/renderer/src/components/write/WriteAssistantPanel.tsx` | Write thread chat sidebar (~120 lines) |
| 17 | `frontend/src/renderer/src/components/write/WriteAssistantPanel.module.css` | Chat sidebar styles (~50 lines) |
| 18 | `frontend/src/renderer/src/components/write/WriteInlineAgent.tsx` | Floating inline prompt (~80 lines) |
| 19 | `frontend/src/renderer/src/components/write/WriteInlineAgent.module.css` | Inline agent styles (~40 lines) |
| 20 | `frontend/src/renderer/src/components/write/WriteExportMenu.tsx` | Export format picker dropdown (~60 lines) |
| 21 | `frontend/src/renderer/src/components/write/extensions/markdown-live-preview.ts` | CM6 live preview extension (~250 lines) |
| 22 | `frontend/src/renderer/src/components/write/extensions/write-inline-plugin.ts` | CM6 inline completion plugin (~120 lines) |
| 23 | `frontend/src/renderer/src/components/write/extensions/image-drop-handler.ts` | CM6 image paste/drop handler (~60 lines) |
| 24 | `frontend/src/renderer/src/components/write/extensions/autosave.ts` | CM6 autosave extension (~40 lines) |
| 25 | `frontend/src/renderer/src/components/write/extensions/quoted-selection.ts` | Selection quote helper (~60 lines) |
| 26 | `frontend/src/renderer/src/components/write/extensions/recent-edits.ts` | Recent edits tracker (~80 lines) |
| 27 | `frontend/src/renderer/src/components/write/extensions/write-prompt-builder.ts` | Compose write prompts with context (~60 lines) |
| 28 | `frontend/src/main/ipc/write.ts` | IPC handlers for workspace/image/export (~100 lines) |
| 29 | `frontend/src/main/ipc/export-write.ts` | Export pipeline: HTML/PDF/DOCX (~200 lines) |
| 30 | `backend/crates/loom-server/src/dispatch/vfs.rs` | VFS JSON-RPC methods (~180 lines) |

### Modified Files

| # | File Path | Change |
|---|-----------|--------|
| 1 | `frontend/src/renderer/src/stores/index.ts` | Add `WriteSlice` to `AppStore` union type and `createWriteSlice` to `useStore` creator |
| 2 | `frontend/src/renderer/src/stores/ui.ts` | Add `appMode` field and `switchMode` action |
| 3 | `frontend/src/renderer/src/components/app/AppShell.tsx` | Replace `<ChatWorkspace />` with `<ModeRouter />` |
| 4 | `frontend/src/preload/index.ts` | Add VFS/image/export/file-watch methods to `LoomApi` and `exposeInMainWorld` |
| 5 | `frontend/src/main/ipc/index.ts` | Add `registerWriteIpc()` call |
| 6 | `frontend/src/renderer/src/components/input/QuotedSelectionCard.tsx` | Add support for Write-mode quoted selections (file path prefixed with workspace) |
| 7 | `backend/crates/loom-server/src/dispatch/mod.rs` | Add `mod vfs` and `vfs::handle(...)` in dispatch chain |

### Deletions

None.

### Total File Count

- **30 new files**, **7 modified files**
- ~2,600 lines of new frontend code (TSX + CSS)
- ~180 lines of new backend code (Rust)
- ~100 lines of new main process code (TypeScript)

---

## 11. Implementation Phases

### Phase 1 (Week 1): Workspace Foundation

**Goal**: User can pick a workspace directory, browse files in a tree, and switch between Chat/Write modes.

| Day | Task | Files |
|-----|------|-------|
| 1.1 | Add `WriteSlice` to store, basic state shape | `stores/write.ts` (init), `stores/index.ts` |
| 1.2 | Add `appMode` to `UiSlice`, create `ModeRouter` | `stores/ui.ts`, `ModeRouter.tsx`, `AppShell.tsx` |
| 1.3 | Implement `vfs.rs` backend module (listDirectory, readFile, writeFile) | `dispatch/vfs.rs`, `dispatch/mod.rs` |
| 1.4 | Add IPC handlers for folder selection, image reading | `main/ipc/write.ts`, `preload/index.ts`, `main/ipc/index.ts` |
| 1.5 | Build `WriteWorkspaceView` shell + `WriteSidebar` with workspace picker | `WriteWorkspaceView.tsx`, `WriteSidebar.tsx` + CSS |
| 1.6 | Build `WriteFileTree` with recursive rendering, expand/collapse | `WriteFileTree.tsx` + CSS |
| 1.7 | Implement remaining VFS methods (createFile, createDirectory, rename, delete) | `dispatch/vfs.rs` |
| 1.8 | Context menu actions in file tree (new file, new folder, rename, delete) + dialogs | `WriteSidebar.tsx` (extend) |

**Deliverable**: Workspace picker works, file tree renders, create/rename/delete work, mode switching works.

---

### Phase 2 (Week 2): Editor + Preview

**Goal**: Fully functional markdown editor with live preview, autosave, and file watching.

| Day | Task | Files |
|-----|------|-------|
| 2.1 | Basic `WriteMarkdownEditor` with CodeMirror 6 + markdown language | `WriteMarkdownEditor.tsx` |
| 2.2 | `WriteWorkspaceDocumentPane` with Source/Live view modes | `WriteWorkspaceDocumentPane.tsx` + CSS |
| 2.3 | Port `markdown-live-preview` CM6 extension from reference | `extensions/markdown-live-preview.ts` |
| 2.4 | Implement `WriteMarkdownPreview` using existing `renderMarkdown()` pipeline (markdown-it + highlight.js + katex + mermaid) | `WriteMarkdownPreview.tsx` |
| 2.5 | Implement Split View (editor + preview side-by-side with resizable pane) | `WriteWorkspaceDocumentPane.tsx` (extend) |
| 2.6 | Implement Preview-only mode | `WriteWorkspaceDocumentPane.tsx` (extend) |
| 2.7 | Autosave (650ms debounce via `saveFile` action) | `extensions/autosave.ts`, `WriteWorkspaceView.tsx` |
| 2.8 | File watching (IPC: watchFile/unwatchFile/onFileChanged) + external sync animation | `main/ipc/write.ts`, `preload/index.ts`, `WriteMarkdownEditor.tsx` |
| 2.9 | `WriteWorkspaceToolbar` with file name, save status, preview mode picker | `WriteWorkspaceToolbar.tsx` + CSS |
| 2.10 | `WriteImagePreview` for png/jpg/gif/webp files | `WriteImagePreview.tsx` |

**Deliverable**: Editor works with all 4 preview modes, autosave works, external changes detected.

---

### Phase 3 (Week 3): Assistant + Export

**Goal**: Inline AI assistance, Write thread management, and full export pipeline.

| Day | Task | Files |
|-----|------|-------|
| 3.1 | `WriteAssistantPanel` with MessageTimeline + FloatingComposer | `WriteAssistantPanel.tsx` + CSS |
| 3.2 | Thread registry logic in store (hydrate, bind, unbind) | `stores/write.ts` (extend) |
| 3.3 | `WriteInlineAgent` — floating composer at cursor position | `WriteInlineAgent.tsx` + CSS |
| 3.4 | Inline completion CM6 plugin + prompt builder | `extensions/write-inline-plugin.ts`, `extensions/write-prompt-builder.ts` |
| 3.5 | Quoted selection extraction + prompt composition | `extensions/quoted-selection.ts` |
| 3.6 | Recent edits tracking | `extensions/recent-edits.ts` |
| 3.7 | Export HTML pipeline (markdown-it → HTML → save) | `main/ipc/export-write.ts` (start) |
| 3.8 | Export PDF pipeline (hidden BrowserWindow → printToPDF) | `main/ipc/export-write.ts` (extend) |
| 3.9 | Export DOCX pipeline (lightweight OOXML builder) | `main/ipc/export-write.ts` (extend) |
| 3.10 | Rich text clipboard (text/html + text/plain) | `main/ipc/export-write.ts` (extend) |
| 3.11 | `WriteExportMenu` dropdown in toolbar | `WriteExportMenu.tsx` |
| 3.12 | Image paste/drop handler in editor | `extensions/image-drop-handler.ts` |
| 3.13 | Polish: loading states, error handling, empty states | All write components |
| 3.14 | Integration testing: open file, edit, AI-edit, preview, export | E2E tests |

**Deliverable**: Full Write mode with inline AI, export, and polish.

---

## 12. Testing Strategy

### 12.1 Unit Tests (Vitest)

| Area | Tests |
|------|-------|
| `stores/write.ts` | Initialize workspace, load directory, toggle directory, open file (text + image), create/rename/delete entries, autosave state transitions, recent edits merging, quoted selection management, thread binding/unbinding |
| `dispatch/vfs.rs` (Rust `#[cfg(test)]`) | Path traversal rejection, symlink rejection, directory listing (sorted, dotfile filtering), readFile with various mime types, writeFile atomicity, createFile duplicate rejection, rename basename-only validation, delete recursive |

### 12.2 Component Tests (Vitest + @testing-library/react)

| Component | Tests |
|-----------|-------|
| `ModeRouter` | Renders ChatWorkspace when appMode='chat', renders WriteWorkspaceView when appMode='write' |
| `WriteSidebar` | Renders workspace picker, lists workspace roots, "Browse..." button triggers IPC, file tree visibility |
| `WriteFileTree` | Renders entries sorted (dirs first), expand/collapse toggle, right-click context menu, empty directory message |
| `WriteWorkspaceToolbar` | Shows file name when active, save status indicator colors, preview mode buttons, export button states |
| `WriteMarkdownPreview` | Renders headings, code blocks, tables, images from markdown content |
| `WriteImagePreview` | Renders img with correct src for data URL |

### 12.3 Integration Tests (Vitest)

| Scenario | Steps |
|----------|-------|
| Mode switch preserves state | Switch Chat→Write→Chat, verify messages/editor content preserved |
| File open flow | Click file in tree → editor loads content → preview renders |
| Autosave flow | Type in editor → wait 650ms → verify saveStatus changes dirty→saving→saved |
| Create file flow | Right-click dir → "New File" → enter name → file appears in tree → auto-opens |
| External change detection | Watch file → modify externally → editor reflects new content |

### 12.4 E2E Tests (Playwright)

| Scenario | Steps |
|----------|-------|
| Full Write workflow | Switch to Write mode → create workspace → create file → type text → preview → save → export to HTML |
| AI inline assistance | Open Write → select text → open inline agent → type "expand this" → verify AI response inserted |
| Export formats | Create markdown with code blocks + images → export HTML → export PDF → export DOCX → verify files exist |
| File operations | Create file → rename → delete → verify tree updates |
| Mode switch with dirty file | Edit file → switch to Chat → verify prompt → switch back → verify content preserved |

### 12.5 Manual QA Checklist

- [ ] Workspace picker: browse, create, switch, recent list
- [ ] File tree: expand/collapse, lazy-load children, dotfiles hidden
- [ ] Editor: typing, paste, undo/redo, keyboard shortcuts (Ctrl+S save, Ctrl+Enter newline)
- [ ] Live preview: markdown syntax hidden, code blocks rendered, tables with borders
- [ ] Split view: resizable pane, scroll sync
- [ ] Autosave: dirty indicator, save on debounce, save on file close
- [ ] File watching: external edit detected, content reloads (non-destructive)
- [ ] Image: paste from clipboard, drop from file system
- [ ] Inline AI: open composer, send prompt, response inserted at cursor
- [ ] Export: HTML rendering, PDF with images, DOCX formatting, rich text clipboard paste into Word
- [ ] Large files: >5MB shows size warning; >300K chars disables live preview
- [ ] Binary files: image files open in preview mode, unknown binaries show error
- [ ] Workspace switching: threads follow workspace
- [ ] Mode switching: Ctrl+Shift+W / Ctrl+Shift+C, state preserved

---

## Appendix A: Existing Dependencies Usable Without New Install

These are already in `package.json` and can be leveraged:

| Package | Usage in Write Mode |
|---------|--------------------|
| `codemirror` + `@codemirror/*` | Editor core |
| `markdown-it` | Markdown to HTML for export and preview |
| `highlight.js` | Code syntax highlighting in preview |
| `katex` | Math rendering in preview |
| `mermaid` | Diagram rendering |
| `zustand` | Store (write slice) |
| `fflate` | Compression for DOCX OOXML (optional) |

**Note**: We use the existing markdown rendering pipeline (`utils/markdown.ts`) for preview and export to avoid adding a new dependency. The `renderMarkdown()` function wraps markdown-it + highlight.js + katex + mermaid. `WriteMarkdownPreview` uses `dangerouslySetInnerHTML` with sanitized output from `renderMarkdown()`. The same pipeline is used in chat message rendering (TextBlock.tsx), ensuring visual consistency between Chat and Write modes. Sanitization utilities already exist in `utils/markdown-sanitizer.ts`.

## Appendix B: Configuration Persistence

Preferences stored via `window.loom.setPreference()`:

| Key | Type | Default |
|-----|------|---------|
| `defaultWorkspaceRoot` | `string \| null` | `null` |
| `recentWorkspaceRoots` | `string[]` | `[]` |
| `writePreviewMode` | `'source' \| 'live' \| 'split' \| 'preview'` | `'live'` |
| `writeAssistantOpen` | `boolean` | `false` |

## Appendix C: Keyboard Shortcuts Summary

| Shortcut | Context | Action |
|----------|---------|--------|
| Ctrl+Shift+W | Global | Switch to Write mode |
| Ctrl+Shift+C | Global | Switch to Chat mode |
| Ctrl+S | Write editor | Manual save |
| Ctrl+Enter | Write editor | Insert newline (does NOT send) |
| Ctrl+N | Write mode | New file (in current directory) |
| Ctrl+Shift+N | Write mode | New folder |
| Delete | Write file tree (selected) | Delete file/folder (with confirmation) |
| F2 | Write file tree (selected) | Rename |
| Ctrl+B | Global | Toggle sidebar |
| Ctrl+P | Write mode | Quick file open (by name, fuzzy search) |

---

## Appendix D: Open Questions & Future Considerations

### Q1: Rich Text Editing (TipTap)?
The codebase has `@tiptap/*` installed but unused. For Phase 1-3, we use plain markdown + CodeMirror. Future phases could explore TipTap for WYSIWYG markdown editing, but this adds significant complexity (serialization bridge, image handling, toolbar UI).

### Q2: Multi-tab File Editing?
The current design opens one file at a time (like DeepSeek-GUI). If users request multi-tab editing, we can extend `WriteSlice` with `openFiles: Map<string, OpenFileState>` and add a tab bar. Out of scope for v1.

### Q3: Real-time Collaboration?
Not in scope. Write mode is local-first.

### Q4: Git Integration in File Tree?
Future: show git status indicators (modified, added, deleted) in the file tree. Requires integrating with the git2 library or spawning git commands.

### Q5: "Open in Write" from Chat?
Low-hanging fruit for Phase 3+. When an agent generates a large markdown block, offer "Open in Write" that copies content to a new file and switches modes.

### Q6: PDF Export via Puppeteer?
The design uses Electron's built-in `printToPDF()` to avoid adding a 300MB Puppeteer dependency. This works well for markdown documents. For complex layouts (CSS grid, custom fonts), Electron's Chromium handles it natively.

# Frontend UI Redesign — Refined Glass v2

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete visual overhaul of openLoom frontend — fix contrast, hierarchy, and UX while keeping Refined Glass direction with Cyan #22D3EE.

**Architecture:** CSS variables define the design system in base.css; components use Tailwind utilities referencing those variables. No new dependencies. Pure styling + layout refactor.

**Tech Stack:** React 19, Tailwind CSS 4, CSS custom properties, Electron 38

**Spec:** `docs/superpowers/specs/2026-05-28-frontend-redesign.md`

---

## File Structure

| File | Responsibility |
|------|---------------|
| `src/renderer/src/styles/base.css` | Design tokens, reset, animations, shared patterns |
| `src/renderer/src/components/app/AppShell.tsx` | Root layout: titlebar + sidebar + main |
| `src/renderer/src/components/app/Sidebar.tsx` | Session list, search, create button |
| `src/renderer/src/components/app/SessionItem.tsx` | Individual session row |
| `src/renderer/src/components/app/StatusBar.tsx` | DELETE this file |
| `src/renderer/src/components/chat/ChatArea.tsx` | Message list container |
| `src/renderer/src/components/chat/AssistantMessage.tsx` | Full-width assistant layout |
| `src/renderer/src/components/chat/UserMessage.tsx` | Right-aligned bubble |
| `src/renderer/src/components/input/InputArea.tsx` | Centered composer |
| `src/renderer/src/components/input/PermissionModeButton.tsx` | Pill button |
| `src/renderer/src/components/input/ThinkingLevelButton.tsx` | Pill button |
| `src/renderer/src/components/input/ModelSelector.tsx` | Pill dropdown |
| `src/renderer/src/components/shared/WelcomeScreen.tsx` | Empty state |
| `src/renderer/src/components/shared/SettingsModal.tsx` | Full redesign |
| `src/renderer/src/components/shared/Overlay.tsx` | Modal wrapper (larger) |
| `src/renderer/src/components/shared/Button.tsx` | Add pill variant |

---

### Task 1: Rewrite Design Tokens (base.css)

**Files:**
- Modify: `src/renderer/src/styles/base.css`

- [ ] **Step 1: Replace CSS variables section**

Replace the entire `:root` block with the new color system:

```css
:root {
  /* ── Surface (3-level depth) ── */
  --bg:         #0D1117;
  --bg-surface: rgba(34,211,238, 0.03);
  --bg-card:    rgba(34,211,238, 0.06);
  --bg-active:  rgba(34,211,238, 0.10);
  --bg-overlay: rgba(0,0,0, 0.65);

  /* ── Ink (3-level text) ── */
  --text:        rgba(255,255,255, 0.88);
  --text-secondary: rgba(255,255,255, 0.60);
  --text-muted:  rgba(255,255,255, 0.30);

  /* ── Borders ── */
  --border:       rgba(34,211,238, 0.06);
  --border-default: rgba(34,211,238, 0.10);
  --border-accent:  rgba(34,211,238, 0.18);

  /* ── Accent: Cyan #22D3EE ── */
  --accent:        #22D3EE;
  --accent-hover:  #67E8F9;
  --accent-rgb:    34,211,238;
  --accent-subtle: rgba(34,211,238, 0.08);
  --accent-medium: rgba(34,211,238, 0.14);
  --accent-glow:   rgba(34,211,238, 0.20);

  /* ── Semantic ── */
  --green:       #2DD4BF;
  --green-light: rgba(45,212,191,0.10);
  --amber:       #F59E0B;
  --amber-light: rgba(245,158,11,0.10);
  --red:         #EF4444;
  --red-light:   rgba(239,68,68,0.10);

  /* ── Typography ── */
  --font:      "Inter",-apple-system,BlinkMacSystemFont,"Segoe UI",Roboto,sans-serif;
  --font-mono: "JetBrains Mono","Cascadia Code","Fira Code","Consolas",monospace;

  /* ── Radii ── */
  --r-sm: 6px;
  --r-md: 8px;
  --r-lg: 10px;
  --r-xl: 14px;
  --r-2xl: 16px;
  --r-full: 9999px;

  /* ── Shadows ── */
  --shadow:       0 1px 3px rgba(0,0,0,0.4);
  --shadow-md:    0 4px 12px rgba(0,0,0,0.5);
  --shadow-lg:    0 8px 28px rgba(0,0,0,0.6);
  --shadow-glow:  0 0 16px var(--accent-glow);
  --shadow-composer: 0 0 24px rgba(34,211,238,0.03), 0 4px 16px rgba(0,0,0,0.3);

  /* ── Animation ── */
  --ease-out:     cubic-bezier(0.16,1,0.3,1);
  --ease-in:      cubic-bezier(0.4,0,1,1);
  --ease-standard:cubic-bezier(0.2,0,0,1);
  --dur-fast:     0.15s;
  --dur-normal:   0.2s;
  --dur-slow:     0.25s;

  /* ── Shell ── */
  --sidebar-w:   220px;
  --titlebar-h:  48px;
}
```

- [ ] **Step 2: Update shared component patterns**

Replace the `.pill` and `.pill-neutral` classes:

```css
.pill{
  display:inline-flex;align-items:center;
  height:26px;padding:0 10px;
  font-size:11px;font-weight:500;
  background:var(--accent-subtle);
  border:1px solid rgba(34,211,238,0.12);
  border-radius:var(--r-full);
  color:var(--accent);
  cursor:pointer;
  transition:all var(--dur-fast) var(--ease-out);
}
.pill:hover{background:var(--accent-medium);border-color:var(--border-accent)}

.pill-neutral{
  display:inline-flex;align-items:center;
  height:26px;padding:0 10px;
  font-size:11px;font-weight:500;
  background:rgba(255,255,255,0.02);
  border:1px solid rgba(255,255,255,0.06);
  border-radius:var(--r-full);
  color:var(--text-secondary);
  cursor:pointer;
  transition:all var(--dur-fast) var(--ease-out);
}
.pill-neutral:hover{background:rgba(255,255,255,0.05);color:var(--text)}
```

- [ ] **Step 3: Update .prose-chat code block colors**

Replace the `.prose-chat pre` and `.prose-chat code` rules to use the new tokens:

```css
.prose-chat pre{
  background:rgba(34,211,238,0.03);padding:.75rem 1rem;
  border-radius:var(--r-md);overflow-x:auto;
  font-size:12.5px;font-family:var(--font-mono);
  border:1px solid var(--border);
}
.prose-chat code{
  font-size:12.5px;font-family:var(--font-mono);
  background:var(--accent-subtle);padding:.15em .4em;
  border-radius:var(--r-sm);color:var(--accent);
}
```

- [ ] **Step 4: Verify by running dev server**

Run: `npm run dev` (user runs manually)
Expected: App loads with updated colors — higher contrast text, visible borders.

- [ ] **Step 5: Commit**

```bash
git add src/renderer/src/styles/base.css
git commit -m "style: rewrite design tokens — new color system, contrast fix, radii update"
```

---

### Task 2: AppShell Layout + Remove StatusBar

**Files:**
- Modify: `src/renderer/src/components/app/AppShell.tsx`
- Delete: `src/renderer/src/components/app/StatusBar.tsx`

- [ ] **Step 1: Rewrite AppShell.tsx**

```tsx
import { useEffect } from 'react'
import { useStore } from '../../stores'
import Sidebar from './Sidebar'
import WindowControls from './WindowControls'
import ChatWorkspace from '../chat/ChatWorkspace'
import { IconPanelLeftClose, IconPanelLeft } from '../../utils/icons'

export default function AppShell() {
  const sidebarOpen = useStore(s => s.sidebarOpen)
  const toggleSidebar = useStore(s => s.toggleSidebar)
  const currentSessionId = useStore(s => s.currentSessionId)
  const sessions = useStore(s => s.sessions)
  const wsState = useStore(s => s.wsState)

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === 'b') {
        e.preventDefault()
        toggleSidebar()
      }
    }
    window.addEventListener('keydown', handler)
    return () => window.removeEventListener('keydown', handler)
  }, [toggleSidebar])

  const currentTitle = currentSessionId
    ? sessions.find(s => s.path === currentSessionId)?.title || 'openLoom'
    : 'openLoom'

  return (
    <div className="h-screen flex flex-col bg-[var(--bg)]">
      {/* Titlebar — 48px */}
      <header
        data-drag
        className="flex items-center shrink-0 bg-[var(--bg)] border-b border-[var(--border)] px-4 z-10"
        style={{ height: 48 }}
      >
        {/* Left: sidebar toggle */}
        <div data-no-drag className="flex items-center gap-3 flex-shrink-0">
          <button
            onClick={toggleSidebar}
            className="flex items-center justify-center w-7 h-7 rounded-[var(--r-md)] text-[var(--text-muted)] hover:text-[var(--accent)] hover:bg-[var(--accent-subtle)] transition-colors"
            title="⌘B 切换侧边栏"
          >
            {sidebarOpen ? <IconPanelLeftClose size={16} /> : <IconPanelLeft size={16} />}
          </button>
        </div>

        {/* Center: session title */}
        <div className="flex-1 text-center">
          <span className="text-[12px] text-[var(--text-muted)]">{currentTitle}</span>
        </div>

        {/* Right: connection + window controls */}
        <div data-no-drag className="flex items-center gap-3 flex-shrink-0">
          <div className="flex items-center gap-1.5">
            <span className="w-[6px] h-[6px] rounded-full"
              style={{
                background: wsState === 'connected' ? 'var(--accent)' : wsState === 'reconnecting' ? 'var(--amber)' : 'var(--red)',
                boxShadow: wsState === 'connected' ? '0 0 4px rgba(34,211,238,0.4)' : 'none',
              }} />
            <span className="text-[10px] text-[var(--text-muted)]">
              {wsState === 'connected' ? '已连接' : wsState === 'reconnecting' ? '重连中' : '离线'}
            </span>
          </div>
          <WindowControls />
        </div>
      </header>

      {/* Body */}
      <div className="flex flex-1 overflow-hidden">
        <div
          className="shrink-0 overflow-hidden transition-all duration-200 ease-[var(--ease-out)]"
          style={{ width: sidebarOpen ? 220 : 0, opacity: sidebarOpen ? 1 : 0 }}
        >
          <Sidebar />
        </div>
        <main data-content className="flex-1 flex flex-col min-w-0 relative bg-[var(--bg)]">
          <ChatWorkspace />
        </main>
      </div>
    </div>
  )
}
```

- [ ] **Step 2: Delete StatusBar.tsx**

Delete file: `src/renderer/src/components/app/StatusBar.tsx`

- [ ] **Step 3: Remove StatusBar import from AppShell**

Already removed in Step 1. Verify no other file imports StatusBar:

Run: Search for `StatusBar` in `src/renderer/src/`

- [ ] **Step 4: Commit**

```bash
git add src/renderer/src/components/app/AppShell.tsx
git rm src/renderer/src/components/app/StatusBar.tsx
git commit -m "style: redesign AppShell — 48px titlebar, remove redundant StatusBar"
```

---

### Task 3: Sidebar Redesign

**Files:**
- Modify: `src/renderer/src/components/app/Sidebar.tsx`

- [ ] **Step 1: Rewrite Sidebar.tsx**

```tsx
import { useState, useMemo, useRef, useEffect } from 'react'
import { useStore } from '../../stores'
import SessionItem from './SessionItem'
import { IconPlus, IconSearch, IconSettings, IconPanelLeftClose } from '../../utils/icons'

function getDateGroup(modified: string): string {
  if (!modified) return '今天'
  const d = new Date(modified)
  const now = new Date()
  const today = new Date(now.getFullYear(), now.getMonth(), now.getDate())
  const yesterday = new Date(today)
  yesterday.setDate(yesterday.getDate() - 1)
  const day = new Date(d.getFullYear(), d.getMonth(), d.getDate())
  if (day >= today) return '今天'
  if (day >= yesterday) return '昨天'
  return `${d.getMonth() + 1}月${d.getDate()}日`
}

export default function Sidebar() {
  const sessions = useStore((s) => s.sessions)
  const pinnedIds = useStore((s) => s.pinnedIds)
  const createSession = useStore((s) => s.createSession)
  const switchSession = useStore((s) => s.switchSession)
  const setSettingsOpen = useStore((s) => s.setSettingsOpen)
  const setSidebarOpen = useStore((s) => s.setSidebarOpen)
  const [query, setQuery] = useState('')
  const inputRef = useRef<HTMLInputElement>(null)

  useEffect(() => { inputRef.current?.focus() }, [])

  const filtered = useMemo(() => {
    if (!query.trim()) return sessions
    const q = query.toLowerCase()
    return sessions.filter(s => (s.title||'').toLowerCase().includes(q) || (s.firstMessage||'').toLowerCase().includes(q) || s.path.toLowerCase().includes(q))
  }, [sessions, query])

  const pinned = useMemo(() => filtered.filter(s => pinnedIds.has(s.path)), [filtered, pinnedIds])

  const dateGroups = useMemo(() => {
    const unpinned = filtered.filter(s => !pinnedIds.has(s.path))
    const map = new Map<string, typeof unpinned>()
    const order: string[] = []
    for (const s of unpinned) {
      const label = getDateGroup(s.modified)
      if (!map.has(label)) { map.set(label, []); order.push(label) }
      map.get(label)!.push(s)
    }
    return order.map(label => ({ label, sessions: map.get(label)! }))
  }, [filtered, pinnedIds])

  const handleCreate = async () => {
    const id = await createSession()
    if (id) await switchSession(id)
  }

  return (
    <aside className="flex flex-col h-full bg-[var(--bg-surface)] border-r border-[var(--border)]" style={{ width: 220 }}>
      {/* Header */}
      <div className="flex items-center justify-between px-4 border-b border-[var(--border)]" style={{ height: 48 }}>
        <div className="flex items-center gap-2">
          <div className="w-[22px] h-[22px] rounded-[6px] bg-[rgba(34,211,238,0.12)] border border-[var(--border-accent)] flex items-center justify-center">
            <span className="text-[9px] font-extrabold text-[var(--accent)]">L</span>
          </div>
          <span className="text-[13px] font-semibold text-[var(--text)] tracking-tight">openLoom</span>
        </div>
        <button
          onClick={() => setSidebarOpen(false)}
          className="flex items-center justify-center w-7 h-7 rounded-[var(--r-md)] text-[var(--text-muted)] hover:text-[var(--accent)] hover:bg-[var(--accent-subtle)] transition-colors"
          title="收起侧边栏 (⌘B)"
        >
          <IconPanelLeftClose size={14} />
        </button>
      </div>

      {/* Search */}
      <div className="px-3 py-2.5">
        <div className="flex items-center gap-2 h-[32px] px-3 rounded-[var(--r-md)] bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.06)]">
          <IconSearch size={13} className="text-[var(--text-muted)] shrink-0" />
          <input
            ref={inputRef}
            value={query}
            onChange={e => setQuery(e.target.value)}
            onKeyDown={e => e.key==='Escape'&&setQuery('')}
            placeholder="搜索会话..."
            className="flex-1 bg-transparent text-[var(--text)] text-[12px] outline-none placeholder:text-[var(--text-muted)]"
          />
        </div>
      </div>

      {/* Session list */}
      <div className="flex-1 overflow-y-auto px-2">
        {filtered.length === 0 ? (
          <div className="px-4 py-16 text-center">
            <p className="text-[12px] text-[var(--text-muted)]">
              {query ? '无匹配会话' : '暂无会话'}
            </p>
          </div>
        ) : (
          <>
            {pinned.length > 0 && (
              <div className="mb-1">
                <div className="px-3 pt-3 pb-1.5 text-[10px] font-semibold uppercase tracking-[0.5px] text-[var(--text-muted)]">
                  已置顶
                </div>
                {pinned.map(s => <SessionItem key={s.path} session={s} />)}
              </div>
            )}
            {dateGroups.map(({ label, sessions: gs }) => (
              <div key={label} className="mb-1">
                <div className="px-3 pt-3 pb-1.5 text-[10px] font-semibold uppercase tracking-[0.5px] text-[var(--text-muted)]">
                  {label}
                </div>
                {gs.map(s => <SessionItem key={s.path} session={s} />)}
              </div>
            ))}
          </>
        )}
      </div>

      {/* Bottom actions */}
      <div className="flex items-center gap-2 px-3 py-3 border-t border-[var(--border)]">
        <button onClick={handleCreate}
          className="flex items-center gap-1.5 flex-1 h-[32px] px-3 text-[12px] font-medium text-[var(--accent)] bg-[var(--accent-subtle)] hover:bg-[var(--accent-medium)] border border-[rgba(34,211,238,0.12)] rounded-[var(--r-md)] transition-colors justify-center">
          <IconPlus size={13} /> 新建会话
        </button>
        <button onClick={() => setSettingsOpen(true)}
          className="w-[32px] h-[32px] flex items-center justify-center rounded-[var(--r-md)] text-[var(--text-muted)] hover:text-[var(--text)] hover:bg-[rgba(255,255,255,0.04)] transition-colors">
          <IconSettings size={15} />
        </button>
      </div>
    </aside>
  )
}
```

- [ ] **Step 2: Commit**

```bash
git add src/renderer/src/components/app/Sidebar.tsx
git commit -m "style: redesign Sidebar — 220px width, better spacing and contrast"
```

---

### Task 4: SessionItem Redesign

**Files:**
- Modify: `src/renderer/src/components/app/SessionItem.tsx`

- [ ] **Step 1: Update SessionItem styling**

Replace the className on the root `<div>` (line 36-40) with:

```tsx
className={`group relative flex items-center gap-2 px-2.5 py-2 cursor-pointer rounded-[var(--r-md)] transition-all duration-150 ${
  isActive
    ? 'bg-[var(--bg-card)] border border-[var(--border-default)]'
    : 'border border-transparent hover:bg-[rgba(255,255,255,0.02)]'
}`}
```

- [ ] **Step 2: Update session text styling**

Replace the inner `<div className="flex-1 min-w-0">` block content:

```tsx
<div className="flex-1 min-w-0">
  <div className={`truncate text-[12px] leading-snug ${isActive ? 'text-[var(--text)] font-medium' : 'text-[var(--text-secondary)]'}`}>
    {session.title || session.firstMessage?.slice(0, 40) || `会话 ${sid.slice(0, 8)}`}
  </div>
  {(session.modified || session.messageCount > 0) && (
    <div className="flex items-center gap-1 text-[10px] text-[var(--text-muted)] mt-0.5">
      {session.modified && <span>{relativeTime(session.modified)}</span>}
      {session.modified && session.messageCount > 0 && <span>·</span>}
      {session.messageCount > 0 && <span>{session.messageCount}条消息</span>}
    </div>
  )}
</div>
```

- [ ] **Step 3: Commit**

```bash
git add src/renderer/src/components/app/SessionItem.tsx
git commit -m "style: redesign SessionItem — card-style active state, better contrast"
```

---

### Task 5: Chat Area + Message Layout (Hybrid)

**Files:**
- Modify: `src/renderer/src/components/chat/ChatArea.tsx`
- Modify: `src/renderer/src/components/chat/AssistantMessage.tsx`
- Modify: `src/renderer/src/components/chat/UserMessage.tsx`

- [ ] **Step 1: Update ChatArea.tsx spacing**

Replace the scrollable container (line 38):

```tsx
<div ref={scrollRef} className="flex-1 overflow-y-auto" style={{ padding: '28px 2rem 140px' }}>
  <div className="max-w-[640px] mx-auto space-y-6">
```

And the empty state logo (lines 28-29) — update cyan values:

```tsx
<div className="w-12 h-12 mx-auto mb-4 rounded-[var(--r-lg)] bg-[var(--accent-subtle)] border border-[var(--border-default)] flex items-center justify-center shadow-[var(--shadow-glow)]">
  <span className="text-xl font-bold text-[var(--accent)]">L</span>
</div>
<p className="text-[14px] text-[var(--text-muted)]">发送消息开始对话</p>
```

- [ ] **Step 2: Rewrite AssistantMessage.tsx (full-width, no bubble)**

```tsx
import type { Message } from '../../stores/chat'
import ThinkingBlock from './ThinkingBlock'
import ToolGroupBlock from './ToolGroupBlock'
import TextBlock from './TextBlock'
import FileBlock from './FileBlock'
import SubagentCard from './SubagentCard'
import MessageFooterActions from './MessageFooterActions'
import TypingIndicator from '../shared/TypingIndicator'

export default function AssistantMessage({ message }: { message: Message }) {
  return (
    <div className="animate-fade-in">
      {/* Header: avatar + name + time */}
      <div className="flex items-center gap-2 mb-2">
        <div className="w-5 h-5 rounded-[6px] bg-[var(--accent-subtle)] border border-[var(--border-default)] flex items-center justify-center">
          <span className="text-[8px] font-extrabold text-[var(--accent)]">L</span>
        </div>
        <span className="text-[11px] font-medium text-[var(--text-secondary)]">Loom</span>
        {message.timestamp && (
          <span className="text-[10px] text-[var(--text-muted)]">
            {new Date(message.timestamp).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}
          </span>
        )}
      </div>

      {/* Content — indented under avatar */}
      <div className="pl-7 space-y-2">
        {message.blocks.map((block, i) => {
          switch (block.type) {
            case 'thinking':
              return <ThinkingBlock key={i} block={block} />
            case 'tool_group':
              return <ToolGroupBlock key={i} block={block} />
            case 'text':
              return <TextBlock key={i} block={block} />
            case 'file':
              return <FileBlock key={i} block={block} />
            case 'subagent':
              return <SubagentCard key={i} block={block} />
            default:
              return null
          }
        })}
        {message.blocks.length === 0 && (
          <div className="flex items-center gap-2 text-[13px] text-[var(--text-muted)]">
            <span>思考中</span>
            <TypingIndicator />
          </div>
        )}
        {message.blocks.length > 0 && (
          <MessageFooterActions messageId={message.id} role="assistant" timestamp={message.timestamp} />
        )}
      </div>
    </div>
  )
}
```

- [ ] **Step 3: Rewrite UserMessage.tsx (right-aligned bubble)**

```tsx
import type { Message } from '../../stores/chat'
import MessageFooterActions from './MessageFooterActions'

export default function UserMessage({ message }: { message: Message }) {
  const textBlock = message.blocks.find((b) => b.type === 'text')

  return (
    <div className="flex justify-end animate-fade-in">
      <div className="max-w-[65%] group">
        <div className="bg-[var(--bg-active)] border border-[var(--border-default)] rounded-[14px_4px_14px_14px] px-3.5 py-2.5">
          {textBlock ? (
            <div
              className="text-[13px] text-[var(--text)] leading-[1.65]"
              dangerouslySetInnerHTML={{
                __html: (textBlock.html as string) || escapeHtml(textBlock.source as string) || '',
              }}
            />
          ) : (
            <span className="text-[var(--text-muted)] italic text-[13px]">(空)</span>
          )}
        </div>
        <MessageFooterActions messageId={message.id} role="user" timestamp={message.timestamp} />
      </div>
    </div>
  )
}

function escapeHtml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
}
```

- [ ] **Step 4: Commit**

```bash
git add src/renderer/src/components/chat/ChatArea.tsx src/renderer/src/components/chat/AssistantMessage.tsx src/renderer/src/components/chat/UserMessage.tsx
git commit -m "style: hybrid message layout — user bubbles right, assistant full-width"
```

---

### Task 6: Input Area — Centered Composer

**Files:**
- Modify: `src/renderer/src/components/input/InputArea.tsx`
- Modify: `src/renderer/src/components/input/PermissionModeButton.tsx`
- Modify: `src/renderer/src/components/input/ThinkingLevelButton.tsx`
- Modify: `src/renderer/src/components/input/ModelSelector.tsx`

- [ ] **Step 1: Rewrite InputArea.tsx**

```tsx
import { useState, useRef, useCallback, useEffect } from 'react'
import { useStore } from '../../stores'
import { loomRpc } from '../../services/jsonrpc'
import ContextRing from './ContextRing'
import ModelSelector from './ModelSelector'
import ThinkingLevelButton from './ThinkingLevelButton'
import PermissionModeButton from './PermissionModeButton'
import TypingIndicator from '../shared/TypingIndicator'
import { IconSend } from '../../utils/icons'

export default function InputArea() {
  const [text, setText] = useState('')
  const [sending, setSending] = useState(false)
  const textareaRef = useRef<HTMLTextAreaElement>(null)
  const sessionId = useStore(s => s.currentSessionId)
  const createSession = useStore(s => s.createSession)
  const switchSession = useStore(s => s.switchSession)
  const isStreaming = useStore(s => sessionId ? s.streamingSessionIds.has(sessionId) : false)
  const wsState = useStore(s => s.wsState)
  const { saveDraft, restoreDraft } = useStore.getState()

  useEffect(() => {
    if (sessionId) { const d = restoreDraft(sessionId); setText(d?.text ?? '') }
    else setText('')
  }, [sessionId])

  useEffect(() => {
    if (sessionId && text) {
      const t = setTimeout(() => saveDraft(sessionId, { text, attachedFiles: [] }), 300)
      return () => clearTimeout(t)
    }
  }, [text, sessionId])

  const ensureSession = useCallback(async (): Promise<string> => {
    if (sessionId) return sessionId
    const id = await createSession()
    if (id) await switchSession(id)
    return id
  }, [sessionId, createSession, switchSession])

  const handleSend = async () => {
    const content = text.trim()
    if (!content || sending || isStreaming) return
    setSending(true); setText('')
    const sid = await ensureSession()
    if (!sid) { setSending(false); setText(content); return }
    const msgId = crypto.randomUUID()
    useStore.getState().ensureSession(sid)
    useStore.getState().appendMessage(sid, {
      id: msgId, role: 'user',
      blocks: [{ type: 'text', html: escapeHtml(content), source: content }],
      timestamp: new Date().toISOString(),
    })
    try { await loomRpc('chat.send', { session_id: sid, content }) }
    catch (e: any) { useStore.getState().setInlineError(sid, e.message||'发送失败'); useStore.getState().deleteMessage(sid, msgId) }
    finally { setSending(false) }
  }

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); handleSend() }
  }

  const isConnected = wsState === 'connected'
  const placeholder = !isConnected ? '正在连接...' : !sessionId ? '开始新对话...' : isStreaming ? 'AI 回复中...' : '输入消息，⏎ 发送'

  return (
    <div className="px-8 pb-5 pt-2">
      <div className="max-w-[620px] mx-auto">
        <div className="flex flex-col bg-[var(--bg-surface)] border border-[var(--border-default)] rounded-[var(--r-2xl)] shadow-[var(--shadow-composer)]">
          <textarea
            ref={textareaRef}
            value={text}
            onChange={e => setText(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder={placeholder}
            rows={2}
            disabled={!isConnected || isStreaming}
            className="w-full bg-transparent text-[var(--text)] text-[13px] leading-relaxed resize-none outline-none placeholder:text-[var(--text-muted)] placeholder:italic px-4 pt-3.5 pb-1 disabled:opacity-40"
          />
          <div className="flex items-center gap-2 px-4 pb-3 pt-1.5 border-t border-[rgba(255,255,255,0.04)]">
            <PermissionModeButton />
            <ThinkingLevelButton />
            <ModelSelector />
            <div className="flex-1" />
            <ContextRing />
            <button
              onClick={handleSend}
              disabled={!text.trim() || !isConnected || isStreaming}
              className="inline-flex items-center justify-center gap-1.5 h-[30px] px-4 text-[11px] font-semibold text-[var(--bg)] bg-[var(--accent)] hover:bg-[var(--accent-hover)] disabled:opacity-25 disabled:cursor-not-allowed rounded-[var(--r-md)] transition-all shrink-0"
            >
              {isStreaming ? <TypingIndicator /> : <><IconSend size={12} />发送</>}
            </button>
          </div>
        </div>
      </div>
    </div>
  )
}

function escapeHtml(s: string): string { return s.replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;') }
```

- [ ] **Step 2: Update PermissionModeButton.tsx — use emoji prefix**

```tsx
import { useStore } from '../../stores'
import type { PermissionMode } from '../../stores/input'

const MODES: { id: PermissionMode; label: string; icon: string }[] = [
  { id: 'operate', label: 'operate', icon: '⚡' },
  { id: 'ask', label: 'ask', icon: '🛡' },
  { id: 'read_only', label: 'read_only', icon: '👁' },
]

export default function PermissionModeButton() {
  const mode = useStore((s) => s.permissionMode)
  const setMode = useStore((s) => s.setPermissionMode)
  const current = MODES.find((m) => m.id === mode) || MODES[0]

  return (
    <button
      onClick={() => {
        const idx = MODES.findIndex((m) => m.id === mode)
        setMode(MODES[(idx + 1) % MODES.length].id)
      }}
      className="pill"
      title={`权限: ${current.label}`}
    >
      {current.icon} {current.label}
    </button>
  )
}
```

- [ ] **Step 3: Update ThinkingLevelButton.tsx — use emoji prefix**

```tsx
import { useStore } from '../../stores'
import type { ThinkingLevel } from '../../stores/model'

const LEVELS: { id: ThinkingLevel; label: string }[] = [
  { id: 'off', label: 'off' },
  { id: 'auto', label: 'auto' },
  { id: 'low', label: 'low' },
  { id: 'medium', label: 'mid' },
  { id: 'high', label: 'high' },
]

export default function ThinkingLevelButton() {
  const level = useStore((s) => s.thinkingLevel)
  const setLevel = useStore((s) => s.setThinkingLevel)
  const label = LEVELS.find((l) => l.id === level)?.label || 'auto'

  return (
    <button
      onClick={() => {
        const idx = LEVELS.findIndex((l) => l.id === level)
        setLevel(LEVELS[(idx + 1) % LEVELS.length].id)
      }}
      className="pill-neutral"
    >
      💭 {label}
    </button>
  )
}
```

- [ ] **Step 4: Update ModelSelector.tsx — pill style**

```tsx
import { useEffect } from 'react'
import { useStore } from '../../stores'
import { loomRpc } from '../../services/jsonrpc'
import type { ModelListItem } from '../../types/bindings'

export default function ModelSelector() {
  const models = useStore((s) => s.models)
  const currentModel = useStore((s) => s.currentModel)
  const { setModels, setCurrentModel } = useStore.getState()

  useEffect(() => {
    loomRpc<{ models: ModelListItem[]; activeModel: string | null }>('model.list')
      .then((result) => {
        if (result.models?.length) {
          setModels(result.models.map((m) => m.model || m.name).filter(Boolean))
          if (result.activeModel) {
            const a = result.models.find((m) => m.name === result.activeModel)
            if (a?.model) setCurrentModel(a.model)
          }
          if (!currentModel && result.models.length && !result.activeModel) {
            setCurrentModel(result.models[0].model || result.models[0].name)
          }
        }
      })
      .catch(() => {})
  }, [])

  const displayModel = currentModel || 'deepseek-v4-flash'

  return (
    <div className="relative flex items-center">
      <select
        value={currentModel || undefined}
        onChange={(e) => {
          setCurrentModel(e.target.value)
          loomRpc('model.switch', { model: e.target.value }).catch(() => {})
        }}
        className="pill-neutral appearance-none pr-5 cursor-pointer"
      >
        {models.map((m) => <option key={m} value={m}>{m}</option>)}
      </select>
      <span className="pointer-events-none absolute right-2 text-[var(--text-muted)] text-[8px]">▼</span>
    </div>
  )
}
```

- [ ] **Step 5: Commit**

```bash
git add src/renderer/src/components/input/InputArea.tsx src/renderer/src/components/input/PermissionModeButton.tsx src/renderer/src/components/input/ThinkingLevelButton.tsx src/renderer/src/components/input/ModelSelector.tsx
git commit -m "style: centered composer input + pill toolbar buttons"
```

---

### Task 7: WelcomeScreen Refresh

**Files:**
- Modify: `src/renderer/src/components/shared/WelcomeScreen.tsx`

- [ ] **Step 1: Rewrite WelcomeScreen.tsx**

```tsx
import { useStore } from '../../stores'
import { IconPlus } from '../../utils/icons'

export default function WelcomeScreen() {
  const createSession = useStore((s) => s.createSession)
  const switchSession = useStore((s) => s.switchSession)

  const handleStart = async () => {
    const id = await createSession()
    if (id) await switchSession(id)
  }

  return (
    <div className="flex-1 flex items-center justify-center">
      <div className="text-center max-w-md animate-fade-up">
        <div className="w-[72px] h-[72px] mx-auto mb-6 rounded-[var(--r-xl)] bg-[var(--accent-subtle)] border border-[var(--border-accent)] flex items-center justify-center shadow-[var(--shadow-glow)]">
          <span className="text-2xl font-bold text-[var(--accent)]">L</span>
        </div>
        <h1 className="text-2xl text-[var(--text)] mb-2 tracking-tight font-semibold">
          openLoom
        </h1>
        <p className="text-[14px] text-[var(--text-secondary)] mb-8">你的私人 AI 助理</p>

        <div className="flex flex-wrap justify-center gap-2 mb-8">
          {['多模型支持', 'MCP 工具', '知识图谱记忆', 'LSP 代码理解', 'Skills 技能'].map(name => (
            <span key={name} className="pill-neutral">
              {name}
            </span>
          ))}
        </div>

        <button
          onClick={handleStart}
          className="inline-flex items-center gap-1.5 px-6 py-2.5 rounded-[var(--r-md)] bg-[var(--accent-subtle)] text-[var(--accent)] hover:bg-[var(--accent-medium)] border border-[var(--border-accent)] text-[13px] font-medium transition-colors"
        >
          <IconPlus size={13} />
          开始新对话
        </button>
        <p className="text-[11px] text-[var(--text-muted)] mt-6">
          所有数据存储在本地 SQLite 数据库中 · 完全离线可用
        </p>
      </div>
    </div>
  )
}
```

- [ ] **Step 2: Commit**

```bash
git add src/renderer/src/components/shared/WelcomeScreen.tsx
git commit -m "style: refresh WelcomeScreen — better contrast and pill tags"
```

---

### Task 8: Settings Modal Complete Redesign

**Files:**
- Modify: `src/renderer/src/components/shared/Overlay.tsx`
- Modify: `src/renderer/src/components/shared/SettingsModal.tsx`

- [ ] **Step 1: Update Overlay.tsx for larger modal**

```tsx
import { useEffect, useRef, type ReactNode } from 'react'
import { IconX } from '../../utils/icons'

interface OverlayProps {
  open: boolean
  onClose: () => void
  children: ReactNode
  title?: string
  size?: 'md' | 'lg'
}

export default function Overlay({ open, onClose, children, title, size = 'md' }: OverlayProps) {
  const overlayRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (!open) return
    const handleEsc = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose()
    }
    document.addEventListener('keydown', handleEsc)
    return () => document.removeEventListener('keydown', handleEsc)
  }, [open, onClose])

  if (!open) return null

  const sizeClass = size === 'lg' ? 'max-w-2xl min-h-[480px]' : 'max-w-xl'

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <div
        ref={overlayRef}
        className="absolute inset-0 bg-[var(--bg-overlay)] backdrop-blur-sm"
        onClick={onClose}
      />
      <div className={`relative bg-[var(--bg)] border border-[var(--border-accent)] rounded-[var(--r-xl)] shadow-[var(--shadow-lg)] ${sizeClass} w-full max-h-[85vh] overflow-hidden m-4 animate-fade-up`}>
        {title && (
          <div className="flex items-center justify-between px-5 py-3.5 border-b border-[var(--border)]">
            <h2 className="text-[14px] font-semibold text-[var(--text)]">{title}</h2>
            <button
              onClick={onClose}
              className="w-6 h-6 flex items-center justify-center rounded-[var(--r-sm)] text-[var(--text-muted)] hover:text-[var(--text)] hover:bg-[rgba(255,255,255,0.04)] transition-colors"
            >
              <IconX size={14} />
            </button>
          </div>
        )}
        <div className={`${title ? '' : 'pt-5'} ${size === 'lg' ? '' : 'p-5'}`}>{children}</div>
      </div>
    </div>
  )
}
```

- [ ] **Step 2: Rewrite SettingsModal.tsx with left nav + theme previews**

```tsx
import { useState } from 'react'
import { useStore } from '../../stores'
import Overlay from './Overlay'
import AgentConfigPanel from './AgentConfigPanel'
import ModelConfigPanel from './ModelConfigPanel'
import { type ThemeId } from '../../stores/ui'

const THEMES: { id: ThemeId; label: string; bg: string; surface: string; text: string; accent: string }[] = [
  { id: 'dark', label: '暗色', bg: '#0D1117', surface: '#151b23', text: '#e2e8f0', accent: '#22d3ee' },
  { id: 'light', label: '亮色', bg: '#ffffff', surface: '#f8fafc', text: '#1e293b', accent: '#0891b2' },
  { id: 'midnight', label: '午夜蓝', bg: '#0f172a', surface: '#1e293b', text: '#e2e8f0', accent: '#818cf8' },
  { id: 'warm-paper', label: '暖纸', bg: '#faf8f5', surface: '#f5f0e8', text: '#44403c', accent: '#d97706' },
]

type Tab = 'appearance' | 'agent' | 'models' | 'about'

export default function SettingsModal({
  open,
  onClose,
}: {
  open: boolean
  onClose: () => void
}) {
  const theme = useStore((s) => s.theme)
  const setTheme = useStore((s) => s.setTheme)
  const wsState = useStore((s) => s.wsState)
  const [tab, setTab] = useState<Tab>('appearance')

  const tabs: { id: Tab; label: string }[] = [
    { id: 'appearance', label: '外观' },
    { id: 'agent', label: 'Agent' },
    { id: 'models', label: '模型' },
    { id: 'about', label: '关于' },
  ]

  return (
    <Overlay open={open} onClose={onClose} size="lg">
      <div className="flex h-full min-h-[440px]">
        {/* Left nav */}
        <div className="w-[160px] shrink-0 bg-[var(--bg-surface)] border-r border-[var(--border)] p-3 flex flex-col gap-0.5">
          <div className="px-3 py-2 text-[10px] font-semibold uppercase tracking-[0.5px] text-[var(--text-muted)]">
            设置
          </div>
          {tabs.map((t) => (
            <button
              key={t.id}
              onClick={() => setTab(t.id)}
              className={`w-full text-left px-3 py-2 text-[12px] font-medium rounded-[var(--r-md)] transition-colors ${
                tab === t.id
                  ? 'bg-[var(--accent-subtle)] text-[var(--accent)] border border-[rgba(34,211,238,0.1)]'
                  : 'text-[var(--text-secondary)] hover:text-[var(--text)] hover:bg-[rgba(255,255,255,0.03)] border border-transparent'
              }`}
            >
              {t.label}
            </button>
          ))}
        </div>

        {/* Right content */}
        <div className="flex-1 p-6 overflow-y-auto">
          {tab === 'appearance' && (
            <div className="space-y-6">
              <div>
                <h3 className="text-[14px] font-semibold text-[var(--text)] mb-1">外观</h3>
                <p className="text-[11px] text-[var(--text-muted)]">选择主题和界面偏好</p>
              </div>

              {/* Theme grid with mini page previews */}
              <div>
                <div className="text-[10px] font-semibold uppercase tracking-[0.5px] text-[var(--text-muted)] mb-3">主题</div>
                <div className="grid grid-cols-2 gap-3">
                  {THEMES.map((t) => (
                    <button
                      key={t.id}
                      onClick={() => setTheme(t.id)}
                      className={`p-3 rounded-[var(--r-lg)] border text-left transition-all ${
                        theme === t.id
                          ? 'border-[var(--border-accent)] bg-[var(--accent-subtle)]'
                          : 'border-[var(--border)] hover:border-[var(--border-default)]'
                      }`}
                    >
                      {/* Mini page preview */}
                      <div className="w-full h-[64px] rounded-[6px] overflow-hidden mb-2 border border-[rgba(255,255,255,0.06)]" style={{ background: t.bg }}>
                        <div style={{ display: 'flex', height: '100%' }}>
                          <div style={{ width: '30%', background: t.surface, borderRight: `1px solid rgba(128,128,128,0.1)`, padding: '6px' }}>
                            <div style={{ width: '70%', height: '3px', background: t.accent, borderRadius: '2px', marginBottom: '4px' }} />
                            <div style={{ width: '90%', height: '2px', background: `${t.text}22`, borderRadius: '1px', marginBottom: '3px' }} />
                            <div style={{ width: '60%', height: '2px', background: `${t.text}22`, borderRadius: '1px' }} />
                          </div>
                          <div style={{ flex: 1, padding: '6px 8px', display: 'flex', flexDirection: 'column', justifyContent: 'space-between' }}>
                            <div>
                              <div style={{ width: '50%', height: '2px', background: `${t.text}44`, borderRadius: '1px', marginBottom: '3px' }} />
                              <div style={{ width: '80%', height: '2px', background: `${t.text}22`, borderRadius: '1px' }} />
                            </div>
                            <div style={{ width: '70%', height: '10px', background: t.surface, borderRadius: '4px', border: `1px solid rgba(128,128,128,0.1)` }} />
                          </div>
                        </div>
                      </div>
                      <span className={`text-[11px] font-medium ${theme === t.id ? 'text-[var(--accent)]' : 'text-[var(--text-secondary)]'}`}>
                        {t.label}
                      </span>
                    </button>
                  ))}
                </div>
              </div>
            </div>
          )}

          {tab === 'agent' && <AgentConfigPanel />}

          {tab === 'models' && <ModelConfigPanel />}

          {tab === 'about' && (
            <div className="space-y-4">
              <div>
                <h3 className="text-[14px] font-semibold text-[var(--text)] mb-1">关于</h3>
                <p className="text-[11px] text-[var(--text-muted)]">版本和连接信息</p>
              </div>
              <div className="space-y-3">
                <div className="flex items-center justify-between py-2.5 px-3 rounded-[var(--r-md)] bg-[rgba(255,255,255,0.02)] border border-[var(--border)]">
                  <span className="text-[12px] text-[var(--text-secondary)]">版本</span>
                  <span className="text-[12px] font-mono text-[var(--text)]">v0.2.0</span>
                </div>
                <div className="flex items-center justify-between py-2.5 px-3 rounded-[var(--r-md)] bg-[rgba(255,255,255,0.02)] border border-[var(--border)]">
                  <span className="text-[12px] text-[var(--text-secondary)]">连接状态</span>
                  <span className={`text-[12px] font-mono ${wsState === 'connected' ? 'text-[var(--green)]' : 'text-[var(--amber)]'}`}>
                    {wsState === 'connected' ? '已连接' : wsState}
                  </span>
                </div>
                <p className="text-[11px] text-[var(--text-muted)] mt-4 leading-relaxed">
                  本地优先的私人 AI 助理。所有数据存储在本地。
                </p>
              </div>
            </div>
          )}
        </div>
      </div>
    </Overlay>
  )
}
```

- [ ] **Step 3: Commit**

```bash
git add src/renderer/src/components/shared/Overlay.tsx src/renderer/src/components/shared/SettingsModal.tsx
git commit -m "style: redesign Settings Modal — left nav, theme page previews, larger size"
```

---

### Task 9: Button Component Update

**Files:**
- Modify: `src/renderer/src/components/shared/Button.tsx`

- [ ] **Step 1: Update Button variants and radii**

```tsx
import { type ReactNode } from 'react'

interface ButtonProps {
  children: ReactNode
  onClick?: () => void
  disabled?: boolean
  variant?: 'primary' | 'secondary' | 'ghost' | 'danger'
  size?: 'sm' | 'md'
  className?: string
  title?: string
  'aria-label'?: string
}

const variants: Record<string, string> = {
  primary: 'bg-[var(--accent-subtle)] text-[var(--accent)] hover:bg-[var(--accent-medium)] border border-[var(--border-accent)]',
  secondary: 'bg-[rgba(255,255,255,0.03)] text-[var(--text-secondary)] hover:bg-[rgba(255,255,255,0.06)] hover:text-[var(--text)] border border-[var(--border)]',
  ghost: 'bg-transparent text-[var(--text-muted)] hover:bg-[rgba(255,255,255,0.04)] hover:text-[var(--text)]',
  danger: 'bg-[var(--red-light)] text-[var(--red)] hover:bg-[rgba(239,68,68,0.15)] border border-[rgba(239,68,68,0.15)]',
}

const sizes: Record<string, string> = {
  sm: 'px-3 py-1.5 text-[11px]',
  md: 'px-4 py-2 text-[12px]',
}

export default function Button({
  children,
  onClick,
  disabled,
  variant = 'secondary',
  size = 'md',
  className = '',
  title,
  'aria-label': ariaLabel,
}: ButtonProps) {
  return (
    <button
      onClick={onClick}
      disabled={disabled}
      title={title}
      aria-label={ariaLabel}
      className={`rounded-[var(--r-md)] font-medium transition-colors disabled:opacity-30 disabled:cursor-not-allowed ${variants[variant]} ${sizes[size]} ${className}`}
    >
      {children}
    </button>
  )
}
```

- [ ] **Step 2: Commit**

```bash
git add src/renderer/src/components/shared/Button.tsx
git commit -m "style: update Button — new variant colors and sizing"
```

---

### Task 10: Final Verification

- [ ] **Step 1: Run typecheck**

Run: `npm run typecheck`
Expected: No TypeScript errors

- [ ] **Step 2: Visual verification**

Run: `npm run dev` (user runs manually)

Check:
- [ ] Sidebar: 220px, readable text, active item highlighted
- [ ] Titlebar: 48px, no duplicate logo, connection dot visible
- [ ] Chat: user messages right-bubble, assistant full-width with avatar+name
- [ ] Input: centered composer with pill toolbar
- [ ] Welcome: features as pill tags, visible text
- [ ] Settings: larger modal, left nav, theme cards with page preview
- [ ] No StatusBar at bottom

- [ ] **Step 3: Final commit (if any fixes needed)**

```bash
git add -A
git commit -m "fix: post-review styling adjustments"
```

# openLoom UI Redesign — Cyan Edge Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Redesign openLoom's desktop frontend to Cyan Edge glassmorphism aesthetic with slide-out sidebar, fused titlebar, and bubble chat layout.

**Architecture:** CSS-first redesign. New design tokens in `base.css` flow through all components via CSS custom properties. Structural changes: sidebar becomes a slide-out drawer toggled via ⌘B; titlebar fuses with navigation; chat switches to bubble layout with avatars. No store restructuring or new dependencies.

**Tech Stack:** React 19, Tailwind CSS v4 (CSS-first config), lucide-react icons, Zustand, Electron 38

---

## File Map

| File | Action | Responsibility |
|------|--------|----------------|
| `styles/base.css` | Rewrite | All design tokens, glassmorphism utilities, prose-chat, animations |
| `themes/light.css` | Rewrite | Light theme overrides |
| `themes/midnight.css` | Rewrite | Midnight theme overrides |
| `themes/warm-paper.css` | Rewrite | Warm paper theme overrides |
| `utils/icons.tsx` | Modify | Add new lucide icon imports |
| `stores/ui.ts` | Modify | Add `sidebarOpen` state + keyboard shortcut |
| `components/app/AppShell.tsx` | Rewrite | Fused titlebar + sidebar drawer layout |
| `components/app/WindowControls.tsx` | Rewrite | Cyan Edge window controls |
| `components/app/StatusBar.tsx` | Rewrite | Minimal Cyan Edge status bar |
| `components/app/Sidebar.tsx` | Rewrite | Slide-out drawer, time grouping, left indicator |
| `components/app/SessionItem.tsx` | Rewrite | Left indicator bar, hover glow |
| `components/chat/ChatArea.tsx` | Rewrite | New empty state, scroll area |
| `components/chat/AssistantMessage.tsx` | Rewrite | Left-aligned glass bubble + avatar |
| `components/chat/UserMessage.tsx` | Rewrite | Right-aligned cyan bubble |
| `components/chat/ThinkingBlock.tsx` | Rewrite | Collapsed card with lucide icons |
| `components/chat/ToolGroupBlock.tsx` | Rewrite | Single-line card with lucide icons |
| `components/chat/SubagentCard.tsx` | Rewrite | Accent update |
| `components/chat/FileBlock.tsx` | Rewrite | Lucide File icon, accent update |
| `components/chat/MessageFooterActions.tsx` | Modify | Lucide icons, Cyan Edge hover |
| `components/chat/TextBlock.tsx` | Modify | Prose-chat token reference |
| `components/input/InputArea.tsx` | Rewrite | Floating glass panel |
| `components/input/ModelSelector.tsx` | Rewrite | Cyan Edge pill style |
| `components/input/PermissionModeButton.tsx` | Rewrite | Cyan Edge pill |
| `components/input/ThinkingLevelButton.tsx` | Rewrite | Cyan Edge pill |
| `components/input/ContextRing.tsx` | Modify | New accent color |
| `components/shared/Button.tsx` | Rewrite | Cyan Edge variants |
| `components/shared/Toggle.tsx` | Modify | New accent |
| `components/shared/Select.tsx` | Modify | Cyan Edge border |
| `components/shared/ContextMenu.tsx` | Modify | Cyan Edge border/bg |
| `components/shared/Overlay.tsx` | Modify | Glass overlay |
| `components/shared/SettingsModal.tsx` | Modify | Accent update |
| `components/shared/ToastContainer.tsx` | Modify | Accent update |
| `components/shared/WelcomeScreen.tsx` | Rewrite | Cyan Edge branding |
| `components/shared/Onboarding.tsx` | Modify | Accent + border-radius update |

All paths relative to `frontend/src/renderer/src/`.

---

### Task 1: Design Tokens & Base CSS

**Files:**
- Rewrite: `styles/base.css`

- [ ] **Step 1: Write the new base.css with all Cyan Edge design tokens**

```css
@import "tailwindcss";

/* ================================================================
   openLoom — Cyan Edge Design
   Glassmorphism · Cyan accent · Deep navy · Apple easing
   ================================================================ */

:root {
  /* ── Surface (3-level depth) ── */
  --bg:        #0A0E14;
  --bg-card:   rgba(0,227,199,0.03);
  --bg-input:  rgba(0,227,199,0.025);
  --bg-tooltip:rgba(0,227,199,0.06);
  --bg-overlay:rgba(0,0,0,0.60);

  /* ── Ink (3-level text) ── */
  --text:       rgba(255,255,255,0.85);
  --text-light: rgba(255,255,255,0.45);
  --text-muted: rgba(255,255,255,0.15);

  /* ── Borders ── */
  --border:     rgba(0,227,199,0.06);
  --border-light:rgba(0,227,199,0.03);
  --border-accent:rgba(0,227,199,0.12);

  /* ── Accent: Cyan #00E3C7 ── */
  --accent:        #00E3C7;
  --accent-hover:  #33EAD3;
  --accent-rgb:    0,227,199;
  --accent-light:  rgba(0,227,199,0.08);
  --accent-strong: rgba(0,227,199,0.14);
  --accent-glow:   rgba(0,227,199,0.20);

  /* ── Semantic ── */
  --green:       #2DD4BF;
  --green-light: rgba(45,212,191,0.08);
  --amber:       #F59E0B;
  --amber-light: rgba(245,158,11,0.08);
  --red:         #EF4444;
  --red-light:   rgba(239,68,68,0.08);

  /* ── Typography ── */
  --font:      "Inter",-apple-system,BlinkMacSystemFont,"Segoe UI",Roboto,sans-serif;
  --font-mono: "JetBrains Mono","Cascadia Code","Fira Code","Consolas",monospace;

  /* ── Radii ── */
  --r-sm: 4px; --r-md: 8px; --r-lg: 12px; --r-xl: 14px; --r-full: 9999px;
  --r-input: 8px;

  /* ── Shadows ── */
  --shadow:       0 1px 3px rgba(0,0,0,0.4);
  --shadow-md:    0 4px 12px rgba(0,0,0,0.5);
  --shadow-lg:    0 8px 28px rgba(0,0,0,0.6);
  --shadow-glow:  0 0 16px var(--accent-glow);
  --shadow-glass: 0 0 40px rgba(0,227,199,0.03),0 4px 20px rgba(0,0,0,0.3);

  /* ── Animation ── */
  --ease-out:    cubic-bezier(0.16,1,0.3,1);
  --ease-in:     cubic-bezier(0.4,0,1,1);
  --ease-standard:cubic-bezier(0.2,0,0,1);
  --dur-fast:    0.15s;
  --dur-normal:  0.2s;
  --dur-slow:    0.25s;

  /* ── Shell ── */
  --sidebar-w:   240px;
  --titlebar-h:  36px;
  --statusbar-h: 22px;
}

/* ================================================================
   RESET
   ================================================================ */

*,*::before,*::after{margin:0;padding:0;box-sizing:border-box}
html,body,#root{width:100vw;height:100vh;overflow:hidden}
body{
  background:var(--bg);
  color:var(--text);
  font-family:var(--font);
  font-size:14px;
  line-height:1.7;
  -webkit-font-smoothing:antialiased;
  -moz-osx-font-smoothing:grayscale;
  user-select:none;
}

::selection{background:rgba(0,227,199,.22);color:#fff}
:focus-visible{outline:2px solid var(--accent);outline-offset:2px;border-radius:var(--r-sm)}
:focus:not(:focus-visible){outline:none}

::-webkit-scrollbar{width:4px;height:4px}
::-webkit-scrollbar-track{background:transparent}
::-webkit-scrollbar-thumb{background:rgba(128,128,128,0.2);border-radius:2px}
::-webkit-scrollbar-thumb:hover{background:rgba(128,128,128,0.4)}

/* ================================================================
   TYPOGRAPHY
   ================================================================ */

.font-mono{font-family:var(--font-mono);font-feature-settings:"tnum","ss02"}

/* ================================================================
   ANIMATIONS
   ================================================================ */

@keyframes fade-up{from{opacity:0;transform:translateY(8px)}to{opacity:1;transform:translateY(0)}}
@keyframes fade-in{from{opacity:0}to{opacity:1}}
@keyframes scale-in{from{opacity:0;transform:scale(.96)}to{opacity:1;transform:scale(1)}}
@keyframes pop-in{from{opacity:0;transform:scale(.92)}to{opacity:1;transform:scale(1)}}
@keyframes slide-left{from{opacity:0;transform:translateX(-6px)}to{opacity:1;transform:translateX(0)}}
@keyframes breathe{0%,100%{opacity:.35}50%{opacity:.6}}
@keyframes bounce-dot{0%,60%,100%{opacity:.2;transform:translateY(0)}30%{opacity:1;transform:translateY(-3px)}}

.animate-fade-up{animation:fade-up var(--dur-slow) var(--ease-out)}
.animate-fade-in{animation:fade-in var(--dur-normal) var(--ease-out)}
.animate-scale-in{animation:scale-in var(--dur-normal) var(--ease-out)}
.animate-pop-in{animation:pop-in var(--dur-fast) var(--ease-out)}
.animate-slide-left{animation:slide-left var(--dur-fast) var(--ease-out)}
.animate-breathe{animation:breathe 3s var(--ease-out) infinite}

.stagger-1{animation-delay:0ms}.stagger-2{animation-delay:50ms}.stagger-3{animation-delay:100ms}

/* ================================================================
   TYPING INDICATOR
   ================================================================ */

.typing-dots{display:inline-flex;gap:3px;align-items:center}
.typing-dots span{width:5px;height:5px;border-radius:50%;background:var(--accent);animation:bounce-dot 1.4s var(--ease-out) infinite}
.typing-dots span:nth-child(2){animation-delay:.2s}
.typing-dots span:nth-child(3){animation-delay:.4s}

/* ================================================================
   TRANSITIONS
   ================================================================ */

.transition-base{transition:all var(--dur-fast) var(--ease-out)}
.transition-colors{transition:background-color var(--dur-fast) var(--ease-out),color var(--dur-fast) var(--ease-out),border-color var(--dur-fast) var(--ease-out)}
.transition-opacity{transition:opacity var(--dur-fast) var(--ease-out)}
.transition-colors-fast,.transition-colors{transition:background-color var(--dur-fast) var(--ease-out),color var(--dur-fast) var(--ease-out),border-color var(--dur-fast) var(--ease-out)}
.transition-opacity-fast,.transition-opacity{transition:opacity var(--dur-fast) var(--ease-out)}
.animate-pulse-soft{animation:breathe 2s var(--ease-out) infinite}
.animate-pulse-dot{animation:breathe 1.5s var(--ease-out) infinite}

/* ================================================================
   SHARED COMPONENT PATTERNS
   ================================================================ */

.pill{
  display:inline-flex;align-items:center;
  height:24px;padding:0 8px;
  font-size:.6875rem;font-weight:500;
  background:rgba(0,227,199,0.08);
  border:1px solid rgba(0,227,199,0.12);
  border-radius:var(--r-sm);
  color:var(--accent);
  cursor:pointer;
  transition:all var(--dur-fast) var(--ease-out);
}
.pill:hover{background:rgba(0,227,199,0.12);border-color:rgba(0,227,199,0.2)}
.pill.active{background:rgba(0,227,199,0.12);color:var(--accent);border-color:rgba(0,227,199,0.2)}

.pill-neutral{
  display:inline-flex;align-items:center;
  height:24px;padding:0 8px;
  font-size:.6875rem;font-weight:500;
  background:rgba(255,255,255,0.03);
  border:1px solid rgba(255,255,255,0.05);
  border-radius:var(--r-sm);
  color:rgba(255,255,255,0.35);
  cursor:pointer;
  transition:all var(--dur-fast) var(--ease-out);
}
.pill-neutral:hover{background:rgba(255,255,255,0.05);color:rgba(255,255,255,0.5)}

.card{
  background:var(--bg-card);
  backdrop-filter:blur(12px);
  border:1px solid var(--border);
  border-radius:var(--r-md);
}
.card-interactive{
  background:var(--bg-card);
  backdrop-filter:blur(12px);
  border:1px solid var(--border);
  border-radius:var(--r-md);
  transition:all var(--dur-fast) var(--ease-out);
}
.card-interactive:hover{
  border-color:var(--border-accent);
  box-shadow:0 0 12px rgba(0,227,199,0.04);
}

/* ================================================================
   PROSE CHAT
   ================================================================ */

.prose-chat{line-height:1.75;color:var(--text)}
.prose-chat p{margin:0 0 .5em 0}
.prose-chat p:first-child{margin-top:0}
.prose-chat p:last-child{margin-bottom:0}
.prose-chat pre{
  background:rgba(0,227,199,0.02);padding:.5rem 1rem;
  border-radius:var(--r-md);overflow-x:auto;
  font-size:.85em;font-family:var(--font-mono);
  border:1px solid var(--border);
}
.prose-chat code{
  font-size:.85em;font-family:var(--font-mono);
  background:rgba(0,227,199,0.06);padding:.15em .35em;
  border-radius:var(--r-sm);color:var(--accent);
}
.prose-chat pre code{background:none;padding:0;color:var(--text)}
.prose-chat blockquote{
  border-left:2px solid var(--border-accent);padding-left:1rem;
  color:var(--text-light);font-style:italic;
}
.prose-chat ul,.prose-chat ol{padding-left:1.25rem}
.prose-chat li{margin:.15em 0}
.prose-chat a{color:var(--accent);text-decoration:none}
.prose-chat a:hover{text-decoration:underline}

.md-content,.prose-chat,[data-selectable],input,textarea{user-select:text}

/* ================================================================
   ELECTRON
   ================================================================ */

[data-chrome]{user-select:none}
[data-content]{-webkit-app-region:no-drag}
[data-drag]{-webkit-app-region:drag}
[data-no-drag]{-webkit-app-region:no-drag}

/* ================================================================
   TOAST
   ================================================================ */

.toast-enter{animation:fade-up var(--dur-slow) var(--ease-out)}
.toast-exit{animation:fade-in var(--dur-fast) var(--ease-in) reverse}

/* ================================================================
   REDUCED MOTION
   ================================================================ */

@media(prefers-reduced-motion:reduce){
  *,*::before,*::after{animation-duration:.01ms!important;transition-duration:.01ms!important}
}
```

- [ ] **Step 2: Verify CSS compiles**

Run: `cd F:/openLoom/frontend && npx electron-vite build 2>&1 | head -20`
Expected: Build succeeds with no CSS errors

- [ ] **Step 3: Commit**

```bash
cd F:/openLoom && git add frontend/src/renderer/src/styles/base.css && git commit -m "feat(ui): Cyan Edge design tokens and base CSS"
```

---

### Task 2: Theme Files Rewrite

**Files:**
- Rewrite: `themes/light.css`
- Rewrite: `themes/midnight.css`
- Rewrite: `themes/warm-paper.css`

- [ ] **Step 1: Write light.css**

```css
[data-theme="light"] {
  --bg:        #F8F9FA;
  --bg-card:   rgba(13,148,136,0.04);
  --bg-input:  rgba(13,148,136,0.03);
  --bg-tooltip:rgba(13,148,136,0.06);
  --bg-overlay:rgba(0,0,0,0.40);

  --text:       rgba(0,0,0,0.87);
  --text-light: rgba(0,0,0,0.55);
  --text-muted: rgba(0,0,0,0.25);

  --border:     rgba(0,0,0,0.08);
  --border-light:rgba(0,0,0,0.04);
  --border-accent:rgba(13,148,136,0.18);

  --accent:        #0D9488;
  --accent-hover:  #14B8A6;
  --accent-rgb:    13,148,136;
  --accent-light:  rgba(13,148,136,0.08);
  --accent-strong: rgba(13,148,136,0.14);
  --accent-glow:   rgba(13,148,136,0.20);

  --shadow:       0 1px 3px rgba(0,0,0,0.08);
  --shadow-md:    0 4px 12px rgba(0,0,0,0.10);
  --shadow-lg:    0 8px 28px rgba(0,0,0,0.12);
  --shadow-glow:  0 0 16px var(--accent-glow);
  --shadow-glass: 0 0 40px rgba(13,148,136,0.03),0 4px 20px rgba(0,0,0,0.08);
}
```

- [ ] **Step 2: Write midnight.css**

```css
[data-theme="midnight"] {
  --bg:        #0F172A;
  --bg-card:   rgba(165,191,248,0.04);
  --bg-input:  rgba(165,191,248,0.03);
  --bg-tooltip:rgba(165,191,248,0.06);
  --bg-overlay:rgba(0,0,0,0.60);

  --text:       rgba(255,255,255,0.88);
  --text-light: rgba(255,255,255,0.50);
  --text-muted: rgba(255,255,255,0.18);

  --border:     rgba(165,191,248,0.06);
  --border-light:rgba(165,191,248,0.03);
  --border-accent:rgba(165,191,248,0.12);

  --accent:        #A5BFF8;
  --accent-hover:  #BFCEFA;
  --accent-rgb:    165,191,248;
  --accent-light:  rgba(165,191,248,0.08);
  --accent-strong: rgba(165,191,248,0.14);
  --accent-glow:   rgba(165,191,248,0.20);

  --shadow-glass: 0 0 40px rgba(165,191,248,0.03),0 4px 20px rgba(0,0,0,0.4);
}
```

- [ ] **Step 3: Write warm-paper.css**

```css
[data-theme="warm-paper"] {
  --bg:        #F5F0E8;
  --bg-card:   rgba(176,90,48,0.04);
  --bg-input:  rgba(176,90,48,0.03);
  --bg-tooltip:rgba(176,90,48,0.06);
  --bg-overlay:rgba(0,0,0,0.40);

  --text:       rgba(45,36,22,0.90);
  --text-light: rgba(45,36,22,0.55);
  --text-muted: rgba(45,36,22,0.25);

  --border:     rgba(176,90,48,0.08);
  --border-light:rgba(176,90,48,0.04);
  --border-accent:rgba(176,90,48,0.14);

  --accent:        #B05A30;
  --accent-hover:  #C06A3A;
  --accent-rgb:    176,90,48;
  --accent-light:  rgba(176,90,48,0.08);
  --accent-strong: rgba(176,90,48,0.14);
  --accent-glow:   rgba(176,90,48,0.20);

  --shadow:       0 1px 3px rgba(0,0,0,0.06);
  --shadow-md:    0 4px 12px rgba(0,0,0,0.08);
  --shadow-lg:    0 8px 28px rgba(0,0,0,0.10);
  --shadow-glass: 0 0 40px rgba(176,90,48,0.03),0 4px 20px rgba(0,0,0,0.06);
}
```

- [ ] **Step 4: Commit**

```bash
cd F:/openLoom && git add frontend/src/renderer/src/themes/ && git commit -m "feat(ui): rewrite all theme CSS files (light, midnight, warm-paper)"
```

---

### Task 3: Icon Barrel & Store Updates

**Files:**
- Modify: `utils/icons.tsx`
- Modify: `stores/ui.ts`

- [ ] **Step 1: Update icons.tsx — add new lucide imports**

Replace entire file:

```tsx
// Icon barrel — re-exports from lucide-react
import { Star } from 'lucide-react'

export {
  Search as IconSearch,
  Plus as IconPlus,
  Settings as IconSettings,
  Star as IconStar,
  ChevronRight as IconChevronRight,
  ChevronDown as IconChevronDown,
  SendHorizonal as IconSend,
  Trash2 as IconTrash,
  Copy as IconCopy,
  Cpu as IconCpu,
  X as IconX,
  X as IconWinClose,
  Wrench as IconWrench,
  Minus as IconWinMin,
  Square as IconWinMax,
  AlertCircle as IconAlertCircle,
  Check as IconCheck,
  RefreshCw as IconRefresh,
  Zap as IconZap,
  Menu as IconMenu,
  Pencil as IconEdit,
  Pin as IconPin,
  PinOff as IconPinOff,
  Command as IconCommand,
  PanelLeftClose as IconPanelLeftClose,
  PanelLeft as IconPanelLeft,
  Shield as IconShield,
  Brain as IconBrain,
  File as IconFile,
  Activity as IconActivity,
  Loader as IconLoader,
  XCircle as IconXCircle,
  Wifi as IconWifi,
  WifiOff as IconWifiOff,
  ExternalLink as IconExternalLink,
} from 'lucide-react'

export function IconStarFilled({ size = 16, className = '' }: { size?: number; className?: string }) {
  return <Star size={size} className={className} fill="currentColor" />
}
```

- [ ] **Step 2: Update stores/ui.ts — add sidebarOpen state + keyboard shortcut**

```ts
import { StateCreator } from 'zustand'

export type ThemeId = 'dark' | 'light' | 'midnight' | 'warm-paper'

export interface UiSlice {
  theme: ThemeId
  settingsOpen: boolean
  sidebarOpen: boolean
  setTheme: (theme: ThemeId) => void
  setSettingsOpen: (open: boolean) => void
  setSidebarOpen: (open: boolean) => void
  toggleSidebar: () => void
}

export const createUiSlice: StateCreator<UiSlice> = (set, get) => ({
  theme: 'dark',
  settingsOpen: false,
  sidebarOpen: true,
  setTheme: (theme) => {
    document.documentElement.setAttribute('data-theme', theme)
    set({ theme })
  },
  setSettingsOpen: (settingsOpen) => set({ settingsOpen }),
  setSidebarOpen: (sidebarOpen) => set({ sidebarOpen }),
  toggleSidebar: () => set({ sidebarOpen: !get().sidebarOpen }),
})
```

- [ ] **Step 3: Add keyboard shortcut listener in App.tsx or AppShell**

This will be handled in Task 4 when we rewrite AppShell. The ⌘B handler will call `useStore.getState().toggleSidebar()`.

- [ ] **Step 4: Commit**

```bash
cd F:/openLoom && git add frontend/src/renderer/src/utils/icons.tsx frontend/src/renderer/src/stores/ui.ts && git commit -m "feat(ui): add new lucide icons + sidebar state to store"
```

---

### Task 4: AppShell — Fused Titlebar + Sidebar Drawer

**Files:**
- Rewrite: `components/app/AppShell.tsx`
- Rewrite: `components/app/WindowControls.tsx`
- Rewrite: `components/app/StatusBar.tsx`

- [ ] **Step 1: Rewrite WindowControls.tsx**

```tsx
import { IconWinMin, IconWinMax, IconWinClose } from '../../utils/icons'

export default function WindowControls() {
  return (
    <div data-no-drag className="flex h-full">
      <button onClick={() => window.hana.windowMinimize()}
        className="w-[36px] h-full flex items-center justify-center text-[rgba(255,255,255,0.35)] hover:text-[rgba(255,255,255,0.8)] hover:bg-[rgba(255,255,255,0.06)] transition-colors"
        aria-label="最小化"><IconWinMin size={10} /></button>
      <button onClick={() => window.hana.windowMaximize()}
        className="w-[36px] h-full flex items-center justify-center text-[rgba(255,255,255,0.35)] hover:text-[rgba(255,255,255,0.8)] hover:bg-[rgba(255,255,255,0.06)] transition-colors"
        aria-label="最大化"><IconWinMax size={10} /></button>
      <button onClick={() => window.hana.windowClose()}
        className="w-[36px] h-full flex items-center justify-center text-[rgba(255,255,255,0.35)] hover:text-white hover:bg-[var(--red)] transition-colors"
        aria-label="关闭"><IconWinClose size={10} /></button>
    </div>
  )
}
```

- [ ] **Step 2: Rewrite StatusBar.tsx**

```tsx
import { useStore } from '../../stores'

export default function StatusBar() {
  const wsState = useStore(s => s.wsState)

  return (
    <div className="flex items-center justify-between h-[var(--statusbar-h)] shrink-0 bg-[var(--bg)] border-t border-[var(--border-light)] px-3 text-[10px] text-[var(--text-muted)]">
      <div className="flex items-center gap-1.5">
        <span className="w-1 h-1 rounded-full"
          style={{
            background: wsState==='connected' ? 'var(--accent)' : wsState==='reconnecting' ? 'var(--amber)' : 'var(--red)',
            boxShadow: wsState==='connected' ? '0 0 4px rgba(0,227,199,0.4)' : 'none',
          }} />
        <span>{wsState==='connected'?'已连接':wsState==='reconnecting'?'重连中':'离线'}</span>
      </div>
    </div>
  )
}
```

- [ ] **Step 3: Rewrite AppShell.tsx with fused titlebar + sidebar drawer**

```tsx
import { useEffect } from 'react'
import { useStore } from '../../stores'
import Sidebar from './Sidebar'
import StatusBar from './StatusBar'
import WindowControls from './WindowControls'
import ChatWorkspace from '../chat/ChatWorkspace'
import { IconPanelLeftClose, IconPanelLeft } from '../../utils/icons'

export default function AppShell() {
  const sidebarOpen = useStore(s => s.sidebarOpen)
  const toggleSidebar = useStore(s => s.toggleSidebar)
  const currentSessionId = useStore(s => s.currentSessionId)
  const sessions = useStore(s => s.sessions)

  // ⌘B shortcut
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
      {/* Fused Titlebar — 36px */}
      <header
        data-drag
        className="flex items-center h-[var(--titlebar-h)] shrink-0 bg-[var(--bg)] border-b border-[var(--border)] px-2 z-10"
      >
        {/* Left: sidebar toggle + logo */}
        <div data-no-drag className="flex items-center gap-2 flex-shrink-0">
          <button
            onClick={toggleSidebar}
            className="flex items-center justify-center w-7 h-7 rounded-[var(--r-sm)] text-[var(--accent)] hover:bg-[rgba(0,227,199,0.06)] transition-colors"
            title="⌘B 切换侧边栏"
          >
            {sidebarOpen ? <IconPanelLeftClose size={15} /> : <IconPanelLeft size={15} />}
          </button>
          <div className="flex items-center gap-1.5">
            <div className="w-4 h-4 rounded-[3px] bg-[var(--accent)] flex items-center justify-center">
              <span className="text-[8px] font-extrabold text-[var(--bg)]">L</span>
            </div>
            <span className="text-[12px] font-medium text-[var(--text-light)] tracking-tight">
              openLoom
            </span>
          </div>
        </div>

        {/* Center: session title (draggable) */}
        <div className="flex-1 text-center">
          <span className="text-[11px] text-[var(--text-muted)]">{currentTitle}</span>
        </div>

        {/* Right: connection + window controls */}
        <div data-no-drag className="flex items-center gap-1.5 flex-shrink-0">
          <WindowControls />
        </div>
      </header>

      {/* Body */}
      <div className="flex flex-1 overflow-hidden">
        {/* Sidebar drawer */}
        <div
          className="shrink-0 overflow-hidden transition-all duration-[200ms] ease-[var(--ease-out)]"
          style={{ width: sidebarOpen ? 'var(--sidebar-w)' : '0px', opacity: sidebarOpen ? 1 : 0 }}
        >
          <Sidebar />
        </div>
        <main data-content className="flex-1 flex flex-col min-w-0 relative bg-[var(--bg)]">
          <ChatWorkspace />
        </main>
      </div>

      {/* StatusBar */}
      <StatusBar />
    </div>
  )
}
```

- [ ] **Step 4: Commit**

```bash
cd F:/openLoom && git add frontend/src/renderer/src/components/app/ && git commit -m "feat(ui): fused titlebar + sidebar drawer + status bar"
```

---

### Task 5: Sidebar + SessionItem

**Files:**
- Rewrite: `components/app/Sidebar.tsx`
- Rewrite: `components/app/SessionItem.tsx`

- [ ] **Step 1: Rewrite SessionItem.tsx with left indicator bar**

```tsx
import { useState, useRef, useEffect } from 'react'
import { useStore } from '../../stores'
import type { SessionSummary } from '../../stores/session'
import ContextMenu, { ContextMenuItem } from '../shared/ContextMenu'
import { IconPin, IconPinOff } from '../../utils/icons'

export default function SessionItem({ session }: { session: SessionSummary }) {
  const currentId = useStore(s => s.currentSessionId)
  const switchSession = useStore(s => s.switchSession)
  const renameSession = useStore(s => s.renameSession)
  const deleteSession = useStore(s => s.deleteSession)
  const pinSession = useStore(s => s.pinSession)
  const unpinSession = useStore(s => s.unpinSession)
  const sid = session.path || ''
  const isActive = sid === currentId
  const isPinned = useStore(s => sid ? s.pinnedIds.has(sid) : false)
  const [menuOpen, setMenuOpen] = useState(false)
  const [menuPos, setMenuPos] = useState({ x: 0, y: 0 })
  const [renaming, setRenaming] = useState(false)
  const [titleDraft, setTitleDraft] = useState(session.title || '')
  const inputRef = useRef<HTMLInputElement>(null)

  useEffect(() => { if (renaming) inputRef.current?.focus() }, [renaming])

  const submitRename = async () => {
    if (titleDraft.trim() && titleDraft !== session.title) await renameSession(sid, titleDraft.trim())
    setRenaming(false)
  }

  if (!sid) return null

  return (
    <div
      onClick={() => switchSession(sid)}
      onContextMenu={e => { e.preventDefault(); setMenuPos({ x: e.clientX, y: e.clientY }); setMenuOpen(true) }}
      className={`group relative flex items-center gap-2 mx-2 px-2.5 py-1.5 cursor-pointer rounded-[var(--r-md)] transition-all duration-[var(--dur-fast)] ${
        isActive
          ? 'bg-[rgba(0,227,199,0.05)] border-l-2 border-l-[var(--accent)]'
          : 'border-l-2 border-l-transparent text-[var(--text-light)] hover:bg-[rgba(0,227,199,0.03)] hover:text-[var(--text)]'
      }`}
    >
      {renaming ? (
        <input ref={inputRef} value={titleDraft} onChange={e => setTitleDraft(e.target.value)}
          onKeyDown={e => { if(e.key==='Enter')submitRename(); if(e.key==='Escape')setRenaming(false) }}
          onBlur={submitRename} onClick={e => e.stopPropagation()}
          className="flex-1 bg-[rgba(255,255,255,0.06)] text-[var(--text)] text-[12px] rounded-[var(--r-sm)] px-1.5 py-0.5 outline-none" />
      ) : (
        <span className="flex-1 truncate text-[12px] leading-snug">
          {session.title || `会话 ${sid.slice(0, 8)}`}
        </span>
      )}
      {!renaming && (
        <button onClick={e => { e.stopPropagation(); isPinned ? unpinSession(sid) : pinSession(sid) }}
          className={`shrink-0 ${isPinned ? 'text-[var(--accent)] opacity-100' : 'text-[var(--text-muted)] opacity-0 group-hover:opacity-100'} transition-opacity`}>
          {isPinned ? <IconPinOff size={11} /> : <IconPin size={11} />}
        </button>
      )}
      <ContextMenu open={menuOpen} x={menuPos.x} y={menuPos.y} onClose={() => setMenuOpen(false)}>
        <ContextMenuItem onClick={()=>{setMenuOpen(false);setRenaming(true);setTitleDraft(session.title||'')}}>重命名</ContextMenuItem>
        <ContextMenuItem onClick={()=>{setMenuOpen(false);if(confirm('确定删除此会话？'))deleteSession(sid)}} danger>删除</ContextMenuItem>
      </ContextMenu>
    </div>
  )
}
```

- [ ] **Step 2: Rewrite Sidebar.tsx with slide-out drawer, time grouping, search**

```tsx
import { useState, useMemo, useRef, useEffect } from 'react'
import { useStore } from '../../stores'
import SessionItem from './SessionItem'
import { IconPlus, IconSearch, IconSettings, IconPanelLeftClose } from '../../utils/icons'

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
    return sessions.filter(s => (s.title||'').toLowerCase().includes(q) || s.path.toLowerCase().includes(q))
  }, [sessions, query])

  const pinned = filtered.filter(s => pinnedIds.has(s.path))
  const unpinned = filtered.filter(s => !pinnedIds.has(s.path))

  const handleCreate = async () => {
    const id = await createSession()
    if (id) await switchSession(id)
  }

  return (
    <aside className="flex flex-col w-[var(--sidebar-w)] h-full bg-[rgba(0,227,199,0.01)] border-r border-[var(--border)]">
      {/* Header */}
      <div className="flex items-center justify-between px-3 py-2.5 border-b border-[var(--border)]">
        <div className="flex items-center gap-2">
          <div className="w-5 h-5 rounded-[4px] bg-[var(--accent)] flex items-center justify-center">
            <span className="text-[9px] font-extrabold text-[var(--bg)]">L</span>
          </div>
          <span className="text-[13px] font-semibold text-[var(--text)] tracking-tight">openLoom</span>
        </div>
        <button
          onClick={() => setSidebarOpen(false)}
          className="flex items-center justify-center w-6 h-6 rounded-[var(--r-sm)] text-[var(--text-muted)] hover:text-[var(--accent)] hover:bg-[rgba(0,227,199,0.06)] transition-colors"
          title="收起侧边栏"
        >
          <IconPanelLeftClose size={14} />
        </button>
      </div>

      {/* Search */}
      <div className="px-2.5 pt-2.5 pb-1.5">
        <div className="flex items-center gap-2 h-[30px] px-2.5 rounded-[var(--r-md)] bg-[rgba(0,227,199,0.03)] border border-[rgba(0,227,199,0.06)]">
          <IconSearch size={12} className="text-[var(--text-muted)] shrink-0" />
          <input
            ref={inputRef}
            value={query}
            onChange={e => setQuery(e.target.value)}
            onKeyDown={e => e.key==='Escape'&&setQuery('')}
            placeholder="⌘K 搜索会话..."
            className="flex-1 bg-transparent text-[var(--text)] text-[12px] outline-none placeholder:text-[var(--text-muted)]"
          />
        </div>
      </div>

      {/* Session list */}
      <div className="flex-1 overflow-y-auto py-0.5">
        {filtered.length === 0 ? (
          <div className="px-4 py-16 text-center">
            <p className="text-[12px] text-[var(--text-muted)]">
              {query ? '无匹配会话' : '暂无会话'}
            </p>
          </div>
        ) : (
          <>
            {pinned.length > 0 && (
              <div>
                <div className="px-3 py-1.5 text-[10px] font-semibold uppercase tracking-[1.2px] text-[var(--text-muted)]">
                  已置顶
                </div>
                {pinned.map(s => <SessionItem key={s.path} session={s} />)}
              </div>
            )}
            {unpinned.length > 0 && (
              <div>
                <div className="px-3 py-1.5 text-[10px] font-semibold uppercase tracking-[1.2px] text-[var(--text-muted)]">
                  {pinned.length > 0 ? '全部' : '今天'}
                </div>
                {unpinned.map(s => <SessionItem key={s.path} session={s} />)}
              </div>
            )}
          </>
        )}
      </div>

      {/* Bottom actions */}
      <div className="flex items-center gap-1.5 px-2.5 py-2 border-t border-[var(--border)]">
        <button onClick={handleCreate}
          className="flex items-center gap-1.5 flex-1 h-[30px] px-3 text-[12px] font-medium text-[var(--accent)] bg-[rgba(0,227,199,0.08)] hover:bg-[rgba(0,227,199,0.12)] border border-[rgba(0,227,199,0.12)] rounded-[var(--r-md)] transition-colors justify-center">
          <IconPlus size={13} /> 新建会话
        </button>
        <button onClick={() => setSettingsOpen(true)}
          className="w-[30px] h-[30px] flex items-center justify-center rounded-[var(--r-md)] text-[var(--text-muted)] hover:text-[var(--text)] hover:bg-[rgba(255,255,255,0.04)] transition-colors">
          <IconSettings size={14} />
        </button>
      </div>
    </aside>
  )
}
```

- [ ] **Step 3: Commit**

```bash
cd F:/openLoom && git add frontend/src/renderer/src/components/app/Sidebar.tsx frontend/src/renderer/src/components/app/SessionItem.tsx && git commit -m "feat(ui): sidebar drawer with search + session items with left indicator"
```

---

### Task 6: Chat Messages — Bubble Layout

**Files:**
- Rewrite: `components/chat/AssistantMessage.tsx`
- Rewrite: `components/chat/UserMessage.tsx`
- Rewrite: `components/chat/ChatArea.tsx`

- [ ] **Step 1: Rewrite AssistantMessage.tsx — glass bubble + avatar**

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
    <div className="flex gap-2.5 max-w-[85%] group animate-fade-in">
      {/* Avatar */}
      <div className="w-7 h-7 rounded-full bg-[rgba(0,227,199,0.08)] border border-[rgba(0,227,199,0.12)] flex items-center justify-center shrink-0 mt-0.5">
        <span className="text-[11px] font-extrabold text-[var(--accent)]">L</span>
      </div>
      {/* Content */}
      <div className="flex-1 min-w-0 space-y-2">
        {message.blocks.map((block, i) => {
          switch (block.type) {
            case 'thinking':
              return <ThinkingBlock key={i} block={block} />
            case 'tool_group':
              return <ToolGroupBlock key={i} block={block} />
            case 'text':
              return (
                <div key={i} className="bg-[rgba(0,227,199,0.03)] backdrop-blur-[12px] border border-[rgba(0,227,199,0.06)] rounded-[4px_var(--r-lg)_var(--r-lg)_var(--r-lg)] px-3 py-2.5">
                  <TextBlock block={block} />
                </div>
              )
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

- [ ] **Step 2: Rewrite UserMessage.tsx — right-aligned cyan bubble**

```tsx
import type { Message } from '../../stores/chat'
import MessageFooterActions from './MessageFooterActions'

export default function UserMessage({ message }: { message: Message }) {
  const textBlock = message.blocks.find((b) => b.type === 'text')

  return (
    <div className="flex justify-end animate-fade-in">
      <div className="max-w-[75%] group">
        <div className="bg-[rgba(0,227,199,0.1)] backdrop-blur-[12px] border border-[rgba(0,227,199,0.15)] rounded-[var(--r-lg)_4px_var(--r-lg)_var(--r-lg)] px-3.5 py-2.5">
          {textBlock ? (
            <div
              className="text-[13.5px] text-[var(--text)] leading-[1.6]"
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

- [ ] **Step 3: Rewrite ChatArea.tsx — new empty states, spacing**

```tsx
import { useStore } from '../../stores'
import { useRef, useEffect } from 'react'
import AssistantMessage from './AssistantMessage'
import UserMessage from './UserMessage'
import TypingIndicator from '../shared/TypingIndicator'

const EMPTY: never[] = []

export default function ChatArea() {
  const sessionId = useStore(s => s.currentSessionId)
  const messagesBySession = useStore(s => s.messagesBySession)
  const messages = sessionId ? (messagesBySession.get(sessionId) ?? EMPTY) : EMPTY
  const streamingIds = useStore(s => s.streamingSessionIds)
  const isStreaming = sessionId ? streamingIds.has(sessionId) : false
  const inlineErrors = useStore(s => s.inlineErrors)
  const error = sessionId ? inlineErrors.get(sessionId)?.text : null
  const scrollRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    const el = scrollRef.current
    if (el) el.scrollTop = el.scrollHeight
  }, [messages[messages.length - 1]?.id, isStreaming])

  if (!sessionId) {
    return (
      <div className="flex-1 flex items-center justify-center">
        <div className="text-center space-y-4 animate-fade-up">
          <div className="w-12 h-12 mx-auto rounded-[var(--r-lg)] bg-[rgba(0,227,199,0.06)] border border-[rgba(0,227,199,0.12)] flex items-center justify-center shadow-[0_0_30px_rgba(0,227,199,0.06)]">
            <span className="text-xl font-bold text-[var(--accent)]">L</span>
          </div>
          <div>
            <h1 className="text-[18px] font-semibold text-[var(--text)] tracking-tight">openLoom</h1>
            <p className="text-[13px] text-[rgba(0,227,199,0.4)] mt-1">你的私人 AI 助理</p>
          </div>
        </div>
      </div>
    )
  }

  if (messages.length === 0 && !isStreaming) {
    return (
      <div className="flex-1 flex items-center justify-center">
        <div className="text-center space-y-3 animate-fade-up">
          <div className="w-11 h-11 mx-auto rounded-[var(--r-lg)] bg-[rgba(0,227,199,0.06)] border border-[rgba(0,227,199,0.12)] flex items-center justify-center shadow-[0_0_30px_rgba(0,227,199,0.06)]">
            <span className="text-lg font-bold text-[var(--accent)]">L</span>
          </div>
          <h1 className="text-[18px] font-semibold text-[var(--text)] tracking-tight">openLoom</h1>
          <p className="text-[13px] text-[rgba(0,227,199,0.4)]">发送消息开始对话</p>
        </div>
      </div>
    )
  }

  return (
    <div ref={scrollRef} className="flex-1 overflow-y-auto" style={{ padding: '24px 1.25rem 120px' }}>
      <div className="max-w-[680px] mx-auto space-y-5">
        {messages.map(msg =>
          msg.role === 'user'
            ? <UserMessage key={msg.id} message={msg} />
            : <AssistantMessage key={msg.id} message={msg} />
        )}

        {isStreaming && (
          <div className="flex items-center gap-2 text-[13px] text-[var(--text-muted)] animate-fade-in">
            <span>AI 回复中</span>
            <TypingIndicator />
          </div>
        )}

        {error && (
          <div className="flex items-start gap-2 px-3.5 py-2.5 rounded-[var(--r-md)] border border-[rgba(239,68,68,0.15)] bg-[var(--red-light)] text-[13px] text-[var(--red)]">
            <span className="font-bold shrink-0">!</span>
            <span>{error}</span>
          </div>
        )}
      </div>
    </div>
  )
}
```

- [ ] **Step 4: Commit**

```bash
cd F:/openLoom && git add frontend/src/renderer/src/components/chat/AssistantMessage.tsx frontend/src/renderer/src/components/chat/UserMessage.tsx frontend/src/renderer/src/components/chat/ChatArea.tsx && git commit -m "feat(ui): bubble chat layout with avatars and glass effects"
```

---

### Task 7: Chat Block Components

**Files:**
- Rewrite: `components/chat/ThinkingBlock.tsx`
- Rewrite: `components/chat/ToolGroupBlock.tsx`
- Rewrite: `components/chat/SubagentCard.tsx`
- Rewrite: `components/chat/FileBlock.tsx`
- Modify: `components/chat/MessageFooterActions.tsx`

- [ ] **Step 1: Rewrite ThinkingBlock.tsx**

```tsx
import { useState } from 'react'
import type { ContentBlock } from '../../stores/chat'
import { IconChevronRight, IconChevronDown } from '../../utils/icons'

export default function ThinkingBlock({ block }: { block: ContentBlock }) {
  const [expanded, setExpanded] = useState(false)
  const sealed = block.sealed as boolean
  const content = block.content as string
  const elapsed = block.elapsed as number | undefined

  return (
    <div className="bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.04)] rounded-[var(--r-md)] overflow-hidden">
      <button
        onClick={() => setExpanded(!expanded)}
        className="flex items-center gap-2 w-full px-3 py-2 text-[11px] text-[var(--text-light)] hover:bg-[rgba(255,255,255,0.02)] transition-colors"
      >
        {expanded ? <IconChevronDown size={10} /> : <IconChevronRight size={10} />}
        <span className="font-medium">思考过程</span>
        {elapsed != null && <span className="text-[var(--text-muted)] ml-1">· {elapsed}s</span>}
        {!sealed && (
          <span className="w-1.5 h-1.5 rounded-full bg-[var(--accent)] animate-pulse-dot ml-auto" />
        )}
      </button>
      {expanded && (
        <div className="px-3 py-2.5 text-[12px] text-[var(--text-light)] border-t border-[rgba(255,255,255,0.04)] whitespace-pre-wrap max-h-56 overflow-y-auto leading-relaxed">
          {content}
        </div>
      )}
    </div>
  )
}
```

- [ ] **Step 2: Rewrite ToolGroupBlock.tsx**

```tsx
import { useState } from 'react'
import type { ContentBlock } from '../../stores/chat'
import { IconZap, IconCheck, IconLoader, IconXCircle, IconChevronRight, IconChevronDown } from '../../utils/icons'

interface ToolCall {
  id: string; name: string; status: 'running' | 'done' | 'error'
  elapsed: number; args: Record<string, unknown>; result?: string
}

const statusIcon = (s: string) => {
  if (s === 'done') return <IconCheck size={10} className="text-[var(--accent)]" />
  if (s === 'running') return <IconLoader size={10} className="text-[var(--amber)] animate-spin" />
  return <IconXCircle size={10} className="text-[var(--red)]" />
}

export default function ToolGroupBlock({ block }: { block: ContentBlock }) {
  const [expandedId, setExpandedId] = useState<string | null>(null)
  const tools = (block.tools as ToolCall[]) || []
  const collapsed = block.collapsed as boolean

  return (
    <div className="bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.04)] rounded-[var(--r-md)] overflow-hidden">
      {!collapsed && tools.map((tool, idx) => (
        <div key={tool.id} className={idx > 0 ? 'border-t border-[rgba(255,255,255,0.03)]' : ''}>
          <button
            onClick={() => setExpandedId(expandedId === tool.id ? null : tool.id)}
            className="flex items-center gap-2.5 w-full px-3 py-2 text-[11px] hover:bg-[rgba(255,255,255,0.02)] transition-colors"
          >
            <IconZap size={10} className="text-[var(--accent)] shrink-0" />
            <span className="font-medium text-[var(--text-light)]">{tool.name}</span>
            <span className="ml-auto">{statusIcon(tool.status)}</span>
            {expandedId !== tool.id ? <IconChevronRight size={9} className="text-[var(--text-muted)]" /> : <IconChevronDown size={9} className="text-[var(--text-muted)]" />}
          </button>
          {expandedId === tool.id && (
            <div className="px-3 pb-2.5 space-y-1.5">
              {Object.keys(tool.args).length > 0 && (
                <pre className="bg-[var(--bg)] rounded-[var(--r-sm)] p-2 overflow-x-auto text-[10px] text-[var(--text-muted)] font-mono">
                  {JSON.stringify(tool.args, null, 2)}
                </pre>
              )}
              {tool.result && (
                <pre className="bg-[var(--bg)] rounded-[var(--r-sm)] p-2 overflow-x-auto text-[11px] text-[var(--text-light)] font-mono max-h-36 overflow-y-auto">
                  {tool.result}
                </pre>
              )}
            </div>
          )}
        </div>
      ))}
    </div>
  )
}
```

- [ ] **Step 3: Rewrite SubagentCard.tsx**

```tsx
import type { ContentBlock } from '../../stores/chat'
import { IconZap, IconCheck, IconLoader } from '../../utils/icons'

export default function SubagentCard({ block }: { block: ContentBlock }) {
  const name = (block.name as string) || '子 Agent'
  const status = (block.streamStatus as string) || 'running'
  const summary = (block.summary as string) || ''

  return (
    <div className="bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.04)] rounded-[var(--r-md)] overflow-hidden">
      <div className="flex items-center gap-2 px-3 py-2 bg-[rgba(0,227,199,0.02)]">
        <IconZap size={10} className="text-[var(--accent)]" />
        <span className="text-[11px] font-medium text-[var(--accent)]">{name}</span>
        <span className="ml-auto">
          {status === 'done' ? <IconCheck size={10} className="text-[var(--green)]" /> : <IconLoader size={10} className="text-[var(--amber)] animate-spin" />}
        </span>
      </div>
      {summary && (
        <div className="px-3 py-2.5 text-[12px] text-[var(--text-light)] border-t border-[rgba(255,255,255,0.04)] leading-relaxed">
          {summary}
        </div>
      )}
    </div>
  )
}
```

- [ ] **Step 4: Rewrite FileBlock.tsx**

```tsx
import type { ContentBlock } from '../../stores/chat'
import { IconFile, IconExternalLink } from '../../utils/icons'

export default function FileBlock({ block }: { block: ContentBlock }) {
  const name = (block.name as string) || 'file'
  const filePath = (block.path as string) || ''
  const size = block.size as number | undefined

  const fmt = (b: number) => b < 1024 ? `${b}B` : b < 1024**2 ? `${(b/1024).toFixed(1)}KB` : `${(b/1024**2).toFixed(1)}MB`

  return (
    <div className="inline-flex items-center gap-2.5 px-3 py-2 rounded-[var(--r-md)] bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.04)] text-[12px]">
      <IconFile size={12} className="text-[var(--accent)] opacity-60 shrink-0" />
      <span className="text-[var(--text)] truncate max-w-[200px]">{name}</span>
      {size != null && <span className="text-[10px] text-[var(--text-muted)] tabular-nums">{fmt(size)}</span>}
      {filePath && (
        <button onClick={() => window.hana.openFile(filePath)}
          className="flex items-center gap-1 text-[10px] text-[var(--accent)] hover:text-[var(--accent-hover)] transition-colors">
          <IconExternalLink size={9} /> 打开
        </button>
      )}
    </div>
  )
}
```

- [ ] **Step 5: Update MessageFooterActions.tsx — use var(--a) → var(--accent), adjust colors**

```tsx
import { useStore } from '../../stores'
import { IconCopy, IconTrash } from '../../utils/icons'

interface Props { messageId: string; role: 'user' | 'assistant'; timestamp: string }

export default function MessageFooterActions({ messageId, role, timestamp }: Props) {
  const deleteMessage = useStore((s) => s.deleteMessage)
  const currentSessionId = useStore((s) => s.currentSessionId)

  const handleCopy = () => {
    const msgs = useStore.getState().messagesBySession.get(currentSessionId || '')
    const msg = msgs?.find((m) => m.id === messageId)
    if (!msg) return
    const text = msg.blocks
      .filter((b) => b.type === 'text')
      .map((b) => (b.source as string) || (b.html as string) || '')
      .join('\n')
    if (text) navigator.clipboard.writeText(text)
  }

  const handleDelete = () => {
    if (!currentSessionId) return
    deleteMessage(currentSessionId, messageId)
  }

  const time = new Date(timestamp).toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' })

  return (
    <div className={`flex items-center gap-0.5 mt-1 opacity-0 group-hover:opacity-100 transition-opacity ${role === 'user' ? 'justify-end' : ''}`}>
      <span className="text-[10px] text-[var(--text-muted)] mr-1 tabular-nums">{time}</span>
      <button onClick={handleCopy} className="flex items-center gap-0.5 text-[10px] text-[var(--text-muted)] hover:text-[var(--accent)] px-1 py-0.5 rounded-[var(--r-sm)] transition-colors">
        <IconCopy size={9} />
      </button>
      <button onClick={handleDelete} className="flex items-center gap-0.5 text-[10px] text-[var(--text-muted)] hover:text-[var(--red)] px-1 py-0.5 rounded-[var(--r-sm)] transition-colors">
        <IconTrash size={9} />
      </button>
    </div>
  )
}
```

- [ ] **Step 6: Commit**

```bash
cd F:/openLoom && git add frontend/src/renderer/src/components/chat/ && git commit -m "feat(ui): chat block components — thinking, tool, subagent, file, footer actions"
```

---

### Task 8: Input Area — Floating Glass Panel

**Files:**
- Rewrite: `components/input/InputArea.tsx`
- Rewrite: `components/input/PermissionModeButton.tsx`
- Rewrite: `components/input/ThinkingLevelButton.tsx`
- Rewrite: `components/input/ModelSelector.tsx`
- Modify: `components/input/ContextRing.tsx`

- [ ] **Step 1: Rewrite PermissionModeButton.tsx as cyan pill**

```tsx
import { useStore } from '../../stores'
import type { PermissionMode } from '../../stores/input'
import { IconShield } from '../../utils/icons'

const MODES: { id: PermissionMode; label: string }[] = [
  { id: 'operate', label: 'operate' },
  { id: 'ask', label: 'ask' },
  { id: 'read_only', label: 'read_only' },
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
      <IconShield size={10} className="mr-1" />
      {current.label}
    </button>
  )
}
```

- [ ] **Step 2: Rewrite ThinkingLevelButton.tsx as neutral pill**

```tsx
import { useStore } from '../../stores'
import type { ThinkingLevel } from '../../stores/model'
import { IconBrain } from '../../utils/icons'

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
      <IconBrain size={10} className="mr-1" />
      think: {label}
    </button>
  )
}
```

- [ ] **Step 3: Rewrite ModelSelector.tsx**

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
    <select
      value={currentModel || undefined}
      onChange={(e) => {
        setCurrentModel(e.target.value)
        loomRpc('model.switch', { model: e.target.value }).catch(() => {})
      }}
      className="bg-transparent text-[11px] text-[rgba(0,227,199,0.3)] hover:text-[rgba(0,227,199,0.5)] outline-none border-0 cursor-pointer transition-colors appearance-none"
    >
      {models.map((m) => <option key={m} value={m}>{m}</option>)}
    </select>
  )
}
```

- [ ] **Step 4: Update ContextRing.tsx — fix var(--a) → var(--accent)**

```tsx
import { useStore } from '../../stores'

export default function ContextRing() {
  const { prompt, completion } = useStore((s) => s.tokenUsage)
  const total = prompt + completion
  if (total === 0) return null

  const maxTokens = 200000
  const pct = Math.min((total / maxTokens) * 100, 100)
  const circ = 2 * Math.PI * 7
  const offset = circ * (1 - pct / 100)
  const color = pct > 80 ? 'var(--red)' : pct > 50 ? 'var(--amber)' : 'var(--accent)'

  return (
    <div className="relative group shrink-0" title={`${total.toLocaleString()} tokens`}>
      <svg width="18" height="18" className="-rotate-90">
        <circle cx="9" cy="9" r="7" fill="none" stroke="rgba(0,227,199,0.1)" strokeWidth="2" />
        <circle cx="9" cy="9" r="7" fill="none" stroke={color} strokeWidth="2" strokeLinecap="round"
          strokeDasharray={circ} strokeDashoffset={offset}
          className="transition-[stroke-dashoffset] duration-500" />
      </svg>
      <span className="absolute inset-0 flex items-center justify-center text-[7px] text-[var(--text-muted)] font-medium">
        {total >= 1000 ? `${(total / 1000).toFixed(0)}k` : total}
      </span>
    </div>
  )
}
```

- [ ] **Step 5: Rewrite InputArea.tsx — floating glass panel**

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
  const placeholder = !isConnected ? '正在连接...' : !sessionId ? '新建会话后开始对话' : isStreaming ? 'AI 回复中...' : '输入消息，⏎ 发送'

  return (
    <div className="absolute bottom-0 left-0 right-0 z-5 px-5 pb-4 pointer-events-none">
      <div className="max-w-[680px] mx-auto pointer-events-auto">
        <div className="flex flex-col gap-2 bg-[rgba(0,227,199,0.025)] backdrop-blur-[24px] border border-[rgba(0,227,199,0.07)] rounded-[var(--r-xl)] p-3.5 pb-2 shadow-[var(--shadow-glass)]">
          <textarea
            ref={textareaRef}
            value={text}
            onChange={e => setText(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder={placeholder}
            rows={2}
            disabled={!isConnected || isStreaming}
            className="w-full bg-transparent text-[var(--text)] text-[0.875rem] leading-relaxed resize-none outline-none placeholder:text-[var(--text-muted)] placeholder:italic disabled:opacity-40"
          />
          <div className="flex items-center gap-1.5 pt-1.5 border-t border-[rgba(255,255,255,0.03)]">
            <PermissionModeButton />
            <ThinkingLevelButton />
            <div className="flex-1" />
            <ModelSelector />
            <ContextRing />
            <button
              onClick={handleSend}
              disabled={!text.trim() || !isConnected || isStreaming}
              className="inline-flex items-center justify-center gap-1 h-[26px] px-3 text-[12px] font-semibold text-[var(--bg)] bg-[var(--accent)] hover:bg-[var(--accent-hover)] disabled:opacity-25 disabled:cursor-not-allowed rounded-[var(--r-md)] transition-all shrink-0"
            >
              {isStreaming ? <TypingIndicator /> : <><IconSend size={12} /> 发送</>}
            </button>
          </div>
        </div>
      </div>
    </div>
  )
}

function escapeHtml(s: string): string { return s.replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;') }
```

- [ ] **Step 6: Commit**

```bash
cd F:/openLoom && git add frontend/src/renderer/src/components/input/ && git commit -m "feat(ui): floating glass input area with cyan pills"
```

---

### Task 9: Shared Components

**Files:**
- Rewrite: `components/shared/Button.tsx`
- Modify: `components/shared/Toggle.tsx`
- Modify: `components/shared/Select.tsx`
- Modify: `components/shared/ContextMenu.tsx`
- Modify: `components/shared/Overlay.tsx`
- Modify: `components/shared/ToastContainer.tsx`
- Modify: `components/shared/SettingsModal.tsx`
- Rewrite: `components/shared/WelcomeScreen.tsx`
- Modify: `components/shared/Onboarding.tsx`

- [ ] **Step 1: Rewrite Button.tsx**

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
  primary: 'bg-[var(--accent-light)] text-[var(--accent)] hover:bg-[rgba(var(--accent-rgb),.25)] border border-[var(--border-accent)]',
  secondary: 'bg-[var(--bg-card)] text-[var(--text-light)] hover:bg-[rgba(255,255,255,0.04)] hover:text-[var(--text)] border border-[var(--border)]',
  ghost: 'bg-transparent text-[var(--text-muted)] hover:bg-[var(--bg-card)] hover:text-[var(--text)]',
  danger: 'bg-[var(--red-light)] text-[var(--red)] hover:bg-red-500/20 border border-[rgba(239,68,68,0.15)]',
}

const sizes: Record<string, string> = {
  sm: 'px-2.5 py-1 text-xs',
  md: 'px-4 py-2 text-sm',
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
      className={`rounded-[var(--r-sm)] font-medium transition-colors disabled:opacity-30 disabled:cursor-not-allowed ${variants[variant]} ${sizes[size]} ${className}`}
    >
      {children}
    </button>
  )
}
```

- [ ] **Step 2: Update Toggle.tsx — accent color already uses var(--accent)**

No changes needed — Toggle already uses `var(--accent)` which resolves to cyan.

- [ ] **Step 3: Update Select.tsx — border-radius + border color**

```tsx
interface SelectProps<T extends string> {
  value: T
  options: { value: T; label: string }[]
  onChange: (value: T) => void
  className?: string
}

export default function Select<T extends string>({
  value,
  options,
  onChange,
  className = '',
}: SelectProps<T>) {
  return (
    <select
      value={value}
      onChange={(e) => onChange(e.target.value as T)}
      className={`bg-[var(--bg-card)] text-[var(--text-light)] text-sm rounded-[var(--r-input)] px-3 py-1.5 outline-none border border-[var(--border)] focus:border-[var(--border-accent)] cursor-pointer transition-colors ${className}`}
    >
      {options.map((opt) => (
        <option key={opt.value} value={opt.value}>
          {opt.label}
        </option>
      ))}
    </select>
  )
}
```

- [ ] **Step 4: Update ContextMenu.tsx — border/bg**

Replace the outer div className:

```tsx
// In ContextMenu, change the container div to:
className="fixed z-50 min-w-[150px] bg-[var(--bg)] border border-[var(--border-accent)] rounded-[var(--r-md)] shadow-xl py-1 animate-fade-in backdrop-blur-xl"
```

Replace ContextMenuItem hover colors:

```tsx
// danger item:
'text-[var(--red)] hover:bg-[var(--red-light)]'
// normal item:
'text-[var(--text-light)] hover:bg-[rgba(0,227,199,0.04)] hover:text-[var(--text)]'
```

Full updated ContextMenu.tsx:

```tsx
import { useEffect, useRef, type ReactNode } from 'react'

interface ContextMenuProps {
  open: boolean
  x: number
  y: number
  onClose: () => void
  children: ReactNode
}

export default function ContextMenu({
  open,
  x,
  y,
  onClose,
  children,
}: ContextMenuProps) {
  const ref = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (!open) return
    const close = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) onClose()
    }
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose()
    }
    document.addEventListener('mousedown', close)
    document.addEventListener('keydown', onKey)
    return () => {
      document.removeEventListener('mousedown', close)
      document.removeEventListener('keydown', onKey)
    }
  }, [open, onClose])

  if (!open) return null

  return (
    <div
      ref={ref}
      className="fixed z-50 min-w-[150px] bg-[var(--bg)] border border-[var(--border-accent)] rounded-[var(--r-md)] shadow-xl py-1 animate-fade-in backdrop-blur-xl"
      style={{ left: x, top: y }}
    >
      {children}
    </div>
  )
}

export function ContextMenuItem({
  onClick,
  danger,
  children,
}: {
  onClick: () => void
  danger?: boolean
  children: ReactNode
}) {
  return (
    <button
      onClick={onClick}
      className={`w-full text-left px-3.5 py-2 text-[13px] transition-colors-fast ${
        danger
          ? 'text-[var(--red)] hover:bg-[var(--red-light)]'
          : 'text-[var(--text-light)] hover:bg-[rgba(0,227,199,0.04)] hover:text-[var(--text)]'
      }`}
    >
      {children}
    </button>
  )
}

export function ContextMenuDivider() {
  return <div className="my-1 border-t border-[var(--border)]" />
}
```

- [ ] **Step 5: Update Overlay.tsx — border-radius + border**

```tsx
import { useEffect, useRef, type ReactNode } from 'react'
import { IconX } from '../../utils/icons'

interface OverlayProps {
  open: boolean
  onClose: () => void
  children: ReactNode
  title?: string
}

export default function Overlay({ open, onClose, children, title }: OverlayProps) {
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

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <div
        ref={overlayRef}
        className="absolute inset-0 bg-black/70 backdrop-blur-sm"
        onClick={onClose}
      />
      <div className="relative bg-[var(--bg)] border border-[var(--border-accent)] rounded-[var(--r-lg)] shadow-[var(--shadow-lg)] max-w-xl w-full max-h-[80vh] overflow-y-auto m-4 animate-fade-in-up">
        {title && (
          <div className="flex items-center justify-between px-5 py-3.5 border-b border-[var(--border)]">
            <h2 className="text-sm font-semibold text-[var(--text)]">{title}</h2>
            <button
              onClick={onClose}
              className="text-[var(--text-muted)] hover:text-[var(--text)] transition-colors-fast"
            >
              <IconX size={16} />
            </button>
          </div>
        )}
        <div className="p-5">{children}</div>
      </div>
    </div>
  )
}
```

- [ ] **Step 6: Update ToastContainer.tsx — accent tokens already auto-update**

Toast uses `var(--accent-rgb)` which resolves to `0,227,199` in dark theme. No code changes needed.

- [ ] **Step 7: Update SettingsModal.tsx — border-radius**

Change the tab button `rounded-sm` to `rounded-[var(--r-sm)]` and accent references already work via CSS vars. No substantive changes needed since all accent references use `var(--accent)` / `var(--accent-rgb)`.

- [ ] **Step 8: Rewrite WelcomeScreen.tsx — Cyan Edge branding**

```tsx
import { useStore } from '../../stores'

export default function WelcomeScreen() {
  const createSession = useStore((s) => s.createSession)
  const switchSession = useStore((s) => s.switchSession)

  const handleStart = async () => {
    const id = await createSession()
    await switchSession(id)
  }

  return (
    <div className="flex items-center justify-center h-full bg-[var(--bg)]">
      <div className="text-center max-w-md animate-fade-up">
        <div className="w-12 h-12 mx-auto mb-6 rounded-[var(--r-lg)] bg-[rgba(0,227,199,0.06)] border border-[rgba(0,227,199,0.12)] flex items-center justify-center shadow-[0_0_30px_rgba(0,227,199,0.06)]">
          <span className="text-xl font-bold text-[var(--accent)]">L</span>
        </div>
        <h1 className="text-2xl text-[var(--text)] mb-3 tracking-tight font-semibold">
          openLoom
        </h1>
        <p className="text-[var(--text-light)] mb-8 text-[13px] leading-relaxed">
          本地优先的 AI 助理。支持多模型、MCP 工具、
          知识图谱记忆、LSP 代码理解和 Skills 技能系统。
        </p>
        <button
          onClick={handleStart}
          className="px-6 py-2.5 rounded-[var(--r-md)] bg-[var(--accent-light)] text-[var(--accent)] hover:bg-[rgba(var(--accent-rgb),.25)] border border-[var(--border-accent)] text-[13px] font-medium transition-colors"
        >
          开始新对话
        </button>
        <p className="text-[10px] text-[var(--text-muted)] mt-5">
          所有数据存储在本地 SQLite 数据库中
        </p>
      </div>
    </div>
  )
}
```

- [ ] **Step 9: Update Onboarding.tsx — border-radius + accent**

Change `rounded-sm` → `rounded-[var(--r-sm)]` and `rounded-full` on progress dots stays. The accent references already use `var(--accent)` so they auto-update.

- [ ] **Step 10: Commit**

```bash
cd F:/openLoom && git add frontend/src/renderer/src/components/shared/ && git commit -m "feat(ui): shared components — Button, Select, ContextMenu, Overlay, WelcomeScreen"
```

---

### Task 10: Build Verification & Final Polish

**Files:**
- All modified files

- [ ] **Step 1: Build the frontend**

Run: `cd F:/openLoom/frontend && npx electron-vite build 2>&1 | tail -20`
Expected: Build succeeds with no errors

- [ ] **Step 2: Check for any remaining old color references**

Run: `cd F:/openLoom/frontend/src/renderer/src && grep -rn "#E6397C\|#1A1A1D\|#212125\|#1E1E22\|#2A2A30\|var(--a)\|rgba(230,57" --include="*.tsx" --include="*.css" | grep -v "node_modules" | grep -v "base.css" | grep -v "themes/"`
Expected: No results (all old Rose/dark references replaced)

- [ ] **Step 3: Check for emoji usage in components**

Run: `cd F:/openLoom/frontend/src/renderer/src && grep -rn "&#128196\|&#9733\|⚡\|📋\|▶\|✓\|✕\|☐\|─" --include="*.tsx"`
Expected: No results (all emoji replaced with lucide icons)

- [ ] **Step 4: Fix any issues found in Steps 2-3**

If any old references or emoji remain, replace them with the correct Cyan Edge values or lucide icon imports.

- [ ] **Step 5: Final commit**

```bash
cd F:/openLoom && git add -A && git commit -m "chore(ui): final polish — clean up remaining old references"
```

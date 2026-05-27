# openLoom UI Redesign — Cyan Edge Design Spec

## Overview

Full UI overhaul of openLoom's Electron desktop frontend. Design language: **Cyan Edge** — a glassmorphism aesthetic with cyan accent on deep navy background, inspired by Cursor's interaction paradigm.

## Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Design language | Glassmorphism (Cyan Edge) | User preference; high tech feel, distinctive |
| Reference product | Cursor | Minimal chrome, focused content, keyboard-first |
| Chat layout | Bubble conversation (user right, AI left) | User preference; clear role distinction |
| Sidebar | Slide-out drawer (⌘B toggle) | Saves horizontal space; modern pattern |
| Titlebar | Minimal fused 36px (recommended option A) | Single-row; maximizes vertical content space |
| Icons | lucide-react (existing dep) | No emoji; consistent stroke-based icon system |
| UI library | None (hand-rolled) | Existing pattern; avoids dependency bloat |

## Color System

### Surfaces

| Token | Value | Usage |
|-------|-------|-------|
| `--bg` | `#0A0E14` | Primary background, deep navy |
| `--bg-card` | `rgba(0,227,199,0.03)` | Cards, bubbles, panels |
| `--bg-input` | `rgba(0,227,199,0.025)` | Input area background |
| `--bg-tooltip` | `rgba(0,227,199,0.06)` | Tooltips, popovers |
| `--bg-overlay` | `rgba(0,0,0,0.60)` | Modal backdrops |

### Accent

| Token | Value | Usage |
|-------|-------|-------|
| `--accent` | `#00E3C7` | Primary accent, cyan |
| `--accent-hover` | `#33EAD3` | Hover state |
| `--accent-rgb` | `0,227,199` | For rgba() compositions |
| `--accent-light` | `rgba(0,227,199,0.08)` | Subtle accent backgrounds |
| `--accent-strong` | `rgba(0,227,199,0.14)` | Emphasized accent backgrounds |
| `--accent-glow` | `rgba(0,227,199,0.20)` | Glow/shadow effects |

### Text

| Token | Value | Usage |
|-------|-------|-------|
| `--text` | `rgba(255,255,255,0.85)` | Primary text |
| `--text-light` | `rgba(255,255,255,0.45)` | Secondary text |
| `--text-muted` | `rgba(255,255,255,0.15)` | Tertiary/hint text |

### Borders

| Token | Value | Usage |
|-------|-------|-------|
| `--border` | `rgba(0,227,199,0.06)` | General borders |
| `--border-light` | `rgba(0,227,199,0.03)` | Subtle dividers |
| `--border-accent` | `rgba(0,227,199,0.12)` | Accent borders (active items, inputs) |

### Semantic

| Token | Value | Usage |
|-------|-------|-------|
| `--green` | `#2DD4BF` | Success, connected |
| `--amber` | `#F59E0B` | Warning |
| `--red` | `#EF4444` | Error, close button hover |

### Glassmorphism

- `backdrop-filter: blur(20px)` on cards/bubbles
- `backdrop-filter: blur(24px)` on input area
- Backgrounds use `rgba(0,227,199,0.02~0.03)` for tinted translucency
- Box-shadows include subtle cyan glow: `0 0 40px rgba(0,227,199,0.03)`

## Layout Architecture

### Titlebar (36px, fused)

```
[☰ toggle] [L logo] [openLoom] ─── [session title, draggable] ─── [● connected] [─] [☐] [✕]
```

- Height: 36px
- Left group (no-drag): sidebar toggle button, logo square (16×16, `#00E3C7`, r-3), app name
- Center (drag): current session title in `--text-muted`
- Right group (no-drag): connection indicator dot + "已连接", window controls
- Window controls: 36×36px each, hover `rgba(255,255,255,0.06)`, close hover `#EF4444`
- Border-bottom: `1px solid var(--border)`
- All icons from lucide-react: `PanelLeftClose`/`PanelLeft` (toggle), `Minux`/`Square`/`X` (window)

### Sidebar (240px, slide-out drawer)

**Behavior:**
- Default: collapsed (0px)
- Toggle: ⌘B keyboard shortcut or ☰ button in titlebar
- Transition: `width 200ms var(--ease-out)` + `opacity 200ms`
- When collapsed, main content takes full width

**Structure (when expanded):**

```
┌──────────────────────────┐
│ [L] openLoom        [◀] │  ← header: logo + close toggle
├──────────────────────────┤
│ [⌘K] 搜索会话...         │  ← search input
├──────────────────────────┤
│ 已置顶                   │  ← section header
│ ▎重构 Agent 循环          │  ← active: left 2px cyan bar
│   记忆系统优化            │
│ 今天                     │
│   MCP 协议调试            │
│   LSP 客户端集成          │
│ 昨天                     │
│   Bridge Telegram 接入   │
├──────────────────────────┤
│ [+ 新建会话]  [⚙]        │  ← bottom actions
└──────────────────────────┘
```

- Section headers: `--text-muted`, 10px, uppercase, `letter-spacing: 1.2px`
- Active session: `background: rgba(0,227,199,0.05)`, `border-left: 2px solid #00E3C7`
- Inactive sessions: transparent bg, `border-left: 2px solid transparent`
- Each session item shows title + relative time + message count
- Hover: `background: rgba(0,227,199,0.03)`
- New session button: `background: rgba(0,227,199,0.08)`, `border: 1px solid rgba(0,227,199,0.12)`, `color: #00E3C7`
- Settings button: ghost style, `color: --text-muted`

### Chat Area

**AI messages (left-aligned):**

```
  [L avatar]  ┌─────────────────────────────┐
              │ Message content with glass    │
              │ background and left-first     │
              │ border-radius                 │
              └─────────────────────────────┘
              14:32  📋
```

- Avatar: 28×28px circle, `background: rgba(0,227,199,0.08)`, `border: 1px solid rgba(0,227,199,0.12)`, contains "L" in `#00E3C7`
- Bubble: `backdrop-filter: blur(12px)`, `background: rgba(0,227,199,0.03)`, `border: 1px solid rgba(0,227,199,0.06)`
- Border-radius: `4px 12px 12px 12px` (top-left sharp for speech direction)
- Padding: `12px 14px`
- Footer: timestamp `--text-muted` + copy button (lucide `Copy` icon), shown on hover

**User messages (right-aligned):**

```
              ┌─────────────────────────────┐
              │ Message content with cyan    │
              │ tinted background            │
              └─────────────────────────────┘ [L avatar]  ← optional
                                      14:33
```

- Bubble: `backdrop-filter: blur(12px)`, `background: rgba(0,227,199,0.1)`, `border: 1px solid rgba(0,227,199,0.15)`
- Border-radius: `12px 4px 12px 12px` (top-right sharp)
- Max-width: 75%
- No avatar on user side (bubble alignment is sufficient)

**Thinking block (collapsed):**

```
  [L]  ▶ 思考过程 · 3.2s
```

- `background: rgba(255,255,255,0.02)`, `border: 1px solid rgba(255,255,255,0.04)`
- Border-radius: `8px`
- Expand on click, shows thinking content with same style as AI bubble
- Duration badge in `--text-muted`
- Icons: lucide `ChevronRight` (collapsed), `ChevronDown` (expanded)

**Tool call block:**

```
  [L]  ⚡ use_skill("memory-optimizer")  ✓
```

- Same card style as thinking block
- Tool name in `--text-light`, status icon in `#00E3C7` (success) / `--amber` (running) / `--red` (error)
- Icons: lucide `Zap` (tool), `Check` (success), `Loader` (running), `XCircle` (error)
- Expand shows args/result JSON

### Input Area (floating glass panel)

```
┌──────────────────────────────────────────────┐
│  输入消息，⏎ 发送                              │  ← textarea
│──────────────────────────────────────────────│
│  [operate] [think: auto]    deepseek-v4  ○ ➤ │  ← controls row
└──────────────────────────────────────────────┘
```

- Position: absolute bottom, `max-width: 680px`, centered
- `backdrop-filter: blur(24px)`
- `background: rgba(0,227,199,0.025)`
- `border: 1px solid rgba(0,227,199,0.07)`
- `border-radius: 14px`
- `box-shadow: 0 0 40px rgba(0,227,199,0.03), 0 4px 20px rgba(0,0,0,0.3)`
- Controls row separated by `border-top: 1px solid rgba(255,255,255,0.03)`
- Pills: permission mode in accent bg, thinking level in neutral bg
- Context ring: SVG circle, `--accent` stroke
- Send button: `background: #00E3C7`, `color: #0A0E14`, `font-weight: 600`, `border-radius: 8px`
- Icons: lucide `Send` (send), `Shield` (permission), `Brain` (thinking), `ChevronDown` (model selector)

### Status Bar (22px)

```
[● 已连接]                              [1 agent · 1 streaming]
```

- Height: 22px
- `border-top: 1px solid rgba(0,227,199,0.03)`
- Connection dot: 4px, `#00E3C7` with glow
- Text: `--text-muted`, 10px
- Icons: lucide `Activity` (agent count), `Radio` (streaming)

## Border Radius System

| Token | Value | Usage |
|-------|-------|-------|
| `--r-sm` | `4px` | Pills, tags, small controls |
| `--r-md` | `8px` | Cards, thinking/tool blocks, buttons |
| `--r-lg` | `12px` | Message bubbles |
| `--r-xl` | `14px` | Input area overall |
| `--r-full` | `9999px` | Avatars, connection dot |

## Animation

| Property | Value |
|----------|-------|
| Easing | `cubic-bezier(0.16,1,0.3,1)` (Apple ease-out) |
| Fast | 150ms — hover, focus, active states |
| Normal | 200ms — sidebar toggle, pill toggle |
| Slow | 250ms — message entrance, modal appear |
| Hover effect | `border-color` + `box-shadow` glow (no transform) |
| Sidebar | `width` + `opacity` transition, 200ms |
| Messages | `fade-up` entrance, 250ms |
| Reduced motion | All durations → 0.01ms |

## Typography

| Property | Value |
|----------|-------|
| Font | Inter, -apple-system, BlinkMacSystemFont, Segoe UI, sans-serif |
| Monospace | JetBrains Mono, Cascadia Code, Fira Code, Consolas, monospace |
| Body size | 14px |
| Line height | 1.7 (body), 1.6 (messages) |
| Selection | `background: rgba(0,227,199,0.22); color: #fff` |

## Theme System

4 themes via `data-theme` attribute on `<html>`:

| Theme | Accent | Background | Status |
|-------|--------|------------|--------|
| `dark` (default) | `#00E3C7` cyan | `#0A0E14` navy | New |
| `light` | `#0D9488` teal | `#F8F9FA` cool gray | Rewrite |
| `midnight` | `#A5BFF8` periwinkle | `#0F1729` deep blue | Rewrite |
| `warm-paper` | `#B05A30` terracotta | `#F5F0E8` cream | Rewrite |

All non-default themes must be rewritten from scratch. Current theme files have duplicate declarations and invalid CSS lines.

## Scrollbar

- Width: 4px
- Track: transparent
- Thumb: `rgba(128,128,128,0.2)`, hover `rgba(128,128,128,0.4)`
- Border-radius: 2px

## Icon Policy

All icons use **lucide-react** (already a dependency). No emoji in UI.

| Usage | Icon |
|-------|------|
| Sidebar toggle | `PanelLeftClose` / `PanelLeft` |
| Search | `Search` |
| Settings | `Settings` |
| New session | `Plus` |
| Send | `Send` |
| Copy | `Copy` |
| Permission mode | `Shield` |
| Thinking level | `Brain` |
| Model selector | `ChevronDown` |
| Tool call | `Zap` |
| Success | `Check` |
| Running/loading | `Loader` |
| Error | `XCircle` |
| Thinking expand | `ChevronRight` / `ChevronDown` |
| Pin/unpin | `Pin` / `PinOff` |
| Delete | `Trash2` |
| Connection | `Wifi` / `WifiOff` |
| Activity | `Activity` |
| Minimize | `Minus` |
| Maximize | `Square` |
| Close | `X` |
| File | `File` |
| Link/URL | `ExternalLink` |

## Scope

### In scope
- `base.css` complete rewrite with new design tokens
- All 3 theme CSS files rewrite
- `AppShell` — new titlebar + sidebar drawer layout
- `Sidebar` — slide-out drawer with Cyan Edge styling
- `SessionItem` — left indicator bar style
- `StatusBar` — minimal Cyan Edge styling
- `WindowControls` — new style matching design
- `ChatArea` — layout + scroll behavior
- `AssistantMessage` — left-aligned glass bubble + avatar
- `UserMessage` — right-aligned cyan bubble
- `ThinkingBlock` — collapsed card with lucide icons
- `ToolGroupBlock` — single-line card with lucide icons
- `TextBlock` — prose-chat rewrite for new tokens
- `InputArea` — floating glass panel redesign
- `ModelSelector` / `PermissionModeButton` / `ThinkingLevelButton` — pill redesign
- `ContextRing` — new accent color
- `SettingsModal` — glass overlay matching new design
- `ToastContainer` — accent color update
- `WelcomeScreen` / empty states — Cyan Edge branding
- `Onboarding` — accent color update
- `Button` / `Toggle` / `Select` / `ContextMenu` — shared component restyle

### Out of scope
- Zustand store structure changes
- WebSocket/RPC/StreamBuffer changes
- Backend changes
- New features (only visual redesign)
- TipTap/CodeMirror editor integration (separate task)

## Files to Modify

All under `frontend/src/renderer/src/`:

| File | Change |
|------|--------|
| `styles/base.css` | Complete rewrite: new tokens, glassmorphism utilities, prose-chat |
| `themes/light.css` | Rewrite from scratch |
| `themes/midnight.css` | Rewrite from scratch |
| `themes/warm-paper.css` | Rewrite from scratch |
| `components/app/AppShell.tsx` | New layout: fused titlebar + sidebar drawer |
| `components/app/Sidebar.tsx` | Slide-out drawer, time grouping, left indicator |
| `components/app/SessionItem.tsx` | Left indicator bar, hover glow |
| `components/app/StatusBar.tsx` | Minimal Cyan Edge style |
| `components/app/WindowControls.tsx` | New window control style |
| `components/chat/ChatArea.tsx` | Layout + new empty state |
| `components/chat/AssistantMessage.tsx` | Glass bubble + avatar |
| `components/chat/UserMessage.tsx` | Right-aligned cyan bubble |
| `components/chat/ThinkingBlock.tsx` | Collapsed card + lucide icons |
| `components/chat/ToolGroupBlock.tsx` | Single-line card + lucide icons |
| `components/chat/TextBlock.tsx` | Prose-chat with new tokens |
| `components/chat/SubagentCard.tsx` | Accent update |
| `components/chat/FileBlock.tsx` | Accent update |
| `components/chat/MessageFooterActions.tsx` | Lucide icons + hover style |
| `components/input/InputArea.tsx` | Floating glass panel |
| `components/input/ModelSelector.tsx` | Pill restyle |
| `components/input/PermissionModeButton.tsx` | Pill restyle |
| `components/input/ThinkingLevelButton.tsx` | Pill restyle |
| `components/input/ContextRing.tsx` | New accent |
| `components/shared/SettingsModal.tsx` | Glass overlay |
| `components/shared/ToastContainer.tsx` | Accent update |
| `components/shared/WelcomeScreen.tsx` | Cyan Edge branding |
| `components/shared/Onboarding.tsx` | Accent update |
| `components/shared/Button.tsx` | Cyan Edge restyle |
| `components/shared/Toggle.tsx` | Accent update |
| `components/shared/Select.tsx` | Accent update |
| `components/shared/ContextMenu.tsx` | Accent update |
| `stores/ui.ts` | Add `sidebarOpen` state + keyboard shortcut |

Total: ~30 files modified

# openLoom UI CSS Variable Fix — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eliminate all CSS variable references from Tailwind v4 arbitrary value brackets and migrate to inline styles, matching the Cyan Edge visual mockup.

**Architecture:** Two-pass approach: (1) Fix remaining `var(--` in className across all component files, (2) Verify visual output matches the target mockup by running the app and checking each screen state.

**Tech Stack:** React 19, Tailwind CSS v4, TypeScript, inline styles, CSS custom properties (for colors only via inline style)

---

## File Structure

| File | Status | Remaining `var(--` refs |
|------|--------|------------------------|
| `App.tsx` | Needs fix | 9 (loading/error screens) |
| `AppShell.tsx` | **Done** | 0 |
| `Sidebar.tsx` | **Done** | 0 |
| `SessionItem.tsx` | Needs fix | 2 (ContextMenu children) |
| `StatusBar.tsx` | **Done** | 0 |
| `WindowControls.tsx` | **Done** | 0 |
| `ChatWorkspace.tsx` | **Done** | 0 |
| `ChatArea.tsx` | **Done** | 0 |
| `AssistantMessage.tsx` | Needs fix | 3 (avatar bg/border/text) |
| `UserMessage.tsx` | Needs fix | 2 (text color, empty state) |
| `ThinkingBlock.tsx` | **Done** | 0 |
| `ToolGroupBlock.tsx` | **Done** | 0 |
| `TextBlock.tsx` | Needs fix | 1 (prose-chat references CSS vars — ok in CSS file) |
| `FileBlock.tsx` | Needs fix | ~3 |
| `SubagentCard.tsx` | Needs fix | ~3 |
| `InputArea.tsx` | **Done** | 0 |
| `ModelSelector.tsx` | **Done** | 0 |
| `PermissionModeButton.tsx` | Fine | Uses `.pill` CSS class (defined in base.css, works) |
| `ThinkingLevelButton.tsx` | Fine | Uses `.pill-neutral` CSS class (defined in base.css, works) |
| `ContextRing.tsx` | Needs fix | 3 (colors in SVG) |
| `WelcomeScreen.tsx` | **Done** | 0 |
| `MessageFooterActions.tsx` | **Done** | 0 |
| `Button.tsx` | Needs fix | 4 (variant definitions) |
| `ContextMenu.tsx` | Needs fix | 3 |
| `Overlay.tsx` | Needs fix | ~4 |
| `SettingsModal.tsx` | Needs fix | ~5 |
| `ToastContainer.tsx` | Needs fix | ~2 |
| `Onboarding.tsx` | Needs fix | ~5 |
| `MarkdownEditor.tsx` | Needs fix | 1 |
| `extensions.ts` | Needs fix | 1 |
| `icons.tsx` | Fine | Just lucide re-exports |

---

### Task 1: Fix Chat Bubbles (AssistantMessage, UserMessage)

**Files:**
- Modify: `frontend/src/renderer/src/components/chat/AssistantMessage.tsx`
- Modify: `frontend/src/renderer/src/components/chat/UserMessage.tsx`

- [ ] **Step 1: Fix AssistantMessage avatar and loading state**

Replace lines 14-16 (avatar with CSS variable refs) and line 49 (loading text color):

```tsx
export default function AssistantMessage({ message }: { message: Message }) {
  return (
    <div className="flex gap-2.5 max-w-[85%] group animate-fade-in">
      {/* Avatar */}
      <div
        className="w-7 h-7 rounded-full flex items-center justify-center shrink-0 mt-0.5"
        style={{
          backgroundColor: 'rgba(0,227,199,0.08)',
          border: '1px solid rgba(0,227,199,0.12)',
        }}
      >
        <span className="text-[11px] font-extrabold" style={{ color: 'var(--accent)' }}>L</span>
      </div>
      {/* Content */}
      <div className="flex-1 min-w-0 space-y-2">
        {/* ... blocks rendering unchanged ... */}
        {message.blocks.length === 0 && (
          <div className="flex items-center gap-2 text-[13px]" style={{ color: 'var(--text-muted)' }}>
            <span>思考中</span>
            <TypingIndicator />
          </div>
        )}
```

- [ ] **Step 2: Fix UserMessage text colors**

Replace line 13 (text color) and line 19 (empty state):

```tsx
            <div
              className="text-[13.5px] leading-[1.6]"
              style={{ color: 'var(--text)' }}
              dangerouslySetInnerHTML={{ ... }}
            />
          ) : (
            <span className="italic text-[13px]" style={{ color: 'var(--text-muted)' }}>(空)</span>
```

- [ ] **Step 3: Verify TypeScript compiles**

Run: `cd F:/openloom/frontend && npx tsc --noEmit`

---

### Task 2: Fix App.tsx Loading/Error Screens

**Files:**
- Modify: `frontend/src/renderer/src/App.tsx`

- [ ] **Step 1: Convert loading state to inline styles**

Replace lines 46-74:

```tsx
  if (error) {
    return (
      <div className="flex items-center justify-center h-screen" style={{ backgroundColor: 'var(--bg)' }}>
        <div className="text-center max-w-sm animate-fade-in">
          <h1 className="text-2xl font-semibold mb-3" style={{ color: 'var(--text)' }}>启动失败</h1>
          <p className="mb-5 text-sm" style={{ color: 'var(--red)' }}>{error}</p>
          <button onClick={handleRetry}
            className="px-5 py-2 rounded text-sm transition-colors"
            style={{
              color: 'var(--text-light)',
              backgroundColor: 'var(--bg-card)',
              border: '1px solid var(--border)',
            }}
          >
            重试
          </button>
        </div>
      </div>
    )
  }

  if (!ready) {
    return (
      <div className="flex items-center justify-center h-screen" style={{ backgroundColor: 'var(--bg)' }}>
        <div className="text-center animate-fade-in">
          <div
            className="w-20 h-20 mx-auto mb-6 rounded flex items-center justify-center animate-breathe"
            style={{
              backgroundColor: 'var(--accent-light)',
              border: '1px solid rgba(var(--accent-rgb),.15)',
              boxShadow: 'var(--shadow-glow)',
            }}
          >
            <span className="text-3xl font-bold" style={{ color: 'var(--accent)' }}>L</span>
          </div>
          <h1 className="text-[32px] font-semibold tracking-tight" style={{ color: 'var(--text)' }}>
            openLoom
          </h1>
          <div className="flex items-center gap-2 justify-center mt-4 text-sm" style={{ color: 'var(--text-muted)' }}>
            <span className="typing-dots"><span/><span/><span/></span>
          </div>
        </div>
      </div>
    )
  }
```

- [ ] **Step 2: Verify TypeScript compiles**

Run: `cd F:/openloom/frontend && npx tsc --noEmit`

---

### Task 3: Fix Shared Components (Button, ContextMenu, Overlay)

**Files:**
- Modify: `frontend/src/renderer/src/components/shared/Button.tsx`
- Modify: `frontend/src/renderer/src/components/shared/ContextMenu.tsx`
- Modify: `frontend/src/renderer/src/components/shared/Overlay.tsx`

- [ ] **Step 1: Fix Button.tsx — convert variant definitions to inline style factory**

Replace the `variants` record with a function:

```tsx
const variantStyle = (variant: string): React.CSSProperties => {
  switch (variant) {
    case 'primary':
      return {
        color: 'var(--accent)',
        backgroundColor: 'var(--accent-light)',
        border: '1px solid var(--border-accent)',
      }
    case 'secondary':
      return {
        color: 'var(--text-light)',
        backgroundColor: 'var(--bg-card)',
        border: '1px solid var(--border)',
      }
    case 'ghost':
      return { color: 'var(--text-muted)', backgroundColor: 'transparent' }
    case 'danger':
      return {
        color: 'var(--red)',
        backgroundColor: 'var(--red-light)',
        border: '1px solid rgba(239,68,68,0.15)',
      }
    default:
      return {}
  }
}

export default function Button({ children, onClick, disabled, variant = 'secondary', size = 'md', className = '', title, 'aria-label': ariaLabel }: ButtonProps) {
  const sizeClass = size === 'sm' ? 'px-2.5 py-1 text-xs' : 'px-4 py-2 text-sm'
  return (
    <button
      onClick={onClick}
      disabled={disabled}
      title={title}
      aria-label={ariaLabel}
      className={`rounded font-medium transition-colors disabled:opacity-30 disabled:cursor-not-allowed ${sizeClass} ${className}`}
      style={variantStyle(variant)}
    >
      {children}
    </button>
  )
}
```

- [ ] **Step 2: Fix ContextMenu.tsx — replace CSS variable className refs**

Replace the outer div (line 41) and MenuItem (line 61-65):

```tsx
    <div
      ref={ref}
      className="fixed z-50 min-w-[150px] shadow-xl py-1 animate-fade-in"
      style={{
        left: x, top: y,
        backgroundColor: 'var(--bg)',
        border: '1px solid var(--border-accent)',
        borderRadius: 8,
        backdropFilter: 'blur(24px)',
      }}
    >
```

```tsx
export function ContextMenuItem({ onClick, danger, children }: { onClick: () => void; danger?: boolean; children: ReactNode }) {
  return (
    <button
      onClick={onClick}
      className="w-full text-left px-3.5 py-2 text-[13px] transition-colors"
      style={danger
        ? { color: 'var(--red)' }
        : { color: 'var(--text-light)' }
      }
    >
      {children}
    </button>
  )
}
```

- [ ] **Step 3: Fix Overlay.tsx — convert CSS variable refs**

```tsx
// Backdrop
<div
  className="fixed inset-0 z-40 flex items-center justify-center animate-fade-in"
  style={{ backgroundColor: 'var(--bg-overlay)', backdropFilter: 'blur(4px)' }}
>
  {/* Panel */}
  <div
    className="relative w-[420px] max-h-[80vh] overflow-y-auto shadow-xl animate-scale-in"
    style={{
      backgroundColor: 'var(--bg)',
      border: '1px solid var(--border-accent)',
      borderRadius: 12,
    }}
  >
```

- [ ] **Step 4: Verify TypeScript compiles**

Run: `cd F:/openloom/frontend && npx tsc --noEmit`

---

### Task 4: Fix Remaining Components (ContextRing, SettingsModal, ToastContainer, Onboarding, FileBlock, SubagentCard)

**Files:**
- Modify: `frontend/src/renderer/src/components/input/ContextRing.tsx`
- Modify: `frontend/src/renderer/src/components/shared/SettingsModal.tsx`
- Modify: `frontend/src/renderer/src/components/shared/ToastContainer.tsx`
- Modify: `frontend/src/renderer/src/components/shared/Onboarding.tsx`
- Modify: `frontend/src/renderer/src/components/chat/FileBlock.tsx`
- Modify: `frontend/src/renderer/src/components/chat/SubagentCard.tsx`
- Modify: `frontend/src/renderer/src/editor/MarkdownEditor.tsx`
- Modify: `frontend/src/renderer/src/editor/extensions.ts`

- [ ] **Step 1: Fix ContextRing.tsx**

Replace stroke colors and text colors that use CSS variables:

```tsx
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
      <span className="absolute inset-0 flex items-center justify-center text-[7px] font-medium" style={{ color: 'var(--text-muted)' }}>
        {total >= 1000 ? `${(total / 1000).toFixed(0)}k` : total}
      </span>
    </div>
  )
}
```

- [ ] **Step 2: Fix SettingsModal.tsx**

Convert tab buttons and about section CSS variable refs to inline styles. Key changes:

```tsx
// Tab button active state
style={tab === t.id
  ? { color: 'var(--accent)', backgroundColor: 'var(--accent-light)' }
  : { color: 'var(--text-muted)' }
}

// About section text  
<p className="text-[13px]" style={{ color: 'var(--text-muted)' }}>
```

- [ ] **Step 3: Fix ToastContainer.tsx, Onboarding.tsx**

Convert remaining `text-[var(--...)]` and `bg-[var(--...)]` to inline styles.

- [ ] **Step 4: Fix FileBlock.tsx and SubagentCard.tsx**

Convert all CSS variable className refs to inline styles.

- [ ] **Step 5: Fix MarkdownEditor.tsx and extensions.ts**

Convert `text-[var(--text)]` to inline style.

- [ ] **Step 6: Verify TypeScript compiles**

Run: `cd F:/openloom/frontend && npx tsc --noEmit`

---

### Task 5: Visual Verification

- [ ] **Step 1: Build the frontend**

Run: `cd F:/openloom/frontend && npm run build`

- [ ] **Step 2: Check for build errors**

Expected: No errors, successful build.

- [ ] **Step 3: Document final state**

All files use inline styles for layout-critical properties. CSS variables are consumed only via `style={{ color: 'var(--text)' }}` pattern.

---

### Task 6: Commit

- [ ] **Step 1: Stage all modified files**

```bash
git add frontend/src/renderer/src/
```

- [ ] **Step 2: Commit**

```bash
git commit -m "fix: migrate CSS variables from Tailwind className to inline styles

Tailwind CSS v4 does not resolve var(--x) inside arbitrary value
brackets, causing collapsed heights, missing border-radius, and
layout shifts. All layout-critical properties (height, width,
border-radius, border, box-shadow, background-color, backdrop-filter)
moved to React inline style. Colors use style={{ color: 'var(--text)' }}.
Non-layout utilities (flex, gap, padding, font-size) stay in className."
```

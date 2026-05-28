# Frontend UI Redesign — Refined Glass v2

## Summary

Full visual overhaul of openLoom frontend. Keep glassmorphism direction but fix all contrast, hierarchy, and UX issues.

## Design Decisions

| Decision | Choice |
|----------|--------|
| Direction | Refined Glass — fix contrast, add real depth |
| Accent color | Cyan #22D3EE (Tailwind cyan-400) |
| Message layout | Hybrid: user bubble right + assistant full-width |
| Input area | Centered Composer (Perplexity style) |
| Settings Modal | Larger, left nav 160px, theme card previews with mini page blocks |

## Color System

```
Background:     #0D1117
Surface-1:      rgba(34,211,238, 0.03)  — sidebar bg
Surface-2:      rgba(34,211,238, 0.06)  — cards, active items
Surface-3:      rgba(34,211,238, 0.10)  — user message bubbles

Text-primary:   rgba(255,255,255, 0.88)
Text-secondary: rgba(255,255,255, 0.60)
Text-muted:     rgba(255,255,255, 0.30)

Border-subtle:  rgba(34,211,238, 0.06)
Border-default: rgba(34,211,238, 0.10)
Border-accent:  rgba(34,211,238, 0.18)

Accent:         #22D3EE
Accent-hover:   #67E8F9
Accent-rgb:     34,211,238
```

## Typography

- Body: 13px, line-height 1.7
- Sidebar items: 12px
- Labels/captions: 11px (minimum anywhere)
- Section headers: 11px uppercase tracking
- Code: JetBrains Mono, 12.5px

## Border Radii

- Buttons/pills: 8px
- Cards/panels: 10px
- Message bubbles: 14px (with 4px on inner corner)
- Input composer: 16px
- Modal: 14px

## Components

### AppShell / Titlebar (48px height)
- Sidebar toggle left
- Session title centered (muted)
- Connection dot + window controls right
- No duplicate logo in titlebar (logo only in sidebar)
- Remove StatusBar entirely (redundant)

### Sidebar (220px)
- Header: logo + brand + collapse button
- Search: 32px height, rounded-8
- Session list: date group headers (uppercase 10px), items with title + meta line
- Active item: Surface-2 bg + accent border
- Bottom: "新建会话" button full-width

### Chat Area — Hybrid Layout
- User messages: right-aligned bubble, Surface-3 bg, rounded 14-4-14-14
- Assistant messages: full-width, no bubble bg
  - Header line: avatar (20px rounded-6) + "Loom" label + timestamp
  - Content indented 28px from left
  - Code blocks full-width within content area
- Gap between messages: 20px
- Max content width: 640px centered

### Input Area — Centered Composer
- Max-width: 620px centered
- 16px border-radius, Surface-1 bg
- Shadow: `0 0 24px rgba(34,211,238,0.03), 0 4px 16px rgba(0,0,0,0.3)`
- Textarea: min 2 rows, auto-expand
- Toolbar below (separated by subtle border-top):
  - Left: pill buttons (Permission, Thinking, Model selector) — each 26px h, rounded-full
  - Right: Send button (accent solid, 30px h, rounded-8)
- Position: docked at bottom of flex column (not absolute/floating)

### Settings Modal
- Size: 640px wide, 480px min-height (larger than current)
- Left nav: 160px, accent bg on active tab
- Right content: 24px padding, section headers
- Theme selector: 4-column grid of cards, each showing mini page preview (sidebar + chat + input mockup in theme colors)
- Agent/Model creation: inline expand (no nested modal)
- Toggle switches for boolean settings
- Close button: top-right of content area

### Shared Patterns
- `.pill`: 26px height, rounded-full, 10-11px text
- `.card-interactive`: Surface-2 on hover, accent border
- Transitions: 150ms ease-out for all interactive state changes
- Focus: 2px accent outline, 2px offset

## Files to Modify

1. `src/renderer/src/styles/base.css` — full variable + utility rewrite
2. `src/renderer/src/components/app/AppShell.tsx` — layout structure, remove StatusBar
3. `src/renderer/src/components/app/Sidebar.tsx` — widths, spacing, active states
4. `src/renderer/src/components/app/SessionItem.tsx` — new styling
5. `src/renderer/src/components/app/StatusBar.tsx` — DELETE
6. `src/renderer/src/components/chat/ChatArea.tsx` — message gap, max-width
7. `src/renderer/src/components/chat/AssistantMessage.tsx` — full-width layout
8. `src/renderer/src/components/chat/UserMessage.tsx` — bubble style
9. `src/renderer/src/components/input/InputArea.tsx` — centered composer
10. `src/renderer/src/components/shared/WelcomeScreen.tsx` — refresh styling
11. `src/renderer/src/components/shared/SettingsModal.tsx` — complete redesign
12. `src/renderer/src/components/shared/Overlay.tsx` — larger size, updated styling
13. `src/renderer/src/components/shared/Button.tsx` — pill variant
14. `src/renderer/src/components/input/ModelSelector.tsx` — pill style
15. `src/renderer/src/components/input/ThinkingLevelButton.tsx` — pill style
16. `src/renderer/src/components/input/PermissionModeButton.tsx` — pill style

## Out of Scope

- Adding new features or new tabs
- Changing state management or data flow
- Modifying backend communication
- Light/warm-paper/midnight themes (just preserve switching, fix dark theme first)

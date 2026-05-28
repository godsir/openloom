# openLoom UI Redesign — CSS Implementation Strategy Addendum

> Supplement to `docs/superpowers/specs/2026-05-28-ui-redesign-design.md`
> Date: 2026-05-28

## Problem

Tailwind CSS v4 does not resolve CSS custom properties (`var(--x)`) inside arbitrary value brackets in this project's build pipeline. Classes like `h-[var(--titlebar-h)]`, `rounded-[var(--r-lg)]`, `bg-[var(--bg-card)]` generate CSS that doesn't render correctly, causing:

- Collapsed heights (titlebar, statusbar, sidebar items)
- Missing border-radius (bubbles, cards, input area)
- Misaligned layout (logo off-center, input stuck to left edge)
- Lost box-shadows and glass effects

## Strategy: Inline Style for Layout, Tailwind for Utilities

### What goes in inline `style={}`

All **layout-critical** properties that previously used CSS variables in Tailwind arbitrary values:

| Property | Example | Why inline |
|----------|---------|------------|
| `height` | `style={{ height: 36 }}` | Titlebar/sidebar/statusbar heights |
| `width` | `style={{ maxWidth: 680 }}` | Input area, sidebar width |
| `border-radius` | `style={{ borderRadius: '12px 4px 12px 12px' }}` | Message bubbles, cards |
| `border` | `style={{ border: '1px solid rgba(0,227,199,0.06)' }}` | All component borders |
| `box-shadow` | `style={{ boxShadow: '0 0 40px rgba(0,227,199,0.03)' }}` | Glass effects, glow |
| `background-color` | `style={{ backgroundColor: 'rgba(0,227,199,0.03)' }}` | Card/bubble surfaces |
| `backdrop-filter` | `style={{ backdropFilter: 'blur(12px)' }}` | Glassmorphism |
| `color` via CSS var | `style={{ color: 'var(--text)' }}` | CSS variable for colors still works in inline style |

### What stays in Tailwind `className`

All non-variable utilities:

- `flex`, `flex-col`, `flex-1`, `items-center`, `justify-center`, `justify-between`
- `gap-2`, `px-3`, `py-2`, `text-[12px]`, `font-medium`
- `overflow-hidden`, `overflow-y-auto`, `truncate`, `min-w-0`
- `opacity-0`, `group-hover:opacity-100`, `transition-opacity`
- `animate-fade-up`, `animate-fade-in`, `animate-scale-in`

### What gets eliminated

- All `h-[var(--...)]`, `w-[var(--...)]`, `rounded-[var(--...)]`, `bg-[var(--...)]` 
- All `border-[var(--...)]`, `text-[var(--...)]`, `shadow-[var(--...)]`
- `hover:` modifiers that reference CSS variables (replaced with `onMouseEnter`/`onMouseLeave`)
- `transition-all duration-[var(--dur-fast)] ease-[var(--ease-out)]` (replaced with explicit values in style)

## CSS Variables

CSS variables in `base.css` and theme files are still the **single source of truth** for color values. They are consumed via `style={{ color: 'var(--text)' }}` pattern. The variables themselves remain unchanged; only the mechanism of referencing them changes.

## Files Affected

All 30 files from the original spec plus `ChatWorkspace.tsx` and `ModelSelector.tsx`. Each file converts layout-critical Tailwind CSS variable references to inline styles following the pattern above.

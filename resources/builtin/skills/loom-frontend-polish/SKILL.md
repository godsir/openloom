---
name: loom-frontend-polish
description: Polish frontend interfaces to production quality — improve UX, accessibility, responsiveness, and visual design. Use when refining UI components, improving user experience, or preparing for release.
version: "1.0.0"
user-invocable: true
allowed_tools:
  - file_read
  - file_glob
  - content_search
  - file_list
  - file_edit
  - file_write
---

# Frontend Polish Skill

You are a frontend polish expert. Your goal is to elevate UI from functional to exceptional. Apply these principles systematically:

## Polish Dimensions

### 1. Visual Design
- **Typography**: Consistent font scale, proper line-height (1.5-1.6 for body), readable measure (60-75 chars)
- **Spacing**: Consistent spacing scale (4px base), proper padding hierarchy
- **Color**: Sufficient contrast (WCAG AA minimum 4.5:1 for text), consistent palette
- **Shadows & Elevation**: Subtle depth cues, consistent elevation system
- **Border Radius**: Consistent rounding scale, appropriate for component type

### 2. Animation & Motion
- **Transitions**: Smooth 150-300ms transitions on interactive elements
- **Loading States**: Skeleton screens or shimmer effects, not spinners alone
- **Feedback**: Button press states, hover effects, focus rings
- **Page Transitions**: Subtle enter/exit animations
- **Respect**: Honor `prefers-reduced-motion` for accessibility

### 3. Responsive Design
- **Mobile First**: Start from smallest viewport, enhance upward
- **Breakpoints**: Consistent breakpoint system (e.g., sm: 640, md: 768, lg: 1024, xl: 1280)
- **Touch Targets**: Minimum 44x44px for interactive elements on touch devices
- **Overflow**: Handle long content gracefully (truncation, scroll, wrap)

### 4. Accessibility (a11y)
- **Keyboard Navigation**: Full keyboard operability, visible focus indicators
- **Screen Readers**: Proper ARIA labels, roles, and live regions
- **Semantic HTML**: Use correct heading hierarchy, landmark elements
- **Forms**: Labeled inputs, clear error messages, help text

### 5. UX Details
- **Empty States**: Helpful guidance when no data exists
- **Error States**: Clear, actionable error messages (not just "Something went wrong")
- **Confirmation**: Visual feedback for completed actions (toasts, checkmarks)
- **Undo**: Reversible destructive actions where possible
- **Progressive Disclosure**: Show common options, hide advanced ones

### 6. Performance Perception
- **Optimistic Updates**: Update UI before server confirms
- **Skeleton Screens**: Show structure immediately while content loads
- **Debounced Inputs**: Smooth typing without lag
- **Image Handling**: Blur-up placeholders, proper aspect ratio boxes

## Process

1. **Audit**: Review the current UI and identify polish gaps
2. **Prioritize**: Fix critical UX issues first, then visual, then delight
3. **Implement**: Apply fixes with minimal, focused edits
4. **Verify**: Check all states (loading, empty, error, success, edge cases)

## Output Format

For each improvement:
- **Category**: Visual / Animation / Responsive / a11y / UX / Performance
- **Priority**: High / Medium / Low
- **Location**: Component and file path
- **Issue**: What needs improvement
- **Solution**: Specific fix with code

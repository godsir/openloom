# Phase 4 CLI-First UX Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Transform openLoom TUI from basic chat to Claude Code / Codex CLI parity (streaming, state machine, keybinds, themes, overlays, diff viewer, bash mode, file picker, token dashboard)

**Architecture:** Incremental evolution of `chat_tui.rs` (889 lines) → `tui/` (8 modules). Replace blocking `handle_message()` with streaming `stream_complete()` via mpsc channel. Event-driven state machine via `EngineEvent` subscription. Static/dynamic render separation for performance.

**Tech Stack:** ratatui 0.29, crossterm 0.28, tui-textarea 0.7, tokio, syntect (diff highlighting), skim (fuzzy file search)

---

## File Structure

```
crates/cli/src/
├── main.rs              # mod tui instead of mod chat_tui
├── chat_tui.rs          # DELETE - replaced by tui/
├── download.rs          # unchanged
├── keymap.rs            # NEW - keybinding config + default bindings
├── tui/
│   ├── mod.rs           # module declarations + re-exports
│   ├── app.rs           # App struct + state machine + run loop
│   ├── render.rs        # draw functions (messages, welcome, scrollbar)
│   ├── input.rs         # input handling + keyboard dispatch
│   ├── commands.rs      # slash command definitions + handler
│   ├── status.rs        # enhanced status bar rendering
│   ├── theme.rs         # semantic color palette + presets
│   ├── overlay.rs       # approval overlay (Milestone C)
│   └── diff.rs          # code diff viewer (Milestone C)
```

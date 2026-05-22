# Mode System Design Spec

## Goal

Add a user-switchable Mode system that controls both agent behavior (tool permissions, agent loop) and persona context (system prompt). Four modes: Chat, Plan, Code, Assistant. New sessions default to Code mode.

## Modes

### Mode Enum

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Mode {
    Chat,
    Plan,
    #[default]
    Code,
    Assistant,
}
```

### ToolScope Enum

```rust
pub enum ToolScope {
    None,       // No tool calls
    ReadOnly,   // read_file, search, list_dir only
    Selective,  // ReadOnly + memory_write, note_create, skill_invoke
    Full,       // All tools
}
```

### ModeConfig

```rust
pub struct ModeConfig {
    pub agent_loop: bool,
    pub tool_scope: ToolScope,
    pub system_suffix: &'static str,
    pub status_label: &'static str,
}
```

### Behavior Matrix

| Mode | agent_loop | tool_scope | system_suffix | status_label |
|------|-----------|------------|---------------|-------------|
| Chat | false | None | "Respond concisely. Do not invoke tools or generate code unless explicitly asked." | `chat` |
| Plan | true | ReadOnly | "You are in Plan mode. Analyze code, explore architecture, propose solutions. Do NOT modify any files. Output plans, diagrams, and recommendations only." | `plan` |
| Code | true | Full | (current default SYSTEM_INSTRUCTION, no suffix) | `code` |
| Assistant | true | Selective | "You are a general-purpose assistant. You can search, read files, write notes and memories, and invoke skills. Do NOT modify code files or execute shell commands." | `asst` |

### Mode::config() method

Each Mode variant returns its ModeConfig via `Mode::config() -> ModeConfig`. This is the single source of truth for mode behavior.

## User Interaction

### Slash Command

`/mode` added to `SlashCommand` enum:

- `/mode` (no args) — show current mode + list all modes
- `/mode chat|plan|code|assistant` — switch mode

Unknown mode names return an error message.

### Command Palette

`/mode` appears in the slash command palette. After typing `/mode `, the palette shows the 4 modes as sub-options.

### Keyboard Shortcut

`Ctrl+M` — cycle to next mode (Chat → Plan → Code → Assistant → Chat). Show a status message on switch.

### Status Bar

Mode label shown in the status bar, after the model name:

```
 ● claude-sonnet [code] │ openloom │ 200k ctx
```

The mode label uses a distinct style (e.g., square brackets, accent color).

### Switch Feedback

When mode changes, push a message to scrollback:

```
  ✦ mode  Switched to Plan mode (read-only, no file modifications)
```

Uses role="mode" with collapsed=false, similar to the "skill" role.

## Engine Integration

### Mode passed per-call

`App.mode: Mode` field (session-level). Passed to engine on each call.

Signature change:
```rust
pub async fn handle_message_streaming(
    &self, msg: ChatMessage, session_id: &str,
    tx: mpsc::Sender<String>, mode: Mode,
) -> Result<()>
```

### Engine behavior changes

1. **agent_loop gate**: If `mode.config().agent_loop == false`, skip agent loop entirely. Use direct LLM call (simple path) regardless of router complexity score.

2. **Tool scope enforcement**: In `agent_loop::execute_tool()`, before executing any tool call, check `mode.config().tool_scope`:
   - `None` → reject all tool calls
   - `ReadOnly` → allow only: `read_file`, `search_code`, `list_directory`, `search_events`
   - `Selective` → ReadOnly + `memory_write`, `create_note`, `skill_invoke`
   - `Full` → allow all

   Rejected tool calls return a message: "Tool '{name}' is not available in {mode} mode."

3. **System prompt suffix**: Append `mode.config().system_suffix` to the system instruction in `Engine::system_instruction()`. The suffix is appended after the existing instruction + project context.

### Mode persistence

- Session-level only (not persisted to SQLite or config)
- `/clear` does NOT reset mode
- `/session new` resets to default (Code)
- New sessions start in Code mode

## Files Changed

### New
- (none — all changes are to existing files)

### Modified
- `crates/models/src/lib.rs` — `Mode`, `ToolScope`, `ModeConfig` types
- `crates/engine/src/lib.rs` — accept Mode in handle_message, system_instruction suffix
- `crates/engine/src/agent_loop.rs` — tool scope check in execute_tool
- `crates/cli/src/tui/app.rs` — `App.mode: Mode` field
- `crates/cli/src/tui/commands.rs` — `/mode` slash command handler
- `crates/cli/src/tui/render.rs` — status bar mode label, "mode" role style, `/mode` in palette
- `crates/cli/src/tui/input.rs` — Ctrl+M keybinding
- `crates/cli/src/tui/keymap.rs` — `Action::CycleMode`, keymap entry
- `crates/cli/src/tui/mod.rs` — pass app.mode to engine calls
- `crates/cli/src/tui/overlays/help.rs` — mode section in help overlay

## Testing

- Unit tests for `Mode::config()` behavior matrix
- Unit test for `/mode` slash command parsing
- Unit test for tool scope filtering logic
- Integration test: tool rejection in Plan mode

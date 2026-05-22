# Mode System Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a user-switchable Mode system (Chat/Plan/Code/Assistant) that controls agent behavior, tool permissions, and system prompt per session.

**Architecture:** `Mode` enum in models crate with a `ModeConfig` struct containing behavioral flags (agent_loop, tool_scope, system_suffix, status_label). The TUI stores `App.mode`, passes it to the engine on each call. The engine uses mode to gate agent loop entry and filter tool calls. Users switch via `/mode` command or `Ctrl+M` shortcut.

**Tech Stack:** Rust, existing engine/TUI/models crates, no new dependencies.

---

## File Structure

### Modified Files

| File | Changes |
|------|---------|
| `crates/models/src/lib.rs` | Add `Mode`, `ToolScope`, `ModeConfig` types |
| `crates/engine/src/lib.rs` | Accept `Mode` in `handle_message()`, append system suffix |
| `crates/engine/src/stream.rs` | Accept `Mode` in `handle_message_streaming()`, mode-based routing |
| `crates/engine/src/agent_loop.rs` | Accept `Mode`, tool scope check in `execute_tool()` |
| `crates/cli/src/tui/app.rs` | `App.mode: Mode` field |
| `crates/cli/src/tui/commands.rs` | `/mode` slash command handler + parsing |
| `crates/cli/src/tui/render.rs` | Status bar mode label, "mode" role style, `/mode` in palette |
| `crates/cli/src/tui/input.rs` | `Ctrl+M` → `CycleMode` action handler |
| `crates/cli/src/tui/keymap.rs` | `Action::CycleMode`, binding entry |
| `crates/cli/src/tui/mod.rs` | Pass `app.mode` to engine calls |
| `crates/cli/src/tui/overlays/help.rs` | Mode section in help overlay |

---

### Task 1: Mode and ToolScope types in models crate

**Files:**
- Modify: `crates/models/src/lib.rs`

- [ ] **Step 1: Write tests for Mode::config()**

Add at the end of `crates/models/src/lib.rs`, inside a `#[cfg(test)] mod tests` block (or add to existing):

```rust
#[cfg(test)]
mod mode_tests {
    use super::*;

    #[test]
    fn test_chat_mode_config() {
        let cfg = Mode::Chat.config();
        assert!(!cfg.agent_loop);
        assert_eq!(cfg.tool_scope, ToolScope::None);
        assert_eq!(cfg.status_label, "chat");
    }

    #[test]
    fn test_plan_mode_config() {
        let cfg = Mode::Plan.config();
        assert!(cfg.agent_loop);
        assert_eq!(cfg.tool_scope, ToolScope::ReadOnly);
        assert_eq!(cfg.status_label, "plan");
    }

    #[test]
    fn test_code_mode_config() {
        let cfg = Mode::Code.config();
        assert!(cfg.agent_loop);
        assert_eq!(cfg.tool_scope, ToolScope::Full);
        assert_eq!(cfg.status_label, "code");
    }

    #[test]
    fn test_assistant_mode_config() {
        let cfg = Mode::Assistant.config();
        assert!(cfg.agent_loop);
        assert_eq!(cfg.tool_scope, ToolScope::Selective);
        assert_eq!(cfg.status_label, "asst");
    }

    #[test]
    fn test_default_mode_is_code() {
        assert_eq!(Mode::default(), Mode::Code);
    }

    #[test]
    fn test_mode_from_str() {
        assert_eq!(Mode::from_str("chat"), Some(Mode::Chat));
        assert_eq!(Mode::from_str("plan"), Some(Mode::Plan));
        assert_eq!(Mode::from_str("code"), Some(Mode::Code));
        assert_eq!(Mode::from_str("assistant"), Some(Mode::Assistant));
        assert_eq!(Mode::from_str("asst"), Some(Mode::Assistant));
        assert_eq!(Mode::from_str("unknown"), None);
    }

    #[test]
    fn test_tool_scope_allows() {
        assert!(!ToolScope::None.allows("file_read"));
        assert!(ToolScope::ReadOnly.allows("file_read"));
        assert!(ToolScope::ReadOnly.allows("content_search"));
        assert!(!ToolScope::ReadOnly.allows("file_write"));
        assert!(!ToolScope::ReadOnly.allows("shell"));
        assert!(ToolScope::Selective.allows("file_read"));
        assert!(ToolScope::Selective.allows("schedule_reminder"));
        assert!(!ToolScope::Selective.allows("shell"));
        assert!(!ToolScope::Selective.allows("file_write"));
        assert!(ToolScope::Full.allows("shell"));
        assert!(ToolScope::Full.allows("file_write"));
    }

    #[test]
    fn test_mode_next() {
        assert_eq!(Mode::Chat.next(), Mode::Plan);
        assert_eq!(Mode::Plan.next(), Mode::Code);
        assert_eq!(Mode::Code.next(), Mode::Assistant);
        assert_eq!(Mode::Assistant.next(), Mode::Chat);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p openloom-models -- mode_tests`
Expected: FAIL — `Mode`, `ToolScope`, `ModeConfig` not defined

- [ ] **Step 3: Implement Mode, ToolScope, ModeConfig**

Add to `crates/models/src/lib.rs` after the `AgentState` enum (around line 232):

```rust
// === Mode system ===

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Chat,
    Plan,
    #[default]
    Code,
    Assistant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolScope {
    None,
    ReadOnly,
    Selective,
    Full,
}

pub struct ModeConfig {
    pub agent_loop: bool,
    pub tool_scope: ToolScope,
    pub system_suffix: &'static str,
    pub status_label: &'static str,
}

const READ_ONLY_TOOLS: &[&str] = &[
    "file_read", "file_search", "content_search", "web_browser",
];

const SELECTIVE_TOOLS: &[&str] = &[
    "file_read", "file_search", "content_search", "web_browser",
    "schedule_reminder",
];

impl ToolScope {
    pub fn allows(&self, tool_name: &str) -> bool {
        match self {
            ToolScope::None => false,
            ToolScope::ReadOnly => READ_ONLY_TOOLS.contains(&tool_name),
            ToolScope::Selective => {
                SELECTIVE_TOOLS.contains(&tool_name)
                    || tool_name.contains(':') // external skills always allowed
            }
            ToolScope::Full => true,
        }
    }
}

impl Mode {
    pub fn config(&self) -> ModeConfig {
        match self {
            Mode::Chat => ModeConfig {
                agent_loop: false,
                tool_scope: ToolScope::None,
                system_suffix: "Respond concisely. Do not invoke tools or generate code unless explicitly asked.",
                status_label: "chat",
            },
            Mode::Plan => ModeConfig {
                agent_loop: true,
                tool_scope: ToolScope::ReadOnly,
                system_suffix: "You are in Plan mode. Analyze code, explore architecture, propose solutions. Do NOT modify any files. Output plans, diagrams, and recommendations only.",
                status_label: "plan",
            },
            Mode::Code => ModeConfig {
                agent_loop: true,
                tool_scope: ToolScope::Full,
                system_suffix: "",
                status_label: "code",
            },
            Mode::Assistant => ModeConfig {
                agent_loop: true,
                tool_scope: ToolScope::Selective,
                system_suffix: "You are a general-purpose assistant. You can search, read files, write notes and memories, and invoke skills. Do NOT modify code files or execute shell commands.",
                status_label: "asst",
            },
        }
    }

    pub fn from_str(s: &str) -> Option<Mode> {
        match s.to_lowercase().as_str() {
            "chat" => Some(Mode::Chat),
            "plan" => Some(Mode::Plan),
            "code" => Some(Mode::Code),
            "assistant" | "asst" => Some(Mode::Assistant),
            _ => None,
        }
    }

    pub fn next(&self) -> Mode {
        match self {
            Mode::Chat => Mode::Plan,
            Mode::Plan => Mode::Code,
            Mode::Code => Mode::Assistant,
            Mode::Assistant => Mode::Chat,
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            Mode::Chat => "Pure conversation, no tools",
            Mode::Plan => "Read-only exploration, no file modifications",
            Mode::Code => "Full agent loop with tool calling",
            Mode::Assistant => "General assistant, read + memory + skills",
        }
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p openloom-models -- mode_tests`
Expected: 8 tests PASS

- [ ] **Step 5: Commit**

```
git add crates/models/src/lib.rs
git commit -m "feat: add Mode, ToolScope, ModeConfig types"
```

---

### Task 2: Engine accepts Mode and enforces tool scope

**Files:**
- Modify: `crates/engine/src/agent_loop.rs`
- Modify: `crates/engine/src/stream.rs`
- Modify: `crates/engine/src/lib.rs`

- [ ] **Step 1: Add Mode parameter to agent_loop methods**

In `crates/engine/src/agent_loop.rs`, change method signatures:

```rust
pub(crate) async fn agent_loop(
    &self,
    msg: &ChatMessage,
    session_id: &str,
    mode: openloom_models::Mode,
) -> Result<ChatResponse> {
    self.agent_loop_inner(msg, session_id, None, mode).await
}

pub(crate) async fn agent_loop_streaming(
    &self,
    msg: &ChatMessage,
    session_id: &str,
    tx: mpsc::Sender<String>,
    mode: openloom_models::Mode,
) -> Result<ChatResponse> {
    self.agent_loop_inner(msg, session_id, Some(tx), mode).await
}

async fn agent_loop_inner(
    &self,
    msg: &ChatMessage,
    session_id: &str,
    tx: Option<mpsc::Sender<String>>,
    mode: openloom_models::Mode,
) -> Result<ChatResponse> {
```

- [ ] **Step 2: Add system suffix and tool scope check in agent_loop_inner**

In `agent_loop_inner`, after building `system_with_tools` (line 63), append the mode suffix:

```rust
let system_with_tools = crate::system_instruction().replace("[tools]", &skill_list);
let mode_cfg = mode.config();
let system_with_mode = if mode_cfg.system_suffix.is_empty() {
    system_with_tools
} else {
    format!("{}\n\n{}", system_with_tools, mode_cfg.system_suffix)
};
```

Replace all uses of `system_with_tools` with `system_with_mode` in that function (there are two: line 64 and line 136).

In `execute_tool`, add the mode-based check. Change `execute_tool` to accept mode:

```rust
pub(crate) async fn execute_tool(
    &self,
    call: &ToolCall,
    mode: openloom_models::Mode,
) -> Result<String> {
    let mode_cfg = mode.config();
    if !mode_cfg.tool_scope.allows(&call.tool) {
        return Ok(format!(
            "Tool '{}' is not available in {} mode.",
            call.tool, mode_cfg.status_label
        ));
    }
    let risk = openloom_sandbox::classify_risk(&call.tool, &call.params);
    if openloom_sandbox::should_block(&risk, self.skip_permissions) {
        let msg = openloom_sandbox::risk_message(&call.tool, &call.params, &risk);
        return Ok(msg);
    }
    self.skills
        .invoke(&call.tool, call.params.clone())
        .await
        .map(|v| v.to_string())
}
```

Update the call site in `agent_loop_inner` (line 96):

```rust
let result = match self.execute_tool(&tool_call, mode).await {
```

- [ ] **Step 3: Add Mode parameter to handle_message**

In `crates/engine/src/lib.rs`, change `handle_message` signature:

```rust
pub async fn handle_message(
    &self, msg: ChatMessage, session_id: &str, mode: Mode,
) -> Result<ChatResponse> {
```

Inside, replace the agent loop decision (line 439):

```rust
let mode_cfg = mode.config();
if mode_cfg.agent_loop && (out.complexity >= 0.8 || out.skill_match.is_some()) {
    return self.agent_loop(&msg, session_id, mode).await;
}
```

For the simple path, append mode suffix to system instruction:

```rust
let system = system_instruction();
let system_with_mode = if mode_cfg.system_suffix.is_empty() {
    system
} else {
    format!("{}\n\n{}", system, mode_cfg.system_suffix)
};
```

Use `system_with_mode` in the `self.weaver.assemble()` call.

- [ ] **Step 4: Add Mode parameter to handle_message_streaming**

In `crates/engine/src/stream.rs`, change signature:

```rust
pub async fn handle_message_streaming(
    &self,
    msg: ChatMessage,
    session_id: &str,
    tx: tokio::sync::mpsc::Sender<String>,
    mode: openloom_models::Mode,
) -> anyhow::Result<()> {
```

Replace agent loop decision (line 57):

```rust
let mode_cfg = mode.config();
if mode_cfg.agent_loop && (out.complexity >= 0.5 || out.skill_match.is_some() || has_cloud) {
    let tx_clone = tx.clone();
    match self.agent_loop_streaming(&msg, session_id, tx_clone, mode).await {
```

For the simple path, append mode suffix:

```rust
let system = crate::system_instruction().replace("[tools]", "None");
let system_with_mode = if mode_cfg.system_suffix.is_empty() {
    system
} else {
    format!("{}\n\n{}", system, mode_cfg.system_suffix)
};
```

Use `system_with_mode` in the `self.weaver.assemble()` call.

- [ ] **Step 5: Fix callers of handle_message**

Search for all callers of `handle_message` and `handle_message_streaming` outside the TUI (server, tests). Add `Mode::Code` as the default mode argument:

In `crates/server/` — any WebSocket/HTTP handler calling `engine.handle_message(msg, sid)` becomes `engine.handle_message(msg, sid, Mode::Code)`.

In `crates/cli/src/tui/commands.rs` — the `/local test` handler calls `engine.handle_message(msg, &app.session_id)`. Change to `engine.handle_message(msg, &app.session_id, Mode::Code)`.

- [ ] **Step 6: Verify it compiles**

Run: `cargo check`
Expected: OK

- [ ] **Step 7: Commit**

```
git add crates/engine/ crates/server/ crates/cli/src/tui/commands.rs
git commit -m "feat: engine accepts Mode parameter, enforces tool scope"
```

---

### Task 3: TUI App.mode field + pass to engine

**Files:**
- Modify: `crates/cli/src/tui/app.rs`
- Modify: `crates/cli/src/tui/mod.rs`

- [ ] **Step 1: Add mode field to App**

In `crates/cli/src/tui/app.rs`, add to the `App` struct:

```rust
pub mode: openloom_models::Mode,
```

In `App::new()`, initialize:

```rust
mode: openloom_models::Mode::default(),
```

- [ ] **Step 2: Pass mode in start_streaming**

In `App::start_streaming()`, change the engine call:

```rust
let mode = self.mode;
let handle = tokio::spawn(async move {
    let _ = engine.handle_message_streaming(msg, &session_id, tx, mode).await;
});
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check -p openloom`
Expected: OK

- [ ] **Step 4: Commit**

```
git add crates/cli/src/tui/app.rs crates/cli/src/tui/mod.rs
git commit -m "feat: TUI stores App.mode, passes to engine"
```

---

### Task 4: /mode slash command

**Files:**
- Modify: `crates/cli/src/tui/commands.rs`
- Modify: `crates/cli/src/tui/render.rs`

- [ ] **Step 1: Write test for /mode parsing**

In `crates/cli/src/tui/commands.rs`, add to the existing `mod tests`:

```rust
#[test]
fn test_parse_mode() {
    match parse_slash_command("/mode") {
        Some(SlashCommand::Mode(a)) => assert!(a.is_empty()),
        other => panic!("expected Mode(\"\"), got {:?}", other),
    }
    match parse_slash_command("/mode plan") {
        Some(SlashCommand::Mode(a)) => assert_eq!(a, "plan"),
        other => panic!("expected Mode(\"plan\"), got {:?}", other),
    }
}
```

- [ ] **Step 2: Add Mode variant to SlashCommand enum**

```rust
pub enum SlashCommand {
    // ... existing variants ...
    Mode(String),
}
```

In `parse_slash_command`, add:

```rust
"mode" => Some(SlashCommand::Mode(args)),
```

- [ ] **Step 3: Implement /mode command handler**

In `execute_command`, add the `SlashCommand::Mode` arm:

```rust
SlashCommand::Mode(args) => {
    let sub = args.trim();
    if sub.is_empty() {
        let current = app.mode.config();
        let all_modes = [
            openloom_models::Mode::Chat,
            openloom_models::Mode::Plan,
            openloom_models::Mode::Code,
            openloom_models::Mode::Assistant,
        ];
        let mut lines = vec![format!(
            "Current mode: {} — {}",
            current.status_label,
            app.mode.description()
        )];
        lines.push(String::new());
        for m in &all_modes {
            let marker = if *m == app.mode { "▸ " } else { "  " };
            lines.push(format!(
                "{}{:12} {}",
                marker,
                m.config().status_label,
                m.description()
            ));
        }
        lines.push(String::new());
        lines.push("Usage: /mode <chat|plan|code|assistant>".into());
        lines.join("\n")
    } else if let Some(new_mode) = openloom_models::Mode::from_str(sub) {
        if new_mode == app.mode {
            format!("Already in {} mode.", app.mode.config().status_label)
        } else {
            let old_label = app.mode.config().status_label;
            app.mode = new_mode;
            let cfg = new_mode.config();
            app.messages.push(crate::tui::app::Message {
                role: "mode".into(),
                content: format!(
                    "Switched from {} to {} mode ({})",
                    old_label, cfg.status_label, new_mode.description()
                ),
                collapsed: false,
                elapsed_ms: None,
            });
            app.viewport.content_added();
            String::new()
        }
    } else {
        format!(
            "Unknown mode: '{}'. Available: chat, plan, code, assistant",
            sub
        )
    }
}
```

- [ ] **Step 4: Add /mode to SLASH_COMMANDS palette**

In `crates/cli/src/tui/render.rs`, add to `SLASH_COMMANDS`:

```rust
("/mode", "Show/switch mode"),
("/mode chat", "Chat mode — pure conversation"),
("/mode plan", "Plan mode — read-only exploration"),
("/mode code", "Code mode — full agent + tools"),
("/mode assistant", "Assistant mode — general helper"),
```

- [ ] **Step 5: Add "mode" role to role_style**

In `render.rs`, `role_style` function, add:

```rust
"mode" => ("\u{2726}", "mode", p.accent),
```

- [ ] **Step 6: Run tests**

Run: `cargo test -p openloom -- test_parse_mode`
Expected: PASS

- [ ] **Step 7: Commit**

```
git add crates/cli/src/tui/commands.rs crates/cli/src/tui/render.rs
git commit -m "feat: /mode slash command with palette entries"
```

---

### Task 5: Ctrl+M keybinding + status bar mode label

**Files:**
- Modify: `crates/cli/src/tui/keymap.rs`
- Modify: `crates/cli/src/tui/input.rs`
- Modify: `crates/cli/src/tui/render.rs`

- [ ] **Step 1: Write test for Ctrl+M binding**

In `crates/cli/src/tui/keymap.rs`, add to existing tests:

```rust
#[test]
fn test_ctrl_m_is_cycle_mode() {
    let km = ResolvedKeymap::default();
    let ev = key(KeyCode::Char('m'), KeyModifiers::CONTROL);
    assert_eq!(km.resolve(&ev, KeyContext::Input), Action::CycleMode);
}
```

- [ ] **Step 2: Add CycleMode action and binding**

In `keymap.rs`, add `CycleMode` to the `Action` enum:

```rust
pub enum Action {
    // ... existing ...
    CycleMode,
    Noop,
}
```

In `default_bindings()`, add:

```rust
KeyBinding {
    key: KeyCode::Char('m'),
    modifiers: KeyModifiers::CONTROL,
    action: Action::CycleMode,
    context: KeyContext::Input,
},
```

- [ ] **Step 3: Handle CycleMode in input.rs**

In `input.rs`, `handle_key` function, add the `Action::CycleMode` arm:

```rust
Action::CycleMode => {
    let old_label = app.mode.config().status_label;
    app.mode = app.mode.next();
    let cfg = app.mode.config();
    app.messages.push(crate::tui::app::Message {
        role: "mode".into(),
        content: format!(
            "Switched from {} to {} mode ({})",
            old_label, cfg.status_label, app.mode.description()
        ),
        collapsed: false,
        elapsed_ms: None,
    });
    app.viewport.content_added();
    false
}
```

- [ ] **Step 4: Add mode label to status bar**

In `crates/cli/src/tui/render.rs`, `draw_status_line` function, after building the model name in `left_parts` (line ~380), add mode label:

In the `_ => { left_parts.push(app.status.model.clone()); }` block, add immediately after:

```rust
left_parts.push(format!("[{}]", app.mode.config().status_label));
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p openloom -- test_ctrl_m`
Expected: PASS

- [ ] **Step 6: Verify full compilation**

Run: `cargo check`
Expected: OK

- [ ] **Step 7: Commit**

```
git add crates/cli/src/tui/keymap.rs crates/cli/src/tui/input.rs crates/cli/src/tui/render.rs
git commit -m "feat: Ctrl+M cycle mode + status bar mode label"
```

---

### Task 6: Help overlay + docs update

**Files:**
- Modify: `crates/cli/src/tui/overlays/help.rs`
- Modify: `docs/tui-usage.md`

- [ ] **Step 1: Add mode section to help overlay**

In `crates/cli/src/tui/overlays/help.rs`, in the `content()` method, add after the "Slash Commands" section (after the `/config` line):

```rust
cmd_line("/mode", "Show/switch agent mode"),
cmd_line("/mode chat|plan|code|asst", "Switch mode"),
Line::from(""),
Line::from(Span::styled(" Modes", Style::new().fg(GREEN).bold())),
Line::from(""),
key_line("Ctrl+M", "Cycle to next mode"),
Line::from(Span::styled(
    "  chat         Pure conversation, no tools",
    Style::new().fg(DIM),
)),
Line::from(Span::styled(
    "  plan         Read-only exploration",
    Style::new().fg(DIM),
)),
Line::from(Span::styled(
    "  code         Full agent loop + tools (default)",
    Style::new().fg(DIM),
)),
Line::from(Span::styled(
    "  assistant    General helper + memory + skills",
    Style::new().fg(DIM),
)),
```

- [ ] **Step 2: Update docs/tui-usage.md**

Add a "## 模式系统 (Modes)" section after the "快捷键" section:

```markdown
## 模式系统 (Modes)

openLoom 支持四种运行模式，控制 Agent 行为和工具权限：

| 模式 | 工具范围 | Agent Loop | 说明 |
|------|---------|-----------|------|
| `chat` | 无 | 否 | 纯对话，不触发工具调用 |
| `plan` | 只读 | 是 | 可读代码、探索架构，不修改文件 |
| `code` | 完整 | 是 | 完整 agent loop + 工具调用（默认） |
| `assistant` | 选择性 | 是 | 可读、搜索、写记忆/技能，不改代码 |

### 切换方式

- `/mode` — 查看当前模式
- `/mode plan` — 切换到 Plan 模式
- `Ctrl+M` — 循环切换到下一个模式

模式为会话级，`/session new` 重置为 Code 模式。状态栏显示当前模式标签（如 `[code]`）。
```

- [ ] **Step 3: Update help_text() in commands.rs**

Add `/mode` entry to the `help_text()` function:

```rust
  /mode           Show/switch agent mode
```

And update the test assertion:

```rust
assert!(text.contains("/mode"));
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check && cargo test -p openloom -- test_help_text`
Expected: OK + PASS

- [ ] **Step 5: Commit**

```
git add crates/cli/src/tui/overlays/help.rs docs/tui-usage.md crates/cli/src/tui/commands.rs
git commit -m "docs: add mode system to help overlay and tui-usage"
```

---

## Summary

| Task | Deliverable | Tests |
|------|------------|-------|
| 1 | `Mode`, `ToolScope`, `ModeConfig` types | 8 unit tests |
| 2 | Engine mode-gated routing + tool scope | compile check |
| 3 | `App.mode` field + pass to engine | compile check |
| 4 | `/mode` slash command + palette | 1 parse test |
| 5 | `Ctrl+M` keybinding + status bar label | 1 keymap test |
| 6 | Help overlay + docs update | help_text test |

Total new tests: 10

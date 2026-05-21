# Phase 4 Milestone A — 基础聊天 REPL 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 构建一个能用的终端聊天 REPL：启动 → 输入消息 → 调用 Engine → 显示回复 → 循环。非流式、无覆盖层、无斜杠命令。纯地基。

**Architecture:** ratatui immediate-mode 渲染，App struct 持有全部可变状态，纯 render 函数生成 frame，crossterm 处理终端 I/O。Milestone A 直接调用 `engine.handle_message()`（阻塞式），Milestone B 再换成流式。

**Tech Stack:** ratatui 0.29, crossterm 0.28, tui-textarea 0.7, tokio, anyhow

> **tui-textarea 0.7 API 注意事项：** `TextArea::input()` 接受 `crossterm::event::Event`（非 `KeyEvent`），需要 `Event::Key(key)` 包裹。`set_placeholder_text()` 和 `insert_str()` 的 API 取决于 tui-textarea 0.7 的精确版本，如编译报错请查阅 [tui-textarea docs](https://docs.rs/tui-textarea/0.7/)。

> **顶层 keymap.rs：** 方案中 `crates/cli/src/keymap.rs`（KeyBinding 类型 + 默认绑定 + 合并逻辑）推迟到 Milestone B/D 实现，Milestone A 只需要 tui 目录内的文件。

> **Token 统计去重：** `engine.handle_message()` 同时通过 broadcast 和返回值发送 `TokenUsage`。Milestone A 的 `poll_engine_events()` 只追踪 `AgentStateChanged`，Token 统计完全走 `ChatResponse.token_usage`，避免重复计数。

---

### Task 1: 恢复 Cargo.toml 依赖

**Files:**
- Modify: `crates/cli/Cargo.toml`

- [ ] **Step 1: 添加 TUI 依赖**

在 `[dependencies]` 末尾追加三个依赖：

```toml
ratatui = "0.29"
crossterm = "0.28"
tui-textarea = "0.7"
```

完整的 Cargo.toml 效果：

```toml
[package]
name = "openloom"
version.workspace = true
edition.workspace = true

[[bin]]
name = "openloom"
path = "src/main.rs"

[dependencies]
openloom-memory = { path = "../memory" }
openloom-models = { path = "../models" }
openloom-engine = { path = "../engine" }
openloom-server = { path = "../server" }
openloom-inference = { path = "../inference" }
clap = { version = "4", features = ["derive"] }
anyhow = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
tokio = { version = "1", features = ["rt-multi-thread", "macros", "signal"] }
serde_json = "1"
chrono = "0.4"
dirs = "5"
toml = "0.8"
serde = { version = "1", features = ["derive"] }
ratatui = "0.29"
crossterm = "0.28"
tui-textarea = "0.7"
reqwest = { version = "0.12", default-features = false, features = ["stream", "rustls-tls"] }
futures = "0.3"
```

- [ ] **Step 2: 验证依赖拉取成功**

```powershell
cargo check --package openloom 2>&1
```

Expected: 编译通过（TUI 代码还没写，但依赖已就绪，main.rs 的 `mod tui;` 还没加所以不会报错）。

- [ ] **Step 3: Commit**

```bash
git add crates/cli/Cargo.toml
git commit -m "chore: add ratatui, crossterm, tui-textarea dependencies for Phase 4 TUI"
```

---

### Task 2: 创建 tui/ 目录结构 + mod.rs 骨架

**Files:**
- Create: `crates/cli/src/tui/mod.rs`
- Create: 空占位文件（后续任务填充）

- [ ] **Step 1: 创建目录**

```powershell
New-Item -ItemType Directory -Force "F:\openloom\crates\cli\src\tui\overlays"
```

- [ ] **Step 2: 创建 mod.rs 模块声明**

创建 `crates/cli/src/tui/mod.rs`：

```rust
pub mod app;
pub mod commands;
pub mod input;
pub mod keymap;
pub mod render;
pub mod status;
pub mod streaming;
pub mod theme;

mod overlays;

use std::sync::Arc;

use openloom_engine::Engine;

pub async fn run(engine: Arc<Engine>) -> anyhow::Result<()> {
    // Milestone A: 先放一个占位实现，后续任务填充
    let _ = engine;
    Ok(())
}
```

- [ ] **Step 3: 创建 overlays/mod.rs 占位**

创建 `crates/cli/src/tui/overlays/mod.rs`：

```rust
// overlays will be wired in Milestone C
#[allow(dead_code)]
pub trait Overlay {}
```

- [ ] **Step 4: 创建各模块占位文件**

为后续任务创建空壳文件，编译能通过 `mod` 声明：

创建 `crates/cli/src/tui/theme.rs`：
```rust
// Milestone A: basic theme
```

创建 `crates/cli/src/tui/app.rs`：
```rust
// Milestone A: App state
```

创建 `crates/cli/src/tui/render.rs`：
```rust
// Milestone A: draw functions
```

创建 `crates/cli/src/tui/input.rs`：
```rust
// Milestone A: input handling
```

创建 `crates/cli/src/tui/commands.rs`：
```rust
// Milestone B: slash commands
#[allow(dead_code)]
pub struct Commands;
```

创建 `crates/cli/src/tui/status.rs`：
```rust
// Milestone A: status bar (replaced in Task 4)
```

创建 `crates/cli/src/tui/keymap.rs`：
```rust
// Milestone B: keybinding system
#[allow(dead_code)]
pub struct KeymapConfig;
```

创建 `crates/cli/src/tui/streaming.rs`：
```rust
// Milestone B: streaming state
#[allow(dead_code)]
pub struct StreamState;
```

- [ ] **Step 5: 在 main.rs 中添加 mod tui 声明**

修改 `crates/cli/src/main.rs`，在 `mod download;` 下一行添加：

```rust
mod tui;
```

- [ ] **Step 6: 验证编译**

```powershell
cargo check --package openloom 2>&1
```

Expected: 编译通过，全是空模块。

- [ ] **Step 7: Commit**

```bash
git add crates/cli/src/tui/ crates/cli/src/main.rs
git commit -m "feat: scaffold tui module structure for Phase 4"
```

---

### Task 3: 实现 theme.rs

**Files:**
- Modify: `crates/cli/src/tui/theme.rs`

- [ ] **Step 1: 写完整实现**

用以下内容覆盖 `crates/cli/src/tui/theme.rs`：

```rust
use ratatui::style::Color;

#[derive(Debug, Clone)]
pub struct Palette {
    pub bg: Color,
    pub surface: Color,
    pub text: Color,
    pub text_dim: Color,
    pub accent: Color,
    pub user_bubble: Color,
    pub assistant_bubble: Color,
    pub success: Color,
    pub warning: Color,
    pub error: Color,
    pub code_bg: Color,
}

#[derive(Debug, Clone)]
pub struct Theme {
    pub palette: Palette,
    pub name: String,
}

impl Theme {
    pub fn dark() -> Self {
        Self {
            name: "dark".into(),
            palette: Palette {
                bg: Color::Rgb(18, 18, 24),
                surface: Color::Rgb(28, 28, 36),
                text: Color::Rgb(220, 220, 230),
                text_dim: Color::Rgb(120, 120, 140),
                accent: Color::Rgb(99, 150, 240),
                user_bubble: Color::Rgb(60, 100, 200),
                assistant_bubble: Color::Rgb(40, 44, 52),
                success: Color::Rgb(80, 200, 120),
                warning: Color::Rgb(220, 180, 60),
                error: Color::Rgb(220, 80, 80),
                code_bg: Color::Rgb(22, 22, 30),
            },
        }
    }

    pub fn light() -> Self {
        Self {
            name: "light".into(),
            palette: Palette {
                bg: Color::Rgb(248, 248, 252),
                surface: Color::Rgb(236, 236, 244),
                text: Color::Rgb(24, 24, 32),
                text_dim: Color::Rgb(140, 140, 155),
                accent: Color::Rgb(40, 80, 200),
                user_bubble: Color::Rgb(60, 110, 220),
                assistant_bubble: Color::Rgb(228, 232, 240),
                success: Color::Rgb(30, 160, 80),
                warning: Color::Rgb(180, 140, 30),
                error: Color::Rgb(200, 50, 50),
                code_bg: Color::Rgb(236, 236, 244),
            },
        }
    }
}
```

- [ ] **Step 2: 验证编译**

```powershell
cargo check --package openloom 2>&1
```

Expected: 编译通过，theme 模块无外部依赖。

- [ ] **Step 3: Commit**

```bash
git add crates/cli/src/tui/theme.rs
git commit -m "feat: add theme system with dark/light palettes"
```

---

### Task 4: 实现 status.rs

**Files:**
- Modify: `crates/cli/src/tui/status.rs`

- [ ] **Step 1: 写完整实现**

用以下内容覆盖 `crates/cli/src/tui/status.rs`：

```rust
use openloom_models::AgentState;

#[derive(Debug, Clone)]
pub struct StatusLine {
    pub model: String,
    pub agent_state: AgentState,
    pub context_pct: f64,
    pub turn_tokens: usize,
    pub git_branch: String,
    pub cwd: String,
    pub context_max: usize,
}

impl StatusLine {
    pub fn state_icon(&self) -> &str {
        match self.agent_state {
            AgentState::Idle => "○",
            AgentState::Thinking => "●",
            AgentState::Acting => "◆",
        }
    }
}
```

- [ ] **Step 2: 验证编译**

```powershell
cargo check --package openloom 2>&1
```

Expected: 编译通过。StatusLine 无外部依赖，独立编译。

- [ ] **Step 3: Commit**

```bash
git add crates/cli/src/tui/status.rs
git commit -m "feat: add StatusLine model for TUI status bar"
```

---

### Task 5: 实现 app.rs（App 状态机）

**依赖：** Task 4（status.rs 必须先完成，app.rs 引用了 `StatusLine`）

**Files:**
- Modify: `crates/cli/src/tui/app.rs`

- [ ] **Step 1: 写完整实现**

用以下内容覆盖 `crates/cli/src/tui/app.rs`：

```rust
use std::sync::Arc;

use openloom_engine::Engine;
use openloom_models::{AgentState, ChatMessage, EngineEvent};
use tokio::sync::broadcast;
use tui_textarea::TextArea;

use crate::tui::status::StatusLine;
use crate::tui::theme::Theme;

#[derive(Debug, Clone, PartialEq)]
pub enum AppState {
    Idle,
    Waiting,
    Streaming,
    Overlay,
}

#[derive(Debug, Clone)]
pub struct Message {
    pub role: String,
    pub content: String,
}

impl Message {
    pub fn user(content: String) -> Self {
        Self { role: "user".into(), content }
    }

    pub fn assistant(content: String) -> Self {
        Self { role: "assistant".into(), content }
    }
}

pub struct App {
    pub engine: Arc<Engine>,
    pub session_id: String,
    pub messages: Vec<Message>,
    pub input: TextArea<'static>,
    pub history: Vec<String>,
    pub history_idx: Option<usize>,
    pub state: AppState,
    pub scroll: u16,
    pub auto_scroll: bool,
    pub status: StatusLine,
    pub theme: Theme,
    pub event_rx: broadcast::Receiver<EngineEvent>,
    pub should_exit: bool,
    pub total_prompt_tokens: usize,
    pub total_completion_tokens: usize,
    pub total_cost: f64,
    pub frame_count: u64,
}

pub fn build_textarea() -> TextArea<'static> {
    let mut ta = TextArea::default();
    ta.set_cursor_line_style(ratatui::style::Style::default());
    ta.set_placeholder_text("Type a message... (Enter to send, Ctrl+C to quit)");
    ta
}

impl App {
    pub fn new(
        engine: Arc<Engine>,
        session_id: String,
        cwd: String,
        model_name: String,
        git_branch: String,
        context_max: usize,
    ) -> Self {
        let event_rx = engine.subscribe();
        Self {
            engine,
            session_id,
            messages: Vec::new(),
            input: build_textarea(),
            history: Vec::new(),
            history_idx: None,
            state: AppState::Idle,
            scroll: 0,
            auto_scroll: true,
            status: StatusLine {
                model: model_name,
                agent_state: AgentState::Idle,
                context_pct: 0.0,
                turn_tokens: 0,
                git_branch,
                cwd,
                context_max,
            },
            theme: Theme::dark(),
            event_rx,
            should_exit: false,
            total_prompt_tokens: 0,
            total_completion_tokens: 0,
            total_cost: 0.0,
            frame_count: 0,
        }
    }

    pub fn current_line(&self) -> String {
        self.input.lines().join("\n")
    }

    pub fn add_user_message(&mut self, content: String) -> ChatMessage {
        self.messages.push(Message::user(content.clone()));
        ChatMessage {
            role: "user".into(),
            content,
            timestamp: chrono::Utc::now(),
        }
    }

    pub fn add_assistant_message(&mut self, content: String) {
        self.messages.push(Message::assistant(content));
    }

    pub fn poll_engine_events(&mut self) {
        // Milestone A: only track agent state from broadcast events.
        // Token stats come from handle_message() return value to avoid double-counting
        // (handle_message broadcasts TokenUsage AND returns it in ChatResponse).
        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                EngineEvent::AgentStateChanged { new_state, .. } => {
                    self.status.agent_state = new_state;
                }
                _ => {}
            }
        }
    }
}
```

- [ ] **Step 2: 验证编译**

```powershell
cargo check --package openloom 2>&1
```

Expected: 编译通过。依赖了 `status::StatusLine`，所以 Task 5 需要在 Task 4 之后做（但 Task 5 的实现可以并行写）。

- [ ] **Step 3: Commit**

```bash
git add crates/cli/src/tui/app.rs
git commit -m "feat: add App state machine for TUI chat REPL"
```

### Task 6: 实现 render.rs（布局 + 绘制）

**Files:**
- Modify: `crates/cli/src/tui/render.rs`

- [ ] **Step 1: 写完整实现**

用以下内容覆盖 `crates/cli/src/tui/render.rs`：

```rust
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Style, Stylize},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

use crate::tui::app::{App, AppState, Message};

pub fn draw(f: &mut Frame, app: &App) {
    let palette = &app.theme.palette;

    let [main_area, status_area, input_area] = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(input_height(app)),
        ])
        .areas(f.area());

    draw_messages(f, main_area, app, palette);
    draw_status(f, status_area, app, palette);
    draw_input(f, input_area, app, palette);
}

fn draw_messages(f: &mut Frame, area: Rect, app: &App, p: &crate::tui::theme::Palette) {
    let visible_messages = visible_range(app.messages.len(), app.scroll as usize, area.height as usize);

    let mut lines: Vec<Line> = Vec::new();

    if app.messages.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("      ", Style::new().fg(p.text_dim)),
        ]));
        lines.push(Line::from(vec![
            Span::styled("  Welcome to openLoom.", Style::new().fg(p.accent).add_modifier(ratatui::style::Modifier::BOLD)),
        ]));
        lines.push(Line::from(vec![
            Span::styled("  Type a message and press Enter to start.", Style::new().fg(p.text_dim)),
        ]));
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("  Ctrl+C ", Style::new().fg(p.warning)),
            Span::styled("quit  ", Style::new().fg(p.text_dim)),
            Span::styled("Enter ", Style::new().fg(p.accent)),
            Span::styled("send  ", Style::new().fg(p.text_dim)),
            Span::styled("PageUp/Down ", Style::new().fg(p.accent)),
            Span::styled("scroll", Style::new().fg(p.text_dim)),
        ]));
    }

    for i in visible_messages {
        if let Some(msg) = app.messages.get(i) {
            match msg.role.as_str() {
                "user" => {
                    lines.push(Line::from(vec![
                        Span::styled("  You ", Style::new().fg(p.user_bubble).add_modifier(ratatui::style::Modifier::BOLD)),
                        Span::styled("", Style::new().fg(p.text_dim)),
                    ]));
                    for line in msg.content.lines() {
                        lines.push(Line::from(vec![
                            Span::styled(format!("  {}", line), Style::new().fg(p.text)),
                        ]));
                    }
                    lines.push(Line::from(""));
                }
                "assistant" => {
                    lines.push(Line::from(vec![
                        Span::styled("  openLoom ", Style::new().fg(p.accent).add_modifier(ratatui::style::Modifier::BOLD)),
                    ]));
                    for line in msg.content.lines() {
                        lines.push(Line::from(vec![
                            Span::styled(format!("  {}", line), Style::new().fg(p.text)),
                        ]));
                    }
                    lines.push(Line::from(""));
                }
                _ => {}
            }
        }
    }

    // Scroll indicator
    if app.scroll > 0 {
        let total = app.messages.len();
        let visible = visible_messages.len();
        lines.push(Line::from(vec![
            Span::styled(
                format!("  ↑ scroll {} / {} total messages", app.scroll, total),
                Style::new().fg(p.text_dim).italic(),
            ),
        ]));
    }

    let paragraph = Paragraph::new(Text::from(lines))
        .block(Block::default().borders(Borders::NONE))
        .wrap(Wrap { trim: false });

    // Scroll to bottom if auto_scroll
    let scroll_offset = if app.auto_scroll {
        paragraph.line_count(area.width).saturating_sub(area.height as usize) as u16
    } else {
        app.scroll
    };

    let paragraph = paragraph.scroll((scroll_offset, 0));
    f.render_widget(paragraph, area);
}

fn draw_status(f: &mut Frame, area: Rect, app: &App, p: &crate::tui::theme::Palette) {
    let left = format!(
        " {} {} | {} | {} ",
        app.status.state_icon(),
        app.status.model,
        app.status.cwd,
        format_tokens(app.status.turn_tokens),
    );

    let right = if app.status.git_branch.is_empty() {
        String::new()
    } else {
        format!(" {} ", app.status.git_branch)
    };

    let bar = ratatui::widgets::Gauge::default()
        .gauge_style(Style::new().fg(p.surface).bg(p.surface))
        .ratio(1.0)
        .label(Span::styled(
            format!("{}{}", left, right),
            Style::new().fg(p.text_dim),
        ));

    f.render_widget(bar, area);
}

fn draw_input(f: &mut Frame, area: Rect, app: &App, p: &crate::tui::theme::Palette) {
    let mut input_widget = app.input.widget();

    let block = match app.state {
        AppState::Waiting => Block::default()
            .borders(Borders::TOP)
            .border_style(Style::new().fg(p.warning))
            .title_top(Span::styled(" Thinking... ", Style::new().fg(p.warning))),
        AppState::Streaming => Block::default()
            .borders(Borders::TOP)
            .border_style(Style::new().fg(p.accent)),
        _ => Block::default()
            .borders(Borders::TOP)
            .border_style(Style::new().fg(p.surface)),
    };

    input_widget = input_widget.block(block);
    f.render_widget(input_widget, area);
}

fn visible_range(total: usize, scroll: usize, height: usize) -> std::ops::Range<usize> {
    let h = height.max(4) - 1;
    let start = scroll.min(total.saturating_sub(1));
    let end = (start + h).min(total);
    start..end
}

fn format_tokens(n: usize) -> String {
    if n >= 1000 {
        format!("{:.1}kt", n as f64 / 1000.0)
    } else if n > 0 {
        format!("{}t", n)
    } else {
        "-".into()
    }
}

fn input_height(app: &App) -> u16 {
    let lines = app.input.lines().len().max(1).min(8) as u16;
    lines + 2 // +2 for borders
}
```

- [ ] **Step 2: 验证编译**

```powershell
cargo check --package openloom 2>&1
```

Expected: 编译通过。如果 `app.input.widget()` 返回类型不匹配，检查 tui-textarea 0.7 的 API —— `TextArea::widget()` 返回 `TextAreaWidget`，直接 `f.render_widget(input_widget, area)` 即可。

- [ ] **Step 3: Commit**

```bash
git add crates/cli/src/tui/render.rs
git commit -m "feat: add TUI render functions for messages, status bar, and input"
```

---

### Task 7: 实现 input.rs（输入处理）

**Files:**
- Modify: `crates/cli/src/tui/input.rs`

- [ ] **Step 1: 写完整实现**

`TextArea::input()` 接受 `crossterm::event::Event`，需要把 `KeyEvent` 包裹为 `Event::Key(key)`。

用以下内容覆盖 `crates/cli/src/tui/input.rs`：

```rust
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

use crate::tui::app::{App, AppState};

pub fn handle_key(app: &mut App, key: KeyEvent) -> bool {
    // Ctrl+C: quit
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        app.should_exit = true;
        return true;
    }

    // Ctrl+L: terminal clear (redraw)
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('l') {
        return false;
    }

    match key.code {
        KeyCode::Enter => {
            let text = app.current_line().trim().to_string();
            if text.is_empty() {
                return false;
            }
            if app.history.last() != Some(&text) {
                app.history.push(text.clone());
            }
            app.history_idx = None;
            app.input = crate::tui::app::build_textarea();
            app.messages.push(crate::tui::app::Message::user(text));
            app.state = AppState::Waiting;
            app.auto_scroll = true;
            false
        }
        KeyCode::Up => {
            navigate_history(app, Direction::Prev);
            false
        }
        KeyCode::Down => {
            navigate_history(app, Direction::Next);
            false
        }
        KeyCode::PageUp => {
            app.auto_scroll = false;
            app.scroll = app.scroll.saturating_sub(10);
            false
        }
        KeyCode::PageDown => {
            app.scroll = app.scroll.saturating_add(10);
            false
        }
        _ => {
            // Delegate remaining keys to tui-textarea
            app.input.input(Event::Key(key));
            false
        }
    }
}

enum Direction {
    Prev,
    Next,
}

fn navigate_history(app: &mut App, dir: Direction) {
    if app.history.is_empty() {
        return;
    }

    let idx = match dir {
        Direction::Prev => match app.history_idx {
            None => Some(app.history.len().saturating_sub(1)),
            Some(0) => Some(0),
            Some(i) => Some(i.saturating_sub(1)),
        },
        Direction::Next => match app.history_idx {
            None => None,
            Some(i) if i + 1 >= app.history.len() => None,
            Some(i) => Some(i + 1),
        },
    };

    app.history_idx = idx;
    let text = match idx {
        Some(i) => app.history.get(i).cloned().unwrap_or_default(),
        None => String::new(),
    };
    app.input = crate::tui::app::build_textarea();
    app.input.insert_str(&text);
}
```

- [ ] **Step 2: 验证编译**

```powershell
cargo check --package openloom 2>&1
```

Expected: 编译通过。`app.input.input(Event::Key(key))` 需要 `crossterm::event::Event` 在作用域内。

- [ ] **Step 3: Commit**

```bash
git add crates/cli/src/tui/input.rs
git commit -m "feat: add TUI input handling with history navigation"
```

---

### Task 8: 实现 mod.rs（事件循环 + Chat 集成）

**Files:**
- Modify: `crates/cli/src/tui/mod.rs`
- Modify: `crates/cli/src/main.rs:245-247`（Chat handler）

- [ ] **Step 1: 写完整实现**

关键：在 `.await` 之前从 `&mut app` 中提取出 owned 数据（`session_id`, `engine.clone()`, `content`），避免跨 await 持有 borrow。

用以下内容覆盖 `crates/cli/src/tui/mod.rs`：

用以下内容覆盖 `crates/cli/src/tui/mod.rs`：

```rust
pub mod app;
pub mod commands;
pub mod input;
pub mod keymap;
pub mod render;
pub mod status;
pub mod streaming;
pub mod theme;

mod overlays;

use std::sync::Arc;
use std::time::Duration;

use crossterm::event;
use openloom_engine::Engine;
use openloom_models::ChatMessage;

use crate::tui::app::{App, AppState};

pub async fn run(engine: Arc<Engine>) -> anyhow::Result<()> {
    let mut terminal = ratatui::init();
    crossterm::execute!(std::io::stdout(), crossterm::event::EnableMouseCapture)?;

    let session_id = engine.create_session().await?.id;
    let cwd = std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "?".into());
    let model_name = engine.model_display_name();
    let git_branch = detect_git_branch();
    let context_size = engine.model_context_size().await;

    let mut app = App::new(engine, session_id, cwd, model_name, git_branch, context_size);

    let res = app_run(&mut terminal, &mut app).await;

    let _ = app.engine.shutdown().await;
    crossterm::execute!(std::io::stdout(), crossterm::event::DisableMouseCapture)?;
    ratatui::restore();
    res
}

async fn app_run(
    terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    app: &mut App,
) -> anyhow::Result<()> {
    loop {
        if app.should_exit {
            break;
        }

        app.poll_engine_events();
        app.frame_count = app.frame_count.wrapping_add(1);

        // Process pending user message (Waiting state means Enter was pressed)
        if app.state == AppState::Waiting {
            let pending_content = app
                .messages
                .last()
                .filter(|m| m.role == "user")
                .map(|m| m.content.clone());

            if let Some(content) = pending_content {
                let sid = app.session_id.clone();
                let engine = app.engine.clone();
                app.state = AppState::Waiting; // stays Waiting until response

                let msg = ChatMessage {
                    role: "user".into(),
                    content,
                    timestamp: chrono::Utc::now(),
                };
                match engine.handle_message(msg, &sid).await {
                    Ok(resp) => {
                        app.add_assistant_message(resp.response);
                        app.total_prompt_tokens += resp.token_usage.prompt_tokens;
                        app.total_completion_tokens += resp.token_usage.completion_tokens;
                        app.status.turn_tokens = resp.token_usage.completion_tokens;
                    }
                    Err(e) => {
                        app.add_assistant_message(format!("Error: {}", e));
                    }
                }
                app.state = AppState::Idle;
            } else {
                app.state = AppState::Idle;
            }
        }

        terminal.draw(|f| render::draw(f, app))?;

        let poll_timeout = match app.state {
            AppState::Waiting | AppState::Streaming => Duration::from_millis(50),
            _ => Duration::from_millis(200),
        };

        if !event::poll(poll_timeout)? {
            continue;
        }

        match event::read()? {
            event::Event::Key(key) => {
                if key.kind == event::KeyEventKind::Release {
                    continue;
                }
                input::handle_key(app, key);
            }
            event::Event::Mouse(mouse) => match mouse.kind {
                event::MouseEventKind::ScrollUp => {
                    app.auto_scroll = false;
                    app.scroll = app.scroll.saturating_sub(3);
                }
                event::MouseEventKind::ScrollDown => {
                    app.scroll = app.scroll.saturating_add(3);
                }
                _ => {}
            },
            event::Event::Resize(_, _) => {}
            _ => {}
        }
    }

    Ok(())
}

fn detect_git_branch() -> String {
    std::process::Command::new("git")
        .args(["branch", "--show-current"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
            } else {
                None
            }
        })
        .unwrap_or_default()
}
```

- [ ] **Step 2: 更新 main.rs Chat handler**

修改 `crates/cli/src/main.rs` 的 Chat handler：

```rust
// 在文件顶部添加 use std::sync::Arc;
use std::sync::Arc;
```

然后在 Chat handler 处（大约 line 245-247），用以下内容替换：

```rust
        Commands::Chat { config } => {
            let engine = Arc::new(build_engine(config.as_deref(), 100, None)?);
            tui::run(engine).await?;
        }
```

- [ ] **Step 3: 验证编译**

```powershell
cargo check --package openloom 2>&1
```

Expected: 编译通过。

如果报 "unused import" 警告，可能需要给占位模块加 `#[allow(dead_code)]` 或 `#![allow(unused)]`。Milestone A 的 commands.rs、keymap.rs、streaming.rs 是空壳，可能触发 dead_code 警告。

解决方案：在 crate 级别处理（main.rs 加 `#![allow(dead_code)]`）或在各空壳文件加 `#![allow(dead_code)]`。

- [ ] **Step 4: Commit**

```bash
git add crates/cli/src/tui/mod.rs crates/cli/src/main.rs
git commit -m "feat: wire up TUI event loop and Chat handler"
```

---

### Task 9: 编译验证 + 修正

- [ ] **Step 1: 完整 check**

```powershell
cargo check --package openloom 2>&1
```

Expected: 0 errors, 0 warnings（允许 dead_code 警告，给空壳模块加 allow）。

- [ ] **Step 2: 如果 Clippy 有警告，修正**

```powershell
cargo clippy --package openloom -- -D warnings 2>&1
```

Expected: 0 warnings。

- [ ] **Step 3: cargo fmt**

```powershell
cargo fmt --package openloom
```

- [ ] **Step 4: 基本编译验证（release check）**

```powershell
cargo check --package openloom --release 2>&1
```

Expected: 0 errors。

- [ ] **Step 5: Commit（如有修正）**

```bash
git add -A
git commit -m "chore: fix clippy warnings and fmt for TUI milestone A"
```

---

### Task 10: 端到端手动测试

- [ ] **Step 1: 构建 debug 版本**

```powershell
cargo build --package openloom 2>&1
```

Expected: 编译成功，生成 `target/debug/openloom.exe`。

- [ ] **Step 2: 启动 TUI（需要有效的 config.toml + 模型或云 API key）**

```powershell
cargo run -- chat
```

Expected: 
- 终端切换到 raw mode，显示 TUI 界面
- 看到 "Welcome to openLoom." 欢迎信息
- 底部有输入区，placeholder 显示 "Type a message..."
- 状态栏显示模型名和 CWD

- [ ] **Step 3: 发送一条消息**

输入 "Hello"，按 Enter。

Expected:
- 消息区显示 "You: Hello"
- 状态栏显示 "Thinking..." 
- 等待响应后显示 "openLoom: ..."
- 状态栏恢复 Idle

- [ ] **Step 4: 测试滚动**

发送足够多的消息填满屏幕后：
- PageUp/PageDown 上下滚动
- 鼠标滚轮也能滚动
- 新消息到达时自动滚到底部

- [ ] **Step 5: 测试历史导航**

在输入区按 Up/Down：
- Up 回显上一条消息
- Down 清除回显
- 可编辑历史消息后发送

- [ ] **Step 6: 测试退出**

按 Ctrl+C：
- TUI 退出，恢复正常终端
- 无 panic 消息

- [ ] **Step 7: Commit 测试记录（如有修改）**

```bash
git add -A
git commit -m "fix: TUI milestone A smoke test fixes"
```

---

## Before Presenting Deliverable — Self-Review

**1. Spec coverage check:**
- ✅ 恢复 Cargo.toml 依赖 → Task 1
- ✅ theme.rs → Task 3
- ✅ status.rs → Task 4
- ✅ app.rs → Task 5
- ✅ render.rs → Task 6
- ✅ input.rs → Task 7
- ✅ mod.rs → Task 8
- ✅ main.rs Chat handler 集成 → Task 8 Step 2
- ✅ 编译验证 → Task 9

**2. Placeholder scan:**
- No TBD, TODO, "implement later"
- No "add appropriate error handling" vagueness
- All code blocks are concrete and complete
- Multi-version editorial content removed from Tasks 5/7/8

**3. Type consistency:**
- `App::new()` takes `(Arc<Engine>, String, String, String, String, usize)` — matches constructor in Task 5 and usage in Task 8
- `StatusLine` defined in Task 4 before `App` imports it in Task 5
- `render::draw(f, app)` — matches signature in Task 6 and call in Task 8
- `input::handle_key(app, key)` — matches signature in Task 7 and call in Task 8
- `Message::user(content)` and `Message::assistant(content)` — defined in Task 5, used in Task 7, Task 8
- `app.add_assistant_message(content)` — defined in Task 5, used in Task 8
- `AppState::Waiting`, `AppState::Idle`, `AppState::Streaming` — defined in Task 5, used in Task 6, Task 7, Task 8

**4. Audit fixes applied:**
- ✅ Token double-counting: `poll_engine_events()` only tracks `AgentStateChanged`; token stats come from `ChatResponse.token_usage`
- ✅ Task ordering: status.rs (Task 4) before app.rs (Task 5), dependency is correct
- ✅ Multi-version cleanup: Tasks 5/7/8 now contain only final code, editorial iteration removed
- ✅ Clippy stubs: empty modules have `#[allow(dead_code)]` pub struct placeholders
- ✅ Top-level keymap.rs: acknowledged as Milestone D scope in plan preamble
- ✅ tui-textarea API risks: documented in plan preamble

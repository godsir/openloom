# Phase 4 TUI 重构方案 — Claude Code / OpenCode 对比分析

## 目标与范围

**目标：** 构建一个 Claude Code / OpenCode 级别的终端聊天界面，支持流式响应、斜杠命令、审批覆盖层、主题系统、可配置按键绑定。

**明确不做（本次范围外）：**
- Vim mode / leader key（Milestone D）
- TUI 插件系统（Phase 5+）
- 全功能 IDE（文件树、LSP 状态面板 — openLoom 是 AI 助理不是 IDE）
- 多会话切换 UI（先用斜杠命令操作,后续加 tab 切换）
- 图片/文件附件渲染

---

## 参考项目分析摘要

### Claude Code

- **框架：** React 19 + @anthropic/ink（自定义 Ink 分支，React reconciler for terminal）
- **架构：** Declarative React component tree → Ink reconciler → Yoga layout → cell-based Screen buffer → diff-based terminal writes
- **规模：** REPL.tsx 单文件 6107 行，PrompInput 2439 行，共 ~80+ TUI 组件
- **流式：** StreamingMarkdown 组件，stable-prefix memoization（稳定前缀用 useRef 缓存不重解析，仅增量尾部重 lex），marked.lexer 正确处理未闭合 code fence
- **按键：** Context-sensitive keybinding（Global / Chat / Autocomplete / Settings / Confirmation），useKeybinding hook 注册
- **状态：** External store + useSyncExternalStore + selector 精准订阅，避免不必要 re-render
- **滚动：** VirtualMessageList（987行）虚拟滚动 + ScrollBox stickyScroll + useUnseenDivider（手动上滚后显示"N new messages"）
- **渲染优化：** 双缓冲 frontFrame/backFrame diff，CharPool/StylePool interning，blitRegion 识别不变区域跳过重绘，SYNC_OUTPUT_SUPPORTED 同步批量写入防闪烁

### OpenCode

- **框架：** TypeScript/Bun + @opentui/*（自研 TUI 框架） + SolidJS 响应式
- **架构：** 双 TUI 模式 — (A) 全量 SolidJS SPA (97文件) 和 (B) 分屏 footer 直交互模式 (36文件)
- **规模：** session/index.tsx 2198 行，prompt/index.tsx 1673 行，共 ~60+ 组件
- **流式：** Effect 事件订阅 + 纯 reducer produce StreamCommit[] (start/progress/final) + ScrollbackSurface 渐进提交
- **按键：** @opentui/keymap mode stack + leader key (ctrl+x) + command palette
- **状态：** SolidJS createStore + reconcile 不可变更新，Effect 结构化并发
- **渲染：** SolidJS 细粒度响应式（无 VDOM），TreeSitter 语法高亮，Kitty keyboard protocol

### 关键差异总结

| 维度 | Claude Code | OpenCode | openLoom（选用） |
|------|-------------|----------|-----------------|
| 语言/运行时 | TypeScript/Node.js | TypeScript/Bun | **Rust/Tokio** |
| TUI框架 | Ink (React reconciler) | @opentui (自研) | **ratatui + crossterm** |
| 渲染范式 | Retained (React) | Hybrid (SolidJS + retained surfaces) | **Immediate-mode** |
| 流式处理 | API stream → React state → markdown progressive render | Effect Stream → pure reducer → StreamCommit[] → ScrollbackSurface | **mpsc channel → try_recv → buffer append** |
| 按键系统 | Context-sensitive keybinding | Mode stack + leader key | **Context-sensitive + keymap.toml** |

---

## 技术选型

| 层 | 选择 | 原因 |
|---|------|------|
| 终端后端 | crossterm 0.28 | 跨平台，raw mode + mouse + resize 事件 |
| TUI 框架 | ratatui 0.29 | Rust 生态标准，immediate-mode，layout 约束系统 |
| 文本输入 | tui-textarea 0.7 | 多行输入、光标移动、选择、历史、vim 模式（后续用） |
| 语法高亮 | syntect 5 | 代码块渲染（diff viewer 和 markdown 代码块） |
| 模糊搜索 | skim 0.15 | 斜杠命令面板、历史搜索 |

---

## 文件结构

```
crates/cli/src/
├── main.rs              # mod tui; Chat handler 调用 tui::run()
├── keymap.rs            # KeyBinding 类型 + 默认绑定 + 合并逻辑
├── tui/
│   ├── mod.rs           # pub async fn run(engine) -> Result<()>
│   ├── app.rs           # App struct, AppState 枚举, 事件循环
│   ├── render.rs        # 全部 draw 函数（消息列表/状态栏/输入区）
│   ├── input.rs         # 按键分发 + 输入处理
│   ├── commands.rs      # 斜杠命令定义 + handle_command()
│   ├── status.rs        # 状态栏数据模型 + 格式化
│   ├── theme.rs         # 语义调色板 + 明暗主题 + Style 工厂
│   ├── keymap.rs        # 按键绑定解析 + 上下文匹配
│   ├── streaming.rs     # 流式状态管理（StreamState + token 缓冲）
│   └── overlays/
│       ├── mod.rs       # Overlay trait（activate/dismiss/confirm/draw）
│       ├── approval.rs  # 工具审批覆盖层
│       ├── diff.rs      # 代码 diff 查看器
│       └── help.rs      # 帮助/命令面板覆盖层
```

---

## 组件架构与数据流

```
┌─────────────────────────────────────────────────┐
│                    main.rs                       │
│  Chat handler → build_engine() → tui::run()     │
└────────────────────┬────────────────────────────┘
                     │
┌────────────────────▼────────────────────────────┐
│                  tui::mod.rs                     │
│  初始化终端 → 创建 App → 事件循环 → 恢复终端     │
│                                                  │
│  loop {                                          │
│    poll EngineEvents (broadcast rx)              │
│    poll stream tokens (mpsc rx)                  │
│    draw frame (render::draw)                     │
│    poll input (crossterm event::poll)            │
│    dispatch (input::handle)                      │
│  }                                               │
└────────────────────┬────────────────────────────┘
                     │
     ┌───────────────┼───────────────┐
     ▼               ▼               ▼
┌─────────┐  ┌──────────┐  ┌──────────────┐
│  app.rs │  │ render.rs│  │ streaming.rs │
│  State  │  │  Draws   │  │ TokenBuffer  │
│  Msgs   │  │  Layout  │  │ StreamState  │
└─────────┘  └──────────┘  └──────────────┘
```

---

## 核心数据结构

```rust
// app.rs

pub struct App {
    pub engine: Arc<Engine>,
    pub session_id: String,
    pub messages: Vec<Message>,      // 当前会话的全部消息
    pub input: TextArea,             // tui-textarea 多行输入
    pub history: Vec<String>,        // 输入历史
    pub history_idx: Option<usize>,
    pub state: AppState,
    pub scroll: u16,                 // 消息区滚动偏移
    pub auto_scroll: bool,
    pub status: StatusLine,          // 状态栏数据
    pub theme: Theme,
    pub keymap: ResolvedKeymap,
    pub stream: StreamState,         // 当前流式状态
    pub event_rx: broadcast::Receiver<EngineEvent>,
    pub overlay: Option<Box<dyn Overlay>>,
    pub should_exit: bool,
    // 统计累加
    pub total_prompt_tokens: usize,
    pub total_completion_tokens: usize,
    pub total_cost: f64,
}

pub enum AppState {
    Idle,
    Streaming,    // 正在接收 token
    Waiting,      // 等待模型响应（流式尚未到达第一个 token）
    Overlay,      // 覆盖层活跃（审批/帮助）
}

pub struct StatusLine {
    pub model: String,
    pub agent_state: AgentState,
    pub context_pct: f64,
    pub turn_tokens: usize,
    pub git_branch: String,
    pub cwd: String,
}

// streaming.rs

pub struct StreamState {
    pub buffer: String,
    pub task: Option<JoinHandle<()>>,
    pub token_rx: Option<mpsc::Receiver<String>>,
}

// keymap.rs

pub struct KeyBinding {
    pub key: String,
    pub modifiers: Vec<String>,
    pub action: String,
    pub context: String,  // "global" | "input" | "streaming" | "overlay"
}

pub struct ResolvedKeymap { bindings: Vec<KeyBinding>; }

// overlays/mod.rs

pub trait Overlay {
    fn draw(&self, f: &mut Frame, area: Rect);
    fn handle_key(&mut self, key: KeyCode) -> OverlayResult;
    fn context(&self) -> &str;
}

pub enum OverlayResult { Consumed, Dismiss, Confirm(Value) }
```

---

## 流式响应流程

```
用户按 Enter 发送消息
  → AppState = Waiting
  → 追加用户消息到 messages
  → 新增空 assistant 消息到 messages（占位）
  → 构建 CompletionRequest { prompt, stream: true, .. }
  → 创建 mpsc channel (tx, rx)
  → spawn: engine.stream_complete(req, tx)
  → 存入 stream.token_rx = Some(rx)

Render Loop（每 80ms 或更短）:
  → poll_stream_tokens():
      while let Ok(token) = rx.try_recv() {
          AppState = Streaming
          追加 token 到 messages.last().content
      }
      if tx dropped { AppState = Idle }

TokenUsage 事件到达:
  → 更新 total_prompt_tokens / total_completion_tokens
  → 计算 total_cost
```

---

## 按键绑定（默认）

**Global 上下文：**
| 按键 | 动作 | 说明 |
|------|------|------|
| Ctrl+C | quit (一次=取消流式, 两次=退出) | 对齐两个参考项目 |
| Ctrl+L | redraw | 清屏重绘 |
| PageUp/PageDown | scroll | 消息区翻页 |

**Input 上下文：**
| 按键 | 动作 |
|------|------|
| Enter | send |
| Shift+Enter | newline |
| Up/Down | history_navigate |
| Tab | autocomplete（后续实现） |
| Ctrl+R | history_search |
| Ctrl+G | external_editor |

**Streaming 上下文：**
| 按键 | 动作 |
|------|------|
| Ctrl+C | cancel_stream |
| Esc | cancel_stream |

**Overlay 上下文：**
| 按键 | 动作 |
|------|------|
| Esc | dismiss_overlay |
| Enter | confirm_overlay |
| Left/Right/Tab | navigate_options |

---

## 主题系统

```rust
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
```

预设 dark / light 两套，后续可从 theme.toml 加载。

---

## 斜杠命令

| 命令 | 说明 |
|------|------|
| `/help` | 显示命令面板覆盖层 |
| `/session [new\|list\|switch]` | 会话管理 |
| `/model` | 显示当前模型 |
| `/memory [persona\|events\|cognitions\|search]` | 记忆查询 |
| `/skills [list\|invoke]` | 技能管理 |
| `/cost` | Token 用量和费用 |
| `/config [get\|set]` | 配置管理 |
| `/theme [dark\|light\|path]` | 主题切换 |
| `/clear` | 清屏 |

---

## Milestones

### Milestone A — 基础聊天 REPL
1. 恢复 Cargo.toml 依赖（ratatui, crossterm, tui-textarea）
2. 实现 theme.rs（调色板 + Style 工厂）
3. 实现 app.rs（App struct + AppState + 构造）
4. 实现 render.rs（消息列表 + 状态栏 + 输入区 layout）
5. 实现 input.rs（Enter 发送 + 输入处理）
6. 实现 mod.rs（run loop）
7. 集成 Chat handler（非流式，调用 engine.handle_message）
8. 编译验证 + 基本功能测试

### Milestone B — 流式 + 斜杠命令
1. 实现 streaming.rs（StreamState + mpsc 集成）
2. 修改 app.rs 的 send_message 走 stream_complete 路径
3. 修改 render loop 在 Streaming 态每 80ms 重绘
4. 实现 commands.rs（斜杠命令解析 + handler 分发）
5. 实现 keymap.rs（按键绑定系统 + context 匹配）
6. 流式取消（Ctrl+C 第一次）

### Milestone C — 覆盖层
1. 实现 overlays/mod.rs（Overlay trait）
2. 实现 overlays/approval.rs（工具审批对话框）
3. 实现 overlays/diff.rs（代码 diff 查看器，syntect 高亮）
4. 实现 overlays/help.rs（斜杠命令面板/帮助）
5. 修改 render loop 支持 overlay 绘制层级

### Milestone D — 打磨
1. 按键绑定配置文件加载（keymap.toml）
2. 扩展主题支持（theme.toml + 更多预设）
3. 输入历史模糊搜索（Ctrl+R）
4. 外部编辑器集成（Ctrl+G → $EDITOR）
5. 性能优化 + 错误处理完善

---

## 方案决策日志

1. **框架选型：ratatui + crossterm + tui-textarea** — 两个参考项目都是 TypeScript 自定义框架，我们是 Rust。ratatui 是生态标准，immediate-mode 对聊天场景性能足够。

2. **架构模式：借鉴 OpenCode 的 reducer 模式** — 避免 Claude Code 的巨型 REPL 单文件，也避免 OpenCode 的 97 文件过度拆分。App struct + 纯 render 函数，职责清晰。

3. **流式处理：mpsc channel + render loop try_recv** — Engine 已提供接口，Cloud 真逐 token，本地模型整段发（已知债务，TUI 侧用 loading 态兜底）。

4. **按键系统：Context-sensitive + keymap.toml** — Global/Input/Streaming/Overlay 四个 context，默认绑定参考两个项目的通用惯例。

5. **文件拆分：10 文件，按关注点分离** — app / render / input / commands / status / theme / keymap / streaming / overlays(approval+diff+help)。

---

## 权衡说明

1. **Immediate-mode vs retained-mode：** ratatui 每帧全量重绘而非增量 diff。对于 80x40 终端全量重绘 <1ms，无感知。但做不到 Claude Code 那种 blitRegion 不变区域跳过的优化。

2. **本地模型流式是假流式：** `inference::complete_stream` 目前一次性发送完整结果（已知技术债）。Cloud 模型是真逐 token。TUI 侧用 "thinking..." 加载态兜底。

3. **Agent Loop 内部不流式：** 复杂任务走 agent loop，内部调用非流式模型。TUI 只能看到状态变化事件，看不到中间 token。参考项目也有类似限制。

4. **没有 Engine.interrupt() 公开 API：** 流式取消需要新增 Engine 方法。Milestone A 用临时方案（drop mpsc sender）。

---

> 分析日期: 2026-05-21
> 参考代码库: F:\claude-code (Claude Code), F:\opencode (OpenCode)
> 目标版本: openLoom Phase 4 Milestone A

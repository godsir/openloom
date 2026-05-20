# Phase 4: CLI-First UX — 对标 Claude Code / Codex CLI

**Status:** Spec (待审批)
**Date:** 2026-05-21
**Scope:** TUI 重构 + CLI 增强，不分 milestone，一次性交付

---

## 1. Goal & Scope

### Goal
将 openLoom 的 CLI/TUI 体验从「能用的聊天界面」提升到「对标 Claude Code / Codex CLI 的专业级 CLI 工具」。

### In Scope
- 流式响应渲染（逐 token，非全文阻塞）
- 智能退出（双击 Ctrl+C）
- Agent 状态可视化（Idle → Thinking → Acting 状态机动画）
- 增强状态栏（上下文窗口用量、Token 成本、模型名、Git 分支）
- 审批覆盖层（文件写入、Shell 执行等破坏性操作确认）
- 代码差异查看器（syntax-highlighted unified diff）
- 外部编辑器集成（Ctrl+G 打开 $EDITOR）
- 斜杠命令系统升级（完整匹配 + 参数解析 + 自定义命令）
- 主题系统（语义色板 + /theme 切换）
- 可配置快捷键（keymap.toml）
- Bash 模式（! 前缀执行 Shell 命令）
- 文件选择器（@ 前缀模糊搜索文件）
- Token 用量仪表板（/cost, /usage）

### Out of Scope
- 会话 Fork/Resume（依赖会话持久化架构调整，Phase 5）
- 自定义 Slash 命令文件加载（Phase 5）
- 多 Subagent 可视化（当前无 subagent 架构）
- Plan Mode（读-only 探索模式，Phase 5）
- Electron 前端改进（本 Phase 只做 CLI/TUI）

---

## 2. Chosen Approach

**增量演进，不重写。** 基于现有 `chat_tui.rs`（860 行）逐步增强，保持 ratatui + crossterm 技术栈不变。

### Ruled Out
- **重写为类似 warp 的框架** — 切换代价大，ratatui 已足够，生态成熟
- **纯 Line-based CLI（非 TUI）** — 失去交互性，不符合对标目标
- **用 GPUI 重写** — 引入新语言/工具链，破坏 Rust-only 栈

### Why This Approach
1. 现有 TUI 架构健康：`draw()` 函数清晰、消息列表管理良好、命令处理有结构
2. 引擎能力已完善但 TUI 只用了 ~30%：流式 API (`stream_complete`)、事件总线 (`subscribe`)、Token 存储均已就绪
3. ratatui 生态完全能支撑 Claude Code 级别的 UI（Codex CLI 也用 ratatui）
4. 增量演进风险低：每步可独立测试，不影响现有功能

---

## 3. Architecture / Components

```
crates/cli/src/
├── main.rs              # CLI 入口（不变）
├── chat_tui.rs          # TUI 主循环 → 拆分为:
├── tui/
│   ├── app.rs           # App 状态机 + 主循环
│   ├── render.rs        # 渲染逻辑（draw 函数族）
│   ├── input.rs         # 输入处理 + 快捷键
│   ├── commands.rs      # 斜杠命令系统
│   ├── overlay.rs       # 审批覆盖层 / 弹窗
│   ├── diff.rs          # 代码差异查看器
│   ├── theme.rs         # 主题/色板系统
│   └── status.rs        # 状态栏渲染
├── download.rs          # 模型下载（不变）
└── keymap.rs            # 快捷键配置加载
```

### 3.1 App 状态机

```
Idle → Waiting → Streaming → Done → Idle
                 ├─ Thinking
                 └─ Acting
```

- `Idle`: 等待用户输入
- `Waiting`: 消息已发送，等待首个 token
- `Streaming`: 正在接收/渲染 token 流
- `Thinking`: Agent Loop 思考中
- `Acting`: Agent Loop 执行 Tool 中
- `Done`: 响应完成，显示用量

状态转换通过引擎 EventBus 驱动（`subscribe()` 接收 `EngineEvent`）。

### 3.2 渲染架构

借鉴 Claude Code 的 static/dynamic 分离：

- **Static area**: 已完成的消息（仅滚动时重绘）
- **Dynamic area**: 流式响应行 + 输入区（每帧重绘）

当前 TUI 每帧重绘所有消息 → 改为缓存已完成消息的 `Vec<Line>`，只在流式行和输入区做增量更新。

### 3.3 流式渲染

关键变更：当前 `handle_message()` 阻塞等待完整响应 → 改为调用 `stream_complete()`，通过 mpsc channel 接收 token。

```rust
// 当前 (阻塞)
let resp = engine.handle_message(msg, &sid).await?;

// 改为 (流式)
let (tx, mut rx) = tokio::sync::mpsc::channel(256);
let engine_clone = engine.clone();
tokio::spawn(async move {
    engine_clone.stream_complete(req, tx).await
});
// TUI 主循环每帧 poll rx.try_recv()
```

Token 到达时立即追加到当前助手消息行，下一帧渲染时自动显示。

### 3.4 审批覆盖层

当引擎返回需要审批的操作（`tool_call` 带风险级别），TUI 弹出覆盖层：

```
┌── Approval Required ──────────────────────────┐
│                                                │
│  ⚠ File Write: src/config.rs                  │
│                                                │
│  +  use std::path::PathBuf;                    │
│  +  pub fn new_path() -> PathBuf { ... }       │
│                                                │
│  [A]pprove  [D]eny  [S]ession  [C]ancel       │
│                                                │
└────────────────────────────────────────────────┘
```

风险级别染色：
- **只读操作**: 灰色边框（始终自动批准）
- **文件写入**: 黄色边框（会话内可记忆）
- **Shell/网络**: 红色边框（每次必须确认）

### 3.5 代码差异查看器

`/diff` 命令或 `CodeAssist` 技能输出变更时，使用 syntax-highlighted unified diff 格式渲染：

- 删除行：红色背景
- 新增行：绿色背景
- 上下文行：默认背景
- 文件名头：粗体
- 支持滚动浏览大型 diff

### 3.6 快捷键系统

双层配置：
1. **硬编码默认**（内置，零配置可用）
2. **`keymap.toml`** 覆盖（用户自定义）

默认快捷键：

| 组合键 | 操作 | 上下文 |
|--------|------|--------|
| `Ctrl+C` x2 | 退出 | global |
| `Ctrl+C` x1 | 取消当前生成 | streaming |
| `Ctrl+G` | 外部编辑器 | input |
| `Ctrl+R` | 历史搜索 | input |
| `Ctrl+L` | 清屏/重绘 | global |
| `Ctrl+J` | 换行 | input |
| `Shift+Enter` | 换行 | input |
| `Enter` | 发送 | input |
| `Tab` | 自动完成命令 | input |
| `↑/↓` | 导航历史/命令列表 | input |
| `Esc` | 关闭弹窗 | overlay |
| `PgUp/PgDn` | 滚动消息 | global |

### 3.7 主题系统

语义色板定义（TOML 格式）：

```toml
[theme]
brand = "#663BF9"
user_msg = "#FFFFFF"
assistant_msg = "#E0E0E0"
thinking = "#FFD700"
error = "#FF4444"
info = "#888888"
diff_add = "#00AA00"
diff_del = "#CC0000"
warning = "#FF8800"
```

内置 3 个预设：`dark`（默认）、`light`、`high-contrast`。
`/theme <name>` 切换。

### 3.8 增强状态栏

```
 ● Ready  │  openloom:main  │  Qwen3-1.7B  │  ∅ 1.2k/8k  │  0.12  │  $0.0018
   状态       Git 分支         模型名           上下文窗口     Token    成本
```

状态指示器：
- `● Ready` (灰色) — Idle
- `◌ Waiting` (黄色闪烁) — 等待响应
- `◉ Streaming` (绿色闪烁) — 接收中
- `◎ Thinking` (品红色) — Agent 思考
- `◍ Acting` (青色) — Agent 执行

### 3.9 Bash 模式

输入以 `!` 开头直接在当前终端执行 Shell 命令：

```
▸ ! cargo test -- --nocapture
```

命令输出在消息区显示（灰色代码块样式），命令结束后返回到消息流。命令执行期间 Ctrl+C 可以终止命令（不是退出 TUI）。

### 3.10 文件选择器

输入 `@` 触发模糊文件搜索：

```
▸ @src/tui/
┌── Files ─────────────────────────┐
│ src/tui/app.rs                   │
│ src/tui/render.rs                │
│ src/tui/input.rs                 │
└──────────────────────────────────┘
```

选择文件后，将其路径插入到输入中。文件路径作为上下文发送给引擎。

---

## 4. Key Decisions Log

| # | Decision | Rationale |
|---|----------|-----------|
| 1 | 增量演进 vs 重写 | 现有 TUI 架构健康，重写成本高、风险大，增量演进可每步独立测试 |
| 2 | ratatui 不换 | Codex CLI 也用 ratatui，生态成熟；切换到 warp/iocraft 无实质收益 |
| 3 | 审批覆盖层（非内联） | Claude Code 和 Codex 都用覆盖层/弹窗，视觉更突出，用户难以忽略 |
| 4 | 静态/动态分离渲染 | Claude Code 的渲染策略，对长对话性能关键 |
| 5 | 状态机驱动 Agent 状态 | 引擎已有 `AgentState` 枚举和 `EngineEvent` 广播，TUI 只需订阅 |
| 6 | 双层快捷键配置 | 对标 Claude Code `keybindings.json` + Codex `keymap`，零配置可用 + 可深度定制 |
| 7 | 语义色板 + 预设主题 | 对标 Claude Code `/theme`，3 个预设覆盖主要需求，避免过度设计 |
| 8 | Bash 模式 `!` 前缀 | Claude Code 和 Codex 都有此功能，实现简单（std::process::Command） |
| 9 | 文件选择器 `@` 前缀 | Codex 的标志性功能，fuzzy matching 用 skim/skim 库 |
| 10 | 暂不实现 Sessions Fork | 依赖数据库架构调整，影响面大，独立 Phase 处理 |
| 11 | TUI 文件拆分（1→8模块） | 当前 860 行单文件，拆分后每个模块 100-200 行，可测试可维护 |
| 12 | `stream_complete` 替代 `handle_message` | 引擎已实现 SSE 流式，之前 TUI 未使用是因为 handle_message 更简单；现在必须接入 |

---

## 5. Implementation Order

### Milestone A: 流式渲染 + 状态机 + 增强状态栏（基础 UX）

1. TUI 文件拆分：`chat_tui.rs` → `tui/` 8 模块
2. App 状态机实现（Idle → Waiting → Streaming → Done）
3. 接入 `stream_complete()` 替代阻塞 `handle_message()`
4. Token-by-token 渲染（增量追加到当前消息行）
5. 增强状态栏：Git 分支、上下文窗口、Token 计数、状态指示器
6. 静态/动态区域分离渲染
7. 测试：`cargo test` + 手动 TUI 测试

### Milestone B: 智能退出 + 快捷键 + 主题

8. 双击 Ctrl+C 退出逻辑
9. 快捷键系统（keymap.rs + keymap.toml 加载）
10. 全部默认快捷键实现
11. 主题系统（semantic palette + 3 presets）
12. `/theme` 命令
13. 外部编辑器集成（Ctrl+G）

### Milestone C: 审批覆盖层 + 差异查看器

14. 审批覆盖层组件
15. 风险级别染色
16. 会话级审批记忆
17. 代码差异查看器（unified diff + syntax highlighting）
18. `/diff` 命令

### Milestone D: Bash 模式 + 文件选择器 + Token 仪表板

19. `!` Bash 模式
20. `@` 文件选择器（fuzzy matching）
21. Token 用量仪表板（/cost, /usage）
22. `/keymap` 交互式查看器
23. 更新 README / CLAUDE.md / 文档

---

## 6. Tradeoffs

| Tradeoff | Why Acceptable |
|----------|----------------|
| 拆分为 8 模块增加文件数 | 每个模块 <200 行，可独立测试，比单文件 1000+ 行强得多 |
| `stream_complete` 需 spawn 任务 | Tokio 已就绪，mpsc channel 开销可忽略（256 buffer） |
| 审批覆盖层需引擎配合 | 引擎已有 `invoke_skill()` 和沙箱权限模型，扩展即可 |
| 主题系统仅 3 预设 | 对标 Claude Code 当前 `/theme` 功能，够用；后续可扩展自定义 |
| 暂不实现 Sessions Fork | 涉及 SQLite schema 变更，影响 memory pipeline，独立 Phase 更安全 |
| fuzzy 文件搜索可能性能有问题 | skim crate 已是成熟方案，大仓库用 `git ls-files` 加速 |

---

## 7. Tests

- **单元测试**: keymap 解析、命令匹配、主题加载、状态机转换
- **集成测试**: TUI 端到端（模拟输入→引擎→流式输出）
- **手动测试**: 所有快捷键、主题切换、审批覆盖层交互、Bash 模式

目标: cargo test ≥ 140 pass, clippy 0 warnings

---

## 8. Remaining Open Questions

无。所有设计决策已在 Key Decisions Log 中记录并闭包。

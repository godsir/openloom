# TUI 功能补全计划

## Context

后端 Engine 有 ~20 个公开方法，Server 层提供了 17 个 JSON-RPC 方法和 4 个 HTTP/WS/SSE 端点。但 TUI (`chat_tui.rs`) 只接了 5 条斜杠命令，其中 `/model` 和 `/session` 是声明了但没实现的 bug。用户对话的核心路径（`handle_message`）能跑，但周边功能（记忆查看、会话管理、系统诊断、技能列表、Token 用量等）完全没有接入 TUI。

## 改动范围

仅改一个文件：`crates/cli/src/chat_tui.rs`（~455 行 → ~600 行）

## 实现方案

所有新命令都在 `ChatApp::run()` 的 `KeyCode::Enter` match 分支中增加 handler。命令响应以 `Message { role: "assistant", ... }` 形式追加到消息列表，复用现有的消息渲染管线。

### 1. 修 Bug

**`/model`** — 调用 `engine.model_display_name()` + `engine.health_check()`，显示模型名和 GPU 信息。

**`/session`** — 子命令：
- `/session` → 列出所有会话（`engine.list_sessions()`）
- `/session new` → 创建新会话并切换（`engine.create_session()`，更新 `self.session_id`）
- `/session switch <id>` → 切换到指定会话（更新 `self.session_id`）

### 2. 加命令

**`/memory`** — 子命令：
- `/memory persona` → `engine.persona_summary()`
- `/memory events [N]` → `engine.search_events("", N)`
- `/memory cognitions [subject]` → `engine.list_cognitions(subject, 20)`

**`/doctor`** — `engine.health_check()`，显示 status / uptime / GPU info

**`/skills`** — `engine.list_skills()`，显示已注册技能名+描述+触发词

**`/agent`** — `engine.agent_state()`，显示 Agent 当前状态

**`/cache`** — `engine.cache_stats()`，显示缓存命中率/block 数

**`/config`** — 子命令：
- `/config` → `engine.get_config(None)` 显示全部
- `/config <key>` → `engine.get_config(Some(key))` 显示单项

**`/version`** — 显示 `env!("CARGO_PKG_VERSION")`

### 3. Token 用量显示

`handle_message` 返回的 `ChatResponse` 已包含 `token_usage` 字段（prompt_tokens, completion_tokens, latency_ms）。在收到回复后，追加一条 dim 风格的消息显示用量统计。

### 4. COMMANDS 常量更新

把所有新命令加入 `COMMANDS` 数组，Tab 自动补全自然生效。

### 5. 命令帮助分组

`/help` 输出按功能分组（Chat / Session / Memory / System），比现在的一维列表清晰。

## 不做的

- **SSE 流式输出**：需要改 inference 层（逐 token 发送），不是 TUI 层的问题
- **EventBus 订阅**：`engine.subscribe()` 可以在 TUI 里启动一个后台 task 接收实时事件并显示，但会让本次改动范围膨胀过大。留到后续迭代
- **cognition_snapshots / rollback**：高级记忆管理操作，TUI 里交互复杂，保留在 CLI 和 JSON-RPC 即可
- **`/config set`**：TUI 里写配置需要输入校验，风险高。保留在 CLI `openloom config set`

## 实现细节

- 命令解析用简单的手写 parser（match 前缀），不引入 clap
- 所有新命令都不需要 `loading` 状态（全是瞬时查询），直接追加结果
- `/session switch` 切换时清空消息列表（`self.messages.clear()`），输入框重置
- 命令表格放在 `COMMANDS` 常量顶部，分组注释

## 验证

1. `cargo clippy -- -D warnings` — 零警告
2. `cargo test` — 全部 129 个测试通过
3. `cargo run -- chat` — 手动测试每个新命令：
   - `/help` — 分组显示
   - `/model` — 显示模型信息
   - `/session` / `/session new` / `/session switch <id>` — 会话管理
   - `/memory persona|events|cognitions` — 记忆查看
   - `/doctor` — 系统诊断
   - `/skills` — 技能列表
   - `/agent` — Agent 状态
   - `/cache` — 缓存统计
   - `/config` / `/config <key>` — 配置查看
   - `/version` — 版本号
   - 发一条普通消息 → 回复后显示 token 用量

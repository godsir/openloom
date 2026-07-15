# 导入 OpenClaw / Codex 会话历史

## 背景
openloom 已支持导入 Claude Code 会话（`claude_import.scan/run` + `loom-import/claude.rs`）。前端 `ImportConversationsTab.tsx` 已预留 openclaw/codex 两个 SourceKey 但 `available:false`。本 spec 实现这两个 source 的会话历史导入。

## 目标
用户在"设置 → 导入会话"里能切换 OpenClaw / Codex source，扫描其本地会话历史，勾选导入为 openLoom session（与 Claude 导入体验一致）。

## 数据格式（已查证）

### Codex CLI
- 位置：`~/.codex/sessions/YYYY/MM/DD/rollout-<ts>-<uuid>.jsonl`，归档在 `~/.codex/archived_sessions/rollout-*.jsonl`；索引 `~/.codex/session_index.jsonl`（含 thread_name，可选用于标题）
- 每行：`{timestamp, type, payload}`
- type：
  - `session_meta`（首行）→ payload `{session_id, cwd, cli_version, ...}`
  - `response_item`（核心）→ payload.type：
    - `message`：role(user/assistant/developer/system) + content[]（`input_text`/`output_text`，各含 text）
    - `function_call`：name/call_id/arguments(JSON 字符串)
    - `function_call_output`：call_id/output
    - `reasoning`：思考内容
- 注意：`event_msg` 的 user_message/agent_message 与 response_item 重复，**只用 response_item**；user 消息文本以 `<` 开头的是 harness 注入（`<user_instructions>`/`<environment_context>` 等），**过滤掉**

### OpenClaw
- 位置：`~/.openclaw/agents/<agentId>/sessions/`
- `sessions.json`：索引（provider/channel → sessionId/sessionFile/updatedAt/model/tokens）。**不依赖它做扫描**（可能为空 `{}`），直接遍历 `*.jsonl`
- `<session-id>.jsonl`：append-only，每行一个事件，`type` 字段
  - `session`（首行）→ `{version, id, timestamp, cwd}`
  - `message` → `message.{role, content[], usage?, stopReason?}`
    - role: user/assistant/toolResult
    - content[]: `{type:text,text}` / `{type:thinking,thinking,thinkingSignature}` / `{type:toolCall,id,name,arguments(object)}`
    - assistant 带 `usage{input,output,cacheRead,cacheWrite,totalTokens,cost}` + `stopReason`
    - toolResult: `message.{role:toolResult, toolCallId, toolName, content[], details, isError}`
- 跳过后缀 `.deleted.<ts>Z` 和 `.jsonl.reset.<ts>Z`？— `.reset` 仍含完整历史可搜索，**保留**；`.deleted` **跳过**。

## 方案 A：独立 RPC（已选定）

claude_import.* 完全不动（零回归），新增 codex_import / openclaw_import，各自独立模块，出错隔离。

### 后端 loom-import
- 新 `codex.rs`：
  - `scan(sessions_root: &Path)`：递归 `sessions/YYYY/MM/DD/` + `archived_sessions/`，收集 `rollout-*.jsonl`，每文件读首行 session_meta 取 id/cwd + 扫 message 计数/时间/首条/模型
  - `build_payload(path)`：解析 session_meta→id/cwd；response_item message role=user/assistant→`Message`（content input_text/output_text→`ContentPart::Text`）；reasoning→`ContentPart::Thinking`；function_call→`ContentPart::ToolCall{id,name,arguments}`；function_call_output→`ContentPart::ToolResult{tool_call_id,name,result}`；过滤 `<`-开头 user 文本
- 新 `openclaw.rs`：
  - `scan(agents_dir: &Path)`：遍历 `agents/*/sessions/*.jsonl`，跳 `.deleted.*`；首行 session 事件取 id/cwd + 扫 message 计数/时间/首条/模型
  - `build_payload(path)`：session 首行→id/cwd；message role=user/assistant→`Message`；content text→Text、thinking→Thinking、toolCall→ToolCall；role=toolResult→ToolResult；assistant usage→`TokenUsage`
- 复用 `ConversationSummary` / `ImportPayload`（loom-types/import.rs，字段已够）
- `lib.rs` 导出 codex/openclaw 模块（不强制统一 scan/build_payload 签名，各模块自有）

### 后端 dispatch
- 新 `codex_import.rs`、`openclaw_import.rs`，结构同 `claude_import.rs`：
  - `handle(state, method, p)`：match `codex_import.scan`/`codex_import.run`（openclaw 同理）
  - `handle_scan`：`loom_import::codex::scan(dir)` + `mark_already_imported`
  - `handle_run`：ids→`is_safe_id`→`resolve_jsonl`→`build_payload`→`orchestrator.import_session_persisted`+`sessions.restore`
- `resolve_jsonl` 各自实现：
  - codex：递归 `sessions/`+`archived_sessions/` 找 `rollout-*<id>*.jsonl`（id 是 uuid，文件名 `rollout-<ts>-<id>.jsonl`）
  - openclaw：`agents/*/sessions/<id>.jsonl`
- 共享 helper：`is_safe_id`/`mark_already_imported` 从 claude_import 提取到 `dispatch/import_common.rs`（或各自 `use super::claude_import::...`），避免复制
- `dispatch/mod.rs` 注册 `codex_import`/`openclaw_import`

### 前端
- `ImportConversationsTab.tsx`：
  - SOURCES 全 `available: true`
  - `source → RPC` 映射：`claude→claude_import`、`codex→codex_import`、`openclaw→openclaw_import`
  - `scan()`/`importSelected()` 按 `source` 拼 RPC 名
  - 删除 `source !== 'claude'` 的 placeholder 分支
- `jsonrpc.ts`：`longMethods` 加 `codex_import.scan`/`codex_import.run`/`openclaw_import.scan`/`openclaw_import.run`
- i18n：确认 `settings.importSourceOpenclaw`/`settings.importSourceCodex` key 存在（SOURSES 已引用），缺则补三语

### 测试（按 backend-changes-self-test 自测）
- `loom-import/tests`：codex/openclaw fixture jsonl（各 1-2 个会话）+ `scan_test`/`build_test` 验证消息计数、role 映射、ToolCall/ToolResult 配对、harness 过滤、.deleted 跳过
- `cargo test -p loom-import` 全绿
- `cargo check -p loom-server` 确认 dispatch 注册

## 边界与复用
- 已导入去重：`session_uuid` vs `list_persisted_sessions()`（复用现有）
- 路径遍历防护：`is_safe_id`（复用）
- 目录不存在 → scan 返回空 Vec（不报错）
- codex/openclaw 目录缺失（用户没装）→ scan 空列表，前端显示"无可导入会话"
- ImportPayload 的 `workspace_path`：codex 用 session_meta.cwd，openclaw 用 session 事件 cwd
- 标题：codex 优先 session_index.jsonl 的 thread_name（按 id 查）→ 首条 user 文本 → "未命名"；openclaw 首条 user 文本 → "未命名"

## 不做（YAGNI）
- 不统一 claude/codex/openclaw 的 scan/build_payload 签名（各自模块自有，避免分派层 bug）
- 不迁移 claude_import → import（零回归）
- 不导入 codex/openclaw 的 token 统计到 openLoom token_usage 表（只导入消息）
- 不做增量导入（每次 scan 全量，already_imported 标记）

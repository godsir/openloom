# openLoom Backend API

## Transport

| 方式 | 端点 | 说明 |
|------|------|------|
| **WebSocket** | `ws://{host}:{port}/ws` | 主要通道，支持双向推送 |
| HTTP POST | `http://{host}:{port}/api` | 兼容通道，无推送 |

协议: **JSON-RPC 2.0**

---

## JSON-RPC 2.0 格式

### 请求
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "chat.send",
  "params": { "content": "hello", "session_id": "default" }
}
```

### 成功响应
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": { "response": "...", "session_id": "default" }
}
```

### 错误响应
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "error": { "code": -32600, "message": "content required" }
}
```

### 服务端推送 (仅 WebSocket)
```json
{
  "jsonrpc": "2.0",
  "method": "chat.stream_delta",
  "params": { "session_id": "default", "delta": "Hello" }
}
```

---

## 错误码

| 码 | 含义 |
|----|------|
| `-32700` | 解析错误 |
| `-32600` | 请求无效 |
| `-32601` | 方法不存在 |
| `-32603` | 内部错误 |
| `-32001` | Agent 不存在 |

---

## 方法列表

### System

#### system.health
无参数。返回服务状态、agent 数量、工具数量。
```json
// → { "status": "ok", "version": "0.2.0", "agent_count": 3, "tool_count": 15 }
```

---

### Chat

#### chat.send
发送消息，触发 Agent 推理循环。返回最终回复及统计信息。
流式增量通过 WebSocket 推送 `chat.stream_delta` 事件。

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `content` | string | * | 用户消息文本 |
| `session_id` | string | | 会话 ID，默认 `"default"` |
| `model` | string | | 单次模型覆盖 |
| `thinking_level` | string | | `off` / `auto` / `low` / `medium` / `high` |
| `attached_files` | array | | 图片附件 [{ mime_type, thumbnail?, path? }] |

```json
// → { "response": "...", "session_id": "default", "tool_calls": 2, "iterations": 3, "tokens": 1500 }
```

---

### Agent

#### agent.list
无参数。返回所有 Agent 摘要列表。
```json
// → { "agents": [...] }
```

#### agent.status
| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `id` | string | | Agent ID |

```json
// → { "id": "...", "state": "idle", ... }
```

#### agent.kill
| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `id` | string | | Agent ID |

```json
// → { "ok": true }
```

#### agent.config.list
无参数。返回所有 Agent 配置。
```json
// → { "configs": [...] }
```

#### agent.config.get
| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `name` | string | | 配置名，默认 `"default"` |

```json
// → { "name": "default", "system_prompt": "...", ... }
```

#### agent.config.create
Body 为完整 `AgentConfig` JSON 对象，`name` 字段必需。
```json
// → { "ok": true }
```

#### agent.config.update
Body 为完整 `AgentConfig` JSON 对象。额外字段：

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `prev_name` | string | | 原名（重命名时使用） |

```json
// → { "ok": true }
```

#### agent.config.delete
| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `name` | string | * | 不能删除 `"default"` |

```json
// → { "ok": true }
```

---

### Session

#### session.list
无参数。返回所有会话摘要。
```json
// → { "sessions": [...] }
```

#### session.create
| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `cwd` | string | | 工作目录，用作初始标题 |

```json
// → { "session_id": "uuid", "path": "uuid", "created_at": "..." }
```

#### session.switch
切换到指定会话，不存在则新建。

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `session_id` | string | | 会话 ID（可用 `path` 别名） |

```json
// → { "session_id": "uuid", "path": "uuid", "title": "..." }
```

#### session.messages
| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `session_id` | string | | 默认 `"default"` |

```json
// → { "messages": [...], "hasMore": false }
```

#### session.rename
| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `session_id` | string | * | |
| `title` | string | * | 新标题 |

```json
// → { "ok": true }
```

#### session.delete
| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `session_id` | string | * | |

```json
// → { "ok": true }
```

#### session.bind_agent
将会话绑定到指定 Agent 配置，后续对话使用该配置。

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `session_id` | string | | 默认 `"default"` |
| `agent_config_name` | string | | Agent 配置名 |

```json
// → { "ok": true }
```

---

### Model

#### model.list
无参数。返回所有模型配置及当前激活的模型。
```json
// → { "models": [...], "activeModel": "deepseek-v4-flash" }
```

#### model.switch
（简写，等同于 `model.config.set_active`）

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `model` | string | * | 模型配置名 |

```json
// → { "ok": true, "model": "deepseek-v4-flash" }
```

#### model.config.list
无参数。返回完整模型配置列表。
```json
// → [ { "name": "...", "model": "...", "backend": "...", ... } ]
```

#### model.config.get
| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `name` | string | * | |

#### model.config.create
Body 为完整 `ModelConfig` JSON 对象，`name` 必需。
```json
// → { "ok": true }
```

#### model.config.update
Body 为完整 `ModelConfig` JSON 对象。`name` 必需。
```json
// → { "ok": true }
```

#### model.config.delete
| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `name` | string | * | |

```json
// → { "ok": true }
```

#### model.config.set_active
| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `name` | string | * | |

```json
// → { "ok": true }
```

#### model.save_key
保存 API Key 到环境变量（当前进程生命周期）。

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `backend` | string | * | `deepseek` / `openai` / `anthropic` / `lmstudio` / `ollama` |
| `api_key` | string | * | |
| `api_key_env` | string | | 自定义环境变量名 |

```json
// → { "ok": true, "env_name": "DEEPSEEK_API_KEY" }
```

#### model.discover
扫描远端 API 端点，发现可用模型列表。LM Studio 额外调用原生 API 获取精确 `context_length`。

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `base_url` | string | * | API 端点 URL |
| `backend` | string | | `lmstudio` / `openai` / 等 |
| `api_format` | string | | `openai` / `anthropic` |
| `api_key_env` | string | | 环境变量名 |

```json
// → { "models": [{ "id": "deepseek-v4-flash", "context_length": 131072 }, ...] }
```

---

### MCP

#### mcp.list_servers
无参数。返回已连接的 MCP 服务器名列表。
```json
// → { "servers": ["leihuo_ai_personal"] }
```

#### mcp.list_tools
无参数。返回所有已连接 MCP 服务器的全部工具定义。
```json
// → { "tools": [{ "name": "mcp__leihuo_ai_personal__get_my_profile", "description": "...", ... }] }
```

#### mcp.list_resources
| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `server` | string | * | 服务器名 |

#### mcp.read_resource
| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `server` | string | * | |
| `uri` | string | * | |

#### mcp.list_resource_templates
| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `server` | string | * | |

#### mcp.list_prompts
| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `server` | string | * | |

#### mcp.get_prompt
| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `server` | string | * | |
| `name` | string | * | |
| `arguments` | object | | Prompt 参数 |

#### mcp.connect
连接并注册 MCP 服务器。可选持久化到 DB 供下次启动自动连接。

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `name` | string | * | 服务器名 |
| `transport` | string | | `stdio` / `http`，默认 `"stdio"` |
| `command` | string | | stdio 模式的启动命令 |
| `args` | string[] | | 命令行参数 |
| `url` | string | | HTTP 模式的 URL |
| `headers` | object | | HTTP 请求头 |
| `env` | object | | 环境变量 |
| `cwd` | string | | 工作目录 |
| `startup_timeout_secs` | number | | 启动超时，默认 30 |
| `tool_timeout_secs` | number | | 工具调用超时，默认 60 |
| `enabled_tools` | string[] | | 白名单 |
| `disabled_tools` | string[] | | 黑名单 |
| `persist` | bool | | 持久化保存，默认 true |
| `autostart` | bool | | 下次启动自动连接，默认 true |

```json
// → { "ok": true }
```

#### mcp.disconnect
| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `name` | string | * | |

```json
// → { "ok": true }
```

#### mcp.server_health
| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `name` | string | * | |

```json
// → { "healthy": true }
```

#### mcp.config.list
无参数。返回所有持久化保存的 MCP 配置，含连接状态。
```json
// → { "configs": [{ "name": "...", "transport": "http", "connected": true, "autostart": true, ... }] }
```

#### mcp.config.save
仅保存配置不连接。参数同 `mcp.connect`。
```json
// → { "ok": true }
```

#### mcp.config.delete
| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `name` | string | * | |

```json
// → { "ok": true }
```

---

### LSP

#### lsp.list_servers
无参数。返回已启动的 LSP 服务器列表。
```json
// → { "servers": ["rust", "typescript"] }
```

#### lsp.diagnostics
| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `file_path` | string | * | 文件绝对路径 |

#### lsp.completion
| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `file_path` | string | * | |
| `line` | number | | 0-based |
| `character` | number | | 0-based |

#### lsp.hover
参数同 `lsp.completion`。

#### lsp.definition
参数同 `lsp.completion`。

#### lsp.references
参数同 `lsp.completion`，外加：

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `include_declaration` | bool | | 默认 true |

#### lsp.symbols
| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `file_path` | string | * | |

#### lsp.shutdown
| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `language` | string | * | 如 `"rust"` / `"typescript"` |

```json
// → { "ok": true }
```

#### lsp.shutdown_all
无参数。关闭所有 LSP 服务器。
```json
// → { "ok": true }
```

#### lsp.supported_languages
无参数。返回内置支持的语言及对应命令。
```json
// → { "languages": [{ "language": "rust", "command": "rust-analyzer" }, ...] }
```

#### lsp.start
手动启动 LSP 服务器。

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `language` | string | * | |
| `command` | string | * | |
| `args` | string[] | | |

```json
// → { "ok": true }
```

---

### Tools

#### tools.list
无参数。返回工具注册表中所有工具名。
```json
// → { "tools": ["shell", "file_read", "file_write", "mcp__leihuo__get_my_profile", ...] }
```

---

### Skills

#### skills.list
无参数。扫描 `~/.loom/skills/` 发现所有已安装技能。
```json
// → { "skills": [{ "name": "...", "description": "...", "version": "...", "user_invocable": true, ... }] }
```

#### skills.get
| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `name` | string | * | 技能名 |

```json
// → { "content": "# SKILL.md content..." }
```

#### skills.import
导入技能文件到 `~/.loom/skills/<name>/`。

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `name` | string | * | 技能名 |
| `files` | array | * | `[{ "path": "SKILL.md", "content": "..." }]` |

```json
// → { "ok": true, "path": "~/.loom/skills/my-skill", "files_written": 1 }
```

#### skills.delete
| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `name` | string | * | |

```json
// → { "ok": true }
```

---

### Plugins

#### plugins.list
无参数。扫描 `~/.loom/plugins/` 和 `~/.claude/plugins/`。
```json
// → { "plugins": [{ "name": "...", "version": "...", "skill_count": 1, "mcp_server_count": 0, ... }] }
```

---

### Knowledge Graph

#### kg.search
全文搜索知识图谱节点。

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `query` | string | * | 搜索词 |
| `limit` | number | | 默认 20 |

```json
// → { "rows": [...] }
```

#### kg.stats
无参数。返回知识图谱统计（节点数、边数、实体类型分布等）。

#### kg.neighbors
| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `node_name` | string | * | |
| `limit` | number | | 默认 30 |

```json
// → { "nodes": [...], "edges": [...] }
```

#### kg.walk
从起始节点沿边遍历子图。

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `start_name` | string | * | |
| `max_depth` | number | | 默认 2（最大 255） |
| `limit` | number | | 默认 50 |

#### kg.list
分页列出所有节点。

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `limit` | number | | 默认 50 |
| `offset` | number | | 默认 0 |

#### kg.node.delete
| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `name` | string | * | 节点名 |

```json
// → { "deleted": true }
```

#### kg.edge.delete
| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `source` | string | * | 源节点名 |
| `target` | string | * | 目标节点名 |
| `relation` | string | | 关系类型，空则删除所有关系 |

```json
// → { "deleted": true }
```

---

### Config

#### config.get | config.set
通用键值配置的 getter/setter。当前为预留桩。

#### config.get_vision
无参数。读取 `~/.loom/vision.json` 视觉配置。
```json
// → { "enabled": false, "model": null }
```

#### config.set_vision
| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `enabled` | bool | | |
| `model` | string | | 视觉模型名 |

```json
// → { "ok": true }
```

---

## 服务端推送事件 (WebSocket only)

这些通知由服务端主动推送，无需客户端请求。

| 事件 | 参数 | 触发时机 |
|------|------|----------|
| `chat.stream_delta` | `{ session_id, delta }` | LLM 逐 token 输出 |
| `chat.stream_end` | `{ session_id }` | 当前回复结束 |
| `chat.token_usage` | `{ session_id, model, prompt_tokens, completion_tokens, context_window }` | Token 消耗统计 |
| `tool.started` | `{ id, name }` | 工具调用开始 |
| `tool.completed` | `{ id, name }` | 工具调用完成 |
| `agent.state_changed` | *agent 状态对象* | Agent 状态变更 |
| `agent.subagent_spawned` | *agent 状态对象* | 子 Agent 创建 |
| `agent.subagent_completed` | *agent 状态对象* | 子 Agent 正常完成 |
| `agent.subagent_errored` | *agent 状态对象* | 子 Agent 异常终止 |

---

## 通信模式

```
Client → Server (JSON-RPC Request)
  → Server → Client (JSON-RPC Response, same id)

Server → Client (JSON-RPC Notification, no id)
  → 仅 WebSocket，HTTP /api 不支持推送

chat.send 特殊流程:
  Client → { "method": "chat.send", ... }
    Server → { "method": "chat.stream_delta", "params": { "delta": "你" } }
    Server → { "method": "chat.stream_delta", "params": { "delta": "好" } }
    Server → { "method": "tool.started", "params": { "id": "1", "name": "file_read" } }
    Server → { "method": "tool.completed", "params": { "id": "1", "name": "file_read" } }
    Server → { "method": "chat.stream_end", "params": { "session_id": "..." } }
    Server → { "result": { "response": "你好", "tool_calls": 1, "iterations": 2, "tokens": 500 } }
```

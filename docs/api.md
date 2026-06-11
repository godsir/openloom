# openLoom Backend API

> Auto-generated from dispatch code. Methods: 136

## Transport

| 方式 | 端点 | 说明 |
|------|------|------|
| **WebSocket** | `ws://{host}:{port}/ws` | 主要通道，支持双向推送 |
| HTTP POST | `http://{host}:{port}/api` | 兼容通道，无推送 |

协议: **JSON-RPC 2.0**

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

## System

### system.health

```json
// → {
        "status": "ok", "version": "0.2.18",
        "agent_count": state.orchestrator.list_agents().await.len(),
        "tool_count": state.orchestrator.tool_registry().await.len(),
    }
```

---

## Chat

### chat.send

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `content` | string |  |  |
| `session_id` | string |  |  |
| `model` | string |  |  |

### chat.stop

```json
// → { "ok": true, "killed": killed }
```

---

## Agent

### agent.list

```json
// → { "agents": state.orchestrator.list_agents().await }
```

### agent.status

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `id` | string |  |  |

### agent.kill

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `id` | string |  |  |

```json
// → { "ok": true }
```

### agent.config.list

```json
// → { "configs": configs }
```

### agent.config.get

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `name` | string |  |  |

### agent.config.create

```json
// → { "ok": true }
```

### agent.config.update

```json
// → { "ok": true }
```

### agent.config.delete

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `name` | string |  |  |

```json
// → { "ok": true }
```

### agent.config.generate

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `description` | string |  |  |

### agent.config.optimize

---

## Session

### session.list

```json
// → { "sessions": state.sessions.list().await }
```

### session.create

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `cwd` | string |  |  |

```json
// → { "session_id": s.id, "path": s.id, "created_at": s.created_at }
```

### session.switch

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `path` | string |  |  |

```json
// → { "session_id": s.id, "path": s.id, "title": s.title }
```

### session.messages

```json
// → { "messages": msgs, "hasMore": false }
```

### session.delete_message

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `session_id` | string |  |  |
| `index` | number |  |  |

```json
// → { "ok": true }
```

### session.rename

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `session_id` | string |  |  |
| `title` | string |  |  |

```json
// → { "ok": ok }
```

### session.auto_title

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `session_id` | string |  |  |

```json
// → { "title": title }
```

### session.delete

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `session_id` | string |  |  |

```json
// → { "ok": ok }
```

### session.bind_agent

```json
// → { "ok": true }
```

---

## Model

### model.list

```json
// → { "models": models, "activeModel": active }
```

### model.switch

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `model` | string |  |  |

```json
// → { "ok": true, "model": name }
```

### model.config.list

### model.config.get

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `name` | string |  |  |

### model.config.create

```json
// → { "ok": true }
```

### model.config.update

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `name` | string |  |  |
| `model_type` | string |  |  |
| `backend` | string |  |  |
| `capabilities` | number |  |  |

```json
// → { "ok": true }
```

### model.config.delete

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `name` | string |  |  |

```json
// → { "ok": true }
```

### model.config.set_active

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `name` | string |  |  |

```json
// → { "ok": true }
```

### model.save_key

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `backend` | string |  |  |
| `api_key` | string |  |  |
| `api_key_env` | string | * |  |

```json
// → { "ok": true, "env_name": env_name }
```

### model.check_key

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `backend` | string |  |  |
| `api_key_env` | string |  |  |

```json
// → { "set": has_key, "env_name": env_name }
```

### model.discover

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `backend` | string |  |  |
| `base_url` | string |  |  |
| `api_key_env` | string | * |  |

---

## Workspace

### workspace.get

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `session_id` | string |  |  |

```json
// → { "workspace": workspace }
```

### workspace.set_session

```json
// → { "ok": true }
```

### workspace.set_default

```json
// → { "ok": true }
```

---

## Skills

### skills.list

```json
// → { "skills": list }
```

### skills.get

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `name` | string |  |  |

```json
// → { "content": content }
```

### skills.import

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `name` | string |  |  |
| `files` | string | * |  |

```json
// → { "ok": true, "path": skill_dir.display().to_string(), "files_written": wrote }
```

### skills.delete

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `name` | string |  |  |

```json
// → { "ok": true }
```

### skills.reload

```json
// → { "ok": true, "skill_count": count }
```

---

## Plugins

### plugins.list

```json
// → { "plugins": plugins }
```

### plugins.reload

```json
// → { "ok": true, "skill_count": count, "plugin_count": n }
```

---

## Marketplace

### marketplace.list

```json
// → { "plugins": results }
```

### marketplace.install

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `plugin_id` | string |  |  |

```json
// → { "ok": true, "path": target.display().to_string() }
```

### marketplace.uninstall

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `plugin_id` | string |  |  |

```json
// → { "ok": true }
```

### marketplace.update

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `plugin_id` | string |  |  |

```json
// → { "ok": true }
```

---

## Clawhub

### clawhub.list

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `search` | string |  |  |
| `force` | string |  |  |
| `base_url` | string |  |  |

```json
// → { "skills": enriched, "cached": false }
```

### clawhub.install

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `base_url` | string |  |  |

```json
// → { "ok": true, "slug": slug, "path": target_dir.to_string_lossy() }
```

### clawhub.uninstall

```json
// → { "ok": true, "slug": slug }
```

---

## MCP

### mcp.list_servers

```json
// → { "servers": names }
```

### mcp.list_tools

```json
// → { "tools": defs }
```

### mcp.list_resources

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `server` | string |  |  |

```json
// → { "resources": resources }
```

### mcp.read_resource

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `server` | string |  |  |
| `uri` | string |  |  |

### mcp.list_resource_templates

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `server` | string |  |  |

```json
// → { "templates": templates }
```

### mcp.list_prompts

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `server` | string |  |  |

```json
// → { "prompts": prompts }
```

### mcp.get_prompt

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `server` | string |  |  |
| `name` | string |  |  |
| `arguments` | any |  |  |

### mcp.connect

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `name` | string |  |  |
| `persist` | string |  |  |
| `autostart` | string |  |  |
| `url` | string |  |  |
| `cwd` | string |  |  |
| `enabled_tools` | string |  |  |
| `disabled_tools` | string |  |  |

```json
// → { "ok": true }
```

### mcp.disconnect

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `name` | string |  |  |

```json
// → { "ok": true }
```

### mcp.server_health

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `name` | string |  |  |

```json
// → { "healthy": healthy }
```

### mcp.config.list

```json
// → { "configs": items }
```

### mcp.config.save

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `name` | string |  |  |
| `autostart` | string |  |  |
| `url` | string |  |  |
| `cwd` | string |  |  |
| `enabled_tools` | string |  |  |
| `disabled_tools` | string |  |  |

```json
// → { "ok": true }
```

### mcp.config.delete

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `name` | string |  |  |

```json
// → { "ok": true }
```

---

## LSP

### lsp.list_servers

```json
// → { "servers": servers }
```

### lsp.diagnostics

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `file_path` | string |  |  |

### lsp.completion

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `file_path` | string |  |  |
| `line` | number |  |  |
| `character` | number |  |  |

### lsp.hover

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `file_path` | string |  |  |
| `line` | number |  |  |
| `character` | number |  |  |

### lsp.definition

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `file_path` | string |  |  |
| `line` | number |  |  |
| `character` | number |  |  |

### lsp.references

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `file_path` | string |  |  |
| `line` | bool |  |  |
| `character` | bool |  |  |

### lsp.symbols

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `file_path` | string |  |  |

### lsp.shutdown

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `language` | string |  |  |

```json
// → { "ok": true }
```

### lsp.shutdown_all

```json
// → { "ok": true }
```

### lsp.supported_languages

```json
// → { "languages": list }
```

### lsp.start

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `language` | string |  |  |
| `command` | string |  |  |

```json
// → { "ok": true }
```

---

## Tools

### tools.list

```json
// → { "tools": names }
```

### tool.respond

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `approved` | bool |  |  |
| `remember` | bool |  |  |

```json
// → { "ok": true, "call_id": call_id, "approved": approved, "remember": remember }
```

---

## Stats

### stats.token_summary

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `to` | string |  |  |

### stats.token_history

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `to` | string |  |  |

### stats.reset

```json
// → { "ok": true }
```

---

## Config

### config.get_vision

### config.set_vision

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `enabled` | string |  |  |

```json
// → { "ok": true }
```

### config.get_auxiliary

### config.set_auxiliary

```json
// → { "ok": true }
```

### config.get_fim

### config.set_fim

```json
// → { "ok": true }
```

### config.get_sandbox

### config.set_sandbox

```json
// → { "ok": true }
```

### config.get_defaults

```json
// → {
        "max_iterations": max_iterations,
        "max_prompt_budget": max_prompt_budget,
    }
```

### config.set_defaults

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `max_iterations` | number |  |  |
| `max_prompt_budget` | number |  |  |

```json
// → { "ok": true }
```

---

## cognitions

### cognitions.list

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `subject` | string |  |  |
| `scope` | string |  |  |
| `limit` | number |  |  |
| `offset` | number |  |  |

### cognitions.snapshots

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `cognition_id` | number |  |  |

### cognitions.subjects

### cognitions.delete

---

## completion

### completion.fim

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `model` | string |  |  |

---

## cron

### cron.list

### cron.create

### cron.delete

### cron.pause

### cron.resume

### cron.history

### cron.run_now

---

## goal

### goal.set

### goal.status

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `session_id` | string |  |  |

---

## kg

### kg.search

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `query` | string |  |  |
| `limit` | number |  |  |

### kg.stats

### kg.neighbors

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `node_name` | string |  |  |
| `limit` | string |  |  |
| `scope` | string |  |  |

### kg.walk

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `start_name` | string |  |  |
| `max_depth` | string |  |  |
| `scope` | string |  |  |
| `limit` | number |  |  |

### kg.list

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `limit` | string |  |  |
| `offset` | string |  |  |
| `scope` | string |  |  |

### kg.edges_between

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `scope` | string |  |  |

### kg.node.delete

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `name` | string |  |  |

### kg.edge.delete

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `source` | string |  |  |
| `target` | string |  |  |
| `relation` | string |  |  |

### kg.prune

---

## memory

### memory.promote

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `session_id` | string |  |  |

### memory.quality

### memory.health

### memory.persona

### memory.patterns

### memory.consolidate

### memory.forget

### memory.promote_to_layer

### memory.pipeline_status

### memory.layer_stats

### memory.vector_search

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `query` | string |  |  |
| `limit` | number |  |  |

---

## plan

### plan.create

### plan.get

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `plan_id` | string |  |  |

### plan.list

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `workspace_root` | string |  |  |

### plan.update

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `plan_id` | string |  |  |
| `status` | string |  |  |

### plan.delete

| Param | 类型 | 必需 | 说明 |
|-------|------|:---:|------|
| `plan_id` | string |  |  |

---

## todo

### todo.list

### todo.update_status

---

## vfs

### vfs.read_file

### vfs.write_file

### vfs.list_directory

### vfs.create_directory

### vfs.rename

### vfs.delete

---


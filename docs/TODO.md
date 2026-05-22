# openLoom 待办事项 & 技术债

> 上次更新: 2026-05-22

## 待实现功能

### 交互式权限确认系统

**优先级**: P2（基础设施已就位，功能可用但非交互式）

**当前状态**:
- `RiskLevel` 枚举已定义（Low / Medium / High / Forbidden）
- `classify_risk()` 风险分类器已完成，集成到 `agent_loop.rs` 的 `execute_tool()`
- `EngineEvent::PermissionRequired` 已定义在 models 中
- TUI `poll_engine_events` 已有 match arm（当前 no-op）
- WebSocket 转发已实现（ws.rs → `permission.required` JSON-RPC）
- `ApprovalOverlay` UI 组件已有完整实现（`crates/cli/src/tui/overlays/approval.rs`，dead code）

**缺失部分**:
- `execute_tool()` 不发送 `PermissionRequired` 事件，而是直接 block/allow
- 无交互式确认流程：Medium 操作应"首次询问、同类后续自动放行"，High 应"每次都询问"
- TUI 未激活 `ApprovalOverlay`

**实现思路**:
1. 在 Engine 中添加 `permission_tx: mpsc::Sender<PermissionRequest>` 和 `permission_rx` 用于双向通信
2. `execute_tool()` 对 Medium/High 操作发送 `PermissionRequired` 事件 + 一个 `oneshot::Sender<bool>` 回执
3. TUI 收到事件后弹出 `ApprovalOverlay`，用户选择后通过 oneshot channel 回传结果
4. Agent loop 阻塞等待用户确认（带 timeout）
5. 维护一个 `approved_tools: HashSet<String>` 实现 Medium 级别的"同类自动放行"

**参考**: Claude Code 的行为——shell 命令首次执行时需要用户按 Enter 确认，确认后同类命令自动执行；`rm -rf` 等始终被阻止。

---

### MCP (Model Context Protocol) 支持

**优先级**: P3（Phase 2 计划）

**描述**: openLoom 作为 MCP client 连接外部工具（文件系统、git、数据库等），类似 Claude Code 的 MCP 集成。

**当前状态**: 未开始。Skill 系统已有 trait 抽象，MCP tool 可作为一种 Skill 实现接入。

---

### 多人格切换

**优先级**: P4（Phase 3 计划）

**描述**: 用户可切换 openLoom 的工作模式/人格（coding agent / 通用助手 / 生活管理等），每种人格加载不同的 skill set 和 system prompt。

**当前状态**: Persona 系统已有认知图谱驱动的画像，但无预设多人格切换机制。

---

## 已知技术债

| 债项 | 位置 | 严重度 | 说明 |
|------|------|--------|------|
| SSE 流式发全文非逐 token | `inference/lib.rs` | LOW | LM Studio/Ollama 走 cloud client 的真实 SSE 流式，此问题仅影响 stub path |
| EventSource 未关闭 | `web/ChatArea.tsx` | MEDIUM | React 组件 unmount 时未 close |
| Heartbeat 依赖本地模型 | `engine/heartbeat.rs` | LOW | 无本地模型时心跳永不触发，可改为调 local_client |
| DiffViewer overlay 是 dead code | `tui/overlays/diff.rs` | LOW | 完整实现但从未实例化，可在 file_edit 后自动弹出 |

---

## 架构说明

### 权限模型当前行为

```
tool call → classify_risk() → 
  Forbidden → 永远阻止（即使 --dangerously-skip-permissions）
  High      → 阻止，除非 --dangerously-skip-permissions
  Medium    → 放行（未来: 首次询问）
  Low       → 放行
```

### 认知图谱分层

```
cognitions 表:
  scope = "global"           → 用户固有特征（跨项目不变）
  scope = "project:F:/xxx"   → 项目相关认知（仅在对应目录加载）

Persona 组装:
  SELECT WHERE scope = 'global' OR scope = ?project_scope
  project scope 的认知获得 +3.0 打分 bonus
```

### Agent Loop 工具链

```
用户消息 → Router 分类 → 
  简单: streaming 直接回复
  复杂: agent_loop_streaming(tx) →
    model 调用 → parse_tool_call → 
      有 tool_call: 发送 THINK/CALL marker → execute_tool (risk check) → 发送 RESULT marker → 继续循环
      无 tool_call: 自然语言回答 → break
    循环结束后无回答: final synthesis call
```

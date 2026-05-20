# openLoom 架构参考

## 架构关键决策

### Memory Pipeline 线程模型
`rusqlite::Connection` 不实现 `Send`，因此 MemoryPipeline 运行在独立 OS 线程（`std::thread::spawn`），Engine 通过 `mpsc::channel` 向其发送 `ProcessRequest`。Fire-and-forget 模式：memory 处理失败不影响用户响应。

### JSON-RPC 2.0 统一协议
Engine 与外界（CLI / Electron / WebSocket / SSE）的唯一通信协议是 JSON-RPC 2.0。15 个方法覆盖 chat、skill、memory、session、agent、cache、config、system 全部能力。Server push notification 由 `EngineEvent` 的 snake_case 变体名派生（如 `CognitionUpdated` → `cognition.updated`）。

### 请求派发流
```
用户输入 → Router.classify(text)
  ├─ 关键词匹配 (零 token, 9 条规则) → 直接命中
  ├─ 关键词未命中 + cloud 可用 → 降级云端分类
  └─ 分类结果:
       ├─ skill 匹配 + 低复杂度 → SkillRegistry.invoke() → 结果
       ├─ 需要模型 + cloud 配置 → CloudClient.complete() → LLM 响应
       └─ 需要模型 + 无 cloud → InferenceEngine.complete() → 本地模型
```

### Electron 侧车生命周期
1. Electron 主进程 spawn `openloom serve --port 0`
2. Engine 向 stdout 输出 `{"type":"ready","port":19876}` JSON 行
3. Electron 解析端口，连接 WebSocket
4. 崩溃恢复：5 次指数退避重试 (1s→2s→4s→8s→30s)
5. 优雅关闭：`before-quit` → `system.shutdown` RPC → 5s 超时 SIGKILL

### Agent Loop（已实现，原计划 Phase 2）
Engine 内置 ReAct 循环：最多 3 轮迭代，解析模型输出的 tool_call，执行对应 Skill，将结果反馈给模型。120s 超时保护。支持 TOCTOU 中断（`interrupted: Arc<AtomicBool>`）。

### 数据库迁移
使用 `refinery` crate，迁移文件在 `migrations/` 目录，由 memory crate 的 `embed_migrations!` 在 Engine 初始化时自动执行。V1 与 Phase 0 内联 DDL 字节级一致，保证升级兼容。

---

## 当前已知限制

以下功能当前为占位实现，后续 Phase 需填充：

| 严重度 | 限制 | 位置 |
|--------|------|------|
| **P0** | 本地推理是占位 — `InferenceEngine::complete()` 返回硬编码提示 | `crates/inference/src/lib.rs` |
| **P0** | 云端 streaming 未实现 — `complete_stream()` 直接 bail | `crates/inference/src/lib.rs` |
| **P1** | SSE 端点只发 `ready` 事件，无 token 流 | `crates/server/src/sse.rs` |
| **P1** | `system.shutdown` RPC 不触发实际关闭 | `crates/server/src/dispatch.rs` |
| **P1** | FTS5 全文搜索未接入 `memory.query` | `crates/server/src/dispatch.rs` |
| **P2** | Skill 权限检查跳过（数据模型已就位） | `crates/skills/src/lib.rs` |
| **P2** | KV Cache 仅 Noop 实现 | `crates/cache/src/lib.rs` |
| **P2** | 前端 SettingsPanel 硬编码 TOML，无保存 | `web/src/components/SettingsPanel.tsx` |

---



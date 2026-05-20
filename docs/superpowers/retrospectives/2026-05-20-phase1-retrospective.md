# Phase 1 复盘：Smart Router + Skill Engine

**日期:** 2026-05-20
**状态:** 复盘完成

---

## 一、总体完成度：~85%

Phase 1 计划 14 个 Task，全部 commit 已落地。当前状态：**113 测试通过，clippy 零警告，release build 成功**。但核心目标 "80% 请求不动大模型" **尚未实际验证**，因为本地推理引擎是占位实现。

---

## 二、架构达成情况

**全部 10 个 crate 已就位**，依赖图与原设计一致：

```
cli → engine + server
server → engine + models
engine → router + skills + inference + memory + models + weaver + cache
router → inference + models
skills → models
inference → models
memory → models
```

另外实际多了两个 Phase 2 提前渗入的 crate：`weaver`（ContextWeaver 提示组装）和 `cache`（KV Cache trait + NoopImpl）。

---

## 三、亮点

1. **MemoryPipeline 线程模型设计正确。** `rusqlite::Connection: !Send` 问题通过 `std::thread::spawn` + `mpsc::channel` 解决，fire-and-forget 模式保证了 memory 处理失败不影响用户响应。

2. **refinery 迁移方案稳妥。** V1 与 Phase 0 内联 DDL 字节级一致，`CREATE TABLE IF NOT EXISTS` 保证了从 Phase 0 升级的兼容性。

3. **Agent Loop 已超前实现。** 原设计将 Agent Loop 标为 Phase 2，但 engine 中已包含最多 3 轮迭代、工具调用解析、120s 超时、TOCTOU 中断保护 — 实际上 Phase 1 交付了 Phase 2 的部分核心能力。

4. **Electron 侧车生命周期完整。** spawn → JSON ready signal → 5 次指数退避重试 → `system.shutdown` 优雅关闭 → 5s SIGKILL 兜底，这条链路是生产可用的。

---

## 四、遗留问题 / 技术债

| 严重度 | 问题 | 位置 |
|--------|------|------|
| **P0** | 本地推理是占位实现 — `InferenceEngine::complete()` 返回硬编码提示文字，`complete_stream()` 是空函数 | `crates/inference/src/lib.rs` |
| **P0** | 云端 streaming 未实现 — `AnthropicClient::complete_stream()` / `OpenAIClient::complete_stream()` 直接 bail | `crates/inference/src/lib.rs` |
| **P1** | SSE 端点只发 `ready` 事件，无实际 token 流 | `crates/server/src/sse.rs` |
| **P1** | `memory.query` / `memory.events` CLI 命令返回硬编码 "Phase 2" | `crates/cli/src/main.rs` |
| **P1** | `system.shutdown` RPC 返回 `{"ok": true}` 但不触发实际关闭 | `crates/server/src/dispatch.rs` |
| **P1** | FTS5 全文搜索未接入 JSON-RPC `memory.query` | `crates/server/src/dispatch.rs` |
| **P2** | `SettingsPanel` 是纯展示组件，TOML 硬编码，无保存功能 | `web/src/components/SettingsPanel.tsx` |
| **P2** | `SkillPermissions` 数据模型存在但权限检查完全跳过 | `crates/skills/src/lib.rs` |
| **P2** | `CognitionStore::insert()` 列清单缺 `source` 列（V3 迁移加了默认值所以能跑） | `crates/memory/src/store.rs` |
| **P2** | `preload.js` 的 `subscribe()` 是 `console.log` 空实现 | `electron/preload.js` |
| **P3** | KV Cache 只有 Noop 实现，无实际前缀缓存 | `crates/cache/src/lib.rs` |
| **P3** | 8 个 `unwrap()` / `expect()` 调用散布在 engine 和 cli 中 | engine 803 行, cli 456 行 |

---

## 五、与原设计的关键偏差

1. **Cloud 路径提前实现。** 设计 spec 明确说 "Phase 1 不支持云端模型，Phase 2 引入"。实际代码中 `AnthropicClient` + `OpenAIClient` 已完整实现（同步 complete），`TargetModel::Cloud` 已在 Router 中使用。这是合理的超前 — 没有云端模型，"80% 请求不动大模型" 的对立面（20% 需要大模型）就无路可走。

2. **Agent Loop 提前。** 同上，设计标 Phase 2，代码已在 engine 中。

3. **weaver / cache crate 不在原设计 scope 中。** 属于 Phase 2 渗入，但已集成到 engine 管线中。

4. **`TargetModel::None` 被标记为 unreachable。** 原设计中 `None` 表示 skill 直接处理、不调模型，但代码中 engine 将其视为 "不应到达" 的分支 — 说明实际派发逻辑与设计流图有偏差。

---

## 六、经验教训

1. **"先占位后填充"策略有效，但容易积累假绿色。** 113 测试全过看起来很健康，但核心推理路径是假的。占位实现应该让测试明确标记为 `#[ignore]` 或返回可区分的错误类型，而不是静默返回假数据。

2. **Phase 边界模糊是好事。** Cloud 路径和 Agent Loop 的提前实现避免了 Phase 1 的 "死代码" — 如果严格按设计只做本地推理，engine 的派发分支会有一半是 `unimplemented!()`。

3. **Electron + React 子系统与 Rust engine 完全解耦，验证了设计。** 两套代码独立开发、独立测试，JSON-RPC 2.0 作为唯一接触面。

4. **Implementation plan 的 14 task 粒度偏大。** 每个 task 实际包含 5-10 个 step，导致单 task commit 变更量过大（最大一个 commit 跨越 800+ 行）。Task 2（inference）如果拆成 "GGUF 加载" / "GPU 检测" / "Token 计数" 三个独立 task，开发节奏会更均匀。

5. **内存管线（MemoryPipeline）的测试覆盖不足。** 集成测试验证了 "不阻塞"，但没有验证 "认知正确聚合" — V3 migration 的 source 列缺漏就是信号。

---

## 七、对 Phase 2 的影响

Phase 2 的当务之急是**填 P0/P1 坑**，否则 Phase 1 的架构成果无法实际运行：

- **P0 阻塞项：** 本地模型加载 + 云端 streaming → 没有这两个，end-to-end 对话流跑不通
- **P1 阻塞项：** SSE token streaming + `system.shutdown` + FTS5 搜索 → 前端无法展示真实流式响应
- **建议：** Phase 2 的前 3-4 个 task 应集中填这些坑，再继续新功能开发

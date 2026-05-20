# Phase 2 复盘

**日期:** 2026-05-20
**范围:** Milestones A–D, 33 tasks, ~30 commits

---

## 1. 数据

| 指标 | 数值 |
|------|------|
| 里程碑 | A (8 tasks) + B (7 tasks) + C (12 tasks) + D (6 tasks) = **33 tasks** |
| 提交数 | ~30 commits |
| 测试数 | 113 → **127** (+14) |
| Clippy | 全程零警告 |
| 审计轮次 | 4 次 subagent audit，发现并修复 ~12 个问题 |
| 新增文件 | ~15 个（类型/缓存/存储/前端组件/配置/设计文档） |

---

## 2. Timeline

```
Milestone A: May 18-19   CloudClient, ContextWeaver, SessionStore, KV stub
Milestone B: May 19      Agent Loop, Persona Projector, Message History
Milestone C: May 20      后端全量补齐（22 gaps: 类型/JSON-RPC/CLI/WebSocket push/运维）
Milestone D: May 20      Electron + 前端（preload 重写/托盘/CSP/React 6 组件全连线）
```

C 和 D 同一天完成——subagent-driven 执行模式叠加完整的 spec + plan，吞吐量远超逐行手写。

---

## 3. 做得好的

### 3.1 Subagent-driven 执行
- **33 tasks 全部通过 subagent 派发完成**，主 agent 只做协调和 review
- 关键瓶颈 task（Engine 7 新字段）通过提供完整代码到 plan 中顺利执行
- 一个 task 一个 commit，粒度清晰可回溯

### 3.2 审计驱动的质量保障
- 每进入新 milestone 前先 subagent audit 发现缺口
- Milestone C 前发现 ~50 gap，Milestone D 前发现 4 gap
- 修复后在下一个 milestone 的 plan 中补齐，形成闭环

### 3.3 Plan 即代码
- Plan 不再是指南而是**可执行代码模板**——每个 step 给出完整代码块
- 实施者 subagent 无需设计决策，只需按 plan 执行+测试+提交
- 消除 "implement later" 模糊占位，实施过程零回退

### 3.4 跨语言协调
- Rust 后端 + TypeScript 前端 + JavaScript Electron 在同一天完成集成
- JSON-RPC 2.0 作为通信契约使前后端解耦——后端 Milestone C 全量实装后，前端 Milestone D 只需连线

---

## 4. 做错了的

### 4.1 Plan 中的编译错误（3 个 Critical）
Milestone C plan 初稿有 3 个会导致编译失败的 bug：
- `#[serde(default)]` 被误认为能解决 Rust struct literal 构造（实际只影响 JSON 反序列化）
- `EngineEvent::TokenUsage` 忘记同步加新字段
- `interruptible` 标志位放错执行位置

**教训:** Plan 的 subagent audit 必须在**实现前**而非实现后执行。Milestone C 的 audit 在 plan 写完后、实现前做了一轮，但仍漏掉 3 个 Critical——说明 audit 指令需要更具体，明确要求检查"这份 plan 的代码如果原样执行会不会编译失败"。

### 4.2 Worktree commit 分叉
Task 2 和 Task 5 的 subagent 使用 worktree isolation，提交到了 worktree 分支而非 main，导致需要手动 cherry-pick。Task 1/3/4 的 subagent 直接提交到 main 是正确的。

**教训:** 对无冲突的独立文件 task，应避免 worktree isolation。Worktree 适合需要完全隔离（可能冲突）的实验性 task。

### 4.3 前端覆盖不完整
Phase 2 原计划包含 "完整 Electron GUI + 认知画像可视化"，但直到 Milestone D 才开始做。导致 Milestone A-C 期间没有任何前端反馈来验证后端 API。

**教训:** 应该在 Milestone A 就做最小前端连线（1-2 个页面），然后每个 milestone 都在前端验证。后端→前端的瀑布模式导致了 3 个 milestone 的反馈真空。

---

## 5. 架构决策回顾

### 5.1 JSON-RPC 2.0 作为唯一通信协议 ✅ 正确
- 后端和前端完全解耦，Milestone C 全量实装 JSON-RPC 方法后前端只需连线
- WebSocket 持久连接 + notification push 的方案被 subagent 调研确认为 VS Code 同款成熟模式

### 5.2 规则引擎优先于 LLM ✅ 正确
- 路由/认知提取用纯规则引擎，Phase 2 没有引入 LLM 依赖
- 127 个测试全部在纯 CPU 环境运行，无需 GPU
- llama-cpp 保持 stub 状态是正确的——模型分发方案需要 Phase 3 的统一规划

### 5.3 Engine 单文件过度膨胀 ⚠️ 需关注
- `crates/engine/src/lib.rs` 已增至 ~730 行，承载了限流/配置/会话/记忆/agent loop/shutdown 等 10+ 关注点
- Phase 3 应考虑拆分为 engine-core / engine-rate-limiter / engine-session 等子模块

---

## 6. Phase 3 建议

| 优先级 | 事项 | 理由 |
|--------|------|------|
| P0 | llama-cpp 真实模型加载 | 当前所有推理返回 "[openLoom] Local model is not yet loaded..." stub |
| P0 | Engine lib.rs 拆分 | 单文件已超过 700 行，10+ 关注点 |
| P1 | SSE 流式 token | 前端 + 后端 SSE stub 都已就绪，差真实实现 |
| P1 | KV Cache Q4 safetensors 块池 | Phase 2 trait 契约已定义，Phase 3 填入实现 |
| P1 | 安全沙箱 (Seatbelt/Landlock/RestrictedTokens) | spec 已设计，代码零实现 |
| P2 | 认知审核面板 | PersonaPanel 已做，差编辑/回滚/历史版本 |
| P2 | 跨平台打包 | Electron 基础就绪，差构建脚本 |

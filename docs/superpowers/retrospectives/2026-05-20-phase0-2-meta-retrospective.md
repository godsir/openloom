# Phase 0–2 综合复盘

**日期:** 2026-05-20
**范围:** Phase 0, Phase 1, Phase 2 (Milestones A–D)
**数据源:** 三份复盘文档 + git log

---

## 1. 全局数据

| 指标 | Phase 0 | Phase 1 | Phase 2 | 总计 |
|------|---------|---------|---------|------|
| 任务数 | 9 | 14 | 33 | **56** |
| 提交数 | 13 | ~20 | ~30 | **~63** |
| 测试数 | 36 | 113 | 127 | **127** |
| Crate 数 | 3 | 10 | 10 | **10** |
| 里程碑 | 1 | 1 | 4 (A/B/C/D) | **6** |
| 工时估算 | 2-3天 | 3-5天 | 4-6天 | **~12天** |

**测试增长曲线：**

```
Phase 0:  ████████████████████ 36
Phase 1:  ██████████████████████████████████████████████████████████████ 113
Phase 2:  ██████████████████████████████████████████████████████████████████████ 127
```

Phase 0→1 翻 3 倍（基础设施爆发），Phase 1→2 增长 12%（核心已稳定，增量在功能完整性）。

---

## 2. 架构演化

```
Phase 0 (3 crates)              Phase 1 (10 crates)              Phase 2 (10 crates, 功能填满)
┌─────────┐                     ┌─────────┐                     ┌─────────┐
│ memory  │                     │ memory  │                     │ memory  │ ← +Persona +MessageStore +EventRow
│ models  │                     │ models  │                     │ models  │ ← +Config/CachePrefs/AgentState
│ cli     │                     │ cli     │                     │ cli     │ ← +live data +signal
└─────────┘                     │ engine  │                     │ engine  │ ← +rate_limit +shutdown +config
                                │ router  │                     │ router  │ ← +route_reason
                                │ skills  │                     │ skills  │
                                │ inference│                    │ inference│ ← +top_p/stop/latency_ms
                                │ server  │                     │ server  │ ← +push +port/pid
                                │ weaver  │                     │ weaver  │ ← +cache accessor
                                │ cache   │                     │ cache   │ ← +stats
                                └─────────┘                     │ electron│ ← +tray +CSP +health
                                                                │ web     │ ← +6 components live
                                                                └─────────┘
```

核心架构决策——Event → Pattern → Cognition 管线、JSON-RPC 2.0 通信、Electron sidecar 生命周期——**在 Phase 0 确定后从未被推翻**，贯穿全部三个 Phase。

---

## 3. 跨 Phase 模式

### 3.1 "先占位后填充" (Stub-then-Real)

| 占位 | 引入 Phase | 填实 Phase | 当前状态 |
|------|-----------|-----------|---------|
| InferenceEngine (返回提示文字) | 1 | ❌ 未填 | **P0 阻塞** |
| NoopCache (KvCache trait) | 1 | ❌ 未填 | Phase 3 |
| SSE streaming | 1 | ❌ 未填 | Phase 3 |
| preload subscribe() | 1 | 2-D | ✅ 已填 |
| JSON-RPC stubs (6个) | 1 | 2-C | ✅ 已填 |
| CLI stubs (6个) | 1 | 2-C | ✅ 已填 |
| PersonaProvider (Noop→Cognitions) | 2-A | 2-B | ✅ 已填 |

**结论：** 该模式在 4/7 项上成功（Phase 2 填了 Phase 1 的大部分坑）。剩余 3 项（推理引擎、KV Cache、SSE）因依赖外部模型/硬件而推迟到 Phase 3。

### 3.2 Phase 边界渗透

Phase 1 提前实现了 Phase 2 的 Cloud 路径和 Agent Loop，避免了 "一半派发分支是 `unimplemented!()`" 的死代码问题。Phase 2 提前实现了 Phase 3 的 CacheStats trait 契约。**渗透是单向的（提前做后续的事），且有益（减少返工）。**

### 3.3 测试覆盖率阶段性特征

- Phase 0：每个模块 `#[cfg(test)]` 高覆盖，36 tests
- Phase 1：集成测试大幅增加（pipeline e2e、session thread、cloud client），77 新增
- Phase 2：增量 14 tests，**质量重于数量**——修复了 AgentState 枚举化、in_flight 泄漏、中断保护等边界问题

---

## 4. 跨 Phase 技术债

| 债项 | 首次出现 | 严重度 | 状态 |
|------|---------|--------|------|
| llama-cpp 真实模型加载 | Phase 1 | P0 | 未解决 |
| 云端 streaming | Phase 1 | P0 | 未解决 |
| PatternAggregator 无滑动窗口 | Phase 0 | P1 | 未解决 |
| Engine lib.rs 单文件 730+ 行 | Phase 2 | P1 | 未解决 |
| KV Cache 磁盘持久化 | Phase 1 | P2 | Phase 3 |
| 规则引擎仅覆盖中文金融 | Phase 0 | P2 | 未解决 |
| FTS5 搜索已接入但未充分测试 | Phase 2 | P2 | 未解决 |

**趋势：** P0/P1 技术债集中在 "需要真实模型" 这一条阻塞上——解开它，SSE streaming、云端推理、认知提取全部解锁。

---

## 5. 过程改进轨迹

| 改进 | 引入 Phase | 效果 |
|------|-----------|------|
| TDD 强制执行 | 0 | 0 返工，127 tests 零回归 |
| refinery 迁移方案 | 1 | Phase 0→1→2 schema 无缝升级 |
| 审计驱动的 subagent audit | 2 | 每 milestone 前发现 4-50 个缺口 |
| Plan 即代码（完整代码块） | 2 | 实施者零设计决策，零回退 |
| Worktree isolation | 2-D | 部分 task 分叉（需改进） |
| 跨语言 JSON-RPC 解耦 | 1 | 后端 C 完成后，前端 D 只需连线 |

**最有效的改进：** Plan 即代码 + subagent audit。Phase 2 的 33 tasks 全部通过 subagent 完成，主 agent 只做协调。

**最需要改进的：** 前端反馈时机。Phase 2 的前端工作集中在最后一个 milestone(D)，导致 A-C 期间后端 API 无前端验证。

---

## 6. 综合教训

### 6.1 结构层面

1. **Phase 边界应该是 "检查点" 而非 "隔离墙"。** Phase 1→2 的渗透（Cloud/Agent Loop 提前）避免了死代码，是正确的。

2. **Crate 数量应在 Phase 1 稳定。** 从 3→10→10 的曲线说明 Phase 1 做了正确的模块拆分决策，Phase 2 只是在填实现。

3. **前后端应该并行而非瀑布。** 三个 Phase 的后端工作完成后才做前端，导致 API 设计在前端使用中才被验证。

### 6.2 执行层面

4. **Plan audit 必须包含 "编译可行性" 检查。** Phase 2-C plan 初稿有 3 个编译错误，因为 audit 指令不够具体。

5. **Worktree 适合实验性分支，不适合确定性 task。** 独立文件的确定性 task 应直接提交到主分支。

6. **"假绿色" 测试是债务。** InferenceEngine 返回硬编码文本但仍通过测试——这类占位实现应标记 `#[ignore]` 或返回可区分的错误类型。

### 6.3 设计层面

7. **JSON-RPC 2.0 作为唯一通信协议是正确的。** 三个阶段的后端→前端集成在 D 阶段只花了一天，因为契约早已稳定。

8. **Event → Pattern → Cognition 的认知管线设计是稳定的。** Phase 0 确定的架构在 Phase 1+2 中被完整保留和扩展，没有打破性变更。

---

## 7. Phase 3 优先级矩阵

```
                    紧急              不紧急
重要    ┌──────────────────┬──────────────────┐
        │ P0: 模型加载     │ P1: Engine 拆分  │
        │ P0: 云端 streaming│ P1: KV Cache     │
        │                  │ P1: 安全沙箱     │
        ├──────────────────┼──────────────────┤
        │ P1: 滑动窗口     │ P2: 规则扩展     │
不重要  │ P1: SSE streaming │ P2: 认知审核面板 │
        │                  │ P2: 跨平台打包   │
        └──────────────────┴──────────────────┘
```

**Phase 3 核心命题：** 解开 "真实模型" 这一个阻塞项，解锁 SSE/云端推理/认知提取一整条链。

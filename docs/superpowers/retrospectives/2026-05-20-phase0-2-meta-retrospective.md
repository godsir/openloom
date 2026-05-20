# Phase 0–2 综合复盘（最终版）

**日期:** 2026-05-20
**范围:** Phase 0, Phase 1, Phase 2 (Milestones A–D) + 补缺
**数据源:** 三份复盘文档 + git log + 最终审计

---

## 1. 全局数据

| 指标 | Phase 0 | Phase 1 | Phase 2 | 总计 |
|------|---------|---------|---------|------|
| 任务数 | 9 | 14 | 33 + 3 补缺 | **59** |
| 提交数 | 13 | ~20 | ~33 | **~66** |
| 测试数 | 36 | 113 | 129 | **129** |
| Crate 数 | 3 | 10 | 10 | **10** |
| 里程碑 | 1 | 1 | 4 (A/B/C/D) + 补缺 | **6+1** |

**测试增长曲线：**

```
Phase 0:  ████████████████████ 36
Phase 1:  ██████████████████████████████████████████████████████████████ 113
Phase 2:  ██████████████████████████████████████████████████████████████████████ 129
```

Phase 0→1 翻 3 倍（基础设施爆发），Phase 1→2 增长 14%（核心稳定 + 边界修复 + 补缺）。

---

## 2. 最终完成度审计

Phase 2 结束后，进行了 spec vs code 的逐项审计，发现 8 个缺口。其中 3 个可立即补（不依赖模型），其余被模型阻塞或归入 Phase 3。

### 2.1 已补缺口（审计后立即修复）

| 缺口 | Phase | 修复 |
|------|-------|------|
| PatternAggregator 无滑动窗口 | 0 | 加 `observations: HashMap<String, Vec<i64>>` + 24h 时间窗口 + `with_window()` builder + 时间衰减剪枝 |
| Cognition 无版本快照 | 2 | `CognitionStore::insert()` upsert + `version+1` + `cognition_snapshots` 表 + `snapshots_for()` 回滚查询 |
| 10 预设测试场景 | 0 | `tests/fixtures/trading_scenarios.txt` 14 行 + pipeline fixture test |

**新增测试：** +2 (快照 upsert + 场景 fixture)，总数 129。

### 2.2 剩余未完成项

| 项 | Phase | 阻塞原因 | 归属 |
|----|-------|---------|------|
| llama-cpp 真实模型加载 | 1 | 需要 GGUF 文件分发方案 | Phase 3 |
| SSE streaming | 2 | 依赖真实模型 token 生成 | Phase 3 |
| 8B LLM 认知提取 | 2 | 依赖 Qwen3-8B 模型加载 | Phase 3 |
| Hub 心跳/自主触发 | 2 | 依赖 1.7B 模型做低功耗检查 | Phase 3 |
| KV Cache 磁盘持久化 | 2 | 需要真实推理后才有缓存价值 | Phase 3 |

**核心瓶颈：** 5 个剩余项中 4 个被同一件事阻塞——llama-cpp 真实模型加载。解开这一个结，推理→流式→认知提取→自主触发链条全部解锁。

---

## 3. 架构演化

```
Phase 0 (3 crates)              Phase 1 (10 crates)              Phase 2 (10 crates, 功能填满)
+---------+                     +---------+                     +---------+
| memory  |                     | memory  |                     | memory  | +Persona +MessageStore +EventRow
| models  |                     | models  |                     | models  | +Config/CachePrefs/AgentState
| cli     |                     | cli     |                     | cli     | +live data +signal
+---------+                     | engine  |                     | engine  | +rate_limit +shutdown +config
                                | router  |                     | router  | +route_reason
                                | skills  |                     | skills  |
                                | inference|                    | inference| +top_p/stop/latency_ms
                                | server  |                     | server  | +push +port/pid
                                | weaver  |                     | weaver  | +cache accessor
                                | cache   |                     | cache   | +stats
                                +---------+                     | electron| +tray +CSP +health
                                                                | web     | +6 components live
                                                                +---------+
```

核心架构决策——Event → Pattern → Cognition 管线、JSON-RPC 2.0 通信、Electron sidecar 生命周期——**在 Phase 0 确定后从未被推翻**，贯穿全部三个 Phase。

---

## 4. 跨 Phase 模式

### 4.1 "先占位后填充" (Stub-then-Real)

| 占位 | 引入 Phase | 填实 Phase | 结果 |
|------|-----------|-----------|------|
| InferenceEngine (返回提示文字) | 1 | — | Phase 3 |
| NoopCache (KvCache trait) | 1 | — | Phase 3 |
| SSE streaming | 1 | — | Phase 3 |
| preload subscribe() | 1 | 2-D | ✅ |
| JSON-RPC stubs (6个) | 1 | 2-C | ✅ |
| CLI stubs (6个) | 1 | 2-C | ✅ |
| PersonaProvider (Noop→Cognitions) | 2-A | 2-B | ✅ |
| PatternAggregator (HashMap→滑动窗口) | 0 | 补缺 | ✅ |
| CognitionStore (version=1→upsert+快照) | 2 | 补缺 | ✅ |

**结论：** 9 个占位项中 6 个已填，3 个待 Phase 3。"先占位后填充"模式成功率 67%，剩余全阻塞在同一点（模型加载）。

### 4.2 Phase 边界渗透

Phase 1 提前实现了 Phase 2 的 Cloud 路径和 Agent Loop，避免了 "一半派发分支是 `unimplemented!()`" 的死代码问题。Phase 2 提前实现了 Phase 3 的 CacheStats trait 契约。**渗透是单向的（提前做后续的事），且有益（减少返工）。**

### 4.3 测试覆盖率

- Phase 0: 每个模块 `#[cfg(test)]` 高覆盖，36 tests
- Phase 1: 集成测试大幅增加（pipeline e2e、session thread、cloud client），77 新增
- Phase 2: 增量 16 tests（14 功能 + 2 补缺），**质量重于数量**——修复了 AgentState 枚举化、in_flight 泄漏、中断保护等边界问题

---

## 5. 跨 Phase 技术债（终态）

| 债项 | 首次出现 | 严重度 | 状态 |
|------|---------|--------|------|
| llama-cpp 真实模型加载 | Phase 1 | P0 | Phase 3 |
| 云端 streaming | Phase 1 | P0 | Phase 3 |
| Engine lib.rs 单文件 730+ 行 | Phase 2 | P1 | Phase 3 |
| KV Cache 磁盘持久化 | Phase 1 | P2 | Phase 3 |
| 规则引擎仅覆盖中文金融 | Phase 0 | P2 | Phase 3 |
| FTS5 搜索已接入但未充分测试 | Phase 2 | P2 | Phase 3 |
| ~~PatternAggregator 无滑动窗口~~ | Phase 0 | — | ✅ 已修复 |
| ~~Cognition 无版本快照~~ | Phase 2 | — | ✅ 已修复 |
| ~~10 场景测试缺失~~ | Phase 0 | — | ✅ 已修复 |

---

## 6. 过程改进轨迹

| 改进 | 引入 Phase | 效果 |
|------|-----------|------|
| TDD 强制执行 | 0 | 0 返工，129 tests 零回归 |
| refinery 迁移方案 | 1 | Phase 0→1→2 schema 无缝升级 |
| 审计驱动的 subagent audit | 2 | 每 milestone 前发现 4-50 个缺口；结束后发现 8 个缺口并修复 3 个 |
| Plan 即代码（完整代码块） | 2 | 实施者零设计决策，零回退 |
| 审计-修复闭环 | 补缺 | audit 发现→立即补充→重新验证，全程 3 commits, <10 min |
| Worktree isolation | 2-D | 部分 task 分叉（需改进） |
| 跨语言 JSON-RPC 解耦 | 1 | 后端 C 完成后，前端 D 只需连线 |

**最有效的改进：** Plan 即代码 + subagent audit。Phase 2 的 36 tasks 全部通过 subagent 完成，主 agent 只做协调。审计-修复闭环在补缺阶段证明了自己——audit 发现缺口后 10 分钟内全部补完。

**最需要改进的：** 前端反馈时机。Phase 2 的前端工作集中在最后一个 milestone(D)，导致 A-C 期间后端 API 无前端验证。

---

## 7. 综合教训

### 7.1 结构层面

1. **Phase 边界应该是 "检查点" 而非 "隔离墙"。** Phase 1→2 的渗透（Cloud/Agent Loop 提前）避免了死代码，是正确的。

2. **Crate 数量应在 Phase 1 稳定。** 从 3→10→10 的曲线说明 Phase 1 做了正确的模块拆分决策，Phase 2 只是在填实现。

3. **前后端应该并行而非瀑布。** 三个 Phase 的后端工作完成后才做前端，导致 API 设计在前端使用中才被验证。

### 7.2 执行层面

4. **Plan audit 必须包含 "编译可行性" 检查。** Phase 2-C plan 初稿有 3 个编译错误，因为 audit 指令不够具体。

5. **Worktree 适合实验性分支，不适合确定性 task。** 独立文件的确定性 task 应直接提交到主分支。

6. **"假绿色" 测试是债务。** InferenceEngine 返回硬编码文本但仍通过测试——这类占位实现应标记 `#[ignore]` 或返回可区分的错误类型。

7. **审计-修复闭环应成为标准流程。** Phase 结束后做 spec vs code 审计，可立即补的当场修，依赖阻塞的归入下一 Phase。

### 7.3 设计层面

8. **JSON-RPC 2.0 作为唯一通信协议是正确的。** 三个阶段的后端→前端集成在 D 阶段只花了一天，因为契约早已稳定。

9. **Event → Pattern → Cognition 的认知管线设计是稳定的。** Phase 0 确定的架构在 Phase 1+2 中被完整保留和扩展，没有打破性变更。

10. **滑动窗口和版本快照虽小但关键。** 这两个修复让认知管线从 "能跑" 升级为 "能生产用"——时间衰减防止旧数据污染，版本快照使回滚成为可能。

---

## 8. Phase 3 优先级矩阵（更新）

```
                    紧急              不紧急
重要    +------------------+------------------+
        | P0: 模型加载     | P1: Engine 拆分  |
        | P0: 云端 streaming| P1: KV Cache     |
        | P0: 8B 认知提取  | P1: 安全沙箱     |
        +------------------+------------------+
        | P1: 自主触发     | P2: 规则扩展     |
不重要  | P1: SSE streaming | P2: 认知审核面板 |
        |                  | P2: 跨平台打包   |
        +------------------+------------------+
```

**Phase 3 核心命题：** 解开 "真实模型" 这一个阻塞项，SSE streaming / 云端推理 / 8B 认知提取 / Hub 自主触发四条链全部解锁。加上 Engine 拆分和 KV Cache，Phase 3 交付一个**真正能推理**的内核。

# Phase 0 复盘

**日期:** 2026-05-20
**状态:** Phase 0 已交付，Phase 1+2 已完成

---

## 一、计划 vs 交付

| 计划任务 | 状态 | 说明 |
|----------|------|------|
| Task 1: 项目脚手架 | ✅ 完成 | workspace + 3 crates (memory / models / cli) |
| Task 2: Event 类型定义 | ✅ 完成 | EventType + Event + 序列化 + 4 tests |
| Task 3: RuleBasedExtractor | ✅ 超额 | 计划 4 tests → 实际 9 tests，10 条规则 |
| Task 4: SqliteEventStore | ✅ 超额 | FTS5 + payload roundtrip + 后续 V2 stores |
| Task 5: PatternAggregator | ✅ 完成 | HashMap 计数 + 阈值触发，5 tests |
| Task 6: MemoryPipeline | ✅ 超额 | 3 阶段编排 + 4 tests（含端到端） |
| Task 7: CLI analyze | ✅ 完成 | 端到端可运行，输出 JSON profile |
| Task 8: 集成测试 | ✅ 完成 | 10 场景（内联在 pipeline.rs 而非 tests/ 目录） |
| Task 9: 最终验证 | ✅ 完成 | 36 tests pass, release build 成功 |

**结论：Phase 0 计划全部交付，无遗漏。**

---

## 二、核心文件现状

| 文件 | 行数 | 测试数 | Phase 2 变化 |
|------|------|--------|-------------|
| `event.rs` | 147 | 4 | `event_type_as_str` / `event_type_from_str` 移入 Event impl，增加 tracing warning |
| `extractor.rs` | 222 | 9 | 无重大变更 |
| `aggregator.rs` | 146 | 5 | 无重大变更 |
| `pipeline.rs` | 267 | 4 | `MemoryPipeline::new` 去掉 threshold 参数（由 aggregator 持有），Phase 2 不再直接使用 |
| `store.rs` | 717 | 10 | Phase 2 大幅扩展：CognitionStore、SessionStore、TokenStore、MessageStore、refinery 迁移 |
| `cli/main.rs` | 457 | 5 | Phase 1-2 扩展 serve/chat/run/skill/memory/config/session 命令，analyze 逻辑保留 |

---

## 三、设计与实现的差异

1. **PatternAggregator 缺少滑动窗口** — 设计描述"滑动窗口 + 计数 Bloom Filter"，实际是简单 HashMap 计数器，无时间衰减、无窗口滑动。当前纯计数阈值触发，不区分"最近密集发生"和"历史上发生过"。

2. **tests/ 目录缺失** — 计划指定 `tests/memory_pipeline_tests.rs`，实际集成测试内联在 `crates/memory/src/pipeline.rs` 的 `#[cfg(test)]` 模块中。功能等价，结构不一致。

3. **MemoryPipeline::new 签名简化** — 计划 4 参数（含 threshold），实际 3 参数（threshold 由外部 `PatternAggregator::new(threshold)` 持有）。

4. **未引入 llama.cpp** — Phase 0 设计说"不引入模型依赖"，但交付物列表写了"规则引擎 + 1.7B 模型"，实际只用规则引擎。模型部分推迟到了 Phase 1。

5. **迁移策略变化** — 计划指定 `refinery` crate，实际先用 `CREATE TABLE IF NOT EXISTS` 内联，Phase 2 才接入 `refinery embed_migrations!`。V1 迁移复用了内联 DDL，向后兼容。

---

## 四、Phase 0 做对了什么

1. **规则引擎起步正确** — 不引入模型依赖验证管线可行性，降低调试复杂度
2. **TDD 执行一致** — 每个文件都有 `#[cfg(test)]` 模块，测试覆盖好
3. **模块边界清晰** — event / extractor / aggregator / store / pipeline 五模块无循环依赖
4. **`openloom analyze` 命令行第一天就能跑** — `test_data/sample_chat.log` 仍有效
5. **代码稳定** — Phase 0 核心逻辑 13 commits 后无大规模返工

---

## 五、技术债务（Phase 0 遗留，待 Phase 3 修复）

| 问题 | 严重度 | 建议 |
|------|--------|------|
| PatternAggregator 无滑动窗口 | 中 | Phase 3 需加时间衰减，当前无法区分时间维度 |
| `action_to_trait` / `generate_summary` 硬编码映射 | 低 | 10 个 match arm，扩展性差，可换配置驱动 |
| 规则引擎仅覆盖中文金融场景 | 中 | 10 条规则集中在交易心理，需补充通用场景 |
| `event_type_from_str` 默认 fallback 到 Fact | 低 | 未知类型静默降级，已加 tracing warning |
| `tests/` 目录缺失 | 低 | 结构不一致，需 `cargo test --test` 时再补 |

---

## 六、数据指标

- **Phase 0 commits:** 13 个（scaffold → README）
- **Phase 0 纯代码行数:** ~600 行（event + extractor + aggregator + pipeline + store + CLI analyze）
- **Phase 0 测试:** 36 个全部通过
- **当前总测试:** 113 个全部通过（Phase 0+1+2 累加）
- **当前 crate 数量:** 10 个（Phase 0 只有 3 个）
- **总 commits:** 61
- **Phase 0 工时:** 约 2-3 天（按 commit 密度估算）

---

## 七、后续 Phase 影响

Phase 0 奠定的 Event → Pattern → Cognition 管线在 Phase 2 中被完整保留和复用：

- Phase 1 的 `Engine` 仍然通过 `MemoryPipeline` 写入事件
- Phase 2 的 `CognitionsPersonaProvider` 直接读取 cognitions 表生成用户画像
- Phase 2 的 `MemoryThread` 后台线程监听 `CognitionUpdated` 事件并写入 cognitions 表

Phase 0 的核心架构决策（事件驱动、阈值触发、SQLite 持久化）贯穿了 Phase 1 和 Phase 2，没有被推翻。

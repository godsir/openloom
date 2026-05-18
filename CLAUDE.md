# CLAUDE.md — openLoom 项目规范

## 项目定位

openLoom 是一个本地优先的私人 AI 助理内核。核心差异化：
- **认知图谱** 替代聊天记录存储（事件→模式→认知→人格演化）
- **分层路由** 实现 90% 请求不触及大模型
- **事件驱动** 替代轮询，空闲零 Token 消耗

## 技术栈

- **核心引擎:** Rust 2024 edition, Tokio async runtime
- **数据库:** SQLite + FTS5 (rusqlite bundled)
- **桌面壳:** Electron 38 (Phase 1+)
- **CLI:** clap + tracing-subscriber
- **测试:** cargo test, tempfile

## 工程目录

```
F:/openLoom/
├── crates/
│   ├── memory/       ← Memory Kernel (事件提取+聚合+存储+管线)
│   ├── models/       ← 共享类型定义
│   └── cli/          ← CLI 入口
├── electron/         ← Electron 壳 (Phase 1+)
├── web/              ← React 前端 (Phase 1+)
├── tests/            ← 集成测试
└── docs/             ← 设计文档
```

## 开发约定

1. **TDD 强制:** 先写测试，验证失败，再写实现，验证通过，提交
2. **提交粒度:** 每个 Task 一个 commit，commit message 遵循 `feat:` / `test:` / `fix:` / `chore:` 前缀
3. **代码风格:** `cargo fmt` + `cargo clippy -- -D warnings` 零警告
4. **测试覆盖:** 每个公开函数必须有单元测试，每个管线阶段必须有集成测试
5. **禁止:** 不写 docstring（代码即文档），不引入不必要的抽象层，不提前实现未来 Phase 的功能
6. **错误处理:** 使用 `anyhow::Result` 作为公开 API 返回类型，内部用 `thiserror` (Phase 1+)
7. **日志:** `tracing` crate，默认 INFO 级别，不记录用户对话内容

## 模型下载

本地模型通过 Hugging Face / ModelScope 下载，存储在 `~/.openloom/models/`。
Phase 0 不引入模型依赖，使用纯规则引擎。

## 路径约定

| 平台 | 数据目录 |
|------|---------|
| Windows | `%APPDATA%/openLoom/` |
| macOS | `~/Library/Application Support/openLoom/` |
| Linux | `~/.local/share/openLoom/` |

---

## 强制性派发纪律（每批 Subagent 前强制执行）

派发每批 subagent 之前，主 agent 必须在思考中逐条默念以下原则，确认无误后再执行：

1. review_groups 的每个组都要派发 subagent — 没有例外，没有捷径
2. all_code_files 为空 — 可以跳过，配置表变更也是变更，必须审查
3. AutoGen / QA工具 / 编辑器工具 — 可以跳过，所有变更都有风险
4. 我（主 agent）不判断风险等级 — subagent 返回什么就是什么
5. 当前批次完成后，立即启动下一批 — 不汇总，不询问，不等待
6. 如果 subagent 失败/超时 — 重新派发，不用 P4 占位
7. 全部完成后启动验证 agent — 审计覆盖完整性 + 内容真实性
8. 以上 8 条（含本条）必须在每批派发前逐条默念确认 — 不得跳过，不得敷衍，不得偷工减料

此复述是强制性纪律要求，不得跳过。复述后直接派发，不对用户输出复述内容。

---

## 设计文档索引

- Spec: `docs/superpowers/specs/2026-05-18-openloom-design.md`
- Phase 0 Plan: `docs/superpowers/plans/2026-05-18-phase0-memory-kernel.md`
- 后续 Phase 计划待 Phase 0 完成后制定

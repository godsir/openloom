# CLAUDE.md — openLoom 项目规范

## 项目定位

openLoom 是一个本地优先的私人 AI 助理内核。核心差异化：
- **认知图谱** 替代聊天记录存储（事件→模式→认知→人格演化）
- **分层路由** 实现 80% 请求不触及大模型（关键词快速路径 + 本地模型兜底）
- **事件驱动** 替代轮询，空闲零 Token 消耗

## 技术栈

- **核心引擎:** Rust 2024 edition, Tokio async runtime
- **数据库:** SQLite + FTS5 (rusqlite bundled), refinery 迁移
- **推理:** LM Studio / Ollama (本地), reqwest (云端 Anthropic/OpenAI/DeepSeek)
- **服务:** Axum 0.7 + WebSocket + SSE + JSON-RPC 2.0
- **桌面壳:** Electron 38
- **前端:** React 19 + Tailwind CSS 4 + Vite 6
- **CLI:** clap + tracing-subscriber
- **测试:** cargo test, tempfile

## 工程目录

```
F:/openLoom/
├── crates/
│   ├── memory/       ← Memory Kernel (事件提取+聚合+存储+管线+人格)
│   ├── models/       ← 共享类型定义 (Intent, JSON-RPC, Config, EngineEvent)
│   ├── inference/    ← 推理封装 (LM Studio/Ollama 本地 + Anthropic/OpenAI/DeepSeek 云端)
│   ├── router/       ← 智能路由 (关键词意图分类 + 技能匹配)
│   ├── skills/       ← Skill trait + Registry + CliBridge + 5 内置技能
│   ├── engine/       ← 编排引擎 (EventBus + 请求派发 + Agent Loop)
│   ├── server/       ← Axum HTTP + WebSocket + SSE + JSON-RPC 2.0
│   ├── weaver/       ← 上下文组装 (system prompt + persona + skill + history)
│   ├── cache/        ← KV Cache trait (当前 Noop 实现)
│   └── cli/          ← CLI 入口 (serve/chat/run/skill/memory/config/doctor)
├── migrations/       ← refinery SQL 迁移 (V1~V3)
├── electron/         ← Electron 壳 (侧车生命周期 + contextBridge)
├── web/              ← React 19 前端 (ChatArea + Sidebar + Settings + Dashboard)
├── tests/            ← 集成测试
└── docs/             ← 设计文档
```

## 开发约定

1. **TDD 强制:** 先写测试，验证失败，再写实现，验证通过，提交
2. **提交粒度:** 每个 Task 一个 commit，commit message 遵循 `feat:` / `test:` / `fix:` / `chore:` 前缀
3. **代码风格:** `cargo fmt` + `cargo clippy -- -D warnings` 零警告
4. **测试覆盖:** 每个公开函数必须有单元测试，每个管线阶段必须有集成测试
5. **禁止:** 不写 docstring（代码即文档），不引入不必要的抽象层。功能实现以当前 Phase 设计 spec 为准；若设计评审认为某跨 Phase 功能是当前 Phase 的硬依赖（如 Cloud 路径对 Router 是必要的出口），可提前实现并在 spec 中注明。禁止自己使用npm run dev启动Electron和server，用户会自己手动启动
6. **错误处理:** 使用 `anyhow::Result` 作为公开 API 返回类型，内部用 `thiserror` (Phase 1+)
7. **日志:** `tracing` crate，默认 INFO 级别，不记录用户对话内容

## 路径约定

| 平台 | 数据目录 |
|------|---------|
| Windows | `%APPDATA%/openLoom/` |
| macOS | `~/Library/Application Support/openLoom/` |
| Linux | `~/.local/share/openLoom/` |

## 参考文档

- [架构决策 / 已知限制](docs/architecture.md)
- [设计文档索引](docs/superpowers/README.md) (specs / plans / retrospectives)

### 当前进度
- **Phase 0** — 完成 (Memory Kernel MVP)
- **Phase 1** — 完成 (Smart Router + Skill Engine)
- **Phase 2** — Milestones A/B/C/D 全部完成 (Agent Loop, Persona, Backend, Electron GUI)
- **Phase 3A** — 完成 (AI Activation: LM Studio, SSE streaming, 8B cognition, Hub heartbeat, cloud streaming)
- **Phase 3B** — 完成 (Productionization: Engine split→11 modules, sandbox, audit panel, KV Cache prep, packaging)
- **测试:** 180+ pass | **Clippy:** 0 warnings | **fmt:** clean

### 已知技术债 (Phase 3 复盘)

| 债项 | 位置 | 严重度 | 阻塞 |
|------|------|--------|------|
| SSE 流式发全文非逐 token | inference/lib.rs | HIGH | 待 LM Studio 逐 token 流式适配 |
| EventSource 组件 unmount 时未关闭 | ChatArea.tsx | MEDIUM | — |
| skills invoke() 错误包装丢失来源链 | skills/lib.rs:90 | LOW | — |

---


## Subagent 派发纪律（Code Review 时强制执行）

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
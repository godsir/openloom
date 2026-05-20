# openLoom 设计文档索引

## Specs (设计规范)

- 总体设计: [2026-05-18-openloom-design.md](specs/2026-05-18-openloom-design.md)
- Phase 1: [2026-05-19-phase1-design.md](specs/2026-05-19-phase1-design.md)
- Phase 2-A (KV Cache + Context Weaver): [2026-05-19-phase2-milestone-a-design.md](specs/2026-05-19-phase2-milestone-a-design.md)
- Phase 2-B (Agent Loop + Cloud + Persona): [2026-05-19-phase2-milestone-b-design.md](specs/2026-05-19-phase2-milestone-b-design.md)
- Phase 2-C (会话持久化 + 消息存储): [2026-05-20-phase2-milestone-c-design.md](specs/2026-05-20-phase2-milestone-c-design.md)

## Plans (实现计划)

- Phase 0: [2026-05-18-phase0-memory-kernel.md](plans/2026-05-18-phase0-memory-kernel.md)
- Phase 1: [2026-05-19-phase1-implementation.md](plans/2026-05-19-phase1-implementation.md)
- Phase 2-A: [2026-05-19-phase2-milestone-a.md](plans/2026-05-19-phase2-milestone-a.md)
- Phase 2-B: [2026-05-19-phase2-milestone-b.md](plans/2026-05-19-phase2-milestone-b.md)
- Phase 2-C: [2026-05-20-phase2-milestone-c.md](plans/2026-05-20-phase2-milestone-c.md)

## Retrospectives (复盘)

- Phase 0: [2026-05-20-phase0-retrospective.md](retrospectives/2026-05-20-phase0-retrospective.md)
- Phase 1: [2026-05-20-phase1-retrospective.md](retrospectives/2026-05-20-phase1-retrospective.md)

## 模型下载

本地模型通过 Hugging Face / ModelScope 下载，存储在 `~/.openloom/models/`。
Phase 1 使用 Qwen3-1.7B (GGUF) 做意图分类，云端模型 (Anthropic/OpenAI/DeepSeek) 处理复杂请求。

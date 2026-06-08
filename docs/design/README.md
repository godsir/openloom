# openLoom Design Documents

> 基于 DeepSeek-GUI 对比分析，为 openLoom 采纳的 6 项功能的技术设计文档。

## 文档索引

| # | 功能 | 工作量 | 范围 | 状态 |
|---|------|--------|------|------|
| [001](./001-prompt-cache-fingerprint.md) | Prompt-Cache 指纹 | 1 天 | 纯后端 | 📋 设计中 |
| [002](./002-inline-selection-editor.md) | 内联选择编辑器 | 1 周 | 前端 + IPC | 📋 设计中 |
| [003](./003-plan-sdd-todo-workflow.md) | Plan/SDD/Todo 工作流 | 2 周 | 全栈 | 📋 设计中 |
| [004](./004-fim-code-completions.md) | FIM 代码补全 | 2 周 | 全栈 | 📋 设计中 |
| [005](./005-write-mode.md) | Write 写作模式 | 3 周 | 前端为主 | 📋 设计中 |
| [006](./006-session-compaction.md) | 会话压缩 | 1.5 周 | 纯后端 | 📋 设计中 |
| [007](./007-neutral-review-framework.md) | 中立评审框架 | — | 流程 | ✅ 已发布 |

## 评审流程

每个功能的实现分为多个 Phase，每个 Phase 完成后触发中立评审：

```
设计完成 → [预实现评审] → Phase 1 实现 → [中期评审] → Phase N 实现 → [后期评审] → 合并
```

详见 [007 中立评审框架](./007-neutral-review-framework.md)。

## 合并顺序

为最小化冲突，推荐合并顺序：**001 → 006 → 003 → 002 → 004 → 005**

## 架构原则

所有功能必须扎根于 openLoom 的现有架构：
- **后端**: Rust 2024 + Tokio, 14 crates, JSON-RPC 2.0, SQLite 3-DB
- **前端**: Electron 38 + React 19, Zustand 17 slices, Tailwind CSS 4, contextBridge
- **不可丢失**: 知识图谱、多提供商推理、LSP 集成、Plugin/Skill/Marketplace、桌面宠物

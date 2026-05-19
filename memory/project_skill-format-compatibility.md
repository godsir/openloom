---
name: skill-format-compatibility
description: Skills system needs to support Claude Code, OpenClaw, and other external skill formats
metadata:
  type: project
---

Phase 2+ 的 Skill 系统需要兼容外部 skill 格式：Claude Code skill (SKILL.md + scripts/)、OpenClaw skill、以及其他 AI agent 的 skill/plugin 格式。

**Why:** Phase 1 只有内置的 5 个 Rust native skill，通过 Skill trait 实现。Phase 2 引入 WASM 编译管线和外部 skill 加载时，需要能解析和加载第三方 skill 格式，扩大 skill 生态。

**How to apply:** 在设计 WASM skill 加载器和 `skills-repo/` 目录结构时，考虑多格式支持：manifest 解析层抽象为 format detector → parser → Skill trait adapter。优先支持 Claude Code skill 格式（用户已在使用）。

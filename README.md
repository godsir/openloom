# openLoom

本地优先的私人 AI 助理内核。用认知图谱替代聊天记录，用事件驱动替代轮询，用本地分级模型替代云端全量调用。

## 核心差异化

现有 AI Agent（OpenClaw / Claude Code / Codex）的共同缺陷：**把所有信息塞进上下文窗口，Token 成本随对话长度指数膨胀，且不会真正"认识"用户。**

openLoom 走另一条路：

| 传统做法 | openLoom 做法 |
|----------|--------------|
| 聊天记录 → Embedding → 相似度检索 | 事件提取 → 模式聚合 → 认知图谱演化 |
| 所有工具定义每轮注入系统 prompt | 技能懒加载，仅激活时注入 ≤200 tokens |
| 心跳检查也烧 120K tokens | 事件驱动，空闲零消耗 |
| 记忆 = 关键词召回 | 记忆 = 人格模型持续演化 |

**一句话：一个用认知图谱替代聊天记录、用事件驱动替代轮询、用本地分级模型替代云端全量调用的 AI 内核。**

## 架构

```
Event Bus (Tokio async)
  ↓
Smart Router (本地 1.7B 意图分类 + 复杂度评分)
  ↓  ← 双路并行
KV Cache Store (Q4 safetensors 块池)  +  Memory Kernel (事件→认知→人格)
  ↓
Skill Engine (WASM sandbox + CLI Bridge, 懒加载)
  ↓
Context Weaver (按需编织: 前缀 + 认知摘要 + 技能上下文)
  ↓
Reasoning Engine (仅复杂任务调用大模型)
```

CLI 和 Electron 桌面壳共享同一个 Rust Engine，走 JSON-RPC 2.0 / WebSocket / SSE 协议。

## 开发状态

| Phase | 内容 | 状态 |
|-------|------|------|
| **Phase 0** | Memory Kernel MVP | ✅ 完成 |
| **Phase 1** | Smart Router + Skill Engine + Electron 骨架 | ✅ 完成 |
| **Phase 2** | Agent Loop + Persona + Backend + Electron GUI (4 milestones) | ✅ 完成 |
| **Phase 3A** | AI Activation: LM Studio, SSE streaming, 8B cognition, Hub heartbeat, cloud streaming | ✅ 完成 |
| **Phase 3B** | Productionization: Engine split, sandbox, audit panel, KV Cache prep, packaging | ✅ 完成 |
| **Phase 4** | CLI-first UX: Inline viewport, loom.md, skills, plugins, Mode system, thinking, model switching, permission system, auto-compaction, Markdown rendering | ✅ 进行中 |

**质量：** 180+ tests pass, clippy 0 warnings, fmt clean

## 快速开始

### 前置要求

- Rust 1.85+
- 6GB+ VRAM 推荐（本地模型推理，最低 4GB GPU / CPU-only 降级可用）
- 安装 [LM Studio](https://lmstudio.ai/) 并启动本地推理服务（localhost:1234）

### 三步启动

```bash
git clone https://github.com/godsir/openloom.git
cd openloom

# 1. 下载本地模型（从魔搭自动拉取 Qwen3-1.7B GGUF，约 1.28GB）
cargo run -- download-model

# 2. 启动 TUI 交互式聊天
cargo run -- chat

# 或者启动 HTTP 服务器（Electron 桌面壳 / API 调用）
cargo run -- serve
```

> **本地推理：** 默认使用 LM Studio (http://localhost:1234)。也可以使用 Ollama (http://localhost:11434)。

### 构建

```bash
# 构建 release 二进制
cargo build --release

# 安装到 ~/.cargo/bin/（全局可用 openloom 命令）
cargo install --path crates/cli
```

### 使用

```bash
# 启动服务器 (Electron sidecar 模式)
./target/release/openloom serve --port 0

# 交互式聊天
./target/release/openloom chat

# 分析对话日志，产出认知画像
./target/release/openloom analyze --input chat.log --output profile.json

# 查看用户认知画像
./target/release/openloom memory persona

# 查看认知图谱
./target/release/openloom memory cognitions

# 系统诊断
./target/release/openloom doctor

# 下载模型（从魔搭拉取 GGUF）
./target/release/openloom download-model

# 列出可用的量化版本
./target/release/openloom download-model --list
```

### 启动 Electron 桌面应用

```bash
cd electron
npm install
npm run build  # 构建前端
npm start       # 启动 Electron
```

### 测试

```bash
cargo test  # 180+ 个测试
```

## 技术栈

| 层 | 选型 |
|----|------|
| 核心引擎 | Rust 2024 + Tokio |
| 数据库 | SQLite + FTS5 + refinery 迁移 |
| 本地推理 | LM Studio / Ollama (HTTP OpenAI-compatible API) |
| 云端推理 | Anthropic / OpenAI / DeepSeek (reqwest) |
| HTTP/WS | Axum 0.7 + WebSocket + SSE + JSON-RPC 2.0 |
| 桌面壳 | Electron 38 |
| 前端 | React 19 + TypeScript + Tailwind CSS 4 + Vite 6 |
| CLI | clap + tracing-subscriber |
| KV Cache | safetensors (架构预留) |
| 安全沙箱 | 声明式权限检查 |

## 项目结构

```
openLoom/
├── crates/
│   ├── memory/       ← Memory Kernel (事件提取+聚合+存储+管线+人格)
│   ├── models/       ← 共享类型定义 (Intent, JSON-RPC, Config, EngineEvent)
│   ├── inference/    ← 推理封装 (LM Studio/Ollama 本地 + Anthropic/OpenAI/DeepSeek 云端)
│   ├── router/       ← 智能路由 (关键词意图分类 + 技能匹配)
│   ├── skills/       ← Skill trait + Registry + ExternalSkill + PluginLoader + LoomContext
│   ├── engine/       ← 编排引擎 (EventBus + 请求派发 + Agent Loop)→11 模块
│   ├── server/       ← Axum HTTP + WebSocket + SSE + JSON-RPC 2.0
│   ├── weaver/       ← 上下文组装 (system prompt + persona + skill + history)
│   ├── cache/        ← KV Cache trait (NoopCache + SafetensorsCache)
│   ├── sandbox/      ← 安全沙箱 (声明式权限检查)
│   └── cli/          ← CLI 入口 (serve/chat/run/download-model/skill/memory/config/doctor)
├── electron/         ← Electron 壳 (sidecar 生命周期 + contextBridge + 打包)
├── web/              ← React 19 前端 (ChatArea + Sidebar + Settings + Dashboard + Audit)
├── migrations/       ← refinery SQL 迁移 (V1~V3)
├── tests/            ← 集成测试
└── docs/             ← 设计文档 (specs / plans / retrospectives)
```

## 项目指令与技能系统

### loom.md（类似 CLAUDE.md）

在项目根目录放置 `loom.md`，内容自动注入 LLM 系统提示。支持全局（`<data_dir>/loom.md`）和项目级两层。

### 外部技能 (SKILL.md)

技能文件使用 YAML frontmatter：

```markdown
---
name: my-skill
description: "技能描述"
---

技能正文，调用时注入 LLM 上下文。
```

加载路径：
- `<data_dir>/plugins/<plugin>/skills/*/SKILL.md` — 插件技能
- `<cwd>/.loom/skills/*/SKILL.md` — 项目本地技能

TUI 中输入 `/<skill-name>` 直接激活，技能上下文持续注入后续对话。

### 插件系统

兼容 Claude Code 插件格式（`.claude-plugin/plugin.json` 或 `.loom-plugin/plugin.json`）。引擎启动时递归扫描 `<data_dir>/plugins/`，支持 Claude Code 的嵌套缓存结构。

### Mode 系统

四种运行模式，`/mode` 或 `Ctrl+M` 切换：
- **chat** — 纯对话 | **plan** — 只读探索 | **code** — 完整 agent（默认） | **assistant** — 通用助手

### 扩展思考

`/think none|low|mid|high|max` 控制 LLM 思考深度（1K~64K token 预算），类似 Claude Code。

### 模型实时切换

`/model use local|cloud|auto` 运行时切换本地/云端模型，无需重启。

### 权限确认

Code 模式下 Medium/High 风险工具调用弹出确认对话框（A 批准 / D 拒绝 / S 全批准）。`--dangerously-skip-permissions` 跳过。

### 自动上下文压缩

对话历史超出上下文窗口预算时自动截断旧消息，保留最近对话。状态栏显示使用百分比（如 `12% of 200k`）。

### Markdown 渲染

助手消息自动渲染：标题（accent 色）、**加粗**、`代码`、列表（bullet）、表格（dim 分隔符）、代码块（dim 色）。

### 工具调用结构化显示

类似 Claude Code 的展示风格：`Update(src/main.rs)` / `Read(file)` / `Bash(cmd)` + diff 统计（`+12 -3 lines`）。

### 输入增强

- `Ctrl+R` 历史搜索（实时过滤 + 弹窗选择）
- `Tab` 文件路径补全（无命令弹窗时）
- `Ctrl+O` 展开/折叠当前轮次的 thinking/tool 消息

### 数据目录

首次启动自动创建（欢迎横幅显示路径）：

| 平台 | 路径 |
|------|------|
| Windows | `%APPDATA%/openLoom/` |
| macOS | `~/Library/Application Support/openLoom/` |
| Linux | `~/.local/share/openLoom/` |

```
<data_dir>/
├── loom.md        ← 全局指令
├── plugins/       ← 插件目录（递归扫描）
├── skills/        ← 全局独立技能
├── models/        ← GGUF 模型
├── db/            ← SQLite
└── config.toml    ← 配置
```

## 已知技术债

| 债项 | 严重度 | 阻塞原因 |
|------|--------|---------|
| SSE 流式发全文非逐 token | HIGH | 待 LM Studio 逐 token 流式适配 |
| EventSource unmount 时未关闭 | MEDIUM | — |
| skills invoke() 错误链丢失 | LOW | — |

## 许可证

Apache 2.0

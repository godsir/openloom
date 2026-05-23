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
| **Phase 4** | CLI-first UX: Inline viewport, loom.md, skills, plugins, Mode system, thinking, model switching, permission system, auto-compaction, Markdown rendering | ✅ 完成 |

**质量：** 180+ tests pass, clippy 0 warnings, fmt clean

## 快速开始

### 前置要求

- Rust 1.85+
- 6GB+ VRAM 推荐（本地模型推理，最低 4GB GPU / CPU-only 降级可用）
- 安装 [LM Studio](https://lmstudio.ai/) 并启动本地推理服务（localhost:1234）

### 安装

```bash
git clone https://github.com/godsir/openloom.git
cd openLoom

# 构建 release 二进制
cargo build -p loom-cli --release

# 安装到 ~/.cargo/bin/（全局可用 loom 命令）
# Windows (PowerShell)
cp target/release/loom.exe $HOME\.cargo\bin\

# Windows (CMD)
cp target/release/loom.exe %USERPROFILE%\.cargo\bin\

# Linux / macOS
cp target/release/loom ~/.cargo/bin/
```

> 需要 Rust 1.85+。国内用户可设置镜像：`~/.cargo/config.toml` 中配置 `replace-with = 'ustc'` 或 `aliyun`。

### 验证

```bash
loom --version
loom --help
loom doctor    # 诊断安装状态
```

### 使用

```bash
loom                          # 陪伴模式（Chat，纯对话无工具）
loom code                     # 编码模式（Code，完整 agent + 工具）
loom "帮我看看这个项目"        # 带初始提示词启动
loom exec "解释这段代码"       # 非交互执行
loom review                    # 代码审查
loom resume --last             # 继续最近会话
loom doctor                    # 系统诊断
loom completion bash           # 生成 shell 补全脚本
```

> 详细用法见 [docs/tui-usage.md](docs/tui-usage.md)

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

## 功能特性

### Mode 系统

四种运行模式，`/mode` 或 `Ctrl+M` 切换：

| 模式 | CLI | 说明 |
|------|-----|------|
| Chat（陪伴） | `loom`（默认） | 纯对话，不调工具 |
| Plan（规划） | — | 只读探索，不改文件 |
| Code（编码） | `loom code` | 完整 agent 循环 + 全工具 |
| Assistant | — | 读文件 + 记忆 + 技能，不写不执行 |

### 模型偏好

`/model use local|cloud|auto` 运行时切换本地/云端模型，无需重启。

### 扩展思考

`/think none|low|mid|high|max` 控制 LLM 思考深度（1K~64K token 预算）。

### 权限确认

Code 模式下 Medium/High 风险工具调用弹出确认对话框（A 批准 / D 拒绝 / S 全批准）。

### 自动上下文压缩

对话历史超出上下文窗口预算时自动截断旧消息，状态栏显示使用百分比。

### 外部技能 (SKILL.md)

兼容 Claude Code 技能格式。TUI 中输入 `/<skill-name>` 直接激活。

### 插件系统

兼容 Claude Code 插件格式（`.claude-plugin/plugin.json` 或 `.loom-plugin/plugin.json`）。

### 项目指令

在项目根目录放置 `loom.md`，内容自动注入 LLM 系统提示。也兼容 `CLAUDE.md` / `AGENTS.md`。

### 快捷键

| 按键 | 功能 |
|------|------|
| `Enter` | 发送消息 |
| `Shift+Enter` | 换行 |
| `Ctrl+R` | 增量历史搜索 |
| `Tab` | 补全 `/` 命令 / 文件路径 |
| `Esc` | 取消 / 关闭弹窗 |
| `Ctrl+C`（一次） | 中断当前生成 |

> 完整快捷键和斜杠命令参考见 [docs/tui-usage.md](docs/tui-usage.md)

## 数据目录

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

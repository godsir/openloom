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
| **Phase 3A** | AI Activation: llama-cpp-2, SSE streaming, 8B cognition, Hub heartbeat, cloud streaming | ✅ 完成 |
| **Phase 3B** | Productionization: Engine split, sandbox, audit panel, KV Cache prep, packaging | ✅ 完成 |

**质量：** 129 tests pass, clippy 0 warnings, fmt clean

## 快速开始

### 前置要求

- Rust 1.85+
- 6GB+ VRAM 推荐（本地模型推理，最低 4GB GPU / CPU-only 降级可用）
- CMake + C++ 工具链（llama-cpp-2 编译需要）

### 构建

```bash
git clone https://github.com/godsir/openloom.git
cd openloom
cargo build --release
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
cargo test  # 129 个测试
```

## 技术栈

| 层 | 选型 |
|----|------|
| 核心引擎 | Rust 2024 + Tokio |
| 数据库 | SQLite + FTS5 + refinery 迁移 |
| 本地推理 | llama-cpp-2 (GGUF, feature-gated) |
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
│   ├── inference/    ← 推理封装 (llama-cpp-2 本地 + Anthropic/OpenAI 云端)
│   ├── router/       ← 智能路由 (关键词意图分类 + 技能匹配)
│   ├── skills/       ← Skill trait + Registry + CliBridge + 5 内置技能
│   ├── engine/       ← 编排引擎 (EventBus + 请求派发 + Agent Loop)→11 模块
│   ├── server/       ← Axum HTTP + WebSocket + SSE + JSON-RPC 2.0
│   ├── weaver/       ← 上下文组装 (system prompt + persona + skill + history)
│   ├── cache/        ← KV Cache trait (NoopCache + SafetensorsCache)
│   ├── sandbox/      ← 安全沙箱 (声明式权限检查)
│   └── cli/          ← CLI 入口 (serve/chat/run/skill/memory/config/doctor)
├── electron/         ← Electron 壳 (sidecar 生命周期 + contextBridge + 打包)
├── web/              ← React 19 前端 (ChatArea + Sidebar + Settings + Dashboard + Audit)
├── migrations/       ← refinery SQL 迁移 (V1~V3)
├── tests/            ← 集成测试
└── docs/             ← 设计文档 (specs / plans / retrospectives)
```

## 已知技术债

| 债项 | 严重度 | 阻塞原因 |
|------|--------|---------|
| llama-cpp-2 功能未验证 | P0 | 需要 CMake + C++ 工具链编译 |
| SSE 流式发全文非逐 token | HIGH | 同上，增量 decode 不可用 |
| 8B 模型加载 + LlmBased 认知提取 | HIGH | 同上 |
| EventSource unmount 时未关闭 | MEDIUM | — |
| skills invoke() 错误链丢失 | LOW | — |
| KV Cache 仅架构预留 (NoopCache) | LOW | 等 llama-cpp state API |

## 许可证

Apache 2.0

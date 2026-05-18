# openLoom — 本地化私人 AI 助理设计规范

**版本:** 1.0
**日期:** 2026-05-18
**状态:** 设计阶段

---

## 1. 项目定位

**openLoom** 是一个本地优先的私人 AI 助理内核，核心目标是解决现有 AI Agent 项目的三个根本缺陷：

1. **Token 经济崩溃** — OpenClaw 心跳 120K tokens/次、系统 prompt 15-20K/轮，长对话成本指数膨胀
2. **虚假记忆** — 现有工具的 "记忆" 本质是关键词召回，AI 不会真正成长
3. **被动响应** — 缺乏事件驱动的自主 Agent 能力，只是 "用户输入→LLM 响应" 的包装

**一句话定位：** 一个用认知图谱替代聊天记录、用事件驱动替代轮询、用本地分级模型替代云端全量调用的 AI 操作系统内核。

---

## 2. 横向对比总结

### 2.1 竞品分析

| 维度 | OpenClaw | OpenHanako | Codex | openLoom (我们) |
|------|----------|------------|-------|-----------------|
| 语言/运行时 | TypeScript/Node.js | TypeScript/Electron | Rust 原生二进制 | Rust 引擎 + Electron 壳 |
| Token 效率 | ❌ 极差 (15-20K/轮固定) | ⚠️ 中等 (LLM编译4次/周期) | ✅ 前缀缓存 (比CC省3.5x) | 🎯 目标再省3-5x |
| 记忆系统 | Dreaming 三阶段睡眠 | 四块分层编译+遗忘曲线 | ❌ 无长期记忆 | 认知图谱+事件压缩 |
| Agent 循环 | ReAct | 简单对话 | 事件驱动状态机 | 事件驱动+分层路由 |
| 人格演化 | 静态文件 | 文件夹隔离 | 每次失忆 | 持续认知演化 |
| 自主性 | Cron+Commitment | Hub独立进程 | Goal→DAG→执行 | 事件驱动懒Agent |
| 开源/生态 | MIT, 373K⭐ | Apache 2.0, 2.7K⭐ | Apache 2.0, 62K⭐ | 先内核后生态 |

### 2.2 三个现有工具的致命缺陷

- **OpenClaw:** 系统 prompt 膨胀 + 心跳 Token 浪费 + 静态记忆 = 越用越贵且不会成长
- **OpenHanako:** 单维护者 + 单进程 Node.js + LLM 重度依赖记忆编译 = 工程天花板低
- **Codex:** 无长期记忆 + 无认知演化 + 每次"失忆工程师" = 只适合短期编码任务

**openLoom 要做三者都无法做到的事：** 极低 Token 成本 + 持续认知演化 + 自主 Agent 能力。

---

## 3. 架构设计

### 3.1 七层架构

```
第0层: Event Bus (Tokio async 事件流)
  ↓
第1层: Smart Router (Qwen3-1.7B 本地意图分类+复杂度评估)
  ↓  (双路并行)
第2a层: KV Cache Store (Q4 safetensors 块池, 前缀一次 prefill 永久复用)
第2b层: Memory Kernel (事件→模式→认知→人格, SQLite + 稀疏向量)
  ↓
第3层: Skill Engine (WASM sandbox + CLI Bridge, 懒加载零上下文税)
  ↓
第4层: Context Weaver (按需检索+精确编织: KV前缀+认知摘要+当前事件+技能上下文)
  ↓
第5层: Reasoning Engine (统一模型抽象, 本地/云端, 仅复杂任务走这层)
```

### 3.2 双入口模式

```
openloom-engine  (Rust 二进制)
  ├── CLI模式:  openloom chat / openloom run "..." / openloom serve
  └── serve模式: 启动 Axum HTTP + WSS, Electron 壳通过 JSON-RPC 2.0 连接
```

- Electron 主进程 spawn `openloom-engine serve` 为 sidecar 进程
- CLI 和 Electron 共享完全相同的认知图谱、KV Cache、Skill 模块
- 借鉴 VS Code 架构（Electron 壳 + 独立语言服务器）

### 3.3 进程通信

- **Engine ↔ CLI:** 同进程直接调用
- **Engine ↔ Electron:** WebSocket + JSON-RPC 2.0
- **HTTP 流式:** SSE 端点用于聊天 token 流（前端直连，绕过 Engine IPC）

### 3.4 与传统架构的关键差异

| 传统做法 | openLoom 做法 |
|----------|--------------|
| 用户输入→拼上下文→调LLM | 用户输入→事件化→路由→检索→精确编织→必要时调LLM |
| 记忆 = Vector DB + 相似度搜索 | 记忆 = 认知图谱 + 事件压缩 + 人格演化 |
| 所有工具定义每轮注入 | 技能懒加载, WASM原生执行, 零上下文税 |
| 心跳/空闲也在烧Token | 事件驱动, 无事件零消耗, 心跳只用本地小模型 |
| Agent = 用户触发→LLM响应 | Agent = 事件流驱动 + 状态机 + 自主调度 |

---

## 4. 核心子系统设计

### 4.1 Smart Router (智能路由器)

**职责:** 作为所有输入的入口，决定事件路由到哪一层处理。

**实现:**
- 本地 Qwen3-1.7B Q4_K_M (~1.2GB)，常驻 GPU/CPU
- 输入：用户文本 / 系统事件
- 输出：`{intent: string, complexity: 0-1, skill_match: string|null, cache_hit: bool, target_model: enum}`
- 规则引擎兜底：常见意图用关键词匹配，不走 LLM
- 置信度 < 0.7 → 降级到中模型二次判断
- 目标：80% 请求在 Router 层处理完毕，不触及大模型

**Token 节省:** 每次 Router 判断消耗 ~50-100 tokens（本地模型），对比直接调用大模型省 100-1000x。

### 4.2 Memory Kernel (记忆内核)

这是 openLoom 最核心的差异化子系统。

#### 四阶段认知管线

```
原始交互
  ↓
阶段1: Event Extractor (Qwen3-1.7B)
  输出: 结构化事件 JSON
  示例: {"type":"behavior_pattern","action":"loss_chase","context":"trading","confidence":0.87}
  关键: 提取行为模式, 不是对话记录。事件存SQLite, 原始文本可丢弃。
  ↓
阶段2: Pattern Aggregator (规则引擎 + 滑动窗口)
  同类事件累积到阈值 (如5次同类行为) → 触发认知更新
  使用计数Bloom Filter + 滑动窗口, 大部分不调LLM。
  ↓
阶段3: Cognition Updater (Qwen3-8B, 仅在阈值触发)
  输入: 聚合后的行为模式
  输出: 认知图谱更新语句
  示例: USER trait["risk_tendency"] = "gambler_chase" (confidence: 0.91)
  存储: SQLite + 稀疏向量索引 (sqlite-vec)
  ↓
阶段4: Persona Projector (决定注入上下文的认知摘要)
  当Context Weaver请求用户画像时:
  返回: "用户偏好短线交易, 有追高倾向, 对止损有抗拒心理, 偏好科技股"
  效果: 一句话 (50 tokens) 替代 50万token聊天记录
```

#### 事件存储设计

```sql
-- 事件表
CREATE TABLE events (
  id INTEGER PRIMARY KEY,
  timestamp INTEGER NOT NULL,
  type TEXT NOT NULL,        -- 'behavior_pattern' | 'preference' | 'fact' | 'relationship'
  action TEXT NOT NULL,
  context TEXT,
  confidence REAL,
  source_session TEXT,
  payload JSON
);

-- 认知图谱表
CREATE TABLE cognitions (
  id INTEGER PRIMARY KEY,
  subject TEXT NOT NULL,      -- 'USER' | 'AGENT' | 'RELATIONSHIP'
  trait TEXT NOT NULL,        -- 'risk_tendency' | 'communication_style' | ...
  value TEXT NOT NULL,        -- 'gambler_chase' | 'direct' | ...
  confidence REAL,
  evidence_count INTEGER,     -- 支撑此认知的事件数
  first_seen INTEGER,
  last_updated INTEGER,
  version INTEGER DEFAULT 1   -- 每次更新+1, 支持回滚
);

-- FTS5 全文索引 (事件搜索)
CREATE VIRTUAL TABLE events_fts USING fts5(type, action, context, payload);

-- 稀疏向量索引 (认知语义搜索)
-- 通过 sqlite-vec 扩展实现
```

#### 与 OpenHanako 记忆系统的对比

| 维度 | OpenHanako | openLoom |
|------|-----------|----------|
| 编译方式 | 每10分钟4次LLM调用 | 事件驱动, 阈值触发, 大部分不用LLM |
| 存储形式 | Markdown 文件 | 结构化事件 + 认知图谱 (SQLite) |
| 检索方式 | FTS5 关键词 | 语义向量 + 图谱遍历 |
| 上限 | 2000 tokens 记忆 | 无硬上限, 认知持续压缩 |
| 演化 | 遗忘曲线 (被动) | 主动认知提炼 + 版本管理 |

### 4.3 Skill Engine (技能引擎)

#### 三层工具架构

**第1层: Native Tools (Rust 内置, 零开销)**
- 文件读写、Shell 执行、进程管理 → 直接编译进 engine crate
- 借鉴 Codex 的 `shell / read_file / apply_patch` 模式

**第2层: WASM Skill Modules (安全懒加载)**
```
skill-name/
  manifest.toml     ← 意图描述 + 触发词 + 所需权限 + 模型需求
  context.md        ← 精简上下文定义 (≤200 tokens, 仅激活时注入)
  main.wasm         ← 核心逻辑 (Rust 编译)
  handler.py        ← 可选 Python 胶水 (需要 LLM 调用等灵活场景)
  tools.toml        ← 工具声明 (被 Router 读取用于路由匹配)
```

**第3层: CLI Bridge (零上下文税)**
- 借鉴 Codex "CLI > MCP" 原则：能用 `gh pr view` 不用 MCP
- 自动发现 PATH 中的 CLI 工具，从 `--help` 解析工具描述

#### 与 OpenClaw 的关键区别

OpenClaw 将所有工具定义 (15-20K tokens) 每轮注入系统 prompt。openLoom 的 Router 本地匹配 `manifest.toml` 触发词，只有路由到该 Skill 时才注入 `context.md` (≤200 tokens)。

### 4.4 KV Cache Store (本地前缀缓存)

借鉴 DeepSeek 论文 *"Agent Memory Below the Prompt: Persistent Q4 KV Cache for Multi-Agent LLM Inference on Edge Devices"* (Shkolnikov, arXiv:2603.04428, 2026.02)。

#### 设计

```
~/.openloom/cache/
  {agent_id}/
    block_{seq}.safetensors   ← Q4 量化 KV cache 块 (1024 tokens/块)
    meta.json                 ← 块元数据 (token范围, 创建时间, 命中次数)
```

#### 三个可控参数

| 参数 | 推荐值 | 说明 |
|------|--------|------|
| `block_size` | 1024 tokens | 每个块覆盖的 token 数 |
| `max_blocks_per_agent` | 32 | 覆盖 32K context |
| `total_cache_budget_mb` | 5120 (5GB) | 磁盘配额硬上限 |
| `eviction_policy` | LRU | 最近最少使用先淘汰 |

#### 工作原理

1. 静态前缀 (system prompt + 认知画像) → 本地做一次 prefill → 量化为 Q4 safetensors → 落盘
2. 后续请求：从磁盘恢复 KV 块到注意力层 → 零 token 消耗，TTFT 加速 11-76x
3. 动态部分 (当前对话) 正常推理，不受缓存影响
4. 实际磁盘占用：静态前缀 (~4K tokens) × 147 KB/token (Qwen3-8B Q4) ≈ 600 MB 常驻

#### Q4 量化精度

- Perplexity 损失仅 +3.0% (DeepSeek-Coder-V2-Lite 16B MoE 实测)
- 认知提取用小模型独立推理，不依赖缓存的 KV，质量不受影响
- 缓存只用于前缀复用，不参与推理计算

### 4.5 Context Weaver (上下文编织器)

**职责:** 在请求到达 Reasoning Engine 之前，按需组装最小必要上下文。

**编织策略:**
1. 检查 KV Cache Store → 静态前缀命中 → 恢复 Q4 块 (零 token)
2. 查询 Memory Kernel → Persona Projector 返回认知摘要 (~50 tokens)
3. 查询当前事件的相关 Skill → 注入 context.md (≤200 tokens)
4. 当前任务相关的工作记忆 (~200 tokens)
5. 组装为最终 prompt，发送到 Reasoning Engine

**前缀缓存对齐:**
- 静态内容 (系统指令 + 认知画像) → 放在 prompt 最前面 → 最大化缓存命中
- 动态内容 (技能上下文 + 当前事件) → 放在 prompt 最后 → 不影响前缀
- 借鉴 Codex 的策略：通过 append 而非 modify 来保护缓存

---

## 5. 技术栈

| 模块 | 选型 | 决策理由 |
|------|------|---------|
| 核心引擎 | Rust + Tokio | Codex 验证过的 Agent 引擎最佳实践，零依赖二进制 ~50MB |
| 事件总线 | Tokio mpsc/broadcast | Rust 原生异步 channel，零外部依赖 |
| CLI/TUI | ratatui + crossterm | Rust 原生 TUI，Codex 同款 |
| 桌面壳 | Electron 38 | Chromium 文本渲染最佳，Claude/ChatGPT 验证，避免 WebView 碎片化 |
| 前端框架 | React 19 + Tailwind | 流式渲染方案成熟，生态最大 |
| 数据库 | SQLite + FTS5 + sqlite-vec | 单文件零配置，FTS5 全文搜索，sqlite-vec 稀疏向量 |
| KV Cache | Q4 safetensors 块池 | DeepSeek 论文方案，TTFT 加速 11-76x |
| 本地推理 | llama.cpp (Rust binding) | 最成熟本地推理，GGUF 量化生态，Apple Silicon 优化 |
| 小模型 | Qwen3-1.7B Q4_K_M | ~1.2GB，最低 4GB GPU 可用，CPU fallback |
| 中模型 | Qwen3-8B Q4_K_M | ~5GB，最低 6GB GPU 可用 |
| 大模型 | OpenAI 兼容 API 抽象层 | Claude/GPT/DeepSeek/Grok 可切换 |
| WASM 运行时 | wasmtime | Rust 原生，安全沙箱，WASI 支持 |
| Web Server | Axum + tower | Rust 最快 HTTP 框架，Tokio 原生 |
| Engine↔前端通信 | WebSocket + JSON-RPC 2.0 | CLI 和 Electron 走同一协议 |
| HTTP 流式 | SSE (前端直连) | 绕过 IPC，避免 token 级延迟 |
| 沙箱 | 声明式权限 + OS 原生 | macOS Seatbelt / Linux Landlock+Bubblewrap / Windows Restricted Tokens |

---

## 6. 目录结构

```
openloom/
├── crates/
│   ├── engine/          ← 核心引擎：EventBus + Agent Loop
│   ├── router/          ← Smart Router：本地小模型意图分类+复杂度评分
│   ├── memory/          ← Memory Kernel：事件提取+模式聚合+认知更新+人格投射
│   ├── cache/           ← KV Cache Store：Q4 safetensors 块池管理
│   ├── skills/          ← Skill Engine：WASM runtime + CLI Bridge
│   ├── weaver/          ← Context Weaver：按需检索+精确上下文编织
│   ├── models/          ← 模型抽象层：llama.cpp binding + 云端 API 适配
│   ├── sandbox/         ← 声明式权限引擎 + OS 原生沙箱适配
│   ├── server/          ← Axum HTTP + WebSocket + JSON-RPC 2.0
│   └── cli/             ← CLI 入口：openloom chat / run / serve
├── electron/            ← Electron 主进程 + preload + 托盘管理
├── web/                 ← React 19 聊天 UI 前端
├── skills-repo/         ← 内置 Skill 模块 (manifest + context + wasm)
├── tests/               ← 集成测试
├── docs/                ← 设计文档 + API 文档
└── Cargo.toml           ← Workspace 根配置
```

---

## 7. 分阶段开发计划

### Phase 0: Memory Kernel MVP (1-2周)

**目标:** 验证"事件→认知"管线可行，认知摘要优于全量上下文。

**交付:**
- Event Extractor：对话文本 → 结构化事件 (规则引擎 + 1.7B 模型)
- SQLite Event Store：事件存储 + 基本查询 + FTS5 索引
- Pattern Aggregator：滑动窗口 + 阈值触发的模式检测
- 手动认知更新：输出用户画像 JSON
- CLI 原型：`openloom analyze --input chat.log --output profile.json`
- 基础测试：10 个预设对话场景，验证摘要质量

**不做的:** Electron、WASM、Agent 循环、KV Cache、自主触发

### Phase 1: Smart Router + Skill Engine (2-3周)

**目标:** 实现"80% 请求不动大模型"的目标。

**交付:**
- llama.cpp Rust binding 集成：Qwen3-1.7B 本地运行
- Smart Router：意图分类 + 复杂度评分 + 技能匹配
- Skill Engine：wasmtime runtime + manifest 解析 + context.md 懒加载
- CLI Bridge：PATH 工具自动发现 + --help 解析
- 3-5 个内置 Skill：文件管理、信息检索、日程提醒、代码辅助、网页浏览
- Axum Server：HTTP + WSS + JSON-RPC 2.0 端点
- Token 监控面板：实时显示节省比例 (CLI + 基础 Web)
- Electron 壳骨架：启动 spawn engine sidecar，基础聊天窗口

**不做的:** 完整 Agent 循环、认知自动化、KV Cache、多 Agent

### Phase 2: Event-Driven Agent + Context Weaver (3-4周)

**目标:** Agent 能自主规划、执行、验证，记忆持续演化。

**交付:**
- Event-driven Agent Loop：事件驱动的 ReAct 循环
- Context Weaver：KV Cache 前缀 + 认知摘要 + 技能上下文 + 工作记忆 四合一编织
- Cognition Updater (自动化)：阈值触发 → 8B 模型提取 → 认知图谱更新 + 版本快照
- Persona Projector：一句话认知画像生成
- 云端模型适配层：OpenAI 兼容 API 统一接口 (Claude/GPT/DeepSeek/Grok)
- 定时/自主触发：Hub 心跳 (1.7B 本地低功耗检查)
- 多 Session 管理：会话隔离 + 跨会话记忆延续
- 完整 Electron GUI：聊天界面 + 设置面板 + 认知画像可视化

**不做的:** KV Cache 持久化、多 Agent 协作

### Phase 3: KV Cache + 生产化 (2-3周)

**目标:** Q4 KV Cache 落地 + 可发布的桌面产品。

**交付:**
- Q4 KV Cache Store：safetensors 块池，per-agent 隔离
- Prefill 一次永久复用：系统前缀缓存命中率 95%+
- 安全沙箱：声明式权限 + OS 原生 (Seatbelt/Landlock/Restricted Tokens)
- 跨平台打包：macOS (.dmg) + Windows (.msi) + Linux (.AppImage)
- 一键安装脚本 + 用户引导向导
- Token 仪表盘 + 性能监控
- 认知审核面板：查看/回滚认知图谱

**后续迭代:** 多 Agent 协作、Skill 市场、移动端适配

---

## 8. 关键风险与缓解

| 风险 | 严重度 | 缓解措施 |
|------|--------|---------|
| 认知图谱演化跑偏 (错误认知固化) | 高 | 认知版本快照 + 回滚机制 + 人工审核模式 + 置信度硬底线 |
| 本地小模型意图分类准确度不足 | 中 | 规则引擎兜底 + 置信度阈值 + 低置信度降级到中模型 |
| Q4 量化后认知提取质量下降 | 中 | 认知提取用独立小模型推理，不依赖缓存的 KV |
| Rust + llama.cpp 集成复杂度 | 中 | 先用 llama-cpp-rs crate，不行就子进程+IPC |
| Electron 内存占用 (始终 ~200MB+) | 低 | 相比 OpenClaw 1.2GB 空闲已属轻量，接受这个基线 |
| 单开发者/小团队维护负担 | 中 | Phase 0-1 验证核心假设后再投入 Phase 2-3 |

---

## 9. GPU 兼容性基线

| GPU | VRAM | Router (1.7B Q4, 1.2GB) | Summarizer (8B Q4, 5GB) | KV Cache (0.6GB) | 状态 |
|-----|------|--------------------------|-------------------------|-------------------|------|
| RTX 3090+ | 24GB | ✅ | ✅ | ✅ | 全驻留 |
| RTX 3060 | 12GB | ✅ | ✅ | ✅ | 全驻留 |
| RTX 2060 | 6GB | ✅ | ⚠️ 按需换入 | ⚠️ 按需换入 | Router 常驻 |
| GTX 1060 | 6GB | ✅ | ⚠️ CPU offload | ⚠️ 按需换入 | 可用 |
| GTX 1050 | 4GB | ✅ | ❌ (换4B模型) | ❌ | 降级可用 |
| CPU Only | 0 | ✅ (2s/分类) | ⚠️ (换4B, CPU推理) | ❌ | 基础可用 |

**最低可接受配置:** 6GB VRAM (GTX 1060 / RTX 2060)
**推荐配置:** 12GB+ (RTX 3060+)
**降级策略:** 自动检测 GPU 显存 → 动态调整模型加载策略

---

## 10. Token 节省预期

| 机制 | 做法 | 预期节省 |
|------|------|---------|
| Smart Router | 80% 请求在 1.7B 本地处理，不动大模型 | 5x |
| Skill 懒加载 | 消除 15-20K/轮固定工具定义开销 | 2x |
| 认知摘要 | 一句话 (50 tokens) 替代全量聊天记录 | 1000x |
| KV Cache 前缀 | 静态前缀 prefill 一次，永久复用 | 无限 (零增量) |
| 事件驱动 | 空闲零消耗，心跳用 1.7B 本地 | 消除空闲浪费 |
| CLI > MCP | 系统命令替代 MCP 协议 | 每调用省数百 tokens |

**总预期：** 相比 Codex 再省 3-5x，相比 OpenClaw 省 10-20x，90% 日常任务不触发云端大模型调用。

---

## 11. 运维与韧性

### 11.1 Sidecar 进程生命周期

Electron 主进程管理 `openloom-engine serve` 的完整生命周期：

| 阶段 | 机制 |
|------|------|
| **启动** | spawn 时传 `--port 0` 让 OS 分配端口，Engine 将实际端口写入 stdout JSON 行，Electron 读取后连接 |
| **就绪信号** | Engine 启动后向 stdout 写入 `{"type":"ready","port":19876}`，Electron 超时 10 秒未收到视为启动失败 |
| **健康检查** | WebSocket 自带 ping/pong，5 秒无响应视为失联。Engine 另暴露 `GET /health` HTTP 端点 |
| **崩溃恢复** | Electron 监听 sidecar `exit` 事件，指数退避重启 (1s → 2s → 4s → 8s → max 30s)，最多重试 5 次 |
| **优雅关闭** | Electron `before-quit` → 发送 JSON-RPC `shutdown` 通知 → Engine 排空请求 + 保存 KV Cache + 关闭 SQLite → 5 秒超时后 SIGKILL |
| **僵尸清理** | Engine 启动时检查并清理上次未正常退出的端口文件和 pid 文件 |

### 11.2 错误处理策略

每个子系统独立处理错误，逐层上报：

| 子系统 | 常见错误 | 处理策略 |
|--------|---------|---------|
| Router | 模型加载失败 / 输出格式错误 | 规则引擎兜底 → 降级为中模型判断 → 最后一次降级为直接调大模型 |
| Memory Kernel | SQLite 锁 / 磁盘满 | WAL 模式避免锁竞争；写失败时返回缓存中的最近一次认知摘要 |
| KV Cache | safetensors 损坏 / 磁盘满 | 校验和检测损坏 → 丢弃损坏块 → 标记为 cache miss → 重新 prefill |
| Skill Engine | WASM panic / 超时 | wasmtime 设置 30 秒超时 + 128MB 内存上限，超时/超限 → 终止 + 返回错误给用户 |
| 云端 API | 超时 / 限流 / 返回错误 | 指数退避重试 (最多 3 次) → 降级为备用模型 → 告知用户并提供本地模型选项 |

全局错误总线：`tracing` crate 记录结构化错误日志，`ErrorEvent` 通过 Event Bus 广播，CLI/Electron 各自订阅并展示。

### 11.3 日志与可观测性

- **日志框架:** `tracing` + `tracing-subscriber`，JSON 格式输出到 `~/.openloom/logs/`
- **级别:** ERROR / WARN / INFO / DEBUG / TRACE，默认 INFO，通过配置或 `--verbose` 调整
- **Token 计量:** Engine 内置 token 计数器，每次模型调用记录 `{model, prompt_tokens, completion_tokens, cached_tokens, latency_ms}`，存入 SQLite 表
- **性能埋点:** 关键路径 span：`router.classify`、`memory.extract_events`、`weaver.assemble`、`reasoning.invoke`，输出到日志
- **隐私:** 日志默认不记录用户对话内容（仅记录 token 数和延迟），需显式开启 `log_content: true`

### 11.4 数据迁移

使用 Rust `refinery` crate 管理 SQLite schema 版本：

```
~/.openloom/data/
  db.sqlite            ← 主数据库
  migrations/          ← 已执行的迁移记录 (refinery 自动管理)
    V1__initial.sql
    V2__add_cognition_confidence.sql
```

- 每次 Engine 启动时自动执行未应用的迁移
- Phase 0→1→2→3 的 schema 变更必须写迁移脚本，不可直接修改原始 DDL
- 认知图谱支持版本快照 (`cognitions.version`)，出问题可回滚到上一版本

### 11.5 向后兼容性

- **配置文件:** 新增字段必有默认值，废弃字段保留 2 个大版本后才删除
- **KV Cache 块:** 包含版本头，不兼容版本自动失效重新 prefill
- **认知图谱:** 迁移脚本负责升级，不可自动升级的 (如认知结构重定义) → 保留旧数据只读 + 新数据写新表
- **Skill 模块:** `manifest.toml` 声明 `min_engine_version`，不兼容的 Skill 拒绝加载并提示升级

---

## 12. JSON-RPC 2.0 API 参考

Engine 与前端（CLI / Electron / 未来的移动端）之间唯一的通信协议。

### 12.1 传输

- **WebSocket** 连接到 `ws://127.0.0.1:{port}/ws`
- 每个消息是一个完整的 JSON-RPC 2.0 帧
- 服务端可主动推送 Notification（不需要客户端 request）

### 12.2 核心方法

| 方法 | 参数 | 返回值 | 说明 |
|------|------|--------|------|
| `chat.send` | `{messages, session_id?, stream: bool}` | `{response, session_id, token_usage}` | 发送消息，stream=true 时通过 SSE 端点流式返回 |
| `chat.stream` | `{session_id}` | SSE 事件流 | 获取流式 token（前端直连 SSE，不走 WS） |
| `skill.invoke` | `{skill_name, params}` | `{result, token_usage}` | 调用指定 Skill |
| `skill.list` | `{}` | `{skills: [{name, description, triggers}]}` | 列出所有可用 Skill |
| `memory.query` | `{query, limit?}` | `{events, cognitions}` | 查询事件和认知图谱 |
| `memory.persona` | `{}` | `{summary, traits, last_updated}` | 获取当前用户认知画像 |
| `agent.status` | `{}` | `{state, active_session, model_info}` | Agent 当前状态 |
| `cache.stats` | `{}` | `{hit_rate, block_count, total_size_mb}` | KV Cache 统计 |
| `config.get` | `{key?}` | `{config}` | 读取配置 |
| `config.set` | `{key, value}` | `{ok: bool}` | 修改配置 |
| `system.health` | `{}` | `{status, uptime, gpu_info}` | 系统健康检查 |
| `system.shutdown` | `{}` | `{ok: bool}` | 通知 Engine 优雅关闭 |

### 12.3 服务端通知 (Engine → Frontend)

| 通知 | 参数 | 说明 |
|------|------|------|
| `cognition.updated` | `{trait, old_value, new_value, confidence}` | 认知图谱发生变化 |
| `agent.state_changed` | `{old_state, new_state}` | Agent 状态变化 (idle/thinking/acting) |
| `token.usage` | `{session_id, model, prompt_tokens, completion_tokens}` | 每次 LLM 调用后推送 |
| `error` | `{code, message, subsystem}` | 非致命错误通知 |

### 12.4 标准错误码

| Code | 含义 |
|------|------|
| -32700 | Parse error |
| -32600 | Invalid Request |
| -32601 | Method not found |
| -32603 | Internal error |
| -32000 | Model unavailable (本地模型未加载 / 云端 API 不可达) |
| -32001 | Skill execution failed |
| -32002 | Permission denied (沙箱拦截) |
| -32003 | Timeout |

### 12.5 认证

本地单用户模式，无认证。Engine 只监听 `127.0.0.1`，不接受外部连接。未来如需远程访问，通过 TLS + API key 扩展。

---

## 13. 安全模型

### 13.1 Electron 安全配置

```
contextIsolation: true        ← 渲染进程无法直接访问 Node.js
nodeIntegration: false        ← 渲染进程无 Node.js 能力
sandbox: true                 ← OS 级渲染进程沙箱
webviewTag: false             ← 禁用 webview 标签
```

- 渲染进程仅通过 `contextBridge` 暴露 `window.openloom` API（发送 JSON-RPC 消息）
- 主进程负责：sidecar 管理、系统托盘、原生对话框、自动更新
- CSP 头：`default-src 'self'; connect-src ws://127.0.0.1:* http://127.0.0.1:*; script-src 'self'`

### 13.2 WASM Skill 权限模型

`manifest.toml` 声明权限，默认全部拒绝：

```toml
[permissions]
fs_read = ["~/Documents", "./workspace"]   # 只读路径白名单
fs_write = ["./workspace"]                 # 可写路径白名单
network = ["api.github.com"]               # 网络域名白名单
shell = false                              # 是否允许执行系统命令
subprocess = false                         # 是否允许启动子进程
max_memory_mb = 128                        # 内存上限
max_runtime_sec = 30                       # 执行超时
```

权限在 wasmtime 层通过 WASI 配置强制执行，Skill 无法绕过。

### 13.3 用户数据隐私

- **零遥测:** 不收集任何使用数据、崩溃报告或对话内容。除非用户显式开启（默认关闭）
- **本地存储:** 所有数据（SQLite、KV Cache、日志）仅在 `~/.openloom/` 下，不上传
- **云端 API:** 仅在用户配置云端模型后，向对应 API 发送推理请求。认知图谱和事件数据不发送到云端
- **声明式权限:** 文件访问、Shell 执行、网络调用均需在 Skill manifest 中声明且用户确认

---

## 14. 测试策略

| 层级 | 框架 | 覆盖目标 | 说明 |
|------|------|---------|------|
| **单元测试** | `cargo test` + `proptest` | 80%+ 行覆盖 | 每个 crate 独立测试，关键路径用 property-based test |
| **集成测试** | `cargo test` (tests/ 目录) | 核心管线全场景 | Router→Weaver→Reasoning 端到端；Memory 四阶段管线 |
| **认知质量测试** | 手工审查 + Golden Dataset | 10+ 预设对话场景 | 比较认知摘要 vs 人工标注，BLUE/ROUGE + 人工评估 |
| **E2E 测试** | Playwright (Electron) | 关键用户流程 | Electron 启动→聊天→认知更新可视化 |
| **性能基准** | `criterion` | 每次 PR | Router 延迟、Token 节省率、Cache 命中率、内存占用 |
| **安全测试** | `cargo-audit` + 手工审查 | 每个 Phase | 依赖漏洞扫描 + WASM 沙箱逃逸测试 |

**CI 流程:** GitHub Actions，每个 PR 跑 `cargo test` + `cargo clippy` + `cargo fmt --check` + `criterion` 基准对比。

---

## 15. CLI 命令参考

```
openloom
├── chat                    启动交互式对话 (TUI)
├── run "任务描述"           单次执行任务
├── serve                   启动 Engine 服务 (Electron sidecar 模式)
├── analyze
│   ├── --input chat.log    分析对话文件，输出认知摘要
│   └── --output profile.json
├── config
│   ├── get [key]           读取配置
│   ├── set key value       设置配置
│   └── path                显示配置文件路径
├── skill
│   ├── list                列出已安装 Skill
│   ├── install <path>      安装 Skill
│   └── remove <name>       移除 Skill
├── memory
│   ├── persona             查看当前用户认知画像
│   ├── events [--limit N]  查看最近事件
│   └── cognitions          查看认知图谱
├── cache
│   ├── stats               缓存统计 (命中率/大小/块数)
│   └── clear               清除所有缓存
├── doctor                  系统诊断 (GPU/模型/数据库状态)
└── version                 版本信息
```

---

## 16. 模型管理

### 16.1 模型下载

Engine 首次启动时检测本地模型是否存在。若缺失：

1. **CLI 模式:** 打印下载指令，用户手动执行或运行 `openloom doctor` 获取指导
2. **Electron 模式:** 引导向导自动从 Hugging Face / ModelScope 下载，显示进度条

### 16.2 模型存储

```
~/.openloom/models/
  qwen3-1.7b-q4_k_m.gguf       ← Router (SHA256 校验)
  qwen3-8b-q4_k_m.gguf         ← Summarizer
  config.toml                   ← 模型配置 (路径/类型/用途)
```

### 16.3 模型配置

```toml
[[models]]
name = "router"
path = "qwen3-1.7b-q4_k_m.gguf"
type = "llama_cpp"
n_gpu_layers = 32        # 全部 GPU
context_size = 4096

[[models]]
name = "summarizer" 
path = "qwen3-8b-q4_k_m.gguf"
type = "llama_cpp"
n_gpu_layers = 32
context_size = 8192

[[models]]
name = "cloud-primary"
provider = "anthropic"
model = "claude-sonnet-4-6"
api_key_env = "ANTHROPIC_API_KEY"
```

---

## 附录 A: Tauri vs Electron 决策记录

**日期:** 2026-05-18
**决策:** 选用 Electron 而非 Tauri 作为桌面壳

**背景:** 对 Tauri 进行了深入的社区调研，发现多个项目从 Tauri 迁移到 Electron（而非反向）：

- **Caido** (安全工具) — 用 Tauri 2 年后迁移，开发者原话："WebView 在 25-35% 的情况下是坏的，造成 90%+ 的 bug 报告"
- **AFFiNE** (Notion 替代) — 正式从 Tauri 迁移到 Electron
- **多个独立开发者** 在 Hacker News 报告同样方向

**核心原因:**

1. **WebView 碎片化** — Windows (Edge/Chromium)、macOS (Safari/WebKit)、Linux (WebKitGTK) 三个完全不同的渲染引擎，行为不一致。AI 聊天应用的代码高亮、Markdown、数学公式渲染在不同 WebView 上表现不同
2. **macOS 系统托盘不可用** — 点击事件不触发 (#11413)、图标随机消失 (#12060)，对常驻型 AI 助手是致命问题
3. **拖放互斥** — 文件拖放和文本拖放不能同时启用 (#14055)
4. **Linux 基本不可用** — Tauri 创建者本人建议：如果 Linux 是目标平台，不要用 Tauri
5. **Chromium 的文本渲染是公认最佳** — 这是 Claude Desktop、ChatGPT Desktop 都选择 Electron 的核心原因

**Tauri 的内存优势（4-5x 比 Electron 轻）是真实的，但不值得用跨平台一致性和开发效率去换。** Tauri v3 计划支持捆绑 CEF/Chromium，届时可重新评估。

---

## 附录 B: 跨平台数据路径

| 平台 | 数据目录 | 缓存目录 |
|------|---------|---------|
| Windows | `%APPDATA%\openLoom\` | `%LOCALAPPDATA%\openLoom\cache\` |
| macOS | `~/Library/Application Support/openLoom/` | `~/Library/Caches/openLoom/` |
| Linux | `~/.local/share/openLoom/` (XDG_DATA_HOME) | `~/.cache/openLoom/` (XDG_CACHE_HOME) |

所有路径通过 Rust `dirs` crate 自动解析，不硬编码。

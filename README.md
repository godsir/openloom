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

CLI（ratatui TUI）和 Electron 桌面壳共享同一个 Rust Engine，走 JSON-RPC 2.0 协议。

## 开发状态

- [x] **Phase 0: Memory Kernel MVP** — 规则引擎事件提取 + SQLite FTS5 存储 + 模式聚合 + 认知管线
- [ ] Phase 1: Smart Router + Skill Engine
- [ ] Phase 2: Event-Driven Agent + Context Weaver + Electron GUI
- [ ] Phase 3: Q4 KV Cache + 生产化

## 快速开始

### 前置要求

- Rust 1.85+
- 6GB+ VRAM 推荐（Phase 1+ 需要本地模型推理，最低 4GB GPU / CPU-only 降级可用）

### 构建

```bash
git clone https://github.com/godsir/openloom.git
cd openloom
cargo build --release
```

### 使用

```bash
# 分析对话日志，产出认知画像
./target/release/openloom analyze \
  --input test_data/sample_chat.log \
  --output profile.json \
  --threshold 3

# 查看结果
cat profile.json
```

**输入格式**（pipe 分隔）：`session_id|context|text`

```
session_1|trading|亏了30%我真的好难受，但是我觉得到底了，又加仓了
session_2|trading|今天又跌了5个点，不过我觉得是洗盘，补仓了
session_3|trading|我已经亏了快一半了，但是我不甘心就这么走，又买入了一手
```

**输出示例：**
```json
{
  "total_events": 7,
  "cognitions": [
    {
      "trait": "risk_tendency",
      "action": "loss_chase",
      "evidence_count": 3,
      "confidence": 0.75,
      "summary": "用户存在赌徒补仓倾向：在亏损状态下多次加仓（3次观察，置信度75%）"
    }
  ],
  "generated_at": "2026-05-18T..."
}
```

### 测试

```bash
cargo test  # 36 个测试 (26 单元 + 10 集成)
```

## 技术栈

| 层 | 选型 |
|----|------|
| 核心引擎 | Rust + Tokio |
| 数据库 | SQLite + FTS5 + sqlite-vec |
| CLI/TUI | clap + ratatui |
| 本地推理 | llama.cpp (Phase 1+) |
| 桌面壳 | Electron 38 |
| 前端 | React 19 + Tailwind |
| 通信协议 | WebSocket + JSON-RPC 2.0 |
| 技能沙箱 | wasmtime (WASM + WASI) |

## 项目结构

```
openLoom/
├── crates/
│   ├── memory/       ← Memory Kernel (事件提取+聚合+存储+管线)
│   ├── models/       ← 共享类型定义
│   └── cli/          ← CLI 入口
├── electron/         ← Electron 壳 (Phase 1+)
├── web/              ← React 前端 (Phase 1+)
├── tests/            ← 集成测试
└── docs/             ← 设计文档
```

## 许可证

Apache 2.0

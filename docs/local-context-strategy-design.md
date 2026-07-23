# 本地模型上下文策略设计

- **日期**：2026-07-23
- **状态**：待评审
- **相关代码**：`agent_loop.rs`、`loom-context/src/lib.rs`、`loom-memory/src/summary.rs`、`loom-inference/src/engine.rs`、`loom-types/src/config/model_config.rs`

## 背景与问题

openloom 对接本地大模型（LM Studio / Ollama）时，出现"第一轮就停止生成、无回复"的故障，调整 `context_size` 也无效。

**根因**：openLoom 在组装请求时，对云端模型和本地模型一视同仁——把全套工具 schema + system prompt + skill 指令等全量塞入请求，**没有任何 backend 判断**：

- `agent_loop.rs:824-906`：无条件注入 `dynamic_context`（skill 指令/KG/workspace）、`persona`、`todo_context`、`few_shots`（"非空就注入"，不分 backend）
- `engine.rs:317`：`if !req.tools.is_empty()` 即发送全套工具 schema，不分 backend

云端模型上下文窗口大（128K+）扛得住，本地模型窗口小（8K/16K）第一轮就超 → 请求失败、生成中断。调 `context_size` 无效，因为固定开销是真实内容、与 `context_size` 无关，而保护阈值又误信 `context_size` 不触发。

此外，本地模型还被用作后台 summarizer 提炼记忆（`loom-memory/summary.rs`）。其 `build_prompt_segmented`（行 95-137）把待总结的历史段**全量拼接、不截断**，历史稍长（尤其主对话产生大量 tool_result）就超过本地窗口，summarizer 调用同样失败。

**用户场景**：本地模型基本不用于工具调用，主要用于生成文本、改文章、提炼记忆。

## 目标

让本地模型在 openloom 里不再"第一轮无声死"，覆盖主对话与 summarizer 两条路径。

## 设计原则

1. **不写死 backend**：行为分流依据是模型配置开关（前端可配），不是 `is_local_inference()` 硬判断。云端模型也能用同一精简路径。
2. **最小化（YAGNI）**：只做解决"第一轮死"必须的两件事，复杂的治本项列为可选后续。

## 方案

### 1. 精简模式（主对话路径 `agent_loop`）

在 `ModelConfig` 新增布尔字段 `compact_mode: bool`（默认 `false`），前端 `ModelConfigPanel` 提供勾选。勾选该模式的模型，在 `agent_loop` 组装时：

- **不发送 tools**：组装时令 `req.tools = []`，`engine.rs:317` 的 `if !req.tools.is_empty()` 自然跳过，省掉全套工具 schema
- **不注入 `dynamic_context` / `persona` / `todo_context` / `few_shots`**：在 `agent_loop.rs:824-906` 按 `compact_mode` 短路这些注入
- **只用精简 system prompt** + 历史 + 当前输入

**效果**：本地固定开销从"万 token 级"降到"几百-千 token 级"，8K 窗口可用，第一轮不再因固定开销超长而死。

**不写死**：`compact_mode` 由前端配置驱动，不与 backend 绑定。本地模型勾了走精简，云端模型勾了也走精简（省 token / 省 cache）。与 `ModelType`（Router/Summarizer/Reasoning）正交，互不影响。

### 2. summarizer 历史长度上限 + 分段（`loom-memory/src/summary.rs`）

`build_prompt_segmented`（行 95）目前把 `history[from..to]` 全量拼接。改造为按预算分段：

- **预算**：`budget = min(model_config.context_size, 8192)`。`8192` 是保守硬上限，避免 `context_size` 误判为 100K 时分段阈值虚高、不触发；该上限后续随"真实窗口感知"上线后替换为真实值
- **算 prompt token**：复用 `loom-inference/src/engine.rs:518` 的 `token_count`（cl100k BPE）
- **超预算分段**：把 `history[from..to]` 切成多个 ≤ `budget` 的子段，分别总结，再用 `DELTA_PROMPT`（行 31，已有"已有摘要 + 新内容"机制）增量合并为最终摘要
- `should_summarize_by_tokens`（行 61）使用 `budget` 而非回退 100K

**效果**：summarizer 不再被超长历史段干死。

## 数据流

- **主对话**：组装时若 `compact_mode == true` → 跳过 tools / dynamic_context / persona / todo → 精简 system + 历史 + 输入 → 发送。agent_loop 截断/总结/续写骨架不动。
- **summarizer**：触发总结 → 算 prompt token → 超预算则分段 → 每段单独总结 → 增量合并。`SummaryEngine` 主结构不动。

## 不做什么（YAGNI，可选后续）

以下为"更稳/治本"项，**不在本次范围**，待精简模式上线验证后按需追加：

- 真实窗口探测（LM Studio 加载后实际 context、`detect_gpu()` 显存估算兜底）
- `effective_window` 不再信 `context_size` 默认 100K 的修正
- LM Studio load `context_length` 合理化 + 失败可见化（`openai.rs:926`）
- Ollama 传 `num_ctx` / 原生 `/api/chat` 路径
- 错误 400/OOM 文本分类与可读化
- 诊断探针日志

## 测试

- **单测**：
  - 精简模式：`compact_mode == true` 时组装出的 messages 不含 tools、不含 dynamic_context/persona/todo
  - summarizer 分段：超预算历史正确切分为多段、合并后摘要完整
- **集成**：mock 本地 endpoint，断言精简模式请求体无 `tools` 字段
- **手测**：LM Studio + 本地模型勾选精简模式，验证第一轮正常生成；触发 summarizer 验证不再失败

## 工作量

| 项 | 工作量 |
|---|---|
| 精简模式：`ModelConfig.compact_mode` 字段 + 前端勾选项 + `agent_loop` 组装短路 | 1-2 天 |
| summarizer 分段：`summary.rs` 预算 + 分段 + 增量合并 | 1-2 天 |
| 测试（单测 + 手测） | 1 天 |
| **合计** | **约 3-5 天** |

## 风险

- 精简模式短路了 `dynamic_context`，若某些场景依赖 skill 指令注入需确认本地场景不依赖（用户已说明本地不调工具，风险低）
- summarizer 分段合并可能引入摘要质量波动，需对比测试
- 前端勾选项需与现有 `ModelConfigPanel` 风格一致（参考 shared 组件规范，不引入 emoji/图形符号）

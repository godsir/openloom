# 本地模型上下文策略 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让本地模型（LM Studio/Ollama）在 openloom 里不再"第一轮无声死"，覆盖主对话与 summarizer 两条路径。

**Architecture:** 配置驱动的"精简模式"——`ModelConfig.compact_mode` 勾选后，主对话组装短路 tools/dynamic_context/persona/todo 注入；summarizer 路径按预算截断超长历史段。不写死 backend，与 `ModelType` 正交，agent_loop/SummaryEngine 骨架不动。

**Tech Stack:** Rust（loom-types / loom-core / loom-memory crates，`cargo test`）、TypeScript + React（前端，`tsc` 类型检查）、JSON-RPC（前后端 `model.config.*`）。

## Global Constraints

（摘自 spec `docs/local-context-strategy-design.md`，每个 task 隐式遵守）

- `compact_mode` 默认 `false`，旧配置反序列化必须兼容（`#[serde(default)]`）
- 分流依据是 `ModelConfig.compact_mode`，**不写死 backend**（`is_local_inference()` 不参与判断）
- 前端文案纯文字，**不引入 emoji/图形符号**（遵循项目既有规范）
- 不加新 crate 依赖；token 估算复用现有 char/4 或 `loom_context::bpe`
- commit message 用中文、不带 `@` 装饰；**不 push 远端**
- 后端改 `.rs` 后须本地 `cargo build`/`cargo test` 验证（不依赖用户手动）

## File Structure

| 文件 | 职责 | 改动 |
|---|---|---|
| `backend/crates/loom-types/src/config/model_config.rs` | `ModelConfig` 结构体 | 新增 `compact_mode: bool` 字段 |
| `backend/crates/loom-core/src/agent_loop.rs` | 主对话组装 + `AgentLoopConfig` | 加 `compact_mode()` 方法；非流式/流式组装段短路注入 |
| `backend/crates/loom-memory/src/summary.rs` | summarizer prompt 构造 | `build_prompt_segmented` 加 `budget` 截断 |
| `backend/crates/loom-core/src/orchestrator.rs` | summarizer 调用点 | 算 `summary_budget` 传入，替换 `max(100_000)` |
| `frontend/.../types/bindings.ts` | `ModelConfig` TS 类型 | 加 `compact_mode?: boolean` |
| `frontend/.../components/shared/ModelConfigPanel.tsx` | 模型编辑面板 | editForm 加字段、保存传参、UI 勾选 |
| `frontend/.../i18n/{zh-CN,zh-TW,en-US}.ts` | 文案 | 加 `modelPanel.compactMode` |

---

### Task 1: `ModelConfig.compact_mode` 字段

**Files:**
- Modify: `backend/crates/loom-types/src/config/model_config.rs:52-140`
- Test: `backend/crates/loom-types/src/config/model_config.rs`（同文件 `#[cfg(test)]`）

**Interfaces:**
- Produces: `ModelConfig { compact_mode: bool }`，serde 默认 `false`，供 Task 2 的 `compact_mode()` 读取

- [ ] **Step 1: 写失败测试**

在 `model_config.rs` 末尾加：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_mode_defaults_false() {
        assert!(!ModelConfig::default().compact_mode);
    }

    #[test]
    fn compact_mode_back_compat_missing_field() {
        // 旧配置（无 compact_mode 字段）反序列化必须得到 false
        let json = r#"{"name":"x","backend":"Ollama","context_size":8192}"#;
        let cfg: ModelConfig = serde_json::from_str(json).unwrap();
        assert!(!cfg.compact_mode);
    }
}
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -p loom-types compact_mode -- --nocapture`
Expected: FAIL（`compact_mode` 字段不存在，编译错误）

- [ ] **Step 3: 实现**

在 `ModelConfig` 结构体（约 `:66` `api_format` 字段后）加：

```rust
    /// 精简模式：勾选后主对话组装跳过 tools / dynamic_context / persona / todo 注入，
    /// 只发精简 system + 历史 + 输入。配置驱动，不与 backend 绑定。默认 false。
    #[serde(default)]
    pub compact_mode: bool,
```

在 `impl Default for ModelConfig`（约 `:118-139`）的初始化体里加：

```rust
            compact_mode: false,
```

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test -p loom-types compact_mode -- --nocapture`
Expected: PASS（2 个测试通过）

- [ ] **Step 5: 全 crate 构建**

Run: `cargo build -p loom-types`
Expected: 编译通过

- [ ] **Step 6: Commit**

```bash
git add backend/crates/loom-types/src/config/model_config.rs
git commit -m "feat: ModelConfig 新增 compact_mode 字段（默认 false，向后兼容）" -m "Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 2: `AgentLoopConfig.compact_mode()` + 组装短路

**Files:**
- Modify: `backend/crates/loom-core/src/agent_loop.rs:180-197`（加方法）、`:815-906`（非流式组装）、流式对称段 `:1758-1908`
- Test: `backend/crates/loom-core/src/agent_loop.rs`（同文件 `#[cfg(test)]`）

**Interfaces:**
- Consumes: `ModelConfig.compact_mode`（Task 1）
- Produces: `AgentLoopConfig::compact_mode(&self) -> bool`

- [ ] **Step 1: 写失败测试**

在 `agent_loop.rs` 测试模块加（若文件无 `#[cfg(test)] mod tests`，新建于文件末尾）：

```rust
#[cfg(test)]
mod compact_mode_tests {
    use super::*;
    use loom_types::config::{ModelConfig, ModelBackend};

    fn cfg_with(compact: bool) -> AgentLoopConfig {
        let mc = ModelConfig {
            name: "local".into(),
            backend: ModelBackend::Ollama,
            context_size: 8192,
            compact_mode: compact,
            ..ModelConfig::default()
        };
        AgentLoopConfig {
            model_configs: vec![mc],
            active_model_name: Some("local".into()),
            ..AgentLoopConfig::default()
        }
    }

    #[test]
    fn compact_mode_reads_active_model() {
        assert!(cfg_with(true).compact_mode());
        assert!(!cfg_with(false).compact_mode());
    }

    #[test]
    fn compact_mode_false_when_no_active_model() {
        let mut c = cfg_with(true);
        c.active_model_name = None;
        assert!(!c.compact_mode());
    }
}
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -p loom-core compact_mode_tests -- --nocapture`
Expected: FAIL（`compact_mode` 方法不存在）

- [ ] **Step 3: 加 `compact_mode()` 方法**

在 `impl AgentLoopConfig`（约 `:180`，`effective_context_window` 前）加：

```rust
    /// 是否对该轮活跃模型启用精简模式（从 active model config 读 compact_mode）。
    /// 不写死 backend：任何模型勾选 compact_mode 都生效。无活跃模型时返回 false。
    pub fn compact_mode(&self) -> bool {
        self.model_configs
            .iter()
            .find(|c| Some(c.name.as_str()) == self.active_model_name.as_deref())
            .map(|c| c.compact_mode)
            .unwrap_or(false)
    }
```

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test -p loom-core compact_mode_tests -- --nocapture`
Expected: PASS

- [ ] **Step 5: 非流式组装段短路注入**

`run_agent_turn_inner` 组装段（`agent_loop.rs:815-906`），做 4 处改动：

(a) `:819` 后（`skill_tool_allowlist` 过滤之后、`keep_recent_pct` 之前）加 tools 清空：

```rust
    // 精简模式：不挂工具（本地小窗口场景不调工具，省掉全套工具 schema）
    if config.compact_mode() {
        tools.clear();
    }
```

(b) `:835` few_shots 注入条件改为：

```rust
    if !config.few_shots.is_empty() && !config.compact_mode() {
```

(c) `:852` dynamic_context 注入条件改为：

```rust
        if let Some(ref dc) = config.dynamic_context
            && !dc.is_empty()
            && !config.compact_mode() {
```

(d) `:866` todo_context 注入条件改为：

```rust
    if let Some(ref tc) = config.todo_context
        && !tc.is_empty()
        && !config.compact_mode() {
```

> 注：`continuation_note`（`:885`）**保留**——它是用户取消后继续的对话语义提示，短且必要，不属于重负载。

- [ ] **Step 6: 流式组装段对称改**

`run_agent_turn_streaming_inner`（`agent_loop.rs:1758-1908`）有与 Step 5 完全对称的 tools 过滤、few_shots、dynamic_context、todo_context 注入。用 Grep 定位：

Run: `grep -n "few_shots.is_empty\|dynamic_context\|todo_context\|skill_tool_allowlist" backend/crates/loom-core/src/agent_loop.rs`

对 1758-1908 区间内的对应行，按 Step 5 的 (a)(b)(c)(d) 同样改动（加 `&& !config.compact_mode()`、tools 清空）。

- [ ] **Step 7: 构建 + 全量测试**

Run: `cargo build -p loom-core && cargo test -p loom-core -- --nocapture`
Expected: 编译通过，现有测试不回归

> 组装短路因内联在 turn 函数中难单测，用 build + 后续 Task 5 前端勾选后手测"请求体无 tools"验证。

- [ ] **Step 8: Commit**

```bash
git add backend/crates/loom-core/src/agent_loop.rs
git commit -m "feat: 精简模式短路 agent_loop 的 tools/dynamic_context/persona/todo 注入" -m "Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 3: `build_prompt_segmented` 加 budget 截断

**Files:**
- Modify: `backend/crates/loom-memory/src/summary.rs:95-137`、测试 `:246-268`
- Test: 同文件 `#[cfg(test)]`

**Interfaces:**
- Consumes: caller 传入 `budget: usize`（Task 4 算）
- Produces: `build_prompt_segmented(history, from, to, existing, budget)` 新签名

> **spec 偏差说明**：spec 方案 2 写的是"分段合并保留全部历史"。本计划实现为"截断"（超 budget 只取最近 budget 的较新历史段总结），这是最小实现（YAGNI）。分段合并作为可选后续（见文末）。两者都覆盖 spec 的核心目标"summarizer 不被超长历史干死"。

- [ ] **Step 1: 写失败测试**

在 `summary.rs` 测试模块加：

```rust
    #[test]
    fn test_build_prompt_segmented_budget_truncates_old() {
        // 10 条历史，budget 只够装 2 条 → 截断较旧，只保留最近 2 条
        let history: Vec<Message> = (0..10)
            .map(|i| Message::user(format!("msg {}", i)))
            .collect();
        // budget 按 char/4 估算；每条 "msg N" ≈ 12 char ≈ 3 token。
        // 给 budget=8（≈2-3 条），应截掉 msg 0..7
        let prompt = SummaryEngine::build_prompt_segmented(&history, 0, 10, None, 8);
        assert!(prompt.contains("msg 9"));
        assert!(prompt.contains("msg 8"));
        assert!(!prompt.contains("msg 0"));
        assert!(!prompt.contains("msg 5"));
    }

    #[test]
    fn test_build_prompt_segmented_budget_zero_no_truncate() {
        let history: Vec<Message> = (0..10)
            .map(|i| Message::user(format!("msg {}", i)))
            .collect();
        // budget=0 表示不限制 → 全量（向后兼容旧行为）
        let prompt = SummaryEngine::build_prompt_segmented(&history, 0, 10, None, 0);
        assert!(prompt.contains("msg 0"));
        assert!(prompt.contains("msg 9"));
    }
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -p loom-memory build_prompt_segmented_budget -- --nocapture`
Expected: FAIL（签名不匹配，编译错误）

- [ ] **Step 3: 改 `build_prompt_segmented` 签名 + 截断实现**

替换 `summary.rs:95-137` 整个函数为：

```rust
    /// Build the prompt from a [from, to) slice of history, using full content
    /// (text + tool calls + tool results), not just text_content().
    ///
    /// `budget` 是 prompt 的 token 预算上限（char/4 估算）。0 = 不限制。
    /// 超 budget 时从 to 往前累加，截断较旧历史，只保留最近 budget 内的较新段——
    /// 防止本地小窗口 summarizer 被超长历史段撑爆。
    pub fn build_prompt_segmented(
        history: &[Message],
        from: usize,
        to: usize,
        existing_summary: Option<&str>,
        budget: usize,
    ) -> String {
        let to = to.min(history.len());
        let from = from.min(to);
        // 从 to 往前累加，超 budget 停（保留较新段）
        let mut acc_chars: usize = 0;
        let mut effective_from = from;
        for i in (from..to).rev() {
            let msg_chars: usize = history[i]
                .content
                .iter()
                .map(|c| match c {
                    loom_types::ContentPart::Text { text } => text.len(),
                    loom_types::ContentPart::Thinking { text } => text.len() + 10,
                    loom_types::ContentPart::ToolCall { name, arguments, .. } => {
                        name.len() + arguments.len() + 20
                    }
                    loom_types::ContentPart::ToolResult { name, result, .. } => {
                        name.len() + result.len() + 20
                    }
                    loom_types::ContentPart::Image { .. }
                    | loom_types::ContentPart::ImageRef { .. } => 8,
                })
                .sum();
            // budget 用 char/4 估算 token；累计 char 超 budget*4 视为超预算
            if budget > 0 && acc_chars + msg_chars > budget * 4 {
                effective_from = i + 1;
                break;
            }
            acc_chars += msg_chars;
            effective_from = i;
        }
        let history_text = history[effective_from..to]
            .iter()
            .map(|m| {
                let body = m
                    .content
                    .iter()
                    .map(|c| match c {
                        loom_types::ContentPart::Text { text } => text.clone(),
                        loom_types::ContentPart::Thinking { text } => {
                            format!("[thinking] {}", text)
                        }
                        loom_types::ContentPart::ToolCall {
                            name, arguments, ..
                        } => {
                            format!("[tool_call {}] {}", name, arguments)
                        }
                        loom_types::ContentPart::ToolResult { name, result, .. } => {
                            format!("[tool_result {}] {}", name, result)
                        }
                        loom_types::ContentPart::Image { .. }
                        | loom_types::ContentPart::ImageRef { .. } => "[image]".to_string(),
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                format!("[{}]: {}", m.role.as_str(), body)
            })
            .collect::<Vec<_>>()
            .join("\n\n");
        match existing_summary {
            Some(prev) if !prev.is_empty() => Self::DELTA_PROMPT
                .replace("{previous}", prev)
                .replace("{new_messages}", &history_text),
            _ => format!("{}\n\n{}", Self::PROMPT, history_text),
        }
    }
```

- [ ] **Step 4: 更新现有调用方测试签名**

`summary.rs:250` 和 `:263` 两处旧调用加 `0`（不限制）：

```rust
        let prompt = SummaryEngine::build_prompt_segmented(&history, 2, 8, None, 0);
```
```rust
        let prompt = SummaryEngine::build_prompt_segmented(&history, 0, 2, None, 0);
```

- [ ] **Step 5: 跑测试确认通过**

Run: `cargo test -p loom-memory -- --nocapture`
Expected: PASS（新 2 个 + 现有全部通过）

- [ ] **Step 6: Commit**

```bash
git add backend/crates/loom-memory/src/summary.rs
git commit -m "feat: build_prompt_segmented 加 budget 截断，防 summarizer 被超长历史撑爆" -m "Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 4: orchestrator caller 算 budget 并传入

**Files:**
- Modify: `backend/crates/loom-core/src/orchestrator.rs:4913-4923`（非流式）、`:6199-6210`（流式）

**Interfaces:**
- Consumes: `build_prompt_segmented` 新签名（Task 3）
- Produces: summarizer 调用按 `summary_budget` 截断

- [ ] **Step 1: 非流式 caller 改 budget**

`orchestrator.rs:4913-4923`，把：

```rust
            let effective_cw = context_window.max(100_000);
            let recent_count =
                ((effective_cw as f32 * self.compaction_config.keep_recent_tokens_pct) as usize)
                    .min(total_msgs);
            let recent_boundary = total_msgs.saturating_sub(recent_count);
            let prompt = loom_memory::SummaryEngine::build_prompt_segmented(
                &history,
                summary_at_count,
                recent_boundary,
                existing_summary.as_deref(),
            );
```

改为：

```rust
            // 本地小窗口 summarizer 防爆：cap 到 8192，下限 2048。
            // context_window 未知(0)时取 2048，不再用 100K 高估导致总结段超长。
            let summary_budget = context_window.min(8192).max(2048);
            let effective_cw = summary_budget;
            let recent_count =
                ((effective_cw as f32 * self.compaction_config.keep_recent_tokens_pct) as usize)
                    .min(total_msgs);
            let recent_boundary = total_msgs.saturating_sub(recent_count);
            let prompt = loom_memory::SummaryEngine::build_prompt_segmented(
                &history,
                summary_at_count,
                recent_boundary,
                existing_summary.as_deref(),
                summary_budget,
            );
```

- [ ] **Step 2: 流式 caller 对称改**

`orchestrator.rs:6199-6210`（与 Step 1 完全对称的 `effective_cw = context_window.max(100_000)` + `build_prompt_segmented` 调用），做同样改动：加 `summary_budget`、替换 `effective_cw`、调用末尾加 `summary_budget` 参数。

Run: `grep -n "context_window.max(100_000)" backend/crates/loom-core/src/orchestrator.rs`
Expected: 输出 2 行（4913 + 6199），两处都改。

- [ ] **Step 3: 构建 + 测试**

Run: `cargo build -p loom-core && cargo test -p loom-core -- --nocapture`
Expected: 编译通过，无回归

- [ ] **Step 4: Commit**

```bash
git add backend/crates/loom-core/src/orchestrator.rs
git commit -m "fix: summarizer 用 summary_budget(cap 8K) 替换 max(100K)，防本地窗口撑爆" -m "Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 5: 前端 compact_mode 勾选

**Files:**
- Modify: `frontend/src/renderer/src/types/bindings.ts:22-34`
- Modify: `frontend/src/renderer/src/components/shared/ModelConfigPanel.tsx:125-131, 391-413, 884-908`
- Modify: `frontend/src/renderer/src/i18n/zh-CN.ts`、`zh-TW.ts`、`en-US.ts`

**Interfaces:**
- Consumes: 后端 `ModelConfig.compact_mode`（Task 1）
- Produces: 前端可勾选保存的 UI

> 前端 compact_mode 是 UI 状态 + RPC 传参，逻辑简单，用 `tsc` 类型检查 + 手测验证（不强 TDD）。

- [ ] **Step 1: bindings.ts 加类型字段**

`bindings.ts:22-34` 的 `interface ModelConfig`，在 `context_size: number` 后加：

```typescript
  compact_mode?: boolean
```

- [ ] **Step 2: ModelConfigPanel editForm 加字段**

`ModelConfigPanel.tsx:125-131` 的 `editForm` 类型与初始值，在 `max_output_tokens?: number` 后加 `compact_mode: boolean`，初始值 `:131` 加 `compact_mode: false`：

```typescript
  const [editForm, setEditForm] = useState<{
    name: string; model: string; backend: ModelBackend; base_url: string;
    context_size: number; max_output_tokens?: number
    compact_mode: boolean
    backend_label?: string; api_format?: string; api_key_env?: string
    vision: boolean; reasoning: boolean; function_calling: boolean
    input_price?: number; output_price?: number; cache_read_price?: number; cache_write_price?: number
  }>({ name: '', model: '', backend: 'DeepSeek', base_url: '', context_size: 4096, max_output_tokens: undefined, compact_mode: false, vision: false, reasoning: false, function_calling: false, input_price: undefined, output_price: undefined, cache_read_price: undefined, cache_write_price: undefined })
```

`handleStartEdit`（`:341-355`）初始化里加：

```typescript
      compact_mode: m.compact_mode ?? false,
```

- [ ] **Step 3: handleSaveEdit 传参**

`ModelConfigPanel.tsx:391-413` 的 `loomRpc('model.config.update', {...})`，在 `max_output_tokens: editForm.max_output_tokens,` 后加：

```typescript
        compact_mode: editForm.compact_mode,
```

- [ ] **Step 4: 编辑 UI 加勾选**

`ModelConfigPanel.tsx:884-908` 的 `pvFieldRow`（context_size + max_output_tokens 那行）之后，插入一个勾选行（复用现有 `styles.pvField` / `styles.pvEditLabel` 类名，纯文字标签，无 emoji）：

```tsx
              <div className={styles.pvFieldRow}>
                <div className={styles.pvField}>
                  <label className={styles.pvEditLabel}>{t('modelPanel.compactMode')}</label>
                  <input
                    type="checkbox"
                    checked={editForm.compact_mode}
                    onChange={e => setEditForm(f => ({ ...f, compact_mode: e.target.checked }))}
                  />
                </div>
              </div>
```

- [ ] **Step 5: i18n 加文案**

在三个 i18n 文件的 `modelPanel` 段（与 `contextSize`/`maxOutputTokens` 同级）各加一行（纯文字，无符号）：

`zh-CN.ts`：`compactMode: '精简模式（小窗口本地模型：不挂工具、精简系统提示）',`
`zh-TW.ts`：`compactMode: '精簡模式（小視窗本地模型：不掛工具、精簡系統提示）',`
`en-US.ts`：`compactMode: 'Compact mode (small-window local models: no tools, slim prompt)',`

> 确切行号以文件现有 `modelPanel.contextSize` 位置为准（用 Grep 定位：`grep -n "contextSize" frontend/src/renderer/src/i18n/*.ts`）。

- [ ] **Step 6: 类型检查 + 手测**

Run: `cd frontend && npx tsc --noEmit`
Expected: 无类型错误

手测（用户侧）：启动前端 → 模型配置面板编辑某本地模型 → 勾选"精简模式" → 保存 → 重新打开确认勾选持久 → 用该模型发一轮对话，确认不再"第一轮无声死"。

- [ ] **Step 7: Commit**

```bash
git add frontend/src/renderer/src/types/bindings.ts frontend/src/renderer/src/components/shared/ModelConfigPanel.tsx frontend/src/renderer/src/i18n/zh-CN.ts frontend/src/renderer/src/i18n/zh-TW.ts frontend/src/renderer/src/i18n/en-US.ts
git commit -m "feat: 前端 ModelConfigPanel 加精简模式勾选 + i18n" -m "Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## 可选后续（YAGNI，不在本次范围）

- **summarizer 分段合并**：把"截断"升级为"分多段总结再 DELTA_PROMPT 增量合并"，保留全部历史（spec 方案 2 原始设想）。需重构 caller 为分段循环。
- **真实窗口感知**：`effective_window = min(声明, 探测, 显存估算)`，LM Studio 探测实际窗口 / `detect_gpu()` 兜底，替换 `context_size` 误判。
- **provider 参数**：LM Studio load 用安全 `context_length`、失败可见化；Ollama 传 `num_ctx`。
- **错误可读化**：解析本地 400/OOM 文本分类成可读提示。

## Self-Review

**1. Spec coverage:**
- 精简模式（不挂 tools + 精简 system，配置驱动不写死 backend）→ Task 1（字段）+ Task 2（组装短路）+ Task 5（前端勾选）✓
- summarizer 历史长度上限 → Task 3（budget 截断）+ Task 4（caller 传 budget）✓
- spec 方案 2 的"分段合并"→ 简化为"截断"实现，分段列为可选后续（已在 Task 3 注明，诚实标注偏差）⚠
- `should_summarize_by_tokens` 用 budget 而非 100K → Task 4 用 `summary_budget` 替换 `context_window.max(100_000)`，等价覆盖该意图 ✓

**2. Placeholder scan:** 无 TBD/TODO；每步有完整代码、确切路径、确切命令。i18n 行号用 Grep 定位（已给命令）。✓

**3. Type consistency:** `compact_mode: bool`（Task 1）→ `compact_mode()` 返回 `bool`（Task 2）→ 前端 `compact_mode?: boolean`（Task 5），一致。`build_prompt_segmented` 新签名 `(history, from, to, existing, budget)` 在 Task 3 定义、Task 4 调用一致。✓

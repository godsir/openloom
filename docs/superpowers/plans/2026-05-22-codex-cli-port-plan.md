# Codex CLI 移植实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 Codex CLI 源码（F:/codex/codex-rs/）移植到 openLoom，替换现有 CLI，通过 loom-app-server 适配层对接 openLoom Engine。

**Architecture:** 分 7 个阶段。先搬纯数据/工具 crate（不改代码），再搬需修改的协议层 crate（去掉 OpenAI 依赖），接着建 loom-app-server 适配层，最后修改 TUI/CLI 对接适配层。每个阶段结束 `cargo check --workspace` 验证。

**Tech Stack:** Rust 2024, tokio, ratatui, crossterm, clap, serde, reqwest

---

## 依赖分层图（决定搬的顺序）

```
Level 0: async-utils, absolute-path, home-dir, string, elapsed, path-utils,
         ansi-escape, arg0, install-context, terminal-detection, rustls-provider, uds
Level 1: fuzzy-match, sandbox-summary, approval-presets, cli, oss, file-system,
         sleep-inhibitor, plugins
Level 2: execpolicy, network-proxy, shell-command, sandboxing, exec-server
Level 3: protocol (deps on L1+L2 crates)
Level 4: app-server-protocol (deps on protocol, shell-command)
Level 5: features (deps on protocol — 需去掉 codex-otel), core-skills, tools
Level 6: config (deps on app-server-protocol, features — 需去掉 model-provider-info)
Level 7: app-server-client (deps on app-server-protocol — 需去掉 app-server/core/feedback)
Level 8: TUI (deps on app-server-client — 需去掉大量 OpenAI crate)
Level 9: CLI (deps on TUI — 去掉 login/cloud/update 等)
```

---

## Phase 1: Workspace 准备 + Level 0 纯工具 crate

### Task 1.1: 搭建目录结构和 workspace 注册

**Files:**
- Modify: `F:/openLoom/Cargo.toml`
- Create: `F:/openLoom/crates/loom-*/` (多个 crate 目录)

- [ ] **Step 1: 在 openLoom workspace 添加 Codex crate 的路径**

在 `F:/openLoom/Cargo.toml` 的 `members` 中追加新 crate 的 glob pattern：

```toml
[workspace]
resolver = "2"
members = [
    "crates/*",
    "crates/loom-utils/*",
    "crates/loom-protocol/*",
]
```

同时添加 `[workspace.package]` 字段：

```toml
[workspace.package]
version = "0.2.0"
edition = "2024"
license = "Apache-2.0"
```

- [ ] **Step 2: 创建目录结构**

```bash
mkdir -p F:/openLoom/crates/loom-utils
mkdir -p F:/openLoom/crates/loom-protocol
```

- [ ] **Step 3: 验证 workspace 解析**

```bash
cd F:/openLoom && cargo check --workspace 2>&1 | head -5
```

Expected: 现有 openLoom crate 编译通过，新目录被识别但无成员报错可忽略。

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml crates/loom-utils/ crates/loom-protocol/
git commit -m "chore: prepare workspace structure for Codex crate migration"
```

### Task 1.2: 搬运 Level 0 纯工具 crate（批量，不改代码）

**搬运清单（15 个 crate）：**

| Codex 路径 | openLoom 路径 | crate 名 |
|-----------|--------------|---------|
| `async-utils/` | `crates/loom-utils/async-utils/` | `loom-async-utils` |
| `utils/absolute-path/` | `crates/loom-utils/absolute-path/` | `loom-absolute-path` |
| `utils/home-dir/` | `crates/loom-utils/home-dir/` | `loom-home-dir` |
| `utils/string/` | `crates/loom-utils/string/` | `loom-string` |
| `utils/elapsed/` | `crates/loom-utils/elapsed/` | `loom-elapsed` |
| `utils/path-utils/` | `crates/loom-utils/path/` | `loom-path-utils` |
| `ansi-escape/` | `crates/loom-utils/ansi-escape/` | `loom-ansi-escape` |
| `arg0/` | `crates/loom-utils/arg0/` | `loom-arg0` |
| `install-context/` | `crates/loom-utils/install-context/` | `loom-install-context` |
| `terminal-detection/` | `crates/loom-utils/terminal-detection/` | `loom-terminal-detection` |
| `utils/rustls-provider/` | `crates/loom-utils/rustls-provider/` | `loom-rustls-provider` |
| `uds/` | `crates/loom-utils/uds/` | `loom-uds` |
| `utils/fuzzy-match/` | `crates/loom-utils/fuzzy-match/` | `loom-fuzzy-match` |
| `utils/sandbox-summary/` | `crates/loom-utils/sandbox-summary/` | `loom-sandbox-summary` |
| `utils/approval-presets/` | `crates/loom-utils/approval-presets/` | `loom-approval-presets` |

对于每个 crate 执行以下步骤：

- [ ] **Step 1: 复制 crate 目录**

```bash
# 示例：第一个
cp -r F:/codex/codex-rs/async-utils F:/openLoom/crates/loom-utils/async-utils
```

- [ ] **Step 2: 修改 Cargo.toml — package name 和内部依赖引用**

示例：`crates/loom-utils/async-utils/Cargo.toml`
```toml
[package]
name = "loom-async-utils"
version.workspace = true
edition.workspace = true
license.workspace = true
```

搜索所有 `.rs` 和 `Cargo.toml` 中的 `codex-async-utils` → 替换为 `loom-async-utils`。

对所有 15 个 crate 重复。

- [ ] **Step 3: 添加到 workspace dependencies**

在 `F:/openLoom/Cargo.toml` 的 `[workspace.dependencies]` 添加：

```toml
loom-async-utils = { path = "crates/loom-utils/async-utils" }
loom-absolute-path = { path = "crates/loom-utils/absolute-path" }
# ... 其余 13 个
```

- [ ] **Step 4: 编译验证**

```bash
cd F:/openLoom && cargo check 2>&1 | tail -20
```

Expected: 新 crate 编译通过。

- [ ] **Step 5: Commit**

```bash
git add crates/loom-utils/ Cargo.toml
git commit -m "chore: port Level 0 utility crates from Codex"
```

---

## Phase 2: Level 1~2 功能 crate + 外部依赖补全

### Task 2.1: 搬运 Level 1~2 crate

**搬运清单：**

| Codex 路径 | openLoom 路径 | crate 名 |
|-----------|--------------|---------|
| `execpolicy/` | `crates/loom-protocol/execpolicy/` | `loom-execpolicy` |
| `network-proxy/` | `crates/loom-protocol/network-proxy/` | `loom-network-proxy` |
| `shell-command/` | `crates/loom-protocol/shell-command/` | `loom-shell-command` |
| `sandboxing/` | `crates/loom-protocol/sandboxing/` | `loom-sandboxing` |
| `exec-server/` | `crates/loom-protocol/exec-server/` | `loom-exec-server` |
| `file-system/` | `crates/loom-utils/file-system/` | `loom-file-system` |
| `file-search/` | `crates/loom-utils/file-search/` | `loom-file-search` |
| `git-utils/` | `crates/loom-utils/git-utils/` | `loom-git-utils` |
| `utils/sleep-inhibitor/` | `crates/loom-utils/sleep-inhibitor/` | `loom-sleep-inhibitor` |
| `utils/oss/` | `crates/loom-utils/oss/` | `loom-oss` |
| `utils/plugins/` | `crates/loom-utils/plugins/` | `loom-plugins` |
| `utils/cli/` | `crates/loom-utils/cli/` | `loom-cli-utils` |
| `core-plugins/` | `crates/loom-protocol/core-plugins/` | `loom-core-plugins` |
| `core-skills/` | `crates/loom-protocol/core-skills/` | `loom-core-skills` |
| `tools/` | `crates/loom-protocol/tools/` | `loom-tools` |
| `hooks/` | `crates/loom-protocol/hooks/` | `loom-hooks` |
| `external-agent-sessions/` | `crates/loom-protocol/external-agent-sessions/` | `loom-external-agent-sessions` |
| `external-agent-migration/` | `crates/loom-protocol/external-agent-migration/` | `loom-external-agent-migration` |

对每个 crate：复制目录 → 改 Cargo.toml name → 全局替换 crate 名引用 → 注册到 workspace。

- [ ] **Step 1: 批量复制**

```bash
SRC=F:/codex/codex-rs
DST=F:/openLoom/crates
cp -r $SRC/execpolicy $DST/loom-protocol/execpolicy
cp -r $SRC/network-proxy $DST/loom-protocol/network-proxy
cp -r $SRC/shell-command $DST/loom-protocol/shell-command
cp -r $SRC/sandboxing $DST/loom-protocol/sandboxing
cp -r $SRC/exec-server $DST/loom-protocol/exec-server
cp -r $SRC/file-system $DST/loom-utils/file-system
cp -r $SRC/file-search $DST/loom-utils/file-search
cp -r $SRC/git-utils $DST/loom-utils/git-utils
cp -r $SRC/utils/sleep-inhibitor $DST/loom-utils/sleep-inhibitor
cp -r $SRC/utils/oss $DST/loom-utils/oss
cp -r $SRC/utils/plugins $DST/loom-utils/plugins
cp -r $SRC/utils/cli $DST/loom-utils/cli
cp -r $SRC/core-plugins $DST/loom-protocol/core-plugins
cp -r $SRC/core-skills $DST/loom-protocol/core-skills
cp -r $SRC/tools $DST/loom-protocol/tools
cp -r $SRC/hooks $DST/loom-protocol/hooks
cp -r $SRC/external-agent-sessions $DST/loom-protocol/external-agent-sessions
cp -r $SRC/external-agent-migration $DST/loom-protocol/external-agent-migration
```

- [ ] **Step 2: 批量重命名 package name**

对每个 crate 的 `Cargo.toml`，将 `name = "codex-xxx"` 改为 `name = "loom-xxx"`。

同时全局替换内部 `codex-xxx` 引用为 `loom-xxx`：

```bash
# 示例：execpolicy
cd F:/openLoom/crates/loom-protocol/execpolicy
sed -i 's/name = "codex-execpolicy"/name = "loom-execpolicy"/' Cargo.toml
grep -rl 'codex-execpolicy' . | xargs sed -i 's/codex-execpolicy/loom-execpolicy/g'
grep -rl 'codex_execpolicy' . | xargs sed -i 's/codex_execpolicy/loom_execpolicy/g'
```

对每个 crate 执行类似替换。注意 `-` 和 `_` 两种形式。

- [ ] **Step 3: 注册 workspace 依赖**

在 `F:/openLoom/Cargo.toml` 的 `[workspace.dependencies]` 添加所有新 crate：

```toml
loom-execpolicy = { path = "crates/loom-protocol/execpolicy" }
loom-network-proxy = { path = "crates/loom-protocol/network-proxy" }
loom-shell-command = { path = "crates/loom-protocol/shell-command" }
loom-sandboxing = { path = "crates/loom-protocol/sandboxing" }
loom-exec-server = { path = "crates/loom-protocol/exec-server" }
loom-file-system = { path = "crates/loom-utils/file-system" }
loom-file-search = { path = "crates/loom-utils/file-search" }
loom-git-utils = { path = "crates/loom-utils/git-utils" }
loom-sleep-inhibitor = { path = "crates/loom-utils/sleep-inhibitor" }
loom-oss = { path = "crates/loom-utils/oss" }
loom-plugins = { path = "crates/loom-utils/plugins" }
loom-cli-utils = { path = "crates/loom-utils/cli" }
loom-core-plugins = { path = "crates/loom-protocol/core-plugins" }
loom-core-skills = { path = "crates/loom-protocol/core-skills" }
loom-tools = { path = "crates/loom-protocol/tools" }
loom-hooks = { path = "crates/loom-protocol/hooks" }
loom-external-agent-sessions = { path = "crates/loom-protocol/external-agent-sessions" }
loom-external-agent-migration = { path = "crates/loom-protocol/external-agent-migration" }
```

- [ ] **Step 4: 补全外部依赖**

Codex 的 workspace `Cargo.toml` 定义了所有外部 crate 版本。需要把用到的外部依赖复制到 openLoom 的 `[workspace.dependencies]`。

读取 `F:/codex/codex-rs/Cargo.toml` 的 `[workspace.dependencies]` 节，提取本次涉及的外部 crate（如 `chardetng`, `encoding_rs`, `globset`, `landlock`, `seccompiler`, `wildmatch`, `rmcp`, `schemars`, `ts-rs`, `strum`, `strum_macros`, `diffy`, `pulldown-cmark`, `ratatui`, `ratatui-macros`, `two-face`, `syntect`, `arboard`, `crossterm` 等），追加到 openLoom 的 `[workspace.dependencies]`。

```bash
# 从 Codex 提取依赖版本
cd F:/codex/codex-rs
# 查看需要的版本
grep -E '^(chardetng|encoding_rs|globset|wildmatch|rmcp|schemars|ts-rs|strum|strum_macros|diffy|pulldown-cmark|ratatui|two-face|syntect|arboard|crossterm|owo-colors|supports-color|dirs|dunce|pathdiff|itertools|lazy_static|derive_more|shlex|textwrap|unicode-segmentation|unicode-width|color-eyre|insta|serial_test|wiremock|pretty_assertions|assert_cmd|assert_matches|predicates|http|libc)' F:/codex/codex-rs/Cargo.toml
```

- [ ] **Step 5: 编译验证**

```bash
cd F:/openLoom && cargo check 2>&1 | tail -30
```

Expected: 可能有缺少外部依赖的错误，逐个补全。

- [ ] **Step 6: Commit**

```bash
git add crates/loom-protocol/ crates/loom-utils/ Cargo.toml
git commit -m "chore: port Level 1-2 functional crates from Codex"
```

---

## Phase 3: Level 3~4 协议核心 + 需修改的 crate

### Task 3.1: 搬运 protocol crate

- [ ] **Step 1: 复制 protocol crate**

```bash
cp -r F:/codex/codex-rs/protocol F:/openLoom/crates/loom-protocol/protocol
```

- [ ] **Step 2: 重命名 + 替换内部引用**

```bash
cd F:/openLoom/crates/loom-protocol/protocol
sed -i 's/name = "codex-protocol"/name = "loom-protocol"/' Cargo.toml
sed -i 's/codex-protocol/loom-protocol/g' Cargo.toml
grep -rl 'codex_protocol' src/ | xargs sed -i 's/codex_protocol/loom_protocol/g'
grep -rl 'codex-protocol' src/ | xargs sed -i 's/codex-protocol/loom-protocol/g'
grep -rl 'codex-async-utils' . | xargs sed -i 's/codex-async-utils/loom-async-utils/g'
grep -rl 'codex-execpolicy' . | xargs sed -i 's/codex-execpolicy/loom-execpolicy/g'
grep -rl 'codex-network-proxy' . | xargs sed -i 's/codex-network-proxy/loom-network-proxy/g'
grep -rl 'codex-utils-absolute-path' . | xargs sed -i 's/codex-utils-absolute-path/loom-absolute-path/g'
grep -rl 'codex-utils-image' . | xargs sed -i 's/codex-utils-image/loom-image/g'
grep -rl 'codex-utils-string' . | xargs sed -i 's/codex-utils-string/loom-string/g'
```

- [ ] **Step 3: 处理 Linux-only 依赖**

在 `Cargo.toml` 中，`landlock` 和 `seccompiler` 是 Linux-only。在 Windows 上编译时需要条件编译。保持不变即可——这些依赖在 `[target.'cfg(target_os = "linux")'.dependencies]` 下。

- [ ] **Step 4: 也需要搬 `utils/image`**

`protocol` 依赖 `codex-utils-image`。需要把 `F:/codex/codex-rs/utils/image/` 也搬过来：

```bash
cp -r F:/codex/codex-rs/utils/image F:/openLoom/crates/loom-utils/image
```

重命名为 `loom-image`，加到 workspace。

- [ ] **Step 5: 注册 + 编译验证**

```bash
cd F:/openLoom && cargo check -p loom-protocol 2>&1 | tail -20
```

- [ ] **Step 6: Commit**

```bash
git add crates/loom-protocol/protocol crates/loom-utils/image Cargo.toml
git commit -m "chore: port protocol crate from Codex"
```

### Task 3.2: 搬运 + 修改 app-server-protocol

- [ ] **Step 1: 复制**

```bash
cp -r F:/codex/codex-rs/app-server-protocol F:/openLoom/crates/loom-protocol/app-server-protocol
```

- [ ] **Step 2: 重命名 crate**

```bash
cd F:/openLoom/crates/loom-protocol/app-server-protocol
sed -i 's/name = "codex-app-server-protocol"/name = "loom-app-server-protocol"/' Cargo.toml
sed -i 's/codex_app_server_protocol/loom_app_server_protocol/g' Cargo.toml
# 替换所有内部依赖引用
grep -rl 'codex-app-server-protocol' . | xargs sed -i 's/codex-app-server-protocol/loom-app-server-protocol/g'
grep -rl 'codex_app_server_protocol' src/ | xargs sed -i 's/codex_app_server_protocol/loom_app_server_protocol/g'
grep -rl 'codex-protocol' . | xargs sed -i 's/codex-protocol/loom-protocol/g'
grep -rl 'codex-shell-command' . | xargs sed -i 's/codex-shell-command/loom-shell-command/g'
grep -rl 'codex-utils-absolute-path' . | xargs sed -i 's/codex-utils-absolute-path/loom-absolute-path/g'
```

- [ ] **Step 3: 移除 codex-experimental-api-macros 依赖**

在 `Cargo.toml` 中删除 `codex-experimental-api-macros` 依赖行。该 crate 用于 derive 宏，openLoom 不需要。

检查 `src/` 中是否有使用该 macro 的代码。如果有 `use codex_experimental_api_macros::*` 或相关 derive，删除 derive 并手动展开或替换为 serde 标准宏。

- [ ] **Step 4: 注册 + 编译验证**

```bash
cd F:/openLoom && cargo check -p loom-app-server-protocol 2>&1 | tail -20
```

- [ ] **Step 5: Commit**

```bash
git add crates/loom-protocol/app-server-protocol Cargo.toml
git commit -m "chore: port app-server-protocol crate from Codex"
```

### Task 3.3: 搬运 + 修改 features crate

`features` 依赖 `codex-otel`（砍掉了）。需要去掉这个依赖并用 stub 类型替换。

- [ ] **Step 1: 复制 + 重命名**

```bash
cp -r F:/codex/codex-rs/features F:/openLoom/crates/loom-protocol/features
cd F:/openLoom/crates/loom-protocol/features
sed -i 's/name = "codex-features"/name = "loom-features"/' Cargo.toml
sed -i 's/codex-features/loom-features/g' Cargo.toml
grep -rl 'codex_features' src/ | xargs sed -i 's/codex_features/loom_features/g'
grep -rl 'codex-otel' . | xargs sed -i 's/codex-otel/loom-otel-stub/g'
grep -rl 'codex-protocol' . | xargs sed -i 's/codex-protocol/loom-protocol/g'
```

- [ ] **Step 2: 创建 loom-otel-stub crate**

创建 `F:/openLoom/crates/loom-utils/otel-stub/` 目录，只提供 `features` 需要的类型：

`F:/openLoom/crates/loom-utils/otel-stub/Cargo.toml`:
```toml
[package]
name = "loom-otel-stub"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
serde = { workspace = true, features = ["derive"] }
```

`F:/openLoom/crates/loom-utils/otel-stub/src/lib.rs`:
```rust
use serde::{Deserialize, Serialize};

/// Stub replacement for codex-otel SessionTelemetry
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SessionTelemetry;

/// Stub replacement for codex-otel RuntimeMetricsSummary
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RuntimeMetricsSummary;

/// Stub replacement for codex-otel TelemetryAuthMode
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum TelemetryAuthMode {
    #[default]
    Disabled,
}
```

- [ ] **Step 3: 编译验证**

```bash
cd F:/openLoom && cargo check -p loom-features 2>&1 | tail -20
```

如果 `features/src/lib.rs` 还引用了其他 `codex-otel` 类型，补充 stub。

- [ ] **Step 4: Commit**

```bash
git add crates/loom-protocol/features crates/loom-utils/otel-stub Cargo.toml
git commit -m "chore: port features crate with otel stub replacement"
```

---

## Phase 4: 搬运 + 修改 config 和 app-server-client（关键接缝）

### Task 4.1: 搬运 + 修改 config crate

`config` 依赖 `codex-model-provider-info`（砍掉）和 `codex-app-server-protocol`（已搬）。

- [ ] **Step 1: 复制 + 重命名**

```bash
cp -r F:/codex/codex-rs/config F:/openLoom/crates/loom-protocol/config
cd F:/openLoom/crates/loom-protocol/config
sed -i 's/name = "codex-config"/name = "loom-config"/' Cargo.toml
grep -rl 'codex-config' . | xargs sed -i 's/codex-config/loom-config/g'
grep -rl 'codex-config' . -l | xargs sed -i 's/codex-config/loom-config/g'
grep -rl 'codex_' src/ -l | xargs sed -i 's/codex_/loom_/g'
grep -rl 'codex-app-server-protocol' . | xargs sed -i 's/codex-app-server-protocol/loom-app-server-protocol/g'
grep -rl 'codex-protocol' . | xargs sed -i 's/codex-protocol/loom-protocol/g'
grep -rl 'codex-features' . | xargs sed -i 's/codex-features/loom-features/g'
grep -rl 'codex-execpolicy' . | xargs sed -i 's/codex-execpolicy/loom-execpolicy/g'
grep -rl 'codex-network-proxy' . | xargs sed -i 's/codex-network-proxy/loom-network-proxy/g'
grep -rl 'codex-file-system' . | xargs sed -i 's/codex-file-system/loom-file-system/g'
grep -rl 'codex-git-utils' . | xargs sed -i 's/codex-git-utils/loom-git-utils/g'
grep -rl 'codex-utils-absolute-path' . | xargs sed -i 's/codex-utils-absolute-path/loom-absolute-path/g'
grep -rl 'codex-utils-path' . | xargs sed -i 's/codex-utils-path/loom-path-utils/g'
```

- [ ] **Step 2: 去掉 model-provider-info 依赖**

在 `Cargo.toml` 中删除 `codex-model-provider-info` 行。

搜索 `src/` 中引用 `model_provider_info` 或 `ModelProviderInfo` 的代码。该类定义了模型元数据（名称、上下文窗口等）。需要创建 openLoom 的模型配置桥接。

在 `F:/openLoom/crates/loom-protocol/config/src/` 中新建 `model_info.rs`：

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelInfo {
    pub name: String,
    pub context_window: Option<usize>,
    pub max_output_tokens: Option<usize>,
}

impl ModelInfo {
    pub fn from_loom_config(model: &openloom_models::ModelConfig) -> Self {
        Self {
            name: model.model.clone(),
            context_window: model.context_size,
            max_output_tokens: model.max_output_tokens,
        }
    }
}
```

在 `config` 的 `Cargo.toml` 中添加：
```toml
openloom-models = { path = "../../models" }
```

- [ ] **Step 3: 编译验证 + 修错**

```bash
cd F:/openLoom && cargo check -p loom-config 2>&1 | tail -30
```

逐个修复编译错误。

- [ ] **Step 4: Commit**

```bash
git add crates/loom-protocol/config Cargo.toml
git commit -m "chore: port config crate with model-info bridge to openLoom"
```

### Task 4.2: 搬运 + 大幅修改 app-server-client（核心接缝）

这是最关键的一步。`app-server-client` 原来依赖 `codex-app-server`（后端实现）和 `codex-core`（核心运行时），这两个都砍掉。需要把 `AppServerClient::InProcess` 替换为直接使用 openLoom Engine。

- [ ] **Step 1: 复制 + 重命名**

```bash
cp -r F:/codex/codex-rs/app-server-client F:/openLoom/crates/loom-protocol/app-server-client
cd F:/openLoom/crates/loom-protocol/app-server-client
sed -i 's/name = "codex-app-server-client"/name = "loom-app-server-client"/' Cargo.toml
grep -rl 'codex-app-server-client' . | xargs sed -i 's/codex-app-server-client/loom-app-server-client/g'
grep -rl 'codex_app_server_client' src/ | xargs sed -i 's/codex_app_server_client/loom_app_server_client/g'
grep -rl 'codex-app-server-protocol' . | xargs sed -i 's/codex-app-server-protocol/loom-app-server-protocol/g'
grep -rl 'codex-protocol' . | xargs sed -i 's/codex-protocol/loom-protocol/g'
grep -rl 'codex-config' . | xargs sed -i 's/codex-config/loom-config/g'
grep -rl 'codex-arg0' . | xargs sed -i 's/codex-arg0/loom-arg0/g'
grep -rl 'codex-exec-server' . | xargs sed -i 's/codex-exec-server/loom-exec-server/g'
grep -rl 'codex-uds' . | xargs sed -i 's/codex-uds/loom-uds/g'
grep -rl 'codex-utils-absolute-path' . | xargs sed -i 's/codex-utils-absolute-path/loom-absolute-path/g'
grep -rl 'codex-utils-rustls-provider' . | xargs sed -i 's/codex-utils-rustls-provider/loom-rustls-provider/g'
```

- [ ] **Step 2: 重写 Cargo.toml 依赖**

删除：
```toml
codex-app-server = { workspace = true }
codex-core = { workspace = true }
codex-feedback = { workspace = true }
```

添加：
```toml
openloom-engine = { path = "../../engine" }
openloom-models = { path = "../../models" }
```

- [ ] **Step 3: 重写 `src/lib.rs` — AppServerClient enum**

将 `InProcess` 变体替换为 `Loom`：

```rust
use openloom_engine::{Engine, EngineConfig};

pub enum AppServerClient {
    Loom(LoomAppServerClient),
    Remote(RemoteAppServerClient),
}
```

- [ ] **Step 4: 实现 LoomAppServerClient**

在 `F:/openLoom/crates/loom-protocol/app-server-client/src/loom.rs` 新建文件：

```rust
use openloom_engine::Engine;
use loom_app_server_protocol::{ClientRequest, ServerNotification, ServerRequest};
use tokio::sync::mpsc;

pub struct LoomAppServerClient {
    engine: Engine,
    event_rx: mpsc::UnboundedReceiver<AppServerEvent>,
}

impl LoomAppServerClient {
    pub async fn new(config: EngineConfig) -> Result<Self> {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let engine = Engine::new(config).await?;
        // Subscribe to engine events and forward to event_tx
        // ...
        Ok(Self { engine, event_rx })
    }

    pub async fn request_typed<R: serde::de::DeserializeOwned>(
        &self,
        req: ClientRequest,
    ) -> Result<R, TypedRequestError> {
        match req {
            ClientRequest::TurnStart(params) => {
                // Translate to engine.handle_message_streaming()
                // ...
            }
            ClientRequest::ThreadStart(params) => {
                // Translate to engine session_tx
                // ...
            }
            // ... more mappings
            _ => Err(TypedRequestError::MethodNotFound),
        }
    }

    pub fn events(&mut self) -> Option<AppServerEvent> {
        self.event_rx.try_recv().ok()
    }
}
```

- [ ] **Step 5: 编译验证**

```bash
cd F:/openLoom && cargo check -p loom-app-server-client 2>&1 | tail -40
```

Expected: 大量错误。逐个修：适配 `ClientRequest` 的所有 variant，映射 `ServerNotification` 的所有 variant。

- [ ] **Step 6: Commit**

```bash
git add crates/loom-protocol/app-server-client Cargo.toml
git commit -m "feat: port app-server-client with LoomAppServerClient adapter"
```

---

## Phase 5: 搬运 MCP 相关 crate + 其他独立功能 crate

### Task 5.1: 搬运 MCP crate

**搬运清单：**

| Codex 路径 | openLoom 路径 | crate 名 |
|-----------|--------------|---------|
| `codex-mcp/` | `crates/loom-protocol/mcp/` | `loom-mcp` |
| `mcp-server/` | `crates/loom-protocol/mcp-server/` | `loom-mcp-server` |
| `rmcp-client/` | `crates/loom-protocol/rmcp-client/` | `loom-rmcp-client` |

- [ ] **Step 1: 复制 + 重命名 + 替换引用**

```bash
cp -r F:/codex/codex-rs/codex-mcp F:/openLoom/crates/loom-protocol/mcp
cp -r F:/codex/codex-rs/mcp-server F:/openLoom/crates/loom-protocol/mcp-server
cp -r F:/codex/codex-rs/rmcp-client F:/openLoom/crates/loom-protocol/rmcp-client
```

每个 crate：改 Cargo.toml name → 替换 codex-* 引用 → 注册 workspace 依赖。

- [ ] **Step 2: 编译验证**

```bash
cd F:/openLoom && cargo check -p loom-mcp -p loom-mcp-server -p loom-rmcp-client 2>&1 | tail -20
```

- [ ] **Step 3: Commit**

```bash
git add crates/loom-protocol/mcp crates/loom-protocol/mcp-server crates/loom-protocol/rmcp-client Cargo.toml
git commit -m "chore: port MCP crates from Codex"
```

### Task 5.2: 搬运 codex-chatgpt（保留 apply/prompt 功能，去掉 auth）

`codex-chatgpt` 提供 ChatGPT 格式的 apply/prompt 功能。去掉 `codex-login` 依赖，保留纯功能。

- [ ] **Step 1: 复制 + 重命名**

```bash
cp -r F:/codex/codex-rs/chatgpt F:/openLoom/crates/loom-protocol/chatgpt
```

重命名为 `loom-chatgpt`，替换所有 codex-* 引用。

- [ ] **Step 2: 去掉 codex-login 和 codex-core 依赖**

在 `Cargo.toml` 中删除这两个依赖。检查 `src/` 中使用它们的代码：
- 如果只是类型引用，用 `loom-app-server-client` 的类型替代
- 如果涉及实际登录逻辑，删除相关函数

- [ ] **Step 3: 注册 + 编译**

```bash
cd F:/openLoom && cargo check -p loom-chatgpt 2>&1 | tail -20
```

- [ ] **Step 4: Commit**

```bash
git add crates/loom-protocol/chatgpt Cargo.toml
git commit -m "chore: port chatgpt crate, strip auth dependencies"
```

---

## Phase 6: 搬运 TUI（最大修改量）

TUI 有最多需要去掉的依赖：`codex-chatgpt`, `codex-login`, `codex-model-provider`, `codex-models-manager`, `codex-otel`, `codex-state`, `codex-rollout`, `codex-plugin`, `codex-connectors`, `codex-cloud-requirements`, `codex-feedback`, `codex-realtime-webrtc`, `codex-message-history`。

### Task 6.1: 搬运 TUI 源码

- [ ] **Step 1: 复制 + 重命名**

```bash
cp -r F:/codex/codex-rs/tui F:/openLoom/crates/loom-protocol/tui
cd F:/openLoom/crates/loom-protocol/tui
sed -i 's/name = "codex-tui"/name = "loom-tui"/' Cargo.toml
sed -i 's/name = "codex-tui"/name = "loom-tui"/g' Cargo.toml
```

- [ ] **Step 2: 全局替换 crate 引用**

```bash
# 保留的 crate 映射
grep -rl 'codex-app-server-client' . | xargs sed -i 's/codex-app-server-client/loom-app-server-client/g'
grep -rl 'codex_app_server_client' src/ | xargs sed -i 's/codex_app_server_client/loom_app_server_client/g'
grep -rl 'codex-app-server-protocol' . | xargs sed -i 's/codex-app-server-protocol/loom-app-server-protocol/g'
grep -rl 'codex-protocol' . | xargs sed -i 's/codex-protocol/loom-protocol/g'
grep -rl 'codex-config' . | xargs sed -i 's/codex-config/loom-config/g'
grep -rl 'codex-features' . | xargs sed -i 's/codex-features/loom-features/g'
grep -rl 'codex-ansi-escape' . | xargs sed -i 's/codex-ansi-escape/loom-ansi-escape/g'
grep -rl 'codex-arg0' . | xargs sed -i 's/codex-arg0/loom-arg0/g'
grep -rl 'codex-install-context' . | xargs sed -i 's/codex-install-context/loom-install-context/g'
grep -rl 'codex-chatgpt' . | xargs sed -i 's/codex-chatgpt/loom-chatgpt/g'
grep -rl 'codex-core-plugins' . | xargs sed -i 's/codex-core-plugins/loom-core-plugins/g'
grep -rl 'codex-core-skills' . | xargs sed -i 's/codex-core-skills/loom-core-skills/g'
grep -rl 'codex-exec-server' . | xargs sed -i 's/codex-exec-server/loom-exec-server/g'
grep -rl 'codex-file-search' . | xargs sed -i 's/codex-file-search/loom-file-search/g'
grep -rl 'codex-git-utils' . | xargs sed -i 's/codex-git-utils/loom-git-utils/g'
grep -rl 'codex-shell-command' . | xargs sed -i 's/codex-shell-command/loom-shell-command/g'
grep -rl 'codex-sandboxing' . | xargs sed -i 's/codex-sandboxing/loom-sandboxing/g'
grep -rl 'codex-terminal-detection' . | xargs sed -i 's/codex-terminal-detection/loom-terminal-detection/g'
grep -rl 'codex-utils' . | xargs sed -i 's/codex-utils/loom-/g'
# ... 继续所有 utils crate
```

- [ ] **Step 2: 清理 Cargo.toml**

删除所有砍掉的依赖行，添加 loom 替代品：

```toml
# 删除
# codex-cloud-requirements, codex-connectors, codex-feedback,
# codex-login, codex-message-history, codex-model-provider,
# codex-model-provider-info, codex-models-manager, codex-otel,
# codex-plugin, codex-realtime-webrtc, codex-rollout, codex-state,
# codex-windows-sandbox

# 添加 loom 替代
loom-app-server-client = { workspace = true }
```

- [ ] **Step 3: 创建 stub crate 提供 TUI 需要的 cut 类型**

TUI 代码里引用了很多 cut crate 的类型。需要创建 `loom-tui-stubs` 提供这些类型的最小定义。

`F:/openLoom/crates/loom-utils/tui-stubs/Cargo.toml`:
```toml
[package]
name = "loom-tui-stubs"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
chrono = { workspace = true, features = ["serde"] }
serde = { workspace = true, features = ["derive"] }
serde_json = { workspace = true }
tokio = { workspace = true }
uuid = { workspace = true, features = ["serde", "v7"] }
```

`F:/openLoom/crates/loom-utils/tui-stubs/src/lib.rs` — 需要根据编译错误逐个补充 stub 类型。关键 stub：

```rust
// 替代 codex-state
pub mod state {
    use std::path::PathBuf;
    
    #[derive(Clone)]
    pub struct StateRuntime;
    impl StateRuntime {
        pub async fn new(_path: PathBuf) -> Result<Self> { Ok(Self) }
    }
    
    pub mod log_db {
        pub struct LogDbLayer;
    }
}

// 替代 codex-rollout
pub mod rollout {
    pub struct RolloutManager;
    impl RolloutManager {
        pub async fn new() -> Self { Self }
    }
}

// 替代 codex-feedback
pub mod feedback {
    use serde::{Deserialize, Serialize};
    
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct FeedbackDiagnostics;
    
    pub const DOCTOR_REPORT_ATTACHMENT_FILENAME: &str = "doctor-report.txt";
}

// 替代 codex-message-history
pub mod message_history {
    pub struct MessageHistory;
    impl MessageHistory {
        pub fn new() -> Self { Self }
    }
}
```

- [ ] **Step 4: 编译 + 循环修错**

```bash
cd F:/openLoom && cargo check -p loom-tui 2>&1 | grep "^error" | head -30
```

逐个分析错误：
- 如果是 cut crate 的类型缺失 → 加 stub
- 如果是函数调用链断了 → 转接到 loom-app-server-client
- 如果是 auth/login 相关 UI → 删除相关 UI 组件

**这个过程会很漫长，预计 50-100 个编译错误需要逐个处理。**

关键修改区域：
- `tui/src/app_server_session.rs` — `turn_start()` 调用链必须通
- `tui/src/chatwidget/protocol.rs` — `handle_server_notification()` 必须保持不变
- `tui/src/streaming/mod.rs` — 流式渲染不动
- `tui/src/app/thread_routing.rs` — 线程路由适配

- [ ] **Step 5: 所有编译错误修完后 Commit**

```bash
git add crates/loom-protocol/tui crates/loom-utils/tui-stubs Cargo.toml
git commit -m "feat: port TUI crate with loom backend integration"
```

---

## Phase 7: 搬运 CLI（入口 + 命令裁剪）

### Task 7.1: 搬运 CLI 源码 + 裁剪 OpenAI 命令

- [ ] **Step 1: 复制 + 重命名**

```bash
cp -r F:/codex/codex-rs/cli F:/openLoom/crates/loom-protocol/cli
cd F:/openLoom/crates/loom-protocol/cli
sed -i 's/name = "codex-cli"/name = "loom-cli"/' Cargo.toml
sed -i 's/name = "codex"/name = "loom"/' Cargo.toml
```

- [ ] **Step 2: 裁剪 Cargo.toml**

删除被砍 crate 的依赖：
```toml
# 删除
# codex-app-server, codex-app-server-daemon, codex-app-server-test-client,
# codex-api, codex-chatgpt, codex-cloud-tasks, codex-core,
# codex-login, codex-memories-write, codex-model-provider,
# codex-models-manager, codex-plugin, codex-responses-api-proxy,
# codex-rollout-trace, codex-state, codex-stdio-to-uds,
# codex-windows-sandbox
```

添加 loom 替代：
```toml
openloom-engine = { path = "../../engine" }
openloom-models = { path = "../../models" }
loom-app-server-client = { workspace = true }
loom-app-server-protocol = { workspace = true }
loom-config = { workspace = true }
loom-features = { workspace = true }
loom-protocol = { workspace = true }
loom-tui = { workspace = true }
```

- [ ] **Step 3: 裁剪 main.rs 子命令**

保留的子命令：
```rust
enum Subcommand {
    // 无子命令 = 默认交互式 TUI
    Exec { ... },       // 非交互编码
    Review { ... },     // 代码审查
    Mcp { ... },        // MCP 管理
    Plugin { ... },     // Plugin 管理
    Completion { ... }, // Shell 补全
    Doctor,             // 诊断
    Debug { ... },      // 调试
    Execpolicy { ... }, // 执行策略
    Apply,              // 应用 diff
    Resume { ... },     // 恢复会话
    Fork { ... },       // 分叉会话
    Sandbox { ... },    // 沙箱
}
```

删除的子命令：
- `Login`, `Logout` → 不需要认证
- `Update` → 不自更新
- `Cloud` → 无云端
- `AppServer`, `App` → 用 loom serve
- `RemoteControl` → 用 loom --remote
- `ResponsesApiProxy`, `StdioToUds`, `McpServer` → 不需要
- `Features` → 合并到 config

- [ ] **Step 4: 修改入口函数**

将原 `cli_main()` 中创建 `AppServer` 的代码替换为创建 `LoomAppServerClient`：

```rust
async fn cli_main() -> anyhow::Result<()> {
    let cli = MultitoolCli::parse();
    
    // Load openLoom config
    let app_config = load_app_config()?;
    let engine_config = EngineConfig::from_app_config(&app_config)?;
    
    // Create the loom app server client (in-process)
    let app_server = AppServerClient::Loom(
        LoomAppServerClient::new(engine_config).await?
    );
    
    match cli.subcommand {
        None => run_interactive_tui(app_server, cli).await?,
        Some(Subcommand::Exec { ... }) => run_exec(app_server, ...).await?,
        // ...
    }
}
```

- [ ] **Step 5: 保留 openLoom 特有命令**

在 CLI 中保留 openLoom 原生命令：
```rust
enum Subcommand {
    // ... Codex 移植命令
    Memory { ... },   // 记忆查看
    Persona,           // 人格查看
    Config { ... },    // 配置管理（合并 Codex config + openLoom config）
    Serve { ... },     // 启动服务（留给 Electron）
}
```

- [ ] **Step 6: 编译 + 修错**

```bash
cd F:/openLoom && cargo check -p loom-cli 2>&1 | grep "^error" | head -40
```

逐个修复。

- [ ] **Step 7: Commit**

```bash
git add crates/loom-protocol/cli Cargo.toml
git commit -m "feat: port CLI crate with Loom backend and stripped OpenAI commands"
```

---

## Phase 8: 集成测试

### Task 8.1: 适配层单元测试

- [ ] **Step 1: 创建测试文件**

`F:/openLoom/crates/loom-protocol/app-server-client/tests/adapter_tests.rs`:

```rust
use loom_app_server_client::LoomAppServerClient;
use loom_app_server_protocol::ClientRequest;
use openloom_engine::EngineConfig;
use tempfile::TempDir;

#[tokio::test]
async fn test_turn_start_maps_to_handle_message() {
    let tmp = TempDir::new().unwrap();
    let config = EngineConfig {
        data_dir: tmp.path().to_path_buf(),
        ..EngineConfig::test_default()
    };
    let client = LoomAppServerClient::new(config).await.unwrap();
    
    let response = client
        .request_typed::<TurnStartResponse>(ClientRequest::TurnStart(
            TurnStartParams {
                thread_id: "test-1".into(),
                items: vec![UserInput::text("Hello")],
                cwd: tmp.path().to_path_buf(),
                model: "test-model".into(),
                ..Default::default()
            }
        ))
        .await
        .unwrap();
    
    assert!(!response.turn_id.is_empty());
}
```

- [ ] **Step 2: 运行测试**

```bash
cd F:/openLoom && cargo test -p loom-app-server-client 2>&1 | tail -20
```

- [ ] **Step 3: 端到端测试**

`F:/openLoom/tests/codex_port_integration.rs`:

```rust
#[tokio::test]
async fn test_e2e_exec_simple_task() {
    // 启动 loom exec "echo hello"
    let output = std::process::Command::new("cargo")
        .args(["run", "-p", "loom-cli", "--", "exec", "echo hello"])
        .output()
        .unwrap();
    
    assert!(output.status.success());
}
```

- [ ] **Step 4: Commit**

```bash
git add tests/ crates/loom-protocol/app-server-client/tests/
git commit -m "test: add adapter unit tests and integration tests"
```

---

## Phase 9: 清理旧 CLI + 最终验证

### Task 9.1: 移除旧 CLI crate

- [ ] **Step 1: 从 workspace 移除旧 CLI**

在 `F:/openLoom/Cargo.toml` 的 `members` 中确认旧 CLI 不再被引用。

```bash
# 备份旧 CLI
mv F:/openLoom/crates/cli F:/openLoom/crates/cli.old
```

- [ ] **Step 2: 编译确认**

```bash
cd F:/openLoom && cargo check --workspace 2>&1 | tail -10
```

Expected: 全部通过。

- [ ] **Step 3: 全量测试**

```bash
cd F:/openLoom && cargo test --workspace 2>&1 | tail -30
```

Expected: 原有 180+ 测试 + 新增适配层测试全部通过。

- [ ] **Step 4: Clippy + fmt**

```bash
cargo fmt --all && cargo clippy --workspace -- -D warnings 2>&1 | tail -10
```

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "chore: remove old CLI crate, finalize Codex port"
```

---

## 附录 A: 完整 crate 清单（保留 vs 砍掉 vs 新建）

### 保留（改名移植）

```
crates/loom-utils/async-utils       (← async-utils)
crates/loom-utils/absolute-path      (← utils/absolute-path)
crates/loom-utils/home-dir           (← utils/home-dir)
crates/loom-utils/string             (← utils/string)
crates/loom-utils/elapsed            (← utils/elapsed)
crates/loom-utils/path               (← utils/path-utils)
crates/loom-utils/ansi-escape        (← ansi-escape)
crates/loom-utils/arg0               (← arg0)
crates/loom-utils/install-context    (← install-context)
crates/loom-utils/terminal-detection (← terminal-detection)
crates/loom-utils/rustls-provider    (← utils/rustls-provider)
crates/loom-utils/uds                (← uds)
crates/loom-utils/fuzzy-match        (← utils/fuzzy-match)
crates/loom-utils/sandbox-summary    (← utils/sandbox-summary)
crates/loom-utils/approval-presets   (← utils/approval-presets)
crates/loom-utils/file-system        (← file-system)
crates/loom-utils/file-search        (← file-search)
crates/loom-utils/git-utils          (← git-utils)
crates/loom-utils/sleep-inhibitor    (← utils/sleep-inhibitor)
crates/loom-utils/oss                (← utils/oss)
crates/loom-utils/plugins            (← utils/plugins)
crates/loom-utils/cli                (← utils/cli)
crates/loom-utils/image              (← utils/image)
crates/loom-utils/otel-stub          (NEW: replaces codex-otel)
crates/loom-utils/tui-stubs          (NEW: stub types for cut crates)

crates/loom-protocol/execpolicy               (← execpolicy)
crates/loom-protocol/network-proxy            (← network-proxy)
crates/loom-protocol/shell-command            (← shell-command)
crates/loom-protocol/sandboxing               (← sandboxing)
crates/loom-protocol/exec-server              (← exec-server)
crates/loom-protocol/core-plugins             (← core-plugins)
crates/loom-protocol/core-skills              (← core-skills)
crates/loom-protocol/tools                    (← tools)
crates/loom-protocol/hooks                    (← hooks)
crates/loom-protocol/external-agent-sessions  (← external-agent-sessions)
crates/loom-protocol/external-agent-migration (← external-agent-migration)
crates/loom-protocol/protocol                 (← protocol)
crates/loom-protocol/app-server-protocol      (← app-server-protocol)
crates/loom-protocol/features                 (← features)
crates/loom-protocol/config                   (← config)
crates/loom-protocol/app-server-client        (← app-server-client, MODIFIED)
crates/loom-protocol/mcp                      (← codex-mcp)
crates/loom-protocol/mcp-server               (← mcp-server)
crates/loom-protocol/rmcp-client              (← rmcp-client)
crates/loom-protocol/chatgpt                  (← chatgpt, MODIFIED)
crates/loom-protocol/tui                      (← tui, MODIFIED)
crates/loom-protocol/cli                      (← cli, MODIFIED)
```

### 砍掉（不搬运）

```
codex-api, codex-client, codex-backend-client, backend-openapi-models,
login, app-server, app-server-transport, app-server-daemon,
app-server-test-client, model-provider, models-manager, model-provider-info,
state, rollout, rollout-trace, cloud-requirements, cloud-tasks,
cloud-tasks-client, cloud-tasks-mock-client, secrets, keyring-store,
analytics, otel, realtime-webrtc, aws-auth, ollama, lmstudio,
linux-sandbox, windows-sandbox, process-hardening, shell-escalation,
extension-api, goal, guardian, memories (all 3), plugin,
thread-store, thread-manager-sample, agent-graph-store, agent-identity,
external-agent-migration-old, v8-poc, connectors, message-history,
collaboration-mode-templates, responses-api-proxy, response-debug-context,
stdi-to-uds, debug-client, test-binary-support, codex-experimental-api-macros,
utils/cargo-bin, utils/cache, utils/json-to-toml, utils/pty,
utils/readiness, utils/output-truncation, utils/stream-parser,
utils/template, apply-patch, bwrap, file-watcher
```

### 新建

```
crates/loom-utils/otel-stub    — 替代 codex-otel 的 stub 类型
crates/loom-utils/tui-stubs    — 替代 codex-state/rollout/feedback/message-history 等 cut crate 的 stub 类型
```

---

## 附录 B: 关键风险

| 风险 | 影响 | 缓解 |
|------|------|------|
| TUI 对 cut crate 的引用太深 | 编译错误 > 100 个 | 用 stub 类型逐步替换，每次聚焦一个 cut crate |
| `app-server-protocol` derive macro 依赖 | 需要手动展开宏 | 看实际使用量，如果多用 sed 替换为 serde 标准宏 |
| 外部依赖版本冲突 | workspace 依赖版本不一致 | 统一使用 Codex workspace 的版本（它们经过验证） |
| Engine API 与 Codex 协议不匹配 | 适配层需要大量类型转换 | 适配层做薄翻译，不引入中间抽象 |
| 编译时间暴增 | 新增 40+ crate | 分阶段 commit，每个 Phase 独立验证 |

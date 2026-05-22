# Codex CLI 移植实现计划（v2 — 审查补全版）

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 Codex CLI 源码（F:/codex/codex-rs/）移植到 openLoom，替换现有 CLI，通过 loom-app-server 适配层对接 openLoom Engine。

**Architecture:** 分 9 个阶段。先搭建 workspace 并复制 Codex 的 `[patch.crates-io]`（fork 了 crossterm/ratatui/tokio-tungstenite/tungstenite），再按依赖层级从底向上搬 crate。每层结束 `cargo check --workspace` 验证。TUI stub 类型基于审查结果精确列出。

**Tech Stack:** Rust 2024, tokio, ratatui (OpenAI fork), crossterm (OpenAI fork), clap, serde, reqwest

---

## 依赖分层图

```
Level 0: async-utils, absolute-path, home-dir, string, elapsed, path-utils,
         ansi-escape, arg0, install-context, terminal-detection, rustls-provider, uds
Level 1: fuzzy-match, sandbox-summary, approval-presets, cli-utils, oss, file-system,
         sleep-inhibitor, plugins, image
Level 2: execpolicy, network-proxy, shell-command, sandboxing, exec-server, skills
Level 3: protocol (deps on L1+L2)
Level 4: app-server-protocol (deps on protocol, shell-command)
Level 5: features (deps on protocol, otel→stub), core-skills (deps on skills, +cut deps→audit)
Level 6: config (deps on app-server-protocol, features, model-provider-info→bridge)
Level 7: app-server-client (deps on protocol crates, app-server/core/feedback→LoomEngine)
Level 8: TUI (deps on app-server-client, many cut crates→stubs)
Level 9: CLI (deps on TUI, exec→inline, cut crates→remove)
```

---

## Phase 1: Workspace 准备 + 补丁 + 外部依赖

### Task 1.1: 搭建目录结构、workspace 注册、[patch.crates-io]

**Files:**
- Modify: `F:/openLoom/Cargo.toml`

- [ ] **Step 1: 扩展 workspace members + 添加 package section**

在 `F:/openLoom/Cargo.toml` 中：

```toml
[workspace]
resolver = "2"
members = [
    "crates/*",
    "crates/loom-utils/*",
    "crates/loom-protocol/*",
]

[workspace.package]
version = "0.2.0"
edition = "2024"
license = "Apache-2.0"
```

- [ ] **Step 2: 复制 Codex 的 [patch.crates-io]**

Codex fork 了 4 个 crate。必须在 openLoom workspace 中加同样的 patch，否则 TUI 的渲染和 WebSocket 可能出错。

读取 `F:/codex/codex-rs/Cargo.toml` 末尾的 `[patch.crates-io]` 节，完整复制到 openLoom 的 `Cargo.toml`：

```toml
[patch.crates-io]
crossterm = { git = "https://github.com/nornagon/crossterm", rev = "<从Codex复制>" }
ratatui = { git = "https://github.com/nornagon/ratatui", rev = "<从Codex复制>" }
tokio-tungstenite = { git = "https://github.com/openai-oss-forks/tokio-tungstenite", rev = "<从Codex复制>" }
tungstenite = { git = "https://github.com/openai-oss-forks/tungstenite-rs", rev = "<从Codex复制>" }
```

**注意：** 必须从 `F:/codex/codex-rs/Cargo.toml` 精确复制 rev hash。

- [ ] **Step 3: 解决 dirs 版本冲突**

openLoom 现有 `dirs = "5"`，Codex 使用 `dirs = "6"`。统一升级到 `dirs = "6"`：

```toml
dirs = "6"
```

检查 openLoom 现有 crate 中有没有直接依赖 `dirs` 的地方，有的话升级版本号。

- [ ] **Step 4: 创建目录结构**

```bash
mkdir -p F:/openLoom/crates/loom-utils
mkdir -p F:/openLoom/crates/loom-protocol
```

- [ ] **Step 5: 从 Codex 提取外部依赖版本**

读取 `F:/codex/codex-rs/Cargo.toml` 的 `[workspace.dependencies]` 节，把用到的外部 crate 版本复制到 openLoom。涉及：

```
chardetng, encoding_rs, globset, wildmatch, rmcp, schemars, ts-rs,
strum, strum_macros, diffy, pulldown-cmark, ratatui, ratatui-macros,
two-face, syntect, arboard, crossterm, owo-colors, supports-color,
dirs, dunce, pathdiff, itertools, lazy_static, derive_more, shlex,
textwrap, unicode-segmentation, unicode-width, color-eyre, insta,
serial_test, wiremock, pretty_assertions, assert_cmd, assert_matches,
predicates, http, libc, base64, chrono, sha2, rand, image,
indexmap, multimap, serde_ignored, serde_path_to_error, gethostname,
dns-lookup, winapi-util, windows-sys, which, core-foundation, cpal,
tokio-stream, tokio-util, webbrowser, url, urlencoding, futures,
include_dir, regex-lite
```

执行：
```bash
grep -E '^[a-z]' F:/codex/codex-rs/Cargo.toml | grep -A2 'workspace.dependencies' | head -200
```

逐个追加到 openLoom 的 `[workspace.dependencies]`。

- [ ] **Step 6: 验证 workspace 解析**

```bash
cd F:/openLoom && cargo check --workspace 2>&1 | head -10
```

Expected: 现有 openLoom crate 编译通过（如有 dirs 版本冲突先修）。

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml crates/loom-utils/ crates/loom-protocol/
git commit -m "chore: prepare workspace, add Codex crate patches, resolve dirs version"
```

---

## Phase 2: Level 0~2 纯数据/工具 crate

### Task 2.1: 批量搬运 Level 0 crate（15 个，不改代码）

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

对每个 crate：
1. `cp -r` 复制目录
2. 修改 `Cargo.toml` 中 `name` 字段
3. 全局替换 `codex-xxx` → `loom-xxx`（包括 `-` 和 `_` 两种形式）
4. 注册到 workspace `[workspace.dependencies]`

- [ ] **Step 1: 批量复制并重命名**

```bash
SRC=F:/codex/codex-rs
DST=F:/openLoom/crates
# Level 0 全部
for crate in async-utils; do cp -r $SRC/$crate $DST/loom-utils/$crate; done
# ... 对每个 crate 重复
```

- [ ] **Step 2: 编译验证**

```bash
cd F:/openLoom && cargo check 2>&1 | tail -20
```

- [ ] **Step 3: Commit**

```bash
git add crates/loom-utils/ Cargo.toml
git commit -m "chore: port Level 0 utility crates from Codex"
```

### Task 2.2: 批量搬运 Level 1~2 crate（19 个）

新增 crate（审查发现的遗漏）：

| Codex 路径 | openLoom 路径 | crate 名 | 备注 |
|-----------|--------------|---------|------|
| `execpolicy/` | `crates/loom-protocol/execpolicy/` | `loom-execpolicy` | |
| `network-proxy/` | `crates/loom-protocol/network-proxy/` | `loom-network-proxy` | |
| `shell-command/` | `crates/loom-protocol/shell-command/` | `loom-shell-command` | |
| `sandboxing/` | `crates/loom-protocol/sandboxing/` | `loom-sandboxing` | |
| `exec-server/` | `crates/loom-protocol/exec-server/` | `loom-exec-server` | |
| `file-system/` | `crates/loom-utils/file-system/` | `loom-file-system` | |
| `file-search/` | `crates/loom-utils/file-search/` | `loom-file-search` | |
| `git-utils/` | `crates/loom-utils/git-utils/` | `loom-git-utils` | |
| `utils/sleep-inhibitor/` | `crates/loom-utils/sleep-inhibitor/` | `loom-sleep-inhibitor` | |
| `utils/oss/` | `crates/loom-utils/oss/` | `loom-oss` | |
| `utils/plugins/` | `crates/loom-utils/plugins/` | `loom-plugins` | |
| `utils/cli/` | `crates/loom-utils/cli/` | `loom-cli-utils` | |
| `utils/image/` | `crates/loom-utils/image/` | `loom-image` | |
| `skills/` | `crates/loom-protocol/skills/` | `loom-skills` | **审查新发现！** `core-skills` 的依赖，bundles SKILL.md 文件 |
| `core-plugins/` | `crates/loom-protocol/core-plugins/` | `loom-core-plugins` | |
| `core-skills/` | `crates/loom-protocol/core-skills/` | `loom-core-skills` | |
| `tools/` | `crates/loom-protocol/tools/` | `loom-tools` | |
| `hooks/` | `crates/loom-protocol/hooks/` | `loom-hooks` | |
| `external-agent-sessions/` | `crates/loom-protocol/external-agent-sessions/` | `loom-external-agent-sessions` | |
| `external-agent-migration/` | `crates/loom-protocol/external-agent-migration/` | `loom-external-agent-migration` | |

特殊处理：
- **`skills/`**：有 `build.rs`（`include_dir!` 打包 SKILL.md），必须保留。
- **`tools/`**：依赖 `codex-code-mode`、`codex-utils-output-truncation`、`codex-utils-pty`（都是 cut crate）。需要去掉这些依赖并 stub 相关类型。
- **`hooks/`**：依赖 `codex-plugin`、`codex-utils-output-truncation`（cut crate）。
- **`core-plugins/`**：依赖 `codex-analytics`、`codex-login`、`codex-model-provider`、`codex-otel`、`codex-plugin`（全是 cut crate）。
- **`core-skills/`**：依赖 `codex-analytics`、`codex-login`、`codex-model-provider`、`codex-otel`、`codex-skills`（已搬）、`codex-utils-output-truncation`。

**重要：** `core-plugins`、`core-skills`、`hooks`、`tools` 这 4 个 crate 的传递 cut 依赖在 Phase 2 暂时不修，先搬过来保留原始 Cargo.toml。Phase 3 会统一创建 shim stub 处理。

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
cp -r $SRC/utils/image $DST/loom-utils/image
cp -r $SRC/skills $DST/loom-protocol/skills
cp -r $SRC/core-plugins $DST/loom-protocol/core-plugins
cp -r $SRC/core-skills $DST/loom-protocol/core-skills
cp -r $SRC/tools $DST/loom-protocol/tools
cp -r $SRC/hooks $DST/loom-protocol/hooks
cp -r $SRC/external-agent-sessions $DST/loom-protocol/external-agent-sessions
cp -r $SRC/external-agent-migration $DST/loom-protocol/external-agent-migration
```

- [ ] **Step 2: 每个 crate 重命名 + 替换内部引用**

```bash
# 模板（以 execpolicy 为例）
cd F:/openLoom/crates/loom-protocol/execpolicy
sed -i 's/name = "codex-execpolicy"/name = "loom-execpolicy"/' Cargo.toml
grep -rl 'codex-execpolicy' . | xargs sed -i 's/codex-execpolicy/loom-execpolicy/g'
grep -rl 'codex_execpolicy' . | xargs sed -i 's/codex_execpolicy/loom_execpolicy/g'
```

对 19 个 crate 逐个执行。

- [ ] **Step 3: 注册 workspace 依赖**

在 `F:/openLoom/Cargo.toml` 的 `[workspace.dependencies]` 追加所有 19 个 crate。

- [ ] **Step 4: 先编译无传递问题的 crate**

```bash
cd F:/openLoom && cargo check -p loom-execpolicy -p loom-network-proxy -p loom-shell-command -p loom-sandboxing -p loom-exec-server -p loom-file-system -p loom-file-search -p loom-git-utils -p loom-sleep-inhibitor -p loom-oss -p loom-plugins -p loom-cli-utils -p loom-image -p loom-skills -p loom-external-agent-sessions 2>&1 | tail -20
```

Expected: 这些 crate 应该直接通过（依赖的都是已搬的 Level 0 crate 或外部 crate）。

- [ ] **Step 5: Commit**

```bash
git add crates/loom-protocol/ crates/loom-utils/ Cargo.toml
git commit -m "chore: port Level 1-2 crates from Codex (core-plugins/skills/hooks/tools pending stub fixes)"
```

---

## Phase 3: 传递依赖 shim + 协议核心 crate

### Task 3.1: 创建 shim stub crate 解决传递 cut 依赖

`core-plugins`、`core-skills`、`hooks`、`tools` 依赖了多个 cut crate。需要在 Phase 3 先创建 shim 让它们能编译。

创建 `F:/openLoom/crates/loom-utils/shim-stubs/`（Phase 3 专用轻量 shim）：

`Cargo.toml`:
```toml
[package]
name = "loom-shim-stubs"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
serde = { workspace = true, features = ["derive"] }
serde_json = { workspace = true }
tokio = { workspace = true }
tracing = { workspace = true }
```

`src/lib.rs`:
```rust
// Shim substitutes for cut crates needed by core-plugins, core-skills, hooks, tools

pub mod analytics {
    pub struct AnalyticsClient;
    impl AnalyticsClient {
        pub fn new() -> Self { Self }
    }
}

pub mod login {
    pub struct AuthManager;
    pub enum CodexAuth { None }
    pub mod default_client {
        pub fn originator() -> String { String::new() }
    }
}

pub mod model_provider {
    pub struct ModelProvider;
    impl ModelProvider {
        pub async fn new() -> Self { Self }
    }
}

pub mod otel {
    pub struct OtelManager;
    impl OtelManager {
        pub fn new() -> Self { Self }
    }
}

pub mod plugin {
    #[derive(Debug, Clone)]
    pub struct AppConnectorId(pub String);
    
    #[derive(Debug, Clone, Default)]
    pub struct PluginCapabilitySummary {
        pub config_name: String,
        pub display_name: String,
        pub description: Option<String>,
        pub has_skills: bool,
        pub mcp_server_names: Vec<String>,
        pub app_connector_ids: Vec<AppConnectorId>,
    }
}

pub mod utils_output_truncation {
    pub fn truncate(_s: &str, _max: usize) -> String { String::new() }
}

pub mod utils_pty {
    // stub
}

pub mod code_mode {
    // stub
}
```

然后修改 `core-plugins`、`core-skills`、`hooks`、`tools` 的 `Cargo.toml`，把 cut crate 引用替换为 `loom-shim-stubs`：

```bash
# core-plugins: analytics, login, model-provider, otel, plugin → loom-shim-stubs
# core-skills: analytics, login, model-provider, otel, utils-output-truncation → loom-shim-stubs
# hooks: plugin, utils-output-truncation → loom-shim-stubs
# tools: code-mode, utils-output-truncation, utils-pty → loom-shim-stubs
```

- [ ] **Step 1: 创建 loom-shim-stubs**

```bash
mkdir -p F:/openLoom/crates/loom-utils/shim-stubs/src
# Write Cargo.toml and src/lib.rs as above
```

- [ ] **Step 2: 修改 4 个 problem crate 的 Cargo.toml**

- [ ] **Step 3: 编译验证**

```bash
cd F:/openLoom && cargo check -p loom-core-plugins -p loom-core-skills -p loom-hooks -p loom-tools 2>&1 | tail -20
```

- [ ] **Step 4: Commit**

```bash
git add crates/loom-utils/shim-stubs crates/loom-protocol/core-plugins crates/loom-protocol/core-skills crates/loom-protocol/hooks crates/loom-protocol/tools Cargo.toml
git commit -m "chore: add shim stubs for transitive cut dependencies in core crates"
```

### Task 3.2: 搬运 protocol crate

- [ ] **Step 1: 复制 + 重命名**

```bash
cp -r F:/codex/codex-rs/protocol F:/openLoom/crates/loom-protocol/protocol
cd F:/openLoom/crates/loom-protocol/protocol
sed -i 's/name = "codex-protocol"/name = "loom-protocol"/' Cargo.toml
# 替换所有 codex-* 引用为 loom-*
grep -rl 'codex_protocol' src/ | xargs sed -i 's/codex_protocol/loom_protocol/g'
grep -rl 'codex-protocol' . | xargs sed -i 's/codex-protocol/loom-protocol/g'
grep -rl 'codex-async-utils' . | xargs sed -i 's/codex-async-utils/loom-async-utils/g'
grep -rl 'codex-execpolicy' . | xargs sed -i 's/codex-execpolicy/loom-execpolicy/g'
grep -rl 'codex-network-proxy' . | xargs sed -i 's/codex-network-proxy/loom-network-proxy/g'
grep -rl 'codex-utils-absolute-path' . | xargs sed -i 's/codex-utils-absolute-path/loom-absolute-path/g'
grep -rl 'codex-utils-image' . | xargs sed -i 's/codex-utils-image/loom-image/g'
grep -rl 'codex-utils-string' . | xargs sed -i 's/codex-utils-string/loom-string/g'
```

- [ ] **Step 2: Linux-only 依赖保持不变**（`landlock`、`seccompiler` 在 `[target.'cfg(target_os = "linux")']` 下）

- [ ] **Step 3: 注册 + 编译**

```bash
cd F:/openLoom && cargo check -p loom-protocol 2>&1 | tail -20
```

- [ ] **Step 4: Commit**

```bash
git add crates/loom-protocol/protocol Cargo.toml
git commit -m "chore: port protocol crate from Codex"
```

### Task 3.3: 搬运 + 修改 app-server-protocol

- [ ] **Step 1: 复制 + 重命名**

```bash
cp -r F:/codex/codex-rs/app-server-protocol F:/openLoom/crates/loom-protocol/app-server-protocol
cd F:/openLoom/crates/loom-protocol/app-server-protocol
sed -i 's/name = "codex-app-server-protocol"/name = "loom-app-server-protocol"/' Cargo.toml
# 替换 codex-* 引用为 loom-*
grep -rl 'codex_app_server_protocol' src/ | xargs sed -i 's/codex_app_server_protocol/loom_app_server_protocol/g'
grep -rl 'codex-app-server-protocol' . | xargs sed -i 's/codex-app-server-protocol/loom-app-server-protocol/g'
grep -rl 'codex-protocol' . | xargs sed -i 's/codex-protocol/loom-protocol/g'
grep -rl 'codex-shell-command' . | xargs sed -i 's/codex-shell-command/loom-shell-command/g'
grep -rl 'codex-utils-absolute-path' . | xargs sed -i 's/codex-utils-absolute-path/loom-absolute-path/g'
```

- [ ] **Step 2: 移除 codex-experimental-api-macros**

在 `Cargo.toml` 中删除该依赖。检查 `src/` 中使用 derive macro 的地方，替换为 serde 标准宏或手动展开。

- [ ] **Step 3: 编译验证**

```bash
cd F:/openLoom && cargo check -p loom-app-server-protocol 2>&1 | tail -20
```

- [ ] **Step 4: Commit**

```bash
git add crates/loom-protocol/app-server-protocol Cargo.toml
git commit -m "chore: port app-server-protocol crate from Codex"
```

### Task 3.4: 搬运 + 修改 features crate

- [ ] **Step 1: 复制 + 创建完整 otel-stub**

```bash
cp -r F:/codex/codex-rs/features F:/openLoom/crates/loom-protocol/features
cd F:/openLoom/crates/loom-protocol/features
sed -i 's/name = "codex-features"/name = "loom-features"/' Cargo.toml
grep -rl 'codex_features' src/ | xargs sed -i 's/codex_features/loom_features/g'
grep -rl 'codex-otel' . | xargs sed -i 's/codex-otel/loom-otel-stub/g'
grep -rl 'codex-protocol' . | xargs sed -i 's/codex-protocol/loom-protocol/g'
```

- [ ] **Step 2: 创建完整 loom-otel-stub**

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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SessionTelemetry;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RuntimeMetricsSummary;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum TelemetryAuthMode {
    #[default]
    Disabled,
}

/// Used by tui/src/history_cell/tests.rs
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RuntimeMetricTotals {
    pub count: u64,
    pub duration_ms: u64,
}

impl RuntimeMetricTotals {
    pub fn is_empty(self) -> bool {
        self.count == 0
    }

    pub fn merge(&mut self, other: Self) {
        self.count += other.count;
        self.duration_ms += other.duration_ms;
    }
}
```

- [ ] **Step 3: 编译验证**

```bash
cd F:/openLoom && cargo check -p loom-features 2>&1 | tail -20
```

- [ ] **Step 4: Commit**

```bash
git add crates/loom-protocol/features crates/loom-utils/otel-stub Cargo.toml
git commit -m "chore: port features crate with full otel stub"
```

---

## Phase 4: config + app-server-client（关键接缝）

### Task 4.1: 搬运 + 修改 config crate

- [ ] **Step 1: 复制 + 重命名**

```bash
cp -r F:/codex/codex-rs/config F:/openLoom/crates/loom-protocol/config
cd F:/openLoom/crates/loom-protocol/config
sed -i 's/name = "codex-config"/name = "loom-config"/' Cargo.toml
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

- [ ] **Step 2: 去掉 model-provider-info 依赖，创建完整桥接类型**

在 `Cargo.toml` 中删除 `codex-model-provider-info`，添加：
```toml
openloom-models = { path = "../../models" }
```

config 使用 `model-provider-info` 的位置在 `config_toml.rs`（7 imports + provider ID constants）和 `thread_config.rs`（struct field + WireApi）。创建桥接文件 `F:/openLoom/crates/loom-protocol/config/src/model_info.rs`：

```rust
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Replaces codex_model_provider_info::WireApi
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WireApi {
    #[default]
    Responses,
}

impl std::fmt::Display for WireApi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WireApi::Responses => write!(f, "responses"),
        }
    }
}

/// Replaces codex_model_provider_info::ModelProviderInfo
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ModelProviderInfo {
    #[serde(default)]
    pub name: String,
    pub base_url: Option<String>,
    pub env_key: Option<String>,
    pub env_key_instructions: Option<String>,
    pub experimental_bearer_token: Option<String>,
    pub auth: Option<ModelProviderAuthInfo>,
    pub aws: Option<ModelProviderAwsAuthInfo>,
    #[serde(default)]
    pub wire_api: WireApi,
    pub query_params: Option<HashMap<String, String>>,
    pub http_headers: Option<HashMap<String, String>>,
    pub env_http_headers: Option<HashMap<String, String>>,
    pub request_max_retries: Option<u64>,
    pub stream_max_retries: Option<u64>,
    pub stream_idle_timeout_ms: Option<u64>,
    pub websocket_connect_timeout_ms: Option<u64>,
    #[serde(default)]
    pub requires_openai_auth: bool,
    #[serde(default)]
    pub supports_websockets: bool,
}

impl ModelProviderInfo {
    pub fn validate(&self) -> std::result::Result<(), String> { Ok(()) }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelProviderAuthInfo {
    pub r#type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelProviderAwsAuthInfo {
    pub region: Option<String>,
    pub profile: Option<String>,
}

// Provider ID constants
pub const OPENAI_PROVIDER_ID: &str = "openai";
pub const OLLAMA_OSS_PROVIDER_ID: &str = "ollama";
pub const LMSTUDIO_OSS_PROVIDER_ID: &str = "lmstudio";
pub const AMAZON_BEDROCK_PROVIDER_ID: &str = "amazon-bedrock";
pub const LEGACY_OLLAMA_CHAT_PROVIDER_ID: &str = "ollama-chat";
pub const OLLAMA_CHAT_PROVIDER_REMOVED_ERROR: &str = "ollama-chat provider has been removed";
pub const DEFAULT_OLLAMA_PORT: u16 = 11434;
pub const DEFAULT_LMSTUDIO_PORT: u16 = 1234;

/// Bridge from openLoom model config
impl ModelProviderInfo {
    pub fn from_loom_config(model: &openloom_models::ModelConfig) -> Self {
        Self {
            name: model.model.clone(),
            base_url: model.base_url.clone(),
            env_key: model.api_key_env.clone(),
            ..Default::default()
        }
    }
}
```

- [ ] **Step 3: 编译验证**

```bash
cd F:/openLoom && cargo check -p loom-config 2>&1 | tail -30
```

- [ ] **Step 4: Commit**

```bash
git add crates/loom-protocol/config Cargo.toml
git commit -m "chore: port config crate with full model-info bridge"
```

### Task 4.2: 搬运 + 大幅修改 app-server-client（核心接缝）

这是最关键的一步。原来的 `AppServerClient` 依赖 `codex-app-server`（后端实现）、`codex-core`（核心运行时）、`codex-feedback`。全部砍掉，替换为直接调用 openLoom Engine。

**原来 `app-server-client/src/lib.rs` 从 `codex-core` 重导出了约 15 个类型**（McpManager, check_execpolicy_for_warnings, InProcessServerEvent, StateDbHandle, LogDbLayer, CodexFeedback 等），这些都需要在 loom 版本中处理。

- [ ] **Step 1: 复制 + 重命名**

```bash
cp -r F:/codex/codex-rs/app-server-client F:/openLoom/crates/loom-protocol/app-server-client
cd F:/openLoom/crates/loom-protocol/app-server-client
sed -i 's/name = "codex-app-server-client"/name = "loom-app-server-client"/' Cargo.toml
grep -rl 'codex_app_server_client' src/ | xargs sed -i 's/codex_app_server_client/loom_app_server_client/g'
grep -rl 'codex-app-server-client' . | xargs sed -i 's/codex-app-server-client/loom-app-server-client/g'
grep -rl 'codex-app-server-protocol' . | xargs sed -i 's/codex-app-server-protocol/loom-app-server-protocol/g'
grep -rl 'codex-protocol' . | xargs sed -i 's/codex-protocol/loom-protocol/g'
grep -rl 'codex-config' . | xargs sed -i 's/codex-config/loom-config/g'
grep -rl 'codex-arg0' . | xargs sed -i 's/codex-arg0/loom-arg0/g'
grep -rl 'codex-exec-server' . | xargs sed -i 's/codex-exec-server/loom-exec-server/g'
grep -rl 'codex-uds' . | xargs sed -i 's/codex-uds/loom-uds/g'
grep -rl 'codex-utils-absolute-path' . | xargs sed -i 's/codex-utils-absolute-path/loom-absolute-path/g'
grep -rl 'codex-utils-rustls-provider' . | xargs sed -i 's/codex-utils-rustls-provider/loom-rustls-provider/g'
```

- [ ] **Step 2: 重写 Cargo.toml**

删除：
```toml
codex-app-server, codex-core, codex-feedback
```

添加：
```toml
openloom-engine = { path = "../../engine" }
openloom-models = { path = "../../models" }
loom-tui-stubs = { path = "../../loom-utils/tui-stubs" }  # 提供 CodexFeedback 等 stub
```

- [ ] **Step 3: 清理 lib.rs 重导出**

删除所有 `pub use codex_core::*` 和 `pub use codex_app_server::*` 重导出。替换为：
- `StateDbHandle` → `pub type StateDbHandle = Arc<loom_tui_stubs::state::StateRuntime>;`
- `CodexFeedback` → `pub use loom_tui_stubs::feedback::CodexFeedback;`
- 其余不需要的 → 删除

- [ ] **Step 4: 实现 LoomAppServerClient**

新建 `F:/openLoom/crates/loom-protocol/app-server-client/src/loom.rs`：

```rust
use openloom_engine::{Engine, EngineConfig};
use loom_app_server_protocol::{ClientRequest, ServerNotification, ServerRequest};
use tokio::sync::mpsc;

pub struct LoomAppServerClient {
    engine: Engine,
    event_tx: mpsc::UnboundedSender<AppServerEvent>,
    event_rx: mpsc::UnboundedReceiver<AppServerEvent>,
}

impl LoomAppServerClient {
    pub async fn new(config: EngineConfig) -> anyhow::Result<Self> {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let engine = Engine::new(config).await?;
        Ok(Self { engine, event_tx, event_rx })
    }

    pub async fn request_typed<R: serde::de::DeserializeOwned>(
        &self,
        req: ClientRequest,
    ) -> Result<R, TypedRequestError> {
        // Map each ClientRequest variant to Engine calls
        // Detailed mappings in next task
        todo!()
    }

    pub fn events(&mut self) -> Option<AppServerEvent> {
        self.event_rx.try_recv().ok()
    }

    pub async fn shutdown(self) -> anyhow::Result<()> {
        // Graceful shutdown
        Ok(())
    }
}
```

修改 `AppServerClient` enum：
```rust
pub enum AppServerClient {
    Loom(LoomAppServerClient),
    Remote(RemoteAppServerClient),  // keep for --remote flag
}
```

- [ ] **Step 5: 编译验证 + 修错**

```bash
cd F:/openLoom && cargo check -p loom-app-server-client 2>&1 | tail -40
```

- [ ] **Step 6: Commit**

```bash
git add crates/loom-protocol/app-server-client Cargo.toml
git commit -m "feat: port app-server-client with LoomAppServerClient adapter"
```

---

## Phase 5: MCP + chatgpt + exec crate

### Task 5.1: 搬运 MCP crate

| Codex 路径 | openLoom 路径 | crate 名 |
|-----------|--------------|---------|
| `codex-mcp/` | `crates/loom-protocol/mcp/` | `loom-mcp` |
| `mcp-server/` | `crates/loom-protocol/mcp-server/` | `loom-mcp-server` |
| `rmcp-client/` | `crates/loom-protocol/rmcp-client/` | `loom-rmcp-client` |

**注意：** `codex-mcp` 依赖 `codex-api`、`codex-login`、`codex-model-provider`、`codex-otel`、`codex-plugin`（5 个 cut crate）。`mcp-server` 依赖 `codex-core`、`codex-extension-api`、`codex-login`、`codex-utils-json-to-toml`。

这些 MCP crate 的 cut 依赖暂时不改——先搬过来，Phase 8 统一处理。如果编译不过，用 `loom-shim-stubs` 补充缺失类型。

- [ ] **Step 1: 复制 + 重命名**

```bash
cp -r F:/codex/codex-rs/codex-mcp F:/openLoom/crates/loom-protocol/mcp
cp -r F:/codex/codex-rs/mcp-server F:/openLoom/crates/loom-protocol/mcp-server
cp -r F:/codex/codex-rs/rmcp-client F:/openLoom/crates/loom-protocol/rmcp-client
```

- [ ] **Step 2: 编译验证**

```bash
cd F:/openLoom && cargo check -p loom-mcp -p loom-mcp-server -p loom-rmcp-client 2>&1 | tail -20
```

- [ ] **Step 3: Commit**

```bash
git add crates/loom-protocol/mcp crates/loom-protocol/mcp-server crates/loom-protocol/rmcp-client Cargo.toml
git commit -m "chore: port MCP crates from Codex"
```

### Task 5.2: 搬运 + 修改 chatgpt crate（大幅）

`codex-chatgpt` 被 TUI 和 CLI 用到。它依赖 `codex-core`、`codex-login`、`codex-model-provider`、`codex-app-server-protocol`、`codex-plugin`。

具体引用：
- `chatgpt/src/connectors.rs` → `codex_core::config::Config`, `codex_core::connectors::*`, `codex_login::AuthManager`, `codex_login::CodexAuth`, `codex_login::default_client::originator`, `codex_plugin::AppConnectorId`

策略：把所有 cut crate 引用替换为 loom 同类（shim 或已搬 crate）。

- [ ] **Step 1: 复制 + 重命名**

```bash
cp -r F:/codex/codex-rs/chatgpt F:/openLoom/crates/loom-protocol/chatgpt
cd F:/openLoom/crates/loom-protocol/chatgpt
sed -i 's/name = "codex-chatgpt"/name = "loom-chatgpt"/' Cargo.toml
```

- [ ] **Step 2: 替换 Cargo.toml 依赖**

删除 `codex-core`、`codex-login`、`codex-model-provider`、`codex-plugin`，替换为：
```toml
loom-app-server-protocol = { workspace = true }
loom-config = { workspace = true }
loom-shim-stubs = { workspace = true }  # 提供 AuthManager, CodexAuth, AppConnectorId
```

- [ ] **Step 3: 修改 src/ 引用**

```bash
grep -rl 'codex_core::' src/ | xargs sed -i 's/codex_core::/loom_config::/g'
grep -rl 'codex_login::' src/ | xargs sed -i 's/codex_login::/loom_shim_stubs::login::/g'
grep -rl 'codex_plugin::' src/ | xargs sed -i 's/codex_plugin::/loom_shim_stubs::plugin::/g'
grep -rl 'codex_model_provider::' src/ | xargs sed -i 's/codex_model_provider::/loom_shim_stubs::model_provider::/g'
```

- [ ] **Step 4: 编译验证**

```bash
cd F:/openLoom && cargo check -p loom-chatgpt 2>&1 | tail -20
```

- [ ] **Step 5: Commit**

```bash
git add crates/loom-protocol/chatgpt Cargo.toml
git commit -m "chore: port chatgpt crate with shim-replaced dependencies"
```

### Task 5.3: 搬运 + 修改 codex-exec（新发现）

`codex-exec`（`exec/`）被 CLI 的 `Exec` 子命令直接依赖。需要搬过来并修改。

- [ ] **Step 1: 复制 + 重命名**

```bash
cp -r F:/codex/codex-rs/exec F:/openLoom/crates/loom-protocol/exec
cd F:/openLoom/crates/loom-protocol/exec
sed -i 's/name = "codex-exec"/name = "loom-exec"/' Cargo.toml
```

- [ ] **Step 2: 替换 cut 依赖**

`codex-exec` 依赖 `codex-app-server-client`、`codex-core`、`codex-cloud-requirements`、`codex-feedback`、`codex-login`、`codex-model-provider-info`、`codex-otel` 等。替换为：
```toml
loom-app-server-client = { workspace = true }
loom-config = { workspace = true }
loom-shim-stubs = { workspace = true }
loom-tui-stubs = { workspace = true }
```

- [ ] **Step 3: 编译验证**

```bash
cd F:/openLoom && cargo check -p loom-exec 2>&1 | tail -20
```

- [ ] **Step 4: Commit**

```bash
git add crates/loom-protocol/exec Cargo.toml
git commit -m "chore: port exec crate from Codex"
```

---

## Phase 6: 搬运 TUI（最大修改量）

TUI 有 12 个需要 stub 的 cut crate。以下是基于审查结果的 **精确 stub 类型清单**。

### Task 6.1: 创建完整的 loom-tui-stubs

- [ ] **Step 1: 创建 crate**

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
tracing = { workspace = true }
tracing-subscriber = { workspace = true }
uuid = { workspace = true, features = ["serde", "v7"] }
loom-protocol = { workspace = true }
loom-absolute-path = { workspace = true }
```

`F:/openLoom/crates/loom-utils/tui-stubs/src/lib.rs`:

```rust
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::io;

// ─── codex-feedback replacements ───

pub mod feedback {
    use super::*;
    use loom_protocol::ThreadId;
    use tracing::Subscriber;
    use tracing_subscriber::layer::Layer;
    use tracing_subscriber::registry::LookupSpan;

    /// Central feedback type — used as field in App, ChatWidget, TuiContext.
    /// Original: F:/codex/codex-rs/feedback/src/lib.rs:161
    #[derive(Clone)]
    pub struct CodexFeedback;

    impl Default for CodexFeedback {
        fn default() -> Self { Self::new() }
    }

    impl CodexFeedback {
        pub fn new() -> Self { Self }

        pub fn make_writer(&self) -> FeedbackMakeWriter { FeedbackMakeWriter }

        pub fn logger_layer<S>(&self) -> impl Layer<S> + Send + Sync + 'static
        where S: Subscriber + for<'a> LookupSpan<'a> {
            tracing_subscriber::layer::Identity::new()
        }

        pub fn metadata_layer<S>(&self) -> impl Layer<S> + Send + Sync + 'static
        where S: Subscriber + for<'a> LookupSpan<'a> {
            tracing_subscriber::layer::Identity::new()
        }

        pub fn snapshot(&self, _session_id: Option<ThreadId>) -> FeedbackSnapshot {
            FeedbackSnapshot
        }
    }

    pub struct FeedbackMakeWriter;
    impl io::Write for FeedbackMakeWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> { Ok(buf.len()) }
        fn flush(&mut self) -> io::Result<()> { Ok(()) }
    }
    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for FeedbackMakeWriter {
        type Writer = Self;
        fn make_writer(&self) -> Self::Writer { FeedbackMakeWriter }
    }

    #[derive(Debug, Clone, Default)]
    pub struct FeedbackSnapshot;

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    pub struct FeedbackDiagnostics;
    pub const DOCTOR_REPORT_ATTACHMENT_FILENAME: &str = "doctor-report.txt";
    pub const FEEDBACK_DIAGNOSTICS_ATTACHMENT_FILENAME: &str = "feedback-diagnostics.json";

    #[derive(Debug, Clone, Default)]
    pub struct FeedbackDiagnostic;
}

// ─── codex-state replacements ───

pub mod state {
    use super::*;

    #[derive(Clone)]
    pub struct StateRuntime;
    impl StateRuntime {
        pub async fn new(_path: PathBuf) -> Result<Self, anyhow::Error> { Ok(Self) }
    }

    pub fn state_db_path(codex_home: &Path) -> PathBuf {
        codex_home.join("state.db")
    }

    pub mod log_db {
        use super::*;

        pub struct LogDbLayer;

        pub async fn start(_state_db: std::sync::Arc<super::StateRuntime>) -> LogDbLayer {
            LogDbLayer
        }
    }
}

// ─── codex-rollout replacements ───

pub mod rollout {
    use super::*;

    /// StateDbHandle = Arc<codex_state::StateRuntime>
    pub type StateDbHandle = Arc<super::state::StateRuntime>;

    pub mod state_db {
        pub use super::StateDbHandle;
    }
}

// ─── codex-message-history replacements ───

pub mod message_history {
    use super::*;

    #[derive(Debug, Clone)]
    pub struct HistoryConfig {
        pub codex_home: PathBuf,
        pub persistence: HistoryPersistence,
        pub max_bytes: Option<usize>,
    }

    #[derive(Debug, Clone)]
    pub enum HistoryPersistence {
        SaveOnEveryMessage,
        SaveNever,
    }

    #[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
    pub struct HistoryEntry {
        pub session_id: String,
        pub ts: u64,
        pub text: String,
    }

    impl HistoryConfig {
        pub fn new(codex_home: impl Into<PathBuf>, _history: &()) -> Self {
            Self {
                codex_home: codex_home.into(),
                persistence: HistoryPersistence::SaveNever,
                max_bytes: None,
            }
        }
    }

    pub async fn history_metadata(_config: &HistoryConfig) -> (u64, usize) {
        (0, 0)
    }

    pub async fn append_entry(
        _text: &str,
        _conversation_id: impl std::fmt::Display,
        _config: &HistoryConfig,
    ) -> io::Result<()> {
        Ok(())
    }

    pub fn lookup(_log_id: u64, _offset: usize, _config: &HistoryConfig) -> Option<HistoryEntry> {
        None
    }
}

// ─── codex-plugin replacements ───

pub mod plugin {
    /// Original: F:/codex/codex-rs/plugin/src/lib.rs:19
    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
    pub struct AppConnectorId(pub String);

    /// Original: F:/codex/codex-rs/plugin/src/lib.rs:22
    #[derive(Debug, Clone, Default, PartialEq, Eq)]
    pub struct PluginCapabilitySummary {
        pub config_name: String,
        pub display_name: String,
        pub description: Option<String>,
        pub has_skills: bool,
        pub mcp_server_names: Vec<String>,
        pub app_connector_ids: Vec<AppConnectorId>,
    }
}

// ─── codex-connectors replacements ───

pub mod connectors {
    use loom_app_server_protocol::AppInfo;

    /// Original: F:/codex/codex-rs/connectors/src/metadata.rs:3
    pub fn connector_display_label(connector: &AppInfo) -> String {
        connector.name.clone()
    }

    pub fn connector_mention_slug(connector: &AppInfo) -> String {
        connector.name.to_lowercase().replace(' ', "-")
    }
}

// ─── codex-model-provider replacements ───

pub mod model_provider {
    pub async fn create_model_provider() -> ModelProvider {
        ModelProvider
    }

    pub struct ModelProvider;
}

// ─── codex-model-provider-info replacements (for TUI direct imports) ───

pub mod model_provider_info {
    pub use loom_config::model_info::{
        ModelProviderInfo, WireApi, ModelProviderAuthInfo, ModelProviderAwsAuthInfo,
        OPENAI_PROVIDER_ID, OLLAMA_OSS_PROVIDER_ID, LMSTUDIO_OSS_PROVIDER_ID,
        AMAZON_BEDROCK_PROVIDER_ID, LEGACY_OLLAMA_CHAT_PROVIDER_ID,
        OLLAMA_CHAT_PROVIDER_REMOVED_ERROR, DEFAULT_OLLAMA_PORT, DEFAULT_LMSTUDIO_PORT,
    };
}

// ─── codex-models-manager replacements ───

pub mod models_manager {
    pub mod model_presets {
        pub const HIDE_GPT_5_1_CODEX_MAX_MIGRATION_PROMPT_CONFIG: &str =
            "hide_gpt-5.1-codex-max_migration_prompt";
        pub const HIDE_GPT5_1_MIGRATION_PROMPT_CONFIG: &str =
            "hide_gpt5_1_migration_prompt";
    }
    pub mod collaboration_mode_presets {
        pub fn builtin_collaboration_mode_presets() -> Vec<()> { vec![] }
    }
}

// ─── codex-cloud-requirements replacement ───

pub mod cloud_requirements {
    pub fn cloud_requirements_loader_for_storage() -> () { () }
}

// ─── codex-login replacement (for TUI) ───

pub mod login {
    pub struct AuthConfig;
    pub enum Auth {}

    pub fn enforce_login_restrictions() {}
    pub fn set_default_client_residency_requirement() {}
    pub fn read_openai_api_key_from_env() -> Option<String> { None }

    pub mod default_client {
        pub fn originator() -> String { String::new() }
    }
}

// ─── codex-realtime-webrtc replacement (non-Linux only) ───

pub mod realtime_webrtc {
    pub enum RealtimeWebrtcEvent {}
    pub struct RealtimeWebrtcSession;
    pub struct RealtimeWebrtcSessionHandle;
}

// ─── codex-windows-sandbox cfg-gated replacement ───

#[cfg(target_os = "windows")]
pub mod windows_sandbox {
    pub struct WindowsSandbox;
    impl WindowsSandbox {
        pub fn new() -> Self { Self }
    }
}
```

- [ ] **Step 2: 编译验证 tui-stubs**

```bash
cd F:/openLoom && cargo check -p loom-tui-stubs 2>&1 | tail -10
```

- [ ] **Step 3: Commit**

```bash
git add crates/loom-utils/tui-stubs Cargo.toml
git commit -m "feat: add complete loom-tui-stubs with all cut crate replacements"
```

### Task 6.2: 搬运 TUI 源码 + 替换依赖

- [ ] **Step 1: 复制 + 重命名**

```bash
cp -r F:/codex/codex-rs/tui F:/openLoom/crates/loom-protocol/tui
cd F:/openLoom/crates/loom-protocol/tui
sed -i 's/name = "codex-tui"/name = "loom-tui"/' Cargo.toml
```

- [ ] **Step 2: 清理 Cargo.toml**

删除 cut crate 依赖，添加 loom 替代品：
```toml
# 删除这些
# codex-cloud-requirements, codex-connectors, codex-feedback,
# codex-login, codex-message-history, codex-model-provider,
# codex-model-provider-info, codex-models-manager, codex-otel,
# codex-plugin, codex-realtime-webrtc, codex-rollout, codex-state,
# codex-windows-sandbox

# 替换为
loom-tui-stubs = { workspace = true }
```

kept deps 替换：
- `codex-chatgpt` → `loom-chatgpt`
- `codex-app-server-client` → `loom-app-server-client`
- `codex-app-server-protocol` → `loom-app-server-protocol`
- `codex-config` → `loom-config`
- `codex-protocol` → `loom-protocol`
- 所有 `codex-utils-*` → `loom-*`
- 所有 `codex-*` 其他 crate → 对应的 `loom-*`

- [ ] **Step 3: 全局替换 use 语句**

```bash
cd F:/openLoom/crates/loom-protocol/tui
# 所有 cut crate 引用 → loom_tui_stubs
grep -rl 'codex_cloud_requirements' src/ | xargs sed -i 's/codex_cloud_requirements/loom_tui_stubs::cloud_requirements/g'
grep -rl 'codex_connectors' src/ | xargs sed -i 's/codex_connectors/loom_tui_stubs::connectors/g'
grep -rl 'codex_feedback' src/ | xargs sed -i 's/codex_feedback/loom_tui_stubs::feedback/g'
grep -rl 'codex_login' src/ | xargs sed -i 's/codex_login/loom_tui_stubs::login/g'
grep -rl 'codex_message_history' src/ | xargs sed -i 's/codex_message_history/loom_tui_stubs::message_history/g'
grep -rl 'codex_model_provider' src/ | xargs sed -i 's/codex_model_provider/loom_tui_stubs::model_provider/g'
grep -rl 'codex_models_manager' src/ | xargs sed -i 's/codex_models_manager/loom_tui_stubs::models_manager/g'
grep -rl 'codex_otel' src/ | xargs sed -i 's/codex_otel/loom_otel_stub/g'
grep -rl 'codex_plugin' src/ | xargs sed -i 's/codex_plugin/loom_tui_stubs::plugin/g'
grep -rl 'codex_realtime_webrtc' src/ | xargs sed -i 's/codex_realtime_webrtc/loom_tui_stubs::realtime_webrtc/g'
grep -rl 'codex_rollout' src/ | xargs sed -i 's/codex_rollout/loom_tui_stubs::rollout/g'
grep -rl 'codex_state' src/ | xargs sed -i 's/codex_state/loom_tui_stubs::state/g'
grep -rl 'codex_windows_sandbox' src/ | xargs sed -i 's/codex_windows_sandbox/loom_tui_stubs::windows_sandbox/g'
```

- [ ] **Step 4: 编译 + 循环修错**

```bash
cd F:/openLoom && cargo check -p loom-tui 2>&1 | grep "^error" | head -40
```

逐个分析错误：
- 缺少的类型 → 加到 `loom-tui-stubs`
- 函数调用链断了 → 适配到 loom-app-server-client
- auth/login 相关 UI 组件 → 删除或变成空操作

关键不变区域（不能动）：
- `tui/src/app_server_session.rs` — `turn_start()` 调用链
- `tui/src/chatwidget/protocol.rs` — `handle_server_notification()`
- `tui/src/streaming/mod.rs` — 流式渲染

- [ ] **Step 5: Commit**

```bash
git add crates/loom-protocol/tui crates/loom-utils/tui-stubs Cargo.toml
git commit -m "feat: port TUI crate with complete stubs and loom backend"
```

---

## Phase 7: 搬运 CLI（入口 + 命令裁剪）

### Task 7.1: 搬运 CLI 源码 + 裁剪

- [ ] **Step 1: 复制 + 保留 build.rs**

```bash
cp -r F:/codex/codex-rs/cli F:/openLoom/crates/loom-protocol/cli
cd F:/openLoom/crates/loom-protocol/cli
sed -i 's/name = "codex-cli"/name = "loom-cli"/' Cargo.toml
sed -i 's/name = "codex"/name = "loom"/' Cargo.toml
```

**关键：** `cli/build.rs` 有 macOS linker flag，必须保留。

- [ ] **Step 2: 裁剪 Cargo.toml**

删除被砍 crate 依赖：
```toml
# 删除
# codex-app-server, codex-app-server-daemon, codex-app-server-test-client,
# codex-api, codex-cloud-tasks, codex-core,
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
loom-exec = { workspace = true }
loom-chatgpt = { workspace = true }
loom-tui-stubs = { workspace = true }
```

- [ ] **Step 3: 裁剪 main.rs 子命令**

保留：
```rust
enum Subcommand {
    // 无子命令 = 默认交互式 TUI
    Exec { ... },       // 委托给 loom-exec
    Review { ... },     // 代码审查
    Mcp { ... },        // MCP 管理
    Plugin { ... },     // Plugin 管理
    Completion { ... }, // Shell 补全
    Doctor,             // 诊断（重写，用 loom 替代）
    Debug { ... },      // 调试
    Execpolicy { ... }, // 执行策略
    Apply,              // 应用 diff
    Resume { ... },     // 恢复会话
    Fork { ... },       // 分叉会话
    Sandbox { ... },    // 沙箱
    // openLoom 特有
    Memory { ... },     // 记忆查看
    Persona,             // 人格查看
    Config { ... },     // 配置管理
    Serve { ... },      // 启动服务（留给 Electron）
}
```

删除：
`Login`, `Logout`, `Update`, `Cloud`, `AppServer`, `App`, `RemoteControl`, `ResponsesApiProxy`, `StdioToUds`, `McpServer`, `Features`, `ExecServer`

- [ ] **Step 4: 修改入口函数 + doctor 模块**

doctor 模块从 `codex-api`、`codex-core`、`codex-login`、`codex-model-provider` 引用类型。替换为 openLoom 的诊断逻辑（已有的 `openloom doctor`）。

入口函数：
```rust
async fn cli_main() -> anyhow::Result<()> {
    let cli = MultitoolCli::parse();
    let app_config = load_app_config()?;
    let engine_config = EngineConfig::from_app_config(&app_config)?;
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

- [ ] **Step 5: 编译 + 修错**

```bash
cd F:/openLoom && cargo check -p loom-cli 2>&1 | grep "^error" | head -50
```

需要修改的模块：
- `cli/src/doctor.rs` → 用 loom 诊断替换
- `cli/src/debug_sandbox.rs` → 删除或 stub
- `cli/src/login.rs` → 删除
- `cli/src/mcp_cmd.rs` → `codex_core::McpManager` → loom 替代
- `cli/src/marketplace_cmd.rs` → `codex_core::config::Config` → loom-config
- `cli/src/plugin_cmd.rs` → `codex_core::config::Config` → loom-config

- [ ] **Step 6: Commit**

```bash
git add crates/loom-protocol/cli Cargo.toml
git commit -m "feat: port CLI crate with Loom backend integration"
```

---

## Phase 8: 适配层 RPC 映射补全 + 集成测试

### Task 8.1: 补全 LoomAppServerClient 的 request_typed 映射

把所有 `todo!()` 替换为实际映射。关键映射表：

| ClientRequest 变体 | → Engine 操作 |
|-------------------|---------------|
| `TurnStart(params)` | `engine.handle_message_streaming(msg, sid, event_tx)` |
| `ThreadStart(params)` | `engine.session_tx.send(SessionCommand::Create)` |
| `ThreadList(params)` | `engine.session_tx.send(SessionCommand::List)` |
| `ThreadRead(params)` | `engine.session_tx.send(SessionCommand::GetMessages)` |
| `ThreadResume(params)` | `engine.session_tx.send(SessionCommand::Get)` |
| `ThreadFork(params)` | `engine.session_tx.send(SessionCommand::Fork)` |
| `ThreadSetName(params)` | `engine.session_tx.send(SessionCommand::SetName)` |
| `TurnInterrupt(_)` | 设置 atomic flag 中断 agent_loop |
| `SkillsList(_)` | `engine.skill_registry.list()` 包装成 Response |
| `ModelList(_)` | 从 `engine.app_config.models` 构建 |
| `ConfigBatchWrite(params)` | `engine.config::set_config()` |
| `ReviewStart(params)` | 构建 review prompt 走 agent_loop |
| `GetAccount` | 返回本地账户信息（无登录态） |
| `LogoutAccount` | no-op |

- [ ] **Step 1: 实现 TurnStart 映射（最关键的路径）**

```rust
ClientRequest::TurnStart(params) => {
    let msg = ChatMessage {
        role: "user".into(),
        content: params.items.into_iter()
            .map(|i| i.to_text())
            .collect::<Vec<_>>()
            .join("\n"),
        timestamp: chrono::Utc::now(),
    };
    let sid = params.thread_id.parse::<SessionId>()?;
    let (tx, mut rx) = mpsc::channel(256);

    // Spawn agent loop
    let engine = self.engine.clone();
    let event_tx = self.event_tx.clone();
    tokio::spawn(async move {
        let result = engine.handle_message_streaming(
            msg, &sid, tx, Mode::Code,
            params.model_preference(),
            params.thinking_level(),
        ).await;

        // Forward streaming tokens as AgentMessageDelta events
        while let Some(token) = rx.recv().await {
            let _ = event_tx.send(AppServerEvent::ServerNotification(
                ServerNotification::AgentMessageDelta(
                    AgentMessageDeltaNotification {
                        thread_id: params.thread_id.clone(),
                        turn_id: turn_id.clone(),
                        item_id: item_id.clone(),
                        delta: token,
                    }
                )
            ));
        }

        // Send TurnCompleted
        let _ = event_tx.send(AppServerEvent::ServerNotification(
            ServerNotification::TurnCompleted(
                TurnCompletedNotification {
                    thread_id: params.thread_id,
                    turn_id,
                    status: TurnStatus::Completed,
                    usage: result.usage,
                }
            )
        ));
    });

    TurnStartResponse { turn_id, thread_id: params.thread_id }
}
```

- [ ] **Step 2: 补全所有其他映射**

- [ ] **Step 3: 编译 + 测试**

```bash
cd F:/openLoom && cargo check -p loom-app-server-client 2>&1 | tail -20
```

- [ ] **Step 4: Commit**

```bash
git add crates/loom-protocol/app-server-client
git commit -m "feat: complete LoomAppServerClient RPC mappings"
```

### Task 8.2: 集成测试

- [ ] **Step 1: 适配层单元测试**

```bash
mkdir -p F:/openLoom/crates/loom-protocol/app-server-client/tests
```

测试 TurnStart → handle_message_streaming 映射、流式 token 转发、Session 操作映射等。

- [ ] **Step 2: 端到端 smoketest**

```bash
cd F:/openLoom && cargo test --workspace 2>&1 | tail -30
```

- [ ] **Step 3: Clippy + fmt**

```bash
cargo fmt --all && cargo clippy --workspace -- -D warnings 2>&1 | tail -10
```

- [ ] **Step 4: Commit**

```bash
git add tests/ crates/loom-protocol/app-server-client/tests/
git commit -m "test: add adapter tests and integration tests"
```

---

## Phase 9: 清理旧 CLI + 最终验证

- [ ] **Step 1: 移除旧 CLI**

```bash
mv F:/openLoom/crates/cli F:/openLoom/crates/cli.old
```

- [ ] **Step 2: 全量编译**

```bash
cd F:/openLoom && cargo check --workspace 2>&1 | tail -10
```

Expected: 全部通过。

- [ ] **Step 3: 全量测试**

```bash
cd F:/openLoom && cargo test --workspace 2>&1 | tail -30
```

Expected: 原有 180+ + 新增测试全部通过。

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "chore: remove old CLI crate, finalize Codex port"
```

---

## 附录 A: 完整 crate 清单（保留 vs 砍掉 vs 新建）

### 保留（改名移植，38 个）

```
crates/loom-utils/async-utils
crates/loom-utils/absolute-path
crates/loom-utils/home-dir
crates/loom-utils/string
crates/loom-utils/elapsed
crates/loom-utils/path
crates/loom-utils/ansi-escape
crates/loom-utils/arg0
crates/loom-utils/install-context
crates/loom-utils/terminal-detection
crates/loom-utils/rustls-provider
crates/loom-utils/uds
crates/loom-utils/fuzzy-match
crates/loom-utils/sandbox-summary
crates/loom-utils/approval-presets
crates/loom-utils/file-system
crates/loom-utils/file-search
crates/loom-utils/git-utils
crates/loom-utils/sleep-inhibitor
crates/loom-utils/oss
crates/loom-utils/plugins
crates/loom-utils/cli
crates/loom-utils/image

crates/loom-protocol/execpolicy
crates/loom-protocol/network-proxy
crates/loom-protocol/shell-command
crates/loom-protocol/sandboxing
crates/loom-protocol/exec-server
crates/loom-protocol/skills             ← 新发现
crates/loom-protocol/core-plugins
crates/loom-protocol/core-skills
crates/loom-protocol/tools
crates/loom-protocol/hooks
crates/loom-protocol/external-agent-sessions
crates/loom-protocol/external-agent-migration
crates/loom-protocol/protocol
crates/loom-protocol/app-server-protocol
crates/loom-protocol/features
crates/loom-protocol/config
crates/loom-protocol/app-server-client  ← MODIFIED
crates/loom-protocol/mcp
crates/loom-protocol/mcp-server
crates/loom-protocol/rmcp-client
crates/loom-protocol/chatgpt            ← MODIFIED
crates/loom-protocol/exec               ← 新发现
crates/loom-protocol/tui                ← MODIFIED
crates/loom-protocol/cli                ← MODIFIED
```

### 新建（3 个）

```
crates/loom-utils/otel-stub     — 替代 codex-otel (SessionTelemetry, RuntimeMetricsSummary,
                                   TelemetryAuthMode, RuntimeMetricTotals)
crates/loom-utils/tui-stubs     — 替代 codex-state/rollout/feedback/message-history/
                                   plugin/connectors/model-provider/model-provider-info/
                                   models-manager/cloud-requirements/login/realtime-webrtc/
                                   windows-sandbox 的完整 stub 类型
crates/loom-utils/shim-stubs    — Phase 3 临时 shim (analytics, login, model_provider, otel,
                                   plugin, utils_output_truncation, utils_pty, code_mode)
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
v8-poc, connectors, message-history, core, core-api,
collaboration-mode-templates, responses-api-proxy, response-debug-context,
stdio-to-uds, debug-client, test-binary-support, codex-experimental-api-macros,
utils/cargo-bin, utils/cache, utils/json-to-toml, utils/pty,
utils/readiness, utils/output-truncation, utils/stream-parser,
utils/template, apply-patch, bwrap, file-watcher, execpolicy-legacy,
code-mode
```

---

## 附录 B: 关键风险（v2 更新）

| 风险 | 影响 | 缓解 |
|------|------|------|
| TUI 对 CodexFeedback 的深度依赖 | 该类型是 App/ChatWidget/TuiContext 的字段 | stub 必须精确匹配原有 trait bounds (Clone + Default + make_writer/logger_layer/metadata_layer/snapshot) |
| `[patch.crates-io]` 未同步 | 使用社区版 crossterm/ratatui 可能导致渲染 bug 或 API 不兼容 | Phase 1 第一步就复制 patch |
| `dirs` 版本冲突 (v5 vs v6) | 现有 openLoom 使用 dirs v5，Codex 用 v6 | Phase 1 统一升级到 v6 |
| `codex-experimental-api-macros` 缺失 | app-server-protocol 使用其 derive 宏 | 检查 src/ 中用此宏的地方，替换为 serde 标准宏 |
| Engine API 与 Codex 协议流式模型不匹配 | Engine 用 mpsc channel，Codex 用 AgentMessageDelta 通知 | 适配层做 channel→通知翻译 |
| TUI 平台 gating（100+ cfg 属性） | Windows/Linux/macOS/Android 不同路径 | stub 类型也要加 cfg gate |
| 编译错误数量被低估 | TUI stub 可能还有遗漏类型（审查只查了 12 个 cut crate 的直接引用） | 循环修错，每轮聚焦一个错误类别 |
| `core-plugins/core-skills` 可能最终需要砍掉 | 如果 stub 不够用 | 先尝试 stub 路线，不行就砍，TUI 中去掉对应的 UI 元素 |

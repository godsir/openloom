# 内置工具设置面板 — 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在设置页新增「内置工具」tab，展示全部 29 个工具，点击展开抽屉查看/修改可配参数，不可配的灰字标注"系统默认"

**Architecture:** 后端新增 `ToolPrefsConfig` 类型 + JSON 文件持久化 + JSON-RPC get/set；`builtin_tools.rs` 构造时注入配置引用，execute 时读取覆盖硬编码默认值；前端新增 `BuiltinToolsTab` 手风琴组件，通过 `config.get_tool_prefs` / `config.set_tool_prefs` RPC 读写

**Tech Stack:** Rust (loom-types / loom-core / loom-server), React + TypeScript + CSS Modules, Zustand

## Global Constraints

- 复用现有 `~/.loom/*.json` JSON 文件持久化模式（参考 sandbox.json）
- 前端复用 SettingModal.module.css 已有样式（aboutRow、mcpTransportToggle 等）
- 后端构造工具时注入 `Arc<RwLock<ToolPrefsConfig>>`，不传全局可变状态
- 所有 i18n key 同步三语言（zh-CN / en-US / zh-TW）

---

### Task 1: 后端 — 类型定义

**Files:**
- Create: `backend/crates/loom-types/src/config/tool_prefs.rs`
- Modify: `backend/crates/loom-types/src/config/mod.rs`

**Interfaces:**
- Produces: `ToolPrefsConfig` struct (derive Serialize/Deserialize/Clone), `ToolSearchEngine` enum

- [ ] **Step 1: 创建 tool_prefs.rs**

```rust
// backend/crates/loom-types/src/config/tool_prefs.rs

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolSearchEngine {
    DuckDuckGoLite,
    Brave,
    SearXNG,
}

impl Default for ToolSearchEngine {
    fn default() -> Self { Self::DuckDuckGoLite }
}

/// 用户可调的内置工具参数，持久化到 ~/.loom/tool_prefs.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolPrefsConfig {
    // --- shell ---
    #[serde(default = "default_shell_timeout")]
    pub shell_default_timeout_secs: u64,
    #[serde(default = "default_shell_max_timeout")]
    pub shell_max_timeout_secs: u64,

    // --- file_read ---
    #[serde(default = "default_file_read_max_kb")]
    pub file_read_max_output_kb: usize,

    // --- web_search ---
    #[serde(default)]
    pub web_search_engine: ToolSearchEngine,
    #[serde(default = "default_web_search_max_results")]
    pub web_search_max_results: usize,

    // --- web_fetch ---
    #[serde(default = "default_web_fetch_max_chars")]
    pub web_fetch_max_chars: usize,

    // --- process_wait ---
    #[serde(default = "default_process_wait_max_timeout")]
    pub process_wait_max_timeout_secs: u64,

    // --- monitor ---
    #[serde(default = "default_monitor_timeout_ms")]
    pub monitor_default_timeout_ms: u64,
}

fn default_shell_timeout() -> u64 { 60 }
fn default_shell_max_timeout() -> u64 { 300 }
fn default_file_read_max_kb() -> usize { 64 }
fn default_web_search_max_results() -> usize { 5 }
fn default_web_fetch_max_chars() -> usize { 5000 }
fn default_process_wait_max_timeout() -> u64 { 3600 }
fn default_monitor_timeout_ms() -> u64 { 300_000 }

impl Default for ToolPrefsConfig {
    fn default() -> Self {
        Self {
            shell_default_timeout_secs: default_shell_timeout(),
            shell_max_timeout_secs: default_shell_max_timeout(),
            file_read_max_output_kb: default_file_read_max_kb(),
            web_search_engine: ToolSearchEngine::default(),
            web_search_max_results: default_web_search_max_results(),
            web_fetch_max_chars: default_web_fetch_max_chars(),
            process_wait_max_timeout_secs: default_process_wait_max_timeout(),
            monitor_default_timeout_ms: default_monitor_timeout_ms(),
        }
    }
}
```

- [ ] **Step 2: 注册模块到 mod.rs**

```rust
// backend/crates/loom-types/src/config/mod.rs — 在 pub mod model_config 下面加
pub mod tool_prefs;
```

- [ ] **Step 3: cargo check**

Run: `cargo check -p loom-types`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add backend/crates/loom-types/src/config/tool_prefs.rs backend/crates/loom-types/src/config/mod.rs
git commit -m "feat: add ToolPrefsConfig type for builtin tool preferences"
```

---

### Task 2: 后端 — Orchestrator 持久化读写

**Files:**
- Modify: `backend/crates/loom-core/src/orchestrator.rs`

**Interfaces:**
- Consumes: `ToolPrefsConfig` from Task 1
- Produces: `tool_prefs` field on Orchestrator, `load_tool_prefs()` / `save_tool_prefs()` public methods

- [ ] **Step 1: 在 Orchestrator struct 加 tool_prefs 字段**

找到 `sandbox_config: Arc<RwLock<...>>` 那行（约 line 291），下面加：

```rust
/// Builtin-tool tunables persisted to tool_prefs.json.
tool_prefs: Arc<RwLock<loom_types::config::tool_prefs::ToolPrefsConfig>>,
```

- [ ] **Step 2: 在 Orchestrator::new() 构造体里初始化 tool_prefs**

找到 `sandbox_config: Arc::new(RwLock::new(SandboxConfig::default())),`，下面加：

```rust
tool_prefs: Arc::new(RwLock::new(loom_types::config::tool_prefs::ToolPrefsConfig::default())),
```

- [ ] **Step 3: 添加 load/save 方法**

找到 `pub async fn load_sandbox_config(...)`（约 line 6112），后面加：

```rust
pub async fn load_tool_prefs(&self) -> loom_types::config::tool_prefs::ToolPrefsConfig {
    let path = self.data_dir.join("tool_prefs.json");
    match tokio::fs::read_to_string(&path).await {
        Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
        Err(_) => loom_types::config::tool_prefs::ToolPrefsConfig::default(),
    }
}

pub async fn save_tool_prefs(&self, config: &loom_types::config::tool_prefs::ToolPrefsConfig) -> Result<()> {
    let _ = tokio::fs::create_dir_all(&self.data_dir).await;
    let path = self.data_dir.join("tool_prefs.json");
    let json = serde_json::to_string_pretty(config)?;
    tokio::fs::write(&path, json).await?;
    Ok(())
}
```

- [ ] **Step 4: 添加 public accessor**

找到 `pub async fn get_default_max_iterations(...)`，下面加：

```rust
pub fn tool_prefs(&self) -> Arc<RwLock<loom_types::config::tool_prefs::ToolPrefsConfig>> {
    self.tool_prefs.clone()
}
```

- [ ] **Step 5: cargo check**

Run: `cargo check -p loom-core`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add backend/crates/loom-core/src/orchestrator.rs
git commit -m "feat: add tool_prefs persistence to Orchestrator"
```

---

### Task 3: 后端 — JSON-RPC handler

**Files:**
- Modify: `backend/crates/loom-server/src/dispatch/system.rs`

**Interfaces:**
- Consumes: Orchestrator `load_tool_prefs()` / `save_tool_prefs()` from Task 2
- Produces: `config.get_tool_prefs` / `config.set_tool_prefs` RPC methods

- [ ] **Step 1: 在 match 块注册两个方法**

在 `system.rs` 的 handle 函数 match 中 `"config.get_defaults"` 上方加：

```rust
"config.get_tool_prefs" => Some(handle_config_get_tool_prefs(state).await),
"config.set_tool_prefs" => Some(handle_config_set_tool_prefs(state, p).await),
```

- [ ] **Step 2: 实现 handler 函数**

在文件末尾加（`handle_config_set_defaults` 下方）：

```rust
// --- config.get_tool_prefs ---

async fn handle_config_get_tool_prefs(state: &AppState) -> Result<Value, JsonRpcError> {
    let config = state.orchestrator.load_tool_prefs().await;
    Ok(serde_json::to_value(config).unwrap_or_default())
}

// --- config.set_tool_prefs ---

async fn handle_config_set_tool_prefs(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let mut config = state.orchestrator.load_tool_prefs().await;
    if let Some(v) = p.get("shell_default_timeout_secs").and_then(|v| v.as_u64()) { config.shell_default_timeout_secs = v; }
    if let Some(v) = p.get("shell_max_timeout_secs").and_then(|v| v.as_u64()) { config.shell_max_timeout_secs = v; }
    if let Some(v) = p.get("file_read_max_output_kb").and_then(|v| v.as_u64()) { config.file_read_max_output_kb = v as usize; }
    if let Some(v) = p.get("web_search_engine").and_then(|v| v.as_str()) {
        config.web_search_engine = serde_json::from_value(serde_json::Value::String(v.to_string())).unwrap_or_default();
    }
    if let Some(v) = p.get("web_search_max_results").and_then(|v| v.as_u64()) { config.web_search_max_results = v as usize; }
    if let Some(v) = p.get("web_fetch_max_chars").and_then(|v| v.as_u64()) { config.web_fetch_max_chars = v as usize; }
    if let Some(v) = p.get("process_wait_max_timeout_secs").and_then(|v| v.as_u64()) { config.process_wait_max_timeout_secs = v; }
    if let Some(v) = p.get("monitor_default_timeout_ms").and_then(|v| v.as_u64()) { config.monitor_default_timeout_ms = v; }
    state.orchestrator.save_tool_prefs(&config).await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
    // Also push into the live ToolPrefsConfig ref so executing tools see the change immediately
    let mut live = state.orchestrator.tool_prefs().write().await;
    *live = config.clone();
    Ok(json!({ "ok": true }))
}
```

- [ ] **Step 3: 确保 AppState 可访问 orchestrator**

验证 `state.orchestrator` 路径可用（AppState 已有 orchestrator 字段，无需改动）。

- [ ] **Step 4: cargo check**

Run: `cargo check -p loom-server`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add backend/crates/loom-server/src/dispatch/system.rs
git commit -m "feat: add config.get_tool_prefs / config.set_tool_prefs RPC"
```

---

### Task 4: 后端 — builtin_tools 读取配置

**Files:**
- Modify: `backend/crates/loom-core/src/builtin_tools.rs`
- Modify: `backend/crates/loom-core/src/orchestrator.rs`

**Interfaces:**
- Consumes: Orchestrator `tool_prefs()` from Task 2, `ToolPrefsConfig` from Task 1
- Produces: ShellTool/FileReadTool/WebSearchTool/WebFetchTool/ProcessWaitTool/MonitorTool 读配置

- [ ] **Step 1: ShellTool 加 tool_prefs 字段**

```rust
// line 22 — 修改 struct 定义
pub struct ShellTool {
    pub tool_prefs: Arc<RwLock<loom_types::config::tool_prefs::ToolPrefsConfig>>,
}
```

在 `execute()` 中（line 71），替换超时读取：

```rust
// 改为
let prefs = self.tool_prefs.read().await;
let timeout_secs = arguments["timeout"].as_u64().unwrap_or(prefs.shell_default_timeout_secs).min(prefs.shell_max_timeout_secs);
```

- [ ] **Step 2: FileReadTool 加 tool_prefs 字段**

```rust
pub struct FileReadTool {
    pub tool_prefs: Arc<RwLock<loom_types::config::tool_prefs::ToolPrefsConfig>>,
}
```

在 `execute()` 输出截断处（line 488），替换：

```rust
let max_bytes = self.tool_prefs.read().await.file_read_max_output_kb * 1024;
if result.len() > max_bytes {
    result = format!("{}...\n[truncated at {}KB]", truncate_utf8(&result, max_bytes), max_bytes / 1024);
}
```

注意：原代码两处用 `65536`（line 178 和 line 488），都替换。

- [ ] **Step 3: WebSearchTool 加 tool_prefs 字段**

```rust
pub struct WebSearchTool {
    pub tool_prefs: Arc<RwLock<loom_types::config::tool_prefs::ToolPrefsConfig>>,
}
```

在 `execute()` 中根据 `web_search_engine` 选择搜索后端。当前只有 DDG Lite + DDG HTML fallback，Brave 和 SearXNG 留后续实现（当前选择非 DuckDuckGo 时返回 "暂不支持"）：

```rust
// 在 execute 开头插入
let prefs = self.tool_prefs.read().await;
match prefs.web_search_engine {
    loom_types::config::tool_prefs::ToolSearchEngine::DuckDuckGoLite => { /* 保持现有逻辑 */ }
    _ => {
        return Ok(ToolResult {
            content: format!("搜索引擎 {:?} 暂未实现，当前仅支持 DuckDuckGo", prefs.web_search_engine),
            is_error: true,
            structured_content: None,
        });
    }
}
```

max_results 替换（line 1560）：

```rust
let max_results = arguments["max_results"].as_u64().unwrap_or(prefs.web_search_max_results as u64).min(20) as usize;
```

- [ ] **Step 4: WebFetchTool 加 tool_prefs 字段**

```rust
pub struct WebFetchTool {
    pub tool_prefs: Arc<RwLock<loom_types::config::tool_prefs::ToolPrefsConfig>>,
}
```

max_chars 替换（line 1756）：

```rust
let max_chars = arguments["max_chars"].as_u64().unwrap_or(prefs.web_fetch_max_chars as u64).min(100000) as usize;
```

- [ ] **Step 5: ProcessWaitTool 和 MonitorTool 同样加 tool_prefs 字段，读取对应的 timeout 配置**

ProcessWait line 2486:
```rust
let timeout_secs = arguments["timeout"].as_u64().unwrap_or(30).min(prefs.process_wait_max_timeout_secs);
```

Monitor line 2718:
```rust
let timeout_ms = arguments["timeout_ms"].as_u64().unwrap_or(prefs.monitor_default_timeout_ms).min(3_600_000);
```

- [ ] **Step 6: 更新 Orchestrator::new() 中所有工具构造**

将所有注册行从无参 struct（如 `Arc::new(crate::builtin_tools::ShellTool)`）改为带 tool_prefs 字段：

```rust
let tp: Arc<RwLock<loom_types::config::tool_prefs::ToolPrefsConfig>> = Arc::new(RwLock::new(loom_types::config::tool_prefs::ToolPrefsConfig::default()));
// ... (在 new() 中统一创建一次 tp)
let _ = registry.register(Arc::new(crate::builtin_tools::ShellTool { tool_prefs: tp.clone() }));
let _ = registry.register(Arc::new(crate::builtin_tools::FileReadTool { tool_prefs: tp.clone() }));
let _ = registry.register(Arc::new(crate::builtin_tools::WebSearchTool { tool_prefs: tp.clone() }));
let _ = registry.register(Arc::new(crate::builtin_tools::WebFetchTool { tool_prefs: tp.clone() }));
// ... (file_write, file_edit, file_list, content_search 等不需要的保持不变)
```

- [ ] **Step 7: cargo check**

Run: `cargo check --workspace`
Expected: PASS

- [ ] **Step 8: Commit**

```bash
git add backend/crates/loom-core/src/builtin_tools.rs backend/crates/loom-core/src/orchestrator.rs
git commit -m "feat: builtin tools read tool_prefs at execution time"
```

---

### Task 5: 前端 — i18n 翻译

**Files:**
- Modify: `frontend/src/renderer/src/i18n/zh-CN.ts`
- Modify: `frontend/src/renderer/src/i18n/en-US.ts`
- Modify: `frontend/src/renderer/src/i18n/zh-TW.ts`

- [ ] **Step 1: zh-CN.ts — 在 `"settings.toolsGroup"` 附近加新 key**

```typescript
// 设置页 tab
'settings.builtinTools': '内置工具',
'settings.builtinToolsDesc': '查看和调整内置工具的默认参数',

// 工具说明（29 个）
'bt.shell': '执行 Shell 命令并等待返回结果',
'bt.file_list': '列出目录内容，支持递归',
'bt.file_read': '读取文件内容',
'bt.file_write': '写入内容到文件',
'bt.file_edit': '精确文本替换，支持批量编辑',
'bt.file_delete': '删除文件或空目录',
'bt.file_glob': 'Glob 模式匹配文件',
'bt.file_find': '按文件名子串搜索',
'bt.content_search': 'Grep 风格文本搜索',
'bt.web_search': '网络搜索',
'bt.web_fetch': '抓取网页并提取文本',
'bt.memory_search': '搜索知识图谱（长记忆）',
'bt.use_skill': '按名称激活已安装技能',
'bt.todo_write': '写入/替换待办列表',
'bt.todo_list': '读取当前待办列表',
'bt.schedule_reminder': '创建 AI 定时任务',
'bt.system_info': '查询自身配置（模型/权限等）',
'bt.token_usage': '检查上下文窗口剩余量',
'bt.ask_user': '向用户提问澄清',
'bt.process_spawn': '启动长进程',
'bt.process_kill': '终止进程',
'bt.process_stdin': '向进程写入 stdin',
'bt.process_list': '列出活跃进程',
'bt.process_wait': '等待进程结束',
'bt.process_peek': '查看进程当前输出',
'bt.monitor': '启动持久监控器',
'bt.monitor_list': '列出活跃监控器',
'bt.monitor_kill': '终止监控器',
'bt.monitor_wait': '等待监控器输出',
'bt.monitor_peek': '查看监控器当前输出',

// 配置项标签
'bt.noConfig': '系统默认，暂无配置项',
'bt.shellDefaultTimeout': '默认超时 (秒)',
'bt.shellMaxTimeout': '最大超时 (秒)',
'bt.fileReadMaxKb': '输出截断 (KB)',
'bt.webSearchEngine': '搜索引擎',
'bt.webSearchMaxResults': '最大结果数',
'bt.webFetchMaxChars': '最大字符数',
'bt.processWaitMaxTimeout': '超时上限 (秒)',
'bt.monitorDefaultTimeout': '默认超时 (秒)',
```

- [ ] **Step 2: en-US.ts — 对应英文翻译**

```typescript
'settings.builtinTools': 'Built-in Tools',
'settings.builtinToolsDesc': 'View and adjust built-in tool defaults',

'bt.shell': 'Execute shell commands and wait for results',
'bt.file_list': 'List directory contents with optional recursion',
'bt.file_read': 'Read file contents',
'bt.file_write': 'Write content to a file',
'bt.file_edit': 'Precise text replacement with batch support',
'bt.file_delete': 'Delete a file or empty directory',
'bt.file_glob': 'Match files by glob pattern',
'bt.file_find': 'Find files by name substring',
'bt.content_search': 'Grep-style text search',
'bt.web_search': 'Web search',
'bt.web_fetch': 'Fetch and extract text from web pages',
'bt.memory_search': 'Search knowledge graph (long-term memory)',
'bt.use_skill': 'Activate an installed skill by name',
'bt.todo_write': 'Write/replace the todo list',
'bt.todo_list': 'Read the current todo list',
'bt.schedule_reminder': 'Create an AI scheduled task',
'bt.system_info': 'Query agent configuration',
'bt.token_usage': 'Check context window budget',
'bt.ask_user': 'Ask the user a clarifying question',
'bt.process_spawn': 'Start a long-running process',
'bt.process_kill': 'Kill a process',
'bt.process_stdin': 'Write to process stdin',
'bt.process_list': 'List active processes',
'bt.process_wait': 'Wait for process to finish',
'bt.process_peek': 'Peek at current process output',
'bt.monitor': 'Start a persistent monitor',
'bt.monitor_list': 'List active monitors',
'bt.monitor_kill': 'Kill a monitor',
'bt.monitor_wait': 'Wait for monitor output',
'bt.monitor_peek': 'Peek at current monitor output',

'bt.noConfig': 'System default, no configurable options',
'bt.shellDefaultTimeout': 'Default timeout (s)',
'bt.shellMaxTimeout': 'Max timeout (s)',
'bt.fileReadMaxKb': 'Output truncation (KB)',
'bt.webSearchEngine': 'Search engine',
'bt.webSearchMaxResults': 'Max results',
'bt.webFetchMaxChars': 'Max characters',
'bt.processWaitMaxTimeout': 'Max timeout (s)',
'bt.monitorDefaultTimeout': 'Default timeout (s)',
```

- [ ] **Step 3: zh-TW.ts — 繁体翻译**

```typescript
'settings.builtinTools': '內置工具',
'settings.builtinToolsDesc': '檢視與調整內置工具的預設參數',

'bt.shell': '執行 Shell 命令並等待返回結果',
'bt.file_list': '列出目錄內容，支援遞迴',
'bt.file_read': '讀取檔案內容',
'bt.file_write': '寫入內容到檔案',
'bt.file_edit': '精確文字替換，支援批次編輯',
'bt.file_delete': '刪除檔案或空目錄',
'bt.file_glob': 'Glob 模式匹配檔案',
'bt.file_find': '按檔案名稱子串搜尋',
'bt.content_search': 'Grep 風格文字搜尋',
'bt.web_search': '網路搜尋',
'bt.web_fetch': '擷取網頁並提取文字',
'bt.memory_search': '搜尋知識圖譜（長期記憶）',
'bt.use_skill': '按名稱啟用已安裝技能',
'bt.todo_write': '寫入/替換待辦列表',
'bt.todo_list': '讀取目前待辦列表',
'bt.schedule_reminder': '建立 AI 定時任務',
'bt.system_info': '查詢自身配置（模型/權限等）',
'bt.token_usage': '檢查上下文視窗剩餘量',
'bt.ask_user': '向使用者提問澄清',
'bt.process_spawn': '啟動長期程序',
'bt.process_kill': '終止程序',
'bt.process_stdin': '寫入 stdin 到程序',
'bt.process_list': '列出活躍程序',
'bt.process_wait': '等待程序結束',
'bt.process_peek': '檢視程序目前輸出',
'bt.monitor': '啟動持久監控器',
'bt.monitor_list': '列出活躍監控器',
'bt.monitor_kill': '終止監控器',
'bt.monitor_wait': '等待監控器輸出',
'bt.monitor_peek': '檢視監控器目前輸出',

'bt.noConfig': '系統預設，暫無配置項目',
'bt.shellDefaultTimeout': '預設逾時 (秒)',
'bt.shellMaxTimeout': '最大逾時 (秒)',
'bt.fileReadMaxKb': '輸出截斷 (KB)',
'bt.webSearchEngine': '搜尋引擎',
'bt.webSearchMaxResults': '最大結果數',
'bt.webFetchMaxChars': '最大字元數',
'bt.processWaitMaxTimeout': '逾時上限 (秒)',
'bt.monitorDefaultTimeout': '預設逾時 (秒)',
```

- [ ] **Step 4: TypeCheck**

Run: `npx tsc --noEmit`
Expected: PASS (需要确保 TranslationMap 类型包含新 key 或使用宽松类型)

- [ ] **Step 5: Commit**

```bash
git add frontend/src/renderer/src/i18n/zh-CN.ts frontend/src/renderer/src/i18n/en-US.ts frontend/src/renderer/src/i18n/zh-TW.ts
git commit -m "feat: add i18n keys for builtin tools settings"
```

---

### Task 6: 前端 — BuiltinToolsTab 组件 + CSS

**Files:**
- Create: `frontend/src/renderer/src/components/settings/BuiltinToolsTab.tsx`
- Create: `frontend/src/renderer/src/components/settings/BuiltinToolsTab.module.css`

**Interfaces:**
- Consumes: `config.get_tool_prefs` / `config.set_tool_prefs` RPC (from Task 3), i18n keys (from Task 5)
- Produces: `<BuiltinToolsTab />` component

- [ ] **Step 1: 创建 CSS 文件 `BuiltinToolsTab.module.css`**

```css
.list {
  display: flex;
  flex-direction: column;
  gap: 1px;
}

.toolItem {
  border: 1px solid var(--border);
  border-radius: 8px;
  overflow: hidden;
}

.toolHeader {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 12px 16px;
  cursor: pointer;
  user-select: none;
  transition: background 0.15s;
}
.toolHeader:hover {
  background: var(--bg-active);
}

.toolName {
  font-family: var(--font-mono);
  font-size: 13px;
  font-weight: 600;
  color: var(--text);
}

.toolDesc {
  font-size: 12px;
  color: var(--text-muted);
  margin-left: 12px;
  flex: 1;
}

.toolChevron {
  color: var(--text-muted);
  transition: transform 0.2s;
  flex-shrink: 0;
}
.toolChevronOpen {
  transform: rotate(180deg);
}

.toolBody {
  padding: 0 16px 16px;
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.noConfig {
  font-size: 12px;
  color: var(--text-muted);
  padding: 4px 0;
}

.configRow {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}

.configLabel {
  font-size: 12px;
  color: var(--text-secondary);
  min-width: 100px;
}

.configValue {
  display: flex;
  align-items: center;
  gap: 8px;
}

.configInput {
  width: 80px;
  padding: 4px 8px;
  border: 1px solid var(--border);
  border-radius: 4px;
  background: var(--bg-input);
  color: var(--text);
  font-size: 13px;
  text-align: center;
}

.configRange {
  font-size: 11px;
  color: var(--text-muted);
  min-width: 60px;
}
```

- [ ] **Step 2: 创建组件 `BuiltinToolsTab.tsx`**

```tsx
import { useState, useEffect, useCallback } from 'react'
import { loomRpc } from '../../services/jsonrpc'
import { useLocale } from '../../i18n'
import { IconChevronDown } from '../../utils/icons'
import Select from '../shared/Select'
import styles from './BuiltinToolsTab.module.css'

interface ToolPrefs {
  shell_default_timeout_secs: number
  shell_max_timeout_secs: number
  file_read_max_output_kb: number
  web_search_engine: string
  web_search_max_results: number
  web_fetch_max_chars: number
  process_wait_max_timeout_secs: number
  monitor_default_timeout_ms: number
}

interface ToolDef {
  name: string
  descKey: string
  configs?: ConfigDef[]
}

interface ConfigDef {
  key: string
  labelKey: string
  type: 'number' | 'select'
  min?: number
  max?: number
  options?: { value: string; label: string }[]
  msToSec?: boolean   // monitor 的 ms → 前端显示秒
}

function buildTools(t: (k: string) => string): ToolDef[] {
  return [
    { name: 'shell', descKey: 'bt.shell', configs: [
      { key: 'shell_default_timeout_secs', labelKey: 'bt.shellDefaultTimeout', type: 'number', min: 10, max: 300 },
      { key: 'shell_max_timeout_secs', labelKey: 'bt.shellMaxTimeout', type: 'number', min: 60, max: 600 },
    ]},
    { name: 'file_list', descKey: 'bt.file_list' },
    { name: 'file_read', descKey: 'bt.file_read', configs: [
      { key: 'file_read_max_output_kb', labelKey: 'bt.fileReadMaxKb', type: 'number', min: 8, max: 512 },
    ]},
    { name: 'file_write', descKey: 'bt.file_write' },
    { name: 'file_edit', descKey: 'bt.file_edit' },
    { name: 'file_delete', descKey: 'bt.file_delete' },
    { name: 'file_glob', descKey: 'bt.file_glob' },
    { name: 'file_find', descKey: 'bt.file_find' },
    { name: 'content_search', descKey: 'bt.content_search' },
    { name: 'web_search', descKey: 'bt.web_search', configs: [
      { key: 'web_search_engine', labelKey: 'bt.webSearchEngine', type: 'select', options: [
        { value: 'duckduckgo_lite', label: 'DuckDuckGo' },
        { value: 'brave', label: 'Brave Search' },
        { value: 'searxng', label: 'SearXNG' },
      ]},
      { key: 'web_search_max_results', labelKey: 'bt.webSearchMaxResults', type: 'number', min: 1, max: 10 },
    ]},
    { name: 'web_fetch', descKey: 'bt.web_fetch', configs: [
      { key: 'web_fetch_max_chars', labelKey: 'bt.webFetchMaxChars', type: 'number', min: 1000, max: 20000 },
    ]},
    { name: 'memory_search', descKey: 'bt.memory_search' },
    { name: 'use_skill', descKey: 'bt.use_skill' },
    { name: 'todo_write', descKey: 'bt.todo_write' },
    { name: 'todo_list', descKey: 'bt.todo_list' },
    { name: 'schedule_reminder', descKey: 'bt.schedule_reminder' },
    { name: 'system_info', descKey: 'bt.system_info' },
    { name: 'token_usage', descKey: 'bt.token_usage' },
    { name: 'ask_user', descKey: 'bt.ask_user' },
    { name: 'process_spawn', descKey: 'bt.process_spawn' },
    { name: 'process_kill', descKey: 'bt.process_kill' },
    { name: 'process_stdin', descKey: 'bt.process_stdin' },
    { name: 'process_list', descKey: 'bt.process_list' },
    { name: 'process_wait', descKey: 'bt.process_wait', configs: [
      { key: 'process_wait_max_timeout_secs', labelKey: 'bt.processWaitMaxTimeout', type: 'number', min: 60, max: 7200 },
    ]},
    { name: 'process_peek', descKey: 'bt.process_peek' },
    { name: 'monitor', descKey: 'bt.monitor', configs: [
      { key: 'monitor_default_timeout_ms', labelKey: 'bt.monitorDefaultTimeout', type: 'number', min: 60, max: 1800, msToSec: true },
    ]},
    { name: 'monitor_list', descKey: 'bt.monitor_list' },
    { name: 'monitor_kill', descKey: 'bt.monitor_kill' },
    { name: 'monitor_wait', descKey: 'bt.monitor_wait' },
    { name: 'monitor_peek', descKey: 'bt.monitor_peek' },
  ]
}

export default function BuiltinToolsTab() {
  const { t } = useLocale()
  const tools = buildTools(t)
  const [prefs, setPrefs] = useState<ToolPrefs | null>(null)
  const [expanded, setExpanded] = useState<Set<string>>(new Set())
  const [loaded, setLoaded] = useState(false)

  useEffect(() => {
    loomRpc<ToolPrefs>('config.get_tool_prefs').then(p => {
      setPrefs(p)
      setLoaded(true)
    }).catch(() => setLoaded(true))
  }, [])

  const toggleExpand = (name: string) => {
    setExpanded(prev => {
      const next = new Set(prev)
      if (next.has(name)) next.delete(name)
      else next.add(name)
      return next
    })
  }

  const getPref = (key: string): number => {
    if (!prefs) return 0
    return (prefs as any)[key] ?? 0
  }

  const setPref = useCallback(async (key: string, val: string | number) => {
    const next: Partial<ToolPrefs> = {}
    // 处理 msToSec: monitor 的 ms → 秒
    const tool = tools.flatMap(t => t.configs || []).find(c => c.key === key)
    if (tool?.msToSec && typeof val === 'number') {
      (next as any)[key] = val * 1000
    } else {
      (next as any)[key] = val
    }
    try {
      await loomRpc('config.set_tool_prefs', next)
      setPrefs(prev => prev ? { ...prev, ...next } : prev)
    } catch {}
  }, [tools])

  if (!loaded) return <p>{t('common.loading')}</p>

  return (
    <div className={styles.list}>
      {tools.map(tool => {
        const open = expanded.has(tool.name)
        const hasConfig = tool.configs && tool.configs.length > 0
        return (
          <div key={tool.name} className={styles.toolItem}>
            <div className={styles.toolHeader} onClick={() => toggleExpand(tool.name)}>
              <div style={{ display: 'flex', alignItems: 'baseline', flex: 1 }}>
                <span className={styles.toolName}>{tool.name}</span>
                <span className={styles.toolDesc}>{t(tool.descKey)}</span>
              </div>
              <IconChevronDown size={14} className={`${styles.toolChevron} ${open ? styles.toolChevronOpen : ''}`} />
            </div>
            {open && (
              <div className={styles.toolBody}>
                {!hasConfig && (
                  <span className={styles.noConfig}>{t('bt.noConfig')}</span>
                )}
                {tool.configs?.map(cfg => {
                  const rawVal = getPref(cfg.key)
                  // monitor ms → display seconds
                  const displayVal = cfg.msToSec ? Math.round(rawVal / 1000) : rawVal
                  return (
                    <div key={cfg.key} className={styles.configRow}>
                      <span className={styles.configLabel}>{t(cfg.labelKey)}</span>
                      <div className={styles.configValue}>
                        {cfg.type === 'select' ? (
                          <Select
                            value={String(rawVal)}
                            options={cfg.options?.map(o => ({ value: o.value, label: o.label })) || []}
                            onChange={(v) => setPref(cfg.key, v)}
                            variant="form"
                          />
                        ) : (
                          <>
                            <input
                              type="number"
                              className={styles.configInput}
                              value={displayVal}
                              min={cfg.min}
                              max={cfg.max}
                              onChange={e => {
                                const v = Number(e.target.value)
                                if (!isNaN(v)) setPref(cfg.key, v)
                              }}
                            />
                            {cfg.min !== undefined && cfg.max !== undefined && (
                              <span className={styles.configRange}>{cfg.min}—{cfg.max}</span>
                            )}
                          </>
                        )}
                      </div>
                    </div>
                  )
                })}
              </div>
            )}
          </div>
        )
      })}
    </div>
  )
}
```

- [ ] **Step 3: TypeCheck**

Run: `npx tsc --noEmit`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add frontend/src/renderer/src/components/settings/BuiltinToolsTab.tsx frontend/src/renderer/src/components/settings/BuiltinToolsTab.module.css
git commit -m "feat: add BuiltinToolsTab component with accordion UI"
```

---

### Task 7: 前端 — 集成到 SettingsPage

**Files:**
- Modify: `frontend/src/renderer/src/components/settings/SettingsPage.tsx`

- [ ] **Step 1: 导入 BuiltinToolsTab**

```tsx
import BuiltinToolsTab from './BuiltinToolsTab'
```

- [ ] **Step 2: 在 Tab 类型和工具分组菜单中加 builtin_tools**

修改 `type Tab` 定义:

```tsx
type Tab = 'software' | 'agent' | 'loom' | 'models' | 'workspace' | 'mcp' | 'skills' | 'pet' | 'kg' | 'token' | 'shortcuts' | 'devtest' | 'write' | 'about' | 'im' | 'builtin_tools'
```

在 `useSettingsTabs()` 的工具分组 items 中:

```tsx
{ label: t('settings.toolsGroup'),
  items: [
    ...
    { id: 'builtin_tools' as Tab, label: t('settings.builtinTools'), icon: <IconSettings size={14} /> },
  ],
},
```

图标复用 `IconSettings`（已 import）。

- [ ] **Step 3: 加渲染分支**

```tsx
{tab === 'builtin_tools' && (
  <>
    <div className={styles.contentHeader}>
      <h3 className={styles.sectionTitle}>{t('settings.builtinTools')}</h3>
      <p className={styles.sectionDesc}>{t('settings.builtinToolsDesc')}</p>
    </div>
    <div className={styles.contentBody}>
      <BuiltinToolsTab />
    </div>
  </>
)}
```

- [ ] **Step 4: TypeCheck**

Run: `npx tsc --noEmit`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add frontend/src/renderer/src/components/settings/SettingsPage.tsx
git commit -m "feat: integrate BuiltinToolsTab into SettingsPage"
```

---

### Task 8: 全量验证

- [ ] **Step 1: cargo check --workspace**
- [ ] **Step 2: npx tsc --noEmit**
- [ ] **Step 3: 手动验证流程** — 打开设置 → 工具分组 → 内置工具 → 展开 shell → 改超时值 → 刷新页面确认值持久化

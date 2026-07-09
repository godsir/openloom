# 内置工具设置面板 — 设计文档

**日期**: 2026-07-09  
**状态**: 已确认

## 概述

在设置页「工具」分组新增「内置工具」tab。列出全部 29 个内置工具，点击展开抽屉查看/修改可配参数，不可配的显示"系统默认"。

## 前端

### 页面结构

设置页 → 工具分组 → 新增 `builtin_tools` tab。

每个工具一行，点击展开（手风琴/抽屉模式），展开后显示：

- **有可配参数的**（如 `web_search`、`shell`）：下方露出配置控件
- **无可配参数的**：显示"系统默认，暂无配置项"灰字提示

### 全部 29 个工具及说明

| 工具 | 说明 | 可配参数 |
|------|------|----------|
| `shell` | 执行 Shell 命令并等待返回结果 | 默认超时(10-300s)、最大超时(60-600s) |
| `file_list` | 列出目录内容，支持递归 | — |
| `file_read` | 读取文件内容，支持限制行数 | 输出截断(8-512KB) |
| `file_write` | 写入内容到文件 | — |
| `file_edit` | 精确文本替换，支持批量编辑 | — |
| `file_delete` | 删除文件或空目录 | — |
| `file_glob` | Glob 模式匹配文件 | — |
| `file_find` | 按文件名子串搜索 | — |
| `content_search` | Grep 风格文本搜索 | — |
| `web_search` | 网络搜索 | 搜索引擎(DuckDuckGo/Brave/SearXNG)、最大结果数(1-10) |
| `web_fetch` | 抓取网页并提取文本 | 最大字符数(1000-20000) |
| `memory_search` | 搜索知识图谱 | — |
| `use_skill` | 按名称激活已安装技能 | — |
| `todo_write` | 写入/替换待办列表 | — |
| `todo_list` | 读取当前待办列表 | — |
| `schedule_reminder` | 创建 AI 定时任务 | — |
| `system_info` | 查询自身配置 | — |
| `token_usage` | 检查上下文窗口剩余量 | — |
| `ask_user` | 向用户提问澄清 | — |
| `process_spawn` | 启动长进程 | — |
| `process_kill` | 终止进程 | — |
| `process_stdin` | 向进程写入 stdin | — |
| `process_list` | 列出活跃进程 | — |
| `process_wait` | 等待进程结束 | 超时上限(60-7200s) |
| `process_peek` | 偷看进程当前输出 | — |
| `monitor` | 启动监控器 | 默认超时(60-1800s) |
| `monitor_list` | 列出活跃监控器 | — |
| `monitor_kill` | 终止监控器 | — |
| `monitor_wait` | 等待监控器输出 | — |
| `monitor_peek` | 偷看监控器当前输出 | — |

### 新建文件

- `frontend/src/renderer/src/components/settings/BuiltinToolsTab.tsx`
- `frontend/src/renderer/src/components/settings/BuiltinToolsTab.module.css`

### 修改文件

- `SettingsPage.tsx`: 新增 `builtin_tools` tab，分组「内置工具」
- i18n 三语言: tab 名称、每个工具的说明、参数标签

## 后端

### 存储

- `~/.loom/tool_prefs.json` — 持久化文件
- 沿用 `vision.json` / `fim.json` 的 JSON 文件读写模式

### JSON-RPC

| 方法 | 参数 | 返回 |
|------|------|------|
| `config.get_tool_prefs` | 无 | `{ shell_timeout: 60, ... }` |
| `config.set_tool_prefs` | 部分参数 | `{ ok: true }` |

### 配置 Schema

```json
{
  "shell_default_timeout_secs": 60,
  "shell_max_timeout_secs": 300,
  "file_read_max_output_kb": 64,
  "web_search_engine": "duckduckgo_lite",
  "web_search_max_results": 5,
  "web_fetch_max_chars": 5000,
  "process_wait_max_timeout_secs": 3600,
  "monitor_default_timeout_ms": 300000
}
```

### 执行时读取

`builtin_tools.rs` 中各工具 struct 新增 `tool_prefs: Arc<RwLock<ToolPrefsConfig>>` 字段，构造时从 orchestrator 传入，`execute()` 时读取覆盖硬编码默认值。

### 新文件

- `loom-types/src/config/tool_prefs.rs`

### 修改文件

- `loom-types/src/config/mod.rs`: 注册新模块
- `loom-core/src/builtin_tools.rs`: Shell/FileRead/WebSearch/WebFetch/ProcessWait/Monitor 读配置
- `loom-core/src/orchestrator.rs`: 构造工具时传入配置
- `loom-server/src/dispatch/system.rs`: 新增 `config.get_tool_prefs` / `config.set_tool_prefs`
- `loom-server/src/dispatch/mod.rs`: 注册方法

## 工作量

| 层 | 预估 |
|----|------|
| 后端 (类型 + RPC + 执行读取) | 2-3h |
| 前端 (Tab 组件 + CSS + i18n) | 2-3h |
| **合计** | 约半天 |

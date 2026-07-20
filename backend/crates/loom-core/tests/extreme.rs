// SPDX-License-Identifier: Apache-2.0
//! 确定性极端测试集 —— 直接调用每个工具的 `execute()`，灌入对抗性输入，
//! 验证日常使用下不会 panic / 挂起 / 崩溃。
//!
//! 每个调用都被 `tokio::time::timeout` 包裹：返回 `Timeout` 即视为挂起 bug。
//! 若 `execute()` 内部 panic，会直接让对应 `#[tokio::test]` 失败并打印 panic 信息。
//!
//! 核心不变量（所有用例共同断言）：
//!   1. 不 panic
//!   2. 不挂起（在超时内返回）
//! 对于已知正确行为，额外断言具体的 `is_error` / 输出内容。

use std::sync::Arc;
use std::time::Duration;

use serde_json::{Value, json};
use tokio::sync::RwLock;
use tokio::sync::mpsc::unbounded_channel;

use loom_core::AgentTool;
use loom_core::MemoryStore;
use loom_core::builtin_tools::*;
use loom_core::entity_cron_tools::ManageCronTool;
use loom_core::entity_skills_tools::ManageSkillsTool;
use loom_core::entity_tools::{ManageAgentTool, ManageModelTool, ManageTeamTool};
use loom_core::event_bus::EventBus;
use loom_core::monitor_manager::MonitorManager;
use loom_core::process_manager::ProcessManager;
use loom_core::tool_context::ToolContext;
use loom_core::tool_registry::ToolResult;
use loom_types::ToolProgress;
use loom_types::config::tool_prefs::ToolPrefsConfig;

/// 每次工具调用的硬超时：超过即判定为挂起 bug。
const CALL_TIMEOUT: Duration = Duration::from_secs(20);

// ── 跨平台命令 ────────────────────────────────────────────────────────────────
#[cfg(windows)]
mod cmd {
    pub const LONG: &str = "cmd /c ping -n 40 127.0.0.1";
    pub const ECHO_UNICODE: &str = "cmd /c echo 中文输出测试";
    pub const NONEXISTENT: &str = "definitely_not_a_real_cmd_xyz_123";
    pub const SHELL_ECHO: &str = "echo hello";
    pub const SHELL_FAIL: &str = "exit 3";
    pub const SHELL_SLEEP: &str = "ping -n 30 127.0.0.1";
    pub const SHELL_UNICODE: &str = "echo 中文输出测试";
    /// 打印 80 行，末行为 line_80（shell 工具在 Windows 优先用 PowerShell）。
    pub const SHELL_MULTILINE: &str = "1..80 | ForEach-Object { \"line_$_\" }";
}
#[cfg(unix)]
mod cmd {
    pub const LONG: &str = "sleep 40";
    pub const ECHO_UNICODE: &str = "echo 中文输出测试";
    pub const NONEXISTENT: &str = "definitely_not_a_real_cmd_xyz_123";
    pub const SHELL_ECHO: &str = "echo hello";
    pub const SHELL_FAIL: &str = "exit 3";
    pub const SHELL_SLEEP: &str = "sleep 30";
    pub const SHELL_UNICODE: &str = "echo 中文输出测试";
    /// 打印 80 行，末行为 line_80。
    pub const SHELL_MULTILINE: &str = "for i in $(seq 1 80); do echo line_$i; done";
}

// ── 调用结果分类 ──────────────────────────────────────────────────────────────
#[derive(Debug)]
enum Outcome {
    /// 工具正常返回（is_error 可能为 true，代表优雅报错）。
    Ok(ToolResult),
    /// 工具返回 anyhow::Err（也是优雅失败，不算 bug）。
    ToolErr(String),
    /// 超时未返回 —— 挂起 bug。
    Timeout,
}

impl Outcome {
    fn is_timeout(&self) -> bool {
        matches!(self, Outcome::Timeout)
    }
}

async fn call<T: AgentTool>(tool: &T, args: Value, ctx: &ToolContext) -> Outcome {
    let (tx, _rx) = unbounded_channel::<ToolProgress>();
    match tokio::time::timeout(CALL_TIMEOUT, tool.execute(args, tx, ctx)).await {
        Ok(Ok(r)) => Outcome::Ok(r),
        Ok(Err(e)) => Outcome::ToolErr(e.to_string()),
        Err(_) => Outcome::Timeout,
    }
}

/// 断言未挂起；若工具正常返回则交出 ToolResult 供进一步断言。
fn no_hang(o: Outcome, label: &str) -> Option<ToolResult> {
    assert!(!o.is_timeout(), "[{label}] 工具挂起（超过 {CALL_TIMEOUT:?} 未返回）");
    match o {
        Outcome::Ok(r) => Some(r),
        _ => None,
    }
}

/// 断言一次"拒绝"：不得挂起，且结果必须是拒绝（优雅 Err 或 is_error=true）。
/// 工具既可以用 ToolResult{is_error:true} 拒绝，也可以用 anyhow::Err 拒绝，两者都算正确。
fn must_reject(o: Outcome, label: &str) {
    assert!(!o.is_timeout(), "[{label}] 工具挂起（超过 {CALL_TIMEOUT:?} 未返回）");
    match o {
        Outcome::ToolErr(_) => {} // anyhow::Err 即拒绝
        Outcome::Ok(r) => assert!(r.is_error, "[{label}] 应拒绝却成功: {}", r.content),
        Outcome::Timeout => unreachable!(),
    }
}

// ── 构造辅助 ─────────────────────────────────────────────────────────────────
fn ws_ctx(ws: &std::path::Path) -> ToolContext {
    ToolContext::with_workspace(Some(ws.to_string_lossy().to_string()))
}

fn tool_prefs() -> Arc<RwLock<ToolPrefsConfig>> {
    Arc::new(RwLock::new(ToolPrefsConfig::default()))
}

fn process_mgr() -> Arc<ProcessManager> {
    Arc::new(ProcessManager::new(EventBus::new(256)))
}

fn monitor_mgr(pm: Arc<ProcessManager>) -> Arc<MonitorManager> {
    Arc::new(MonitorManager::new(EventBus::new(256), pm))
}

fn memory_none() -> Arc<RwLock<Option<Box<dyn MemoryStore>>>> {
    Arc::new(RwLock::new(None))
}

/// 带 todo_store + session_id 的上下文（供 todo_* 工具）。
fn todo_ctx(ws: &std::path::Path) -> ToolContext {
    let mut c = ws_ctx(ws);
    c.session_id = Some("extreme-test-session".into());
    let db = ws.join("todo.db");
    if let Ok(store) = loom_memory::TodoStore::open(&db) {
        c.todo_store = Some(Arc::new(store));
    }
    c
}

/// 从 structured_content 取字符串字段。
fn sc_str(r: &ToolResult, key: &str) -> Option<String> {
    r.structured_content
        .as_ref()
        .and_then(|v| v.get(key))
        .and_then(|v| v.as_str().map(|s| s.to_string()).or_else(|| v.as_i64().map(|n| n.to_string())))
}

// ============================================================================
// file_read
// ============================================================================

#[tokio::test]
async fn file_read_extremes() {
    let dir = tempfile::tempdir().unwrap();
    let c = ws_ctx(dir.path());
    let t = FileReadTool { tool_prefs: tool_prefs() };

    // 空路径
    no_hang(call(&t, json!({"path": ""}), &c).await, "file_read 空路径");
    // 不存在的文件
    let r = no_hang(call(&t, json!({"path": "nope.txt"}), &c).await, "file_read 不存在").unwrap();
    assert!(r.is_error, "读不存在的文件应报错");
    // 读目录
    let r = no_hang(call(&t, json!({"path": "."}), &c).await, "file_read 目录").unwrap();
    assert!(r.is_error, "读目录应报错");
    // 路径穿越（相对工作区）
    no_hang(call(&t, json!({"path": "../../etc/passwd"}), &c).await, "file_read 穿越");
    // CJK 文件名
    std::fs::write(dir.path().join("中文文件.txt"), "你好世界").unwrap();
    let r = no_hang(call(&t, json!({"path": "中文文件.txt"}), &c).await, "file_read CJK").unwrap();
    assert!(!r.is_error, "读 CJK 文件名应成功: {}", r.content);
    assert!(r.content.contains("你好世界"));
    // NUL 字节 / 二进制内容 —— 不应 panic
    std::fs::write(dir.path().join("bin.dat"), b"a\x00b\x00\xff\xfe c").unwrap();
    no_hang(call(&t, json!({"path": "bin.dat"}), &c).await, "file_read 二进制");
    // 大文件（30MB）—— 应截断返回，不 OOM / 不挂起
    let big = "x".repeat(30 * 1024 * 1024);
    std::fs::write(dir.path().join("big.txt"), &big).unwrap();
    let r = no_hang(call(&t, json!({"path": "big.txt"}), &c).await, "file_read 30MB").unwrap();
    let preview: String = r.content.chars().take(200).collect();
    assert!(!r.is_error, "读大文件应返回（截断），而非报错: {preview}");
    assert!(r.content.len() < big.len(), "大文件应被截断");
    // max_lines = 0
    no_hang(call(&t, json!({"path": "中文文件.txt", "max_lines": 0}), &c).await, "file_read max_lines=0");
    // max_lines 巨大
    no_hang(call(&t, json!({"path": "中文文件.txt", "max_lines": 999999999u64}), &c).await, "file_read max_lines 巨大");
}

// ============================================================================
// file_write
// ============================================================================

#[tokio::test]
async fn file_write_extremes() {
    let dir = tempfile::tempdir().unwrap();
    let c = ws_ctx(dir.path());
    let t = FileWriteTool;

    // 空路径
    let r = no_hang(call(&t, json!({"path": "", "content": "x"}), &c).await, "file_write 空路径").unwrap();
    assert!(r.is_error, "空路径写入应报错");
    // 路径穿越
    no_hang(call(&t, json!({"path": "../../escape.txt", "content": "x"}), &c).await, "file_write 穿越");
    // CJK 内容 + 文件名
    let r = no_hang(call(&t, json!({"path": "写.txt", "content": "中文内容\n第二行"}), &c).await, "file_write CJK").unwrap();
    assert!(!r.is_error, "{}", r.content);
    assert_eq!(std::fs::read_to_string(dir.path().join("写.txt")).unwrap(), "中文内容\n第二行");
    // NUL 字节内容
    no_hang(call(&t, json!({"path": "nul.dat", "content": "a\u{0}b"}), &c).await, "file_write NUL");
    // 大内容（10MB）
    let big = "y".repeat(10 * 1024 * 1024);
    let r = no_hang(call(&t, json!({"path": "bigw.txt", "content": big.clone()}), &c).await, "file_write 10MB").unwrap();
    assert!(!r.is_error);
    assert_eq!(std::fs::metadata(dir.path().join("bigw.txt")).unwrap().len(), big.len() as u64);
    // 追加到不存在的文件
    let r = no_hang(call(&t, json!({"path": "ap.txt", "content": "z", "append": true}), &c).await, "file_write append 不存在").unwrap();
    assert!(!r.is_error);
    // 写入到一个已存在的目录路径
    std::fs::create_dir_all(dir.path().join("somedir")).unwrap();
    no_hang(call(&t, json!({"path": "somedir", "content": "x"}), &c).await, "file_write 目标是目录");
}

// ============================================================================
// file_edit
// ============================================================================

#[tokio::test]
async fn file_edit_extremes() {
    let dir = tempfile::tempdir().unwrap();
    let c = ws_ctx(dir.path());
    let t = FileEditTool;

    std::fs::write(dir.path().join("e.txt"), "line one\nline two\nline one\n").unwrap();
    // 先读以满足 read-before-edit 守卫
    let _ = FileReadTool { tool_prefs: tool_prefs() }
        .execute(json!({"path": "e.txt"}), { let (tx,_r)=unbounded_channel::<ToolProgress>(); tx }, &c)
        .await;

    // 空 oldText（曾导致死循环 OOM）—— 必须拒绝且不挂起
    let r = no_hang(call(&t, json!({"path": "e.txt", "oldText": "", "newText": "X", "replace_all": true}), &c).await, "file_edit 空oldText").unwrap();
    assert!(r.is_error, "空 oldText 应被拒绝");
    // oldText 未找到
    let r = no_hang(call(&t, json!({"path": "e.txt", "oldText": "NOT_THERE", "newText": "X"}), &c).await, "file_edit 未匹配").unwrap();
    assert!(r.is_error, "未匹配应报错");
    // 多匹配但没给 replace_all
    no_hang(call(&t, json!({"path": "e.txt", "oldText": "line one", "newText": "L1"}), &c).await, "file_edit 多匹配");
    // oldText == newText
    no_hang(call(&t, json!({"path": "e.txt", "oldText": "line two", "newText": "line two"}), &c).await, "file_edit 相同");
    // 编辑不存在的文件
    let r = no_hang(call(&t, json!({"path": "ghost.txt", "oldText": "a", "newText": "b"}), &c).await, "file_edit 不存在").unwrap();
    assert!(r.is_error);
    // edits 数组：含一个空 oldText 项
    no_hang(call(&t, json!({"path": "e.txt", "edits": [{"oldText": "", "newText": "Z"}]}), &c).await, "file_edit edits 空项");
    // CJK 替换
    std::fs::write(dir.path().join("cjk.txt"), "你好世界\n").unwrap();
    let _ = FileReadTool { tool_prefs: tool_prefs() }
        .execute(json!({"path": "cjk.txt"}), { let (tx,_r)=unbounded_channel::<ToolProgress>(); tx }, &c)
        .await;
    let r = no_hang(call(&t, json!({"path": "cjk.txt", "oldText": "世界", "newText": "openLoom"}), &c).await, "file_edit CJK").unwrap();
    assert!(!r.is_error, "{}", r.content);
    assert!(std::fs::read_to_string(dir.path().join("cjk.txt")).unwrap().contains("你好openLoom"));
}

// ============================================================================
// file_delete
// ============================================================================

#[tokio::test]
async fn file_delete_extremes() {
    let dir = tempfile::tempdir().unwrap();
    let c = ws_ctx(dir.path());
    let t = FileDeleteTool;

    // 空路径
    let r = no_hang(call(&t, json!({"path": ""}), &c).await, "file_delete 空路径").unwrap();
    assert!(r.is_error);
    // 工作区根（已修复：应拒绝）
    let r = no_hang(call(&t, json!({"path": "."}), &c).await, "file_delete 根").unwrap();
    assert!(r.is_error, "删除工作区根应被拒绝");
    // 不存在的文件
    no_hang(call(&t, json!({"path": "ghost.txt"}), &c).await, "file_delete 不存在");
    // 路径穿越
    no_hang(call(&t, json!({"path": "../../x"}), &c).await, "file_delete 穿越");
    // 删除未读文件 —— 应触发读后删守卫，优雅报错（is_error），绝不 panic
    std::fs::write(dir.path().join("unread.txt"), "x").unwrap();
    let r = no_hang(call(&t, json!({"path": "unread.txt"}), &c).await, "file_delete 未读守卫").unwrap();
    assert!(r.is_error, "删除未读文件应被守卫拒绝: {}", r.content);
    assert!(dir.path().join("unread.txt").exists(), "被守卫拒绝后文件不应被删");

    // 正常删除：先读后删
    std::fs::write(dir.path().join("del.txt"), "x").unwrap();
    let _ = FileReadTool { tool_prefs: tool_prefs() }
        .execute(json!({"path": "del.txt"}), { let (tx,_r)=unbounded_channel::<ToolProgress>(); tx }, &c)
        .await;
    let r = no_hang(call(&t, json!({"path": "del.txt"}), &c).await, "file_delete 正常").unwrap();
    assert!(!r.is_error, "{}", r.content);
    assert!(!dir.path().join("del.txt").exists());
}

// ============================================================================
// content_search
// ============================================================================

#[tokio::test]
async fn content_search_extremes() {
    let dir = tempfile::tempdir().unwrap();
    let c = ws_ctx(dir.path());
    let t = ContentSearchTool;

    std::fs::write(dir.path().join("a.txt"), "hello world\n你好\n").unwrap();
    std::fs::create_dir_all(dir.path().join("sub")).unwrap();
    std::fs::write(dir.path().join("sub/b.txt"), "world peace\n").unwrap();

    // 空 pattern
    no_hang(call(&t, json!({"pattern": ""}), &c).await, "content_search 空pattern");
    // 正则特殊字符（不应 panic）
    for p in [".*", "(", "[", "\\", "a{", "(?", "*"] {
        no_hang(call(&t, json!({"pattern": p}), &c).await, &format!("content_search 特殊 {p:?}"));
    }
    // CJK 搜索
    let r = no_hang(call(&t, json!({"pattern": "你好"}), &c).await, "content_search CJK").unwrap();
    assert!(!r.is_error, "{}", r.content);
    // 正常命中（结构化 matches，含 Windows 盘符路径也不应丢失）
    let r = no_hang(call(&t, json!({"pattern": "world"}), &c).await, "content_search world").unwrap();
    assert!(!r.is_error);
    assert!(r.content.contains("world"), "应命中 world: {}", r.content);
    // 不存在目录
    no_hang(call(&t, json!({"pattern": "x", "path": "no_such_dir"}), &c).await, "content_search 不存在目录");
    // NUL pattern
    no_hang(call(&t, json!({"pattern": "a\u{0}b"}), &c).await, "content_search NUL");
    // max_results = 0
    no_hang(call(&t, json!({"pattern": "world", "max_results": 0}), &c).await, "content_search max=0");
}

// ============================================================================
// file_glob / file_find / file_list
// ============================================================================

#[tokio::test]
async fn file_glob_find_list_extremes() {
    let dir = tempfile::tempdir().unwrap();
    let c = ws_ctx(dir.path());
    std::fs::create_dir_all(dir.path().join("d1/d2/d3")).unwrap();
    std::fs::write(dir.path().join("d1/x.rs"), "fn main(){}").unwrap();
    std::fs::write(dir.path().join("d1/d2/y.rs"), "").unwrap();
    std::fs::write(dir.path().join("中文.rs"), "").unwrap();

    let g = GlobTool;
    // 空 pattern / 非法 pattern
    no_hang(call(&g, json!({"pattern": ""}), &c).await, "glob 空");
    no_hang(call(&g, json!({"pattern": "[invalid"}), &c).await, "glob 非法");
    no_hang(call(&g, json!({"pattern": "**/*.rs"}), &c).await, "glob 递归");
    no_hang(call(&g, json!({"pattern": "../**/*"}), &c).await, "glob 穿越");

    let f = FindTool;
    no_hang(call(&f, json!({"directory": ".", "name_pattern": "*.rs"}), &c).await, "find 正常");
    no_hang(call(&f, json!({"directory": "no_dir", "name_pattern": "*"}), &c).await, "find 不存在目录");
    no_hang(call(&f, json!({"directory": ".", "name_pattern": "", "max_depth": 0}), &c).await, "find depth=0");
    no_hang(call(&f, json!({"directory": ".", "name_pattern": "[bad"}), &c).await, "find 非法模式");

    let l = FileListTool;
    let r = no_hang(call(&l, json!({"path": "."}), &c).await, "list 正常").unwrap();
    assert!(!r.is_error, "{}", r.content);
    no_hang(call(&l, json!({"path": "no_dir"}), &c).await, "list 不存在");
    // 把文件当目录列
    no_hang(call(&l, json!({"path": "d1/x.rs"}), &c).await, "list 目标是文件");
    no_hang(call(&l, json!({"path": ".", "recursive": true}), &c).await, "list 递归");
}

// ============================================================================
// shell
// ============================================================================

#[tokio::test]
async fn shell_extremes() {
    let dir = tempfile::tempdir().unwrap();
    let c = ws_ctx(dir.path());
    let t = ShellTool { tool_prefs: tool_prefs() };

    // 空命令
    no_hang(call(&t, json!({"command": ""}), &c).await, "shell 空命令");
    // 简单回显
    let r = no_hang(call(&t, json!({"command": cmd::SHELL_ECHO}), &c).await, "shell echo").unwrap();
    assert!(r.content.contains("hello"), "echo 输出应含 hello: {}", r.content);
    // unicode 输出（不应 panic / 丢字符）
    no_hang(call(&t, json!({"command": cmd::SHELL_UNICODE}), &c).await, "shell unicode");
    // 非零退出码（应优雅返回，is_error 或含退出码）
    no_hang(call(&t, json!({"command": cmd::SHELL_FAIL}), &c).await, "shell 失败退出");
    // 超时：睡眠命令 + 小 timeout，必须在超时内返回（被杀），不得挂起
    no_hang(call(&t, json!({"command": cmd::SHELL_SLEEP, "timeout": 1}), &c).await, "shell 超时杀");
    // 超长命令字符串
    let long = format!("echo {}", "a".repeat(20000));
    no_hang(call(&t, json!({"command": long}), &c).await, "shell 超长命令");
    // 引号 / 特殊字符
    no_hang(call(&t, json!({"command": "echo \"a && b || c | d > e\""}), &c).await, "shell 特殊字符");
    // 尾部输出不丢失：进程正常退出时的最后几行必须被捕获（尾部竞态回归测试）。
    // 修复前 child.wait() 一返回就做一次性 try_recv，读取任务尚未推送的末行会丢失。
    let r = no_hang(call(&t, json!({"command": cmd::SHELL_MULTILINE}), &c).await, "shell 多行").unwrap();
    assert!(r.content.contains("line_80"), "末行 line_80 丢失（正常退出尾部竞态）: 末尾={:?}",
        r.content.lines().last().unwrap_or("<none>"));
}

// ============================================================================
// process_*
// ============================================================================

#[tokio::test]
async fn process_extremes() {
    let dir = tempfile::tempdir().unwrap();
    let c = ws_ctx(dir.path());
    let pm = process_mgr();
    let spawn = ProcessSpawnTool { process_manager: pm.clone() };
    let kill = ProcessKillTool { process_manager: pm.clone() };
    let wait = ProcessWaitTool { process_manager: pm.clone(), tool_prefs: tool_prefs() };
    let peek = ProcessPeekTool { process_manager: pm.clone() };
    let stdin = ProcessStdinTool { process_manager: pm.clone() };
    let list = ProcessListTool { process_manager: pm.clone() };

    // 空命令
    let r = no_hang(call(&spawn, json!({"command": ""}), &c).await, "spawn 空").unwrap();
    assert!(r.is_error);
    // 不存在的命令
    no_hang(call(&spawn, json!({"command": cmd::NONEXISTENT}), &c).await, "spawn 不存在命令");
    // unicode 输出的进程
    no_hang(call(&spawn, json!({"command": cmd::ECHO_UNICODE}), &c).await, "spawn unicode");

    // 启动一个长进程，随后 peek / wait(短超时) / kill
    let r = no_hang(call(&spawn, json!({"command": cmd::LONG, "name": "longproc"}), &c).await, "spawn 长进程").unwrap();
    assert!(!r.is_error, "长进程应启动成功: {}", r.content);
    let pid = sc_str(&r, "pid").expect("spawn 应返回 pid");

    no_hang(call(&peek, json!({"pid": pid}), &c).await, "peek 运行中");
    // wait 短超时：进程仍在跑，应优雅超时返回而非挂起
    no_hang(call(&wait, json!({"pid": pid, "timeout": 1}), &c).await, "wait 运行中短超时");
    // kill —— 必须真的杀掉（P1 修复点）
    let r = no_hang(call(&kill, json!({"pid": pid}), &c).await, "kill 运行中").unwrap();
    assert!(!r.is_error, "kill 应成功: {}", r.content);
    // 稍等让 waiter 收尾，然后确认进程不再 running
    tokio::time::sleep(Duration::from_millis(500)).await;
    let r = no_hang(call(&peek, json!({"pid": pid}), &c).await, "peek 已杀").unwrap();
    let running = r.structured_content.as_ref().and_then(|v| v.get("running")).and_then(|v| v.as_bool());
    assert_eq!(running, Some(false), "kill 后进程应不再 running: {}", r.content);

    // 对不存在 pid 的操作
    no_hang(call(&kill, json!({"pid": "999999"}), &c).await, "kill 不存在pid");
    no_hang(call(&wait, json!({"pid": "999999", "timeout": 1}), &c).await, "wait 不存在pid");
    no_hang(call(&peek, json!({"pid": "999999"}), &c).await, "peek 不存在pid");
    no_hang(call(&stdin, json!({"pid": "999999", "input": "x"}), &c).await, "stdin 不存在pid");
    // 空 pid
    no_hang(call(&kill, json!({"pid": ""}), &c).await, "kill 空pid");
    // stdin 到已退出进程
    no_hang(call(&stdin, json!({"pid": pid, "input": "data"}), &c).await, "stdin 已退出进程");
    // list
    no_hang(call(&list, json!({}), &c).await, "process_list");
}

// ============================================================================
// monitor_*
// ============================================================================

#[tokio::test]
async fn monitor_extremes() {
    let dir = tempfile::tempdir().unwrap();
    let c = ws_ctx(dir.path());
    let pm = process_mgr();
    let mm = monitor_mgr(pm);
    let mon = MonitorTool { monitor_manager: mm.clone(), tool_prefs: tool_prefs() };
    let mwait = MonitorWaitTool { monitor_manager: mm.clone() };
    let mpeek = MonitorPeekTool { monitor_manager: mm.clone() };
    let mkill = MonitorKillTool { monitor_manager: mm.clone() };
    let mlist = MonitorListTool { monitor_manager: mm.clone() };

    // 启动一个长监控
    let r = no_hang(call(&mon, json!({"command": cmd::LONG, "description": "longmon"}), &c).await, "monitor 启动").unwrap();
    assert!(!r.is_error, "监控应启动: {}", r.content);
    let mid = sc_str(&r, "id").expect("monitor 应返回 id");

    no_hang(call(&mpeek, json!({"monitor_id": mid}), &c).await, "monitor_peek 运行中");
    // monitor_wait 短超时：应优雅返回而非挂起
    no_hang(call(&mwait, json!({"monitor_id": mid, "timeout": 1}), &c).await, "monitor_wait 短超时");
    // kill 监控
    let r = no_hang(call(&mkill, json!({"monitor_id": mid}), &c).await, "monitor_kill").unwrap();
    assert!(!r.is_error, "monitor_kill 应成功: {}", r.content);

    // 不存在 / 空 id
    no_hang(call(&mwait, json!({"monitor_id": "nope", "timeout": 1}), &c).await, "monitor_wait 不存在id");
    no_hang(call(&mpeek, json!({"monitor_id": "nope"}), &c).await, "monitor_peek 不存在id");
    no_hang(call(&mkill, json!({"monitor_id": "nope"}), &c).await, "monitor_kill 不存在id");
    no_hang(call(&mkill, json!({"monitor_id": ""}), &c).await, "monitor_kill 空id");
    // 对已杀监控再 wait（M1：死监控不应被报"仍在运行"）
    let r = no_hang(call(&mwait, json!({"monitor_id": mid, "timeout": 2}), &c).await, "monitor_wait 已杀").unwrap();
    assert!(!r.content.contains("仍在运行") || r.is_error || r.content.contains("退出"),
        "已杀监控不应无限报仍在运行: {}", r.content);
    // list
    no_hang(call(&mlist, json!({}), &c).await, "monitor_list");
    // 启动一个不存在的命令做监控
    no_hang(call(&mon, json!({"command": cmd::NONEXISTENT}), &c).await, "monitor 不存在命令");
}

// ============================================================================
// web_fetch / web_search
// ============================================================================

#[tokio::test]
async fn web_extremes() {
    let dir = tempfile::tempdir().unwrap();
    let c = ws_ctx(dir.path());
    // 用 3s 短超时构造 web_fetch：坏 host 必须在该时限内优雅超时，而不是无限挂起。
    let fast_prefs = Arc::new(RwLock::new(ToolPrefsConfig {
        web_fetch_timeout_secs: 3,
        ..ToolPrefsConfig::default()
    }));
    let fetch = WebFetchTool { tool_prefs: fast_prefs };
    let search = WebSearchTool { tool_prefs: tool_prefs() };

    // 空 URL / 非法 URL（不应 panic）
    let r = no_hang(call(&fetch, json!({"url": ""}), &c).await, "fetch 空url").unwrap();
    assert!(r.is_error, "空 url 应报错");
    no_hang(call(&fetch, json!({"url": "not a url at all"}), &c).await, "fetch 非法url");
    no_hang(call(&fetch, json!({"url": "ftp://example.com/x"}), &c).await, "fetch 非http协议");
    // 不存在的 host —— 必须在短超时内优雅返回（超时守卫），不得挂起或 panic
    must_reject(call(&fetch, json!({"url": "http://nonexistent.invalid.domain.xyz/"}), &c).await, "fetch 坏host超时守卫");
    // 带 unicode 的 url
    no_hang(call(&fetch, json!({"url": "http://example.com/中文"}), &c).await, "fetch unicode url");
    // max_chars = 0
    no_hang(call(&fetch, json!({"url": "http://example.com/", "max_chars": 0}), &c).await, "fetch max_chars=0");

    // 搜索：空 query / unicode
    no_hang(call(&search, json!({"query": ""}), &c).await, "search 空query");
    no_hang(call(&search, json!({"query": "中文搜索 测试"}), &c).await, "search unicode");
}

// ============================================================================
// memory_* / todo_*
// ============================================================================

#[tokio::test]
async fn memory_todo_extremes() {
    let dir = tempfile::tempdir().unwrap();
    let c = todo_ctx(dir.path());

    let ms = MemorySearchTool { memory_store: memory_none() };
    let mr = MemoryRememberTool { memory_store: memory_none() };
    // 无 store 时应优雅报错，不 panic
    no_hang(call(&ms, json!({"query": ""}), &c).await, "memory_search 空query");
    no_hang(call(&ms, json!({"query": "中文"}), &c).await, "memory_search unicode");
    no_hang(call(&mr, json!({"fact": ""}), &c).await, "memory_remember 空");
    no_hang(call(&mr, json!({"fact": "记住这一点", "importance": 99.0}), &c).await, "memory_remember 越界importance");
    no_hang(call(&mr, json!({"fact": "x".repeat(100000)}), &c).await, "memory_remember 巨大fact");

    let tw = TodoWriteTool;
    let tl = TodoListTool;
    // 空 todos
    no_hang(call(&tw, json!({"todos": []}), &c).await, "todo_write 空");
    // 畸形项（缺字段 / 错类型）
    no_hang(call(&tw, json!({"todos": [{"foo": "bar"}, "not-an-object", 42]}), &c).await, "todo_write 畸形项");
    // 正常写
    let r = no_hang(call(&tw, json!({"todos": [{"content": "任务一", "status": "pending"}, {"content": "任务二", "status": "completed"}]}), &c).await, "todo_write 正常").unwrap();
    assert!(!r.is_error, "{}", r.content);
    // 巨大列表
    let huge: Vec<Value> = (0..2000).map(|i| json!({"content": format!("t{i}"), "status": "pending"})).collect();
    no_hang(call(&tw, json!({"todos": huge}), &c).await, "todo_write 巨大列表");
    // 列出
    no_hang(call(&tl, json!({}), &c).await, "todo_list");
    // 无 todo_store 的上下文
    let bare = ws_ctx(dir.path());
    no_hang(call(&tw, json!({"todos": [{"content": "x", "status": "pending"}]}), &bare).await, "todo_write 无store");
    no_hang(call(&tl, json!({}), &bare).await, "todo_list 无store");
}

// ============================================================================
// system_info / token_usage / update_config / schedule_reminder
// ============================================================================

#[tokio::test]
async fn misc_config_extremes() {
    let dir = tempfile::tempdir().unwrap();
    let c = ws_ctx(dir.path());

    let si = SystemInfoTool {
        active_model_name: Arc::new(RwLock::new(None)),
        model_configs: Arc::new(RwLock::new(std::collections::HashMap::new())),
        sandbox_config: Arc::new(RwLock::new(loom_types::config::SandboxConfig::default())),
        tool_prefs: tool_prefs(),
        data_dir: dir.path().to_path_buf(),
    };
    let r = no_hang(call(&si, json!({}), &c).await, "system_info").unwrap();
    assert!(!r.is_error);
    no_hang(call(&si, json!({"query": "bogus_key"}), &c).await, "system_info 非法query");

    let tu = TokenUsageTool { memory_store: memory_none() };
    no_hang(call(&tu, json!({}), &c).await, "token_usage");

    // update_config
    let uc = UpdateConfigTool {
        tool_prefs: tool_prefs(),
        data_dir: dir.path().to_path_buf(),
        event_bus: None,
        config_store: Arc::new(loom_types::config::unified::ConfigStore::load_or_default(dir.path())),
    };
    // 空参数
    no_hang(call(&uc, json!({}), &c).await, "update_config 空");
    // 尝试改 permission_mode（已修复：应拒绝，模型不得自提权）
    let r = no_hang(call(&uc, json!({"permission_mode": "bypassPermissions"}), &c).await, "update_config 提权").unwrap();
    assert!(r.is_error || r.content.contains("拒绝") || r.content.contains("不能") || r.content.contains("用户"),
        "改 permission_mode 应被拒绝: {}", r.content);
    // 尝试读取/回灌 api key（已修复：应脱敏）
    let r = no_hang(call(&uc, json!({"web_search_api_key": "SECRET123"}), &c).await, "update_config key").unwrap();
    assert!(!r.content.contains("SECRET123"), "api key 不应明文回显: {}", r.content);

    // schedule_reminder（无 scheduler，应优雅）
    let sr = ScheduleReminder { cron: Arc::new(RwLock::new(None)) };
    no_hang(call(&sr, json!({"name": "r", "prompt": "p", "cron_expression": "", "kind": "at"}), &c).await, "reminder 空cron");
    no_hang(call(&sr, json!({"name": "r", "prompt": "p", "cron_expression": "not a cron", "kind": "cron"}), &c).await, "reminder 非法cron");
    no_hang(call(&sr, json!({"name": "r", "prompt": "p", "cron_expression": "0 0 1 1 * 2000", "kind": "at"}), &c).await, "reminder 过去时间");
}

// ============================================================================
// ask_user / push_notification / report_findings / use_skill（无 event_bus → 应优雅）
// ============================================================================

#[tokio::test]
async fn notification_extremes() {
    let dir = tempfile::tempdir().unwrap();
    let c = ws_ctx(dir.path());

    let au = AskUserTool;
    no_hang(call(&au, json!({"question": ""}), &c).await, "ask_user 空问题");
    no_hang(call(&au, json!({"question": "选一个", "options": ["a", "b"]}), &c).await, "ask_user 带选项");

    let pn = PushNotificationTool;
    no_hang(call(&pn, json!({"title": "", "body": ""}), &c).await, "push 空");
    no_hang(call(&pn, json!({"title": "标题", "body": "x".repeat(50000)}), &c).await, "push 巨大body");

    let rf = ReportFindingsTool;
    no_hang(call(&rf, json!({"findings": []}), &c).await, "report 空");
    no_hang(call(&rf, json!({"findings": "not-an-array"}), &c).await, "report 非数组");

    let us = UseSkillTool { skill_state: Arc::new(RwLock::new(loom_skills::SkillState::default())) };
    no_hang(call(&us, json!({"skill_name": ""}), &c).await, "use_skill 空名");
    no_hang(call(&us, json!({"skill_name": "no_such_skill"}), &c).await, "use_skill 不存在");
}

// ============================================================================
// entity 管理工具（manage_skills / agent / model / team / cron）
// ============================================================================

#[tokio::test]
async fn entity_manage_extremes() {
    let dir = tempfile::tempdir().unwrap();
    let c = ws_ctx(dir.path());

    // manage_skills
    let skills = ManageSkillsTool { skill_state: Arc::new(RwLock::new(loom_skills::SkillState::default())) };
    no_hang(call(&skills, json!({"action": "list"}), &c).await, "skills list");
    no_hang(call(&skills, json!({"action": ""}), &c).await, "skills 空action");
    no_hang(call(&skills, json!({"action": "bogus"}), &c).await, "skills 非法action");
    // 路径穿越导入名（已修复：应拒绝）
    must_reject(call(&skills, json!({"action": "import", "name": "../evil", "files": [{"path": "SKILL.md", "content": "x"}]}), &c).await, "skills 穿越name");
    // 绝对路径 rel_path（已修复：应拒绝）
    must_reject(call(&skills, json!({"action": "import", "name": "okskill", "files": [{"path": "C:\\evil\\x.md", "content": "x"}]}), &c).await, "skills 绝对rel_path");
    // 缺 name 的 import
    no_hang(call(&skills, json!({"action": "import"}), &c).await, "skills import 缺name");
    // 删除不存在
    no_hang(call(&skills, json!({"action": "delete", "name": "ghost"}), &c).await, "skills delete 不存在");

    // manage_agent / model / team（无 store，应优雅报错）
    let agent = ManageAgentTool { memory_store: memory_none(), cache: Arc::new(RwLock::new(std::collections::HashMap::new())) };
    no_hang(call(&agent, json!({"action": "list"}), &c).await, "agent list");
    no_hang(call(&agent, json!({"action": ""}), &c).await, "agent 空action");
    no_hang(call(&agent, json!({"action": "bogus"}), &c).await, "agent 非法action");
    no_hang(call(&agent, json!({"action": "get", "name": "ghost"}), &c).await, "agent get 不存在");
    no_hang(call(&agent, json!({"action": "create"}), &c).await, "agent create 缺字段");

    let model = ManageModelTool { memory_store: memory_none(), cache: Arc::new(RwLock::new(std::collections::HashMap::new())), active_model_name: Arc::new(RwLock::new(None)) };
    no_hang(call(&model, json!({"action": "list"}), &c).await, "model list");
    no_hang(call(&model, json!({"action": "set_active", "name": "ghost"}), &c).await, "model set_active 不存在");
    no_hang(call(&model, json!({"action": "bogus"}), &c).await, "model 非法action");

    let team = ManageTeamTool { memory_store: memory_none(), cache: Arc::new(RwLock::new(std::collections::HashMap::new())) };
    no_hang(call(&team, json!({"action": "list"}), &c).await, "team list");
    no_hang(call(&team, json!({"action": "bogus"}), &c).await, "team 非法action");
    // 畸形 members（N9：格式错配不应损坏）
    no_hang(call(&team, json!({"action": "create", "name": "t", "members": "not-an-array"}), &c).await, "team 畸形members");
    no_hang(call(&team, json!({"action": "create", "name": "t", "members": [{"role": "x"}]}), &c).await, "team members对象");

    // manage_cron（无 scheduler，应优雅）
    let cron = ManageCronTool { cron_scheduler: Arc::new(RwLock::new(None)) };
    no_hang(call(&cron, json!({"action": "list"}), &c).await, "cron list");
    no_hang(call(&cron, json!({"action": "bogus"}), &c).await, "cron 非法action");
    no_hang(call(&cron, json!({"action": "add", "cron_expression": "bad"}), &c).await, "cron add 非法表达式");
}

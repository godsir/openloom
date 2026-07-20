# openLoom 工具审计报告

> 范围：全部内置工具（builtin_tools / entity_* / process_manager / monitor_manager / loom-security 权限层）。
> 方法：5 个并行 agent 分组精读 + 逐行对抗性核实 + CLI 端到端实测（deepseek-v4-pro 跑 10 工具任务）。
> 状态标记：✅ 已修复 ｜ ⏳ 待修复（建议后续处理）

## 本次修复汇总（15 项，均已通过 cargo test + clippy）

| 类别 | 项 | 说明 |
|------|----|------|
| 关键 | D1 | 流式收尾竞态丢工具参数 → 补全 drain 循环的 ToolCall 分支 |
| 安全 | N6 | manage_mcp/manage_skills 权限定级 Low 兜底 → 显式 High 并挂 shell/fs_write 权限位 |
| 安全 | N7 | 沙箱黑名单漏 manage_skills → 补上 |
| 安全 | S4 | process_spawn 绕过沙箱 → 补 check_exec |
| 安全 | S5 | file_delete 可删工作区根 → 空路径校验 + 禁删 workspace 根 |
| 安全 | C1 | UpdateConfig 明文回灌 web_search_api_key → 输出脱敏 `***` |
| 安全 | C2 | UpdateConfig 允许模型改 permission_mode（自提权）→ 拒绝并引导用户手动切换 |
| 安全 | N5 | manage_skills 路径穿越 / 任意目录删除 → name + rel_path 双重校验 |
| 正确性 | F1 | file_edit 空 oldText + replace_all 死循环 OOM → 拒绝空 oldText + 循环兜底 |
| 正确性 | F2 | content_search Windows 盘符冒号破坏结构化解析 → 底层直接返回结构化命中 |
| 正确性 | P1 | process_kill 形同虚设（Child 被 waiter 取走）→ oneshot 信号让 waiter 执行 kill |
| 正确性 | P2 | 输出行 `&line[..8192]` 字节切片 panic → 字符边界安全截断 |
| 正确性 | P3 | 非 UTF-8(GBK) 输出静默停读（process/shell/monitor）→ 按字节读 + from_utf8_lossy |
| 正确性 | M1 | monitor_wait 死监控被报"仍在运行"→ 以 running 字段为权威判据 |
| 正确性 | N3 | WebFetch 按字节切 String panic（中文页面）→ 按字符数截断 |

---

## 第二轮：确定性极端测试（tests/extreme.rs）

> 方法：不依赖模型，直接构造每个工具的 `execute()`，灌入对抗性输入（空值 / 超大内容 / unicode / NUL 字节 / 路径穿越 / 死进程 / 坏 URL / 畸形参数），每次调用包 20s `tokio::time::timeout` 防挂死。14 个测试族覆盖全部 42 个工具（file / shell / process / monitor / web / memory / todo / config / entity）。
> 核心不变量：任何输入下 **不 panic、不无限挂起**。

### 结论

所有工具面对对抗输入 **零 panic、零无限挂起**。第一轮修复全部经受住极端输入复验（file_edit 死循环、process kill、monitor 死监控、content_search 结构化、WebFetch 字符安全、manage_skills 路径校验等均稳定）。

### 本轮新修复（2 项，shell 真实潜在 bug）

| ID | 问题 | 位置 | 修复 |
|----|------|------|------|
| SH1 | shell 正常退出**尾部竞态丢输出**：`child.wait()` 一返回就做一次性 `try_recv`，与读取任务竞态，进程最后几行被静默丢弃 | builtin_tools.rs ShellTool | 收尾改为 `recv().await` 排空到 EOF（读取任务全部结束才返回 None），末行必被捕获 |
| SH2 | shell 输出**无上限累积**：stdout/stderr Vec 累积全部输出直到进程退出，仅末尾截断 → 话多/长命令可撑爆内存 | builtin_tools.rs ShellTool | 新增 `shell_accumulate` 按字节封顶，超限丢弃（结果本就截断），前端实时进度不受影响 |

**修复中的二次坑（已处理）**：SH1 改为 `recv().await` 排空到 EOF 后，在 Windows 杀进程场景挂死——杀 shell 不会杀其子进程（如 `powershell -Command ping` 派生的 `ping.exe`），孤儿孙进程持有 stdout 管道不放，EOF 永不到来。故排空**有界化**：正常退出给 2s（读取任务微秒级 EOF，宽裕捕获末行），杀进程/超时只给 500ms（仅抓已缓冲输出，不等孤儿）。

### 回归守卫（tests/extreme.rs，14 测试全绿）

- `shell 多行`：打印 80 行，断言末行 `line_80` 必须存在（SH1 回归）。
- `shell 超时杀`：睡眠命令 + 1s timeout，必须按时返回不挂起（验证有界排空）。
- `file_delete 未读守卫`：删未读文件返回 `is_error`（优雅），且文件不被删。
- `fetch 坏host超时守卫`：web_fetch 配 3s 短超时，坏 host 必须按时优雅返回。
- `skills 穿越name / 绝对rel_path`：`must_reject` 断言被拒（anyhow::Err 或 is_error）。
- 其余：file_read 30MB 截断、file_write 10MB、file_edit 空 oldText / CJK、content_search 正则特殊字符、process kill 后 running==false、monitor 死监控不误报等。

### 澄清（非 bug，极端测试验证其优雅）

- **file_delete 读后删守卫**：删除未先读的文件返回 `is_error`（优雅报错，非 panic），防止误删。
- **web_fetch 有 30s 超时守卫**（`web_fetch_timeout_secs` 默认 30）：坏/慢 host 不会无限挂起。
- **manage_skills 穿越 / 绝对路径导入**：name 与 rel_path 双重校验，`../evil`、`C:\evil\x` 均被拒。

### 仍待处理（择要）

- **file_read 全量加载**：`read_to_string` 载入整个文件后截断，多 GB 文件 OOM（中危，建议流式读 / 大小守卫）。本轮 30MB 测试通过，仅超大文件有风险。
- **process_wait `-1` 哨兵撞车（P11）**：信号杀死的进程 exit_code 可能落到 `-1`，与"仍在运行"哨兵冲突（`process_peek` 的 `running` 字段已是权威判据，`process_wait` 仍按 exit_code 推断）。
- 其余中/低危项见第四、五节。

---

## 一、关键（系统性）

| ID | 问题 | 位置 | 状态 |
|----|------|------|------|
| D1 | 流式 drain 循环丢 ToolCall 参数块 → 工具调用 JSON 不完整、首次调用间歇失败 | agent_loop.rs drain loop | ✅ |

## 二、高危 — 安全

| ID | 问题 | 位置 | 状态 |
|----|------|------|------|
| N6 | manage_mcp/manage_skills 权限定级 Low 兜底，绕过所有权限模式 | loom-security/lib.rs | ✅ |
| N7 | 沙箱黑名单漏 manage_skills → 沙箱逃逸 | tool_registry.rs | ✅ |
| S4 | process_spawn 完全忽略 sandbox | builtin_tools.rs ProcessSpawnTool | ✅ |
| S5 | file_delete 可删 workspace 根 | builtin_tools.rs FileDeleteTool | ✅ |
| C1 | UpdateConfig 明文回灌 web_search_api_key | builtin_tools.rs UpdateConfigTool | ✅ |
| C2 | UpdateConfig 允许模型改 permission_mode 自提权 | builtin_tools.rs UpdateConfigTool | ✅ |
| N5 | manage_skills 路径穿越 + remove_dir_all 删任意目录 | entity_skills_tools.rs | ✅ |
| N4 | manage_mcp command/args/env 零校验立即 spawn + autostart 持久化 → RCE | entity_mcp_tools.rs | ⏳ 建议：风险定级 High 强制确认 + command 白名单 + autostart 默认 false |
| N1 | WebFetch 仅校验 scheme，零 host/IP 校验 → SSRF（云元数据/内网） | builtin_tools.rs WebFetchTool | ⏳ 建议：私网/环回/链路本地 IP 黑名单 + 解析最终 IP 二次校验 |
| N2 | WebFetch 默认跟随重定向且不校验目标 | builtin_tools.rs WebFetchTool | ⏳ 建议：redirect::Policy::none() 或逐跳校验 |
| N8 | manage_model api_key_env 可读任意环境变量并外泄 + base_url SSRF | entity_tools.rs | ⏳ 建议：api_key_env 限已知键名 + base_url 禁私网 |

## 三、高危 — 正确性 / 崩溃

| ID | 问题 | 位置 | 状态 |
|----|------|------|------|
| F1 | file_edit 空 oldText + replace_all 死循环 OOM | builtin_tools.rs FileEditTool | ✅ |
| F2 | content_search Windows 盘符冒号 → 结构化 matches 恒空 | builtin_tools.rs ContentSearchTool | ✅ |
| P1 | process_kill 形同虚设 | process_manager.rs | ✅ |
| P2 | 输出行字节切片 panic | process_manager.rs | ✅ |
| P3 | 非 UTF-8(GBK) 输出静默停读 | process_manager.rs + shell | ✅ |
| M1 | monitor_wait 死监控报"仍在运行"→ 空转 | builtin_tools.rs MonitorWaitTool | ✅ |
| N3 | WebFetch 字节切片 panic（中文页面） | builtin_tools.rs WebFetchTool | ✅ |
| N9 | manage_team parse_members 格式错配 → 静默损坏配置 | entity_tools.rs | ⏳ 建议：按 untagged 格式解析 |

## 四、中危（择要，⏳ 待后续批次）

- file_find 用 `current_dir()` 而非搜索根做相对化（F4，builtin_tools.rs:1855）
- content_search 递归遍历无深度/symlink 防护 → 目录链接环栈溢出（F5）
- file_find 不透传 sandbox → 被拒目录下文件名仍被枚举（F6）
- file_edit 混用 CRLF/LF 时全量改写换行；重叠检测 O(n²)
- file_read 全量加载后截断 → 大文件 OOM
- shell 输出无上限累积；超时丢弃已采集输出；正常退出尾部竞态丢失
- process_wait `-1` 哨兵与信号杀死撞车（P11）；timeout 实际不作硬上限
- monitor 环形缓冲绝对 cursor → 饱和后永久卡死；超时在持续输出下失效；持写锁跨 await；订阅晚于启动丢早期输出
- UpdateConfig 读-改-写无锁 lost-update；UI-only 改动无 event_bus 时报成功未生效
- PushNotification/ReportFindings 无 event_bus 时静默丢弃却报成功
- manage_model list/get 明文回显 key；set_active 名字不存在静默清零
- create 命中已存在实体静默覆盖（manage_agent/model/team）；删除无引用完整性检查
- WebFetch 全量读响应体 OOM；WebSearch key 拼进 URL 经错误回流
- schedule_reminder/manage_cron 无频率下限（每秒任务 → 费用爆炸）；定时 prompt 全工具权限无人值守执行；cron 按 UTC 求值与本地时区文案不符

## 五、低危（择要，⏳）

- 多处 schema 声明 required/enum 但 execute 不强制
- 删除/查询不存在实体返回成功（多工具）
- manage_cron 文档字段数错误（5 vs 6/7）
- WebSearch 硬编码 cx 兜底、搜索后端无读取超时
- UseSkill 目录用 summaries、execute 查 bodies，不同步
- schedule_reminder 过去时间一次性任务返回成功但永不触发；触发后不清理
- tool 文案/回执小错

## 六、确认无问题（已排查）

- 不可信参数零 panic（serde_json Index 缺键返回 Null；唯一真实 panic 是 N3/P2 字节切片，已修）。
- SQL 全参数化，无注入。
- cron 表达式持久化前校验。
- manage_skills delete 路径有三重校验（与 import 缺失形成对比 → 是遗漏，已补）。
- MCP stdio 数组传参无 shell 拼接。
- 进程/监控 gc 有界、无无限增长。

## 后续修复顺序建议

1. **N4**（manage_mcp RCE：定级 High + 强制确认 + command 白名单 + autostart 默认 false）
2. **N1/N2**（WebFetch SSRF：host/IP 黑名单 + 重定向策略）
3. **N8**（manage_model 环境变量外泄 + base_url SSRF）
4. **N9**（manage_team 格式错配）
5. 中危批次：monitor 环形缓冲/超时/锁；process_wait 哨兵；file_read 流式读；UpdateConfig lost-update
6. 低危批量清理（schema 对齐、回执文案）

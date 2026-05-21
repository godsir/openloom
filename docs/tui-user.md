# openLoom TUI 用户手册

## 启动

```bash
openloom chat [--config <path>]
```

启动后进入全屏终端交互界面，自动创建一个新会话。

---

## 键盘快捷键

### 编辑

| 快捷键 | 功能 |
|--------|------|
| `Enter` | 发送消息 |
| `Shift+Enter` / `Ctrl+J` | 插入换行（多行输入） |
| `Tab` | 自动补全 `/` 命令 |
| `Ctrl+G` | 打开外部编辑器（`$EDITOR` / `$VISUAL` / notepad）编辑多行内容 |
| `Ctrl+R` | 增量历史搜索 |
| `Ctrl+L` | 跳回底部 + 重绘 |
| `Ctrl+C` (一次) | 取消当前流式生成 |
| `Ctrl+C` (两次, 500ms 内) | 退出（优雅关机，自动 WAL checkpoint） |

### 导航

| 快捷键 | 功能 |
|--------|------|
| `↑` / `↓` | 非命令弹窗时：浏览输入历史；命令弹窗时：选择命令 |
| `PgUp` | 向上翻 10 行（看旧消息） |
| `PgDn` | 向下翻 10 行（看新消息） |
| `Esc` | 关闭弹窗（命令补全 / diff / 审批 / 历史搜索） |

### 鼠标

| 操作 | 功能 |
|------|------|
| 滚轮上 | 向上看旧消息（退出自动跟随模式） |
| 滚轮下 | 向下看新消息（滚到底部自动恢复跟随模式） |

---

## 输入模式

### 普通对话

直接输入文字按 `Enter`，消息发送给 Engine 进行推理路由（关键词快速路径 / 本地模型 / 云端模型 / Agent Loop）。

### Bash 模式 `!`

```
!ls -la
!git status
```

以 `!` 开头会在本地执行 shell 命令，输出显示在聊天区。Windows 用 `cmd /C`，Unix 用 `sh -c`。

### 文件列表 `@`

```
@src/
@.
```

以 `@` 开头会列出指定目录的文件/子目录，最多显示 30 条。

### 命令模式 `/`

以 `/` 开头触发斜杠命令，输入时自动弹出补全菜单。详见下方命令列表。

---

## 斜杠命令

### 聊天

| 命令 | 说明 |
|------|------|
| `/help` | 显示所有命令分组列表 |
| `/clear` | 清空当前对话消息 |
| `/exit` | 优雅退出（执行 Engine shutdown + WAL checkpoint） |

### 会话管理

| 命令 | 说明 |
|------|------|
| `/session` | 列出所有会话（ID、创建时间、消息数），当前会话标 `*` |
| `/session new` | 创建新会话并切换过去 |
| `/session switch <id>` | 切换到指定会话，自动从后端加载历史消息 |

### 记忆系统

| 命令 | 说明 |
|------|------|
| `/memory persona` | 显示当前人格摘要（基于认知图谱生成） |
| `/memory events [limit]` | 列出最近事件，默认 10 条 |
| `/memory search <query>` | 全文搜索事件（FTS5），最多返回 20 条 |
| `/memory cognitions` | 列出 USER 主题的认知列表（含 ID、版本、置信度） |
| `/memory cognition-history <id>` | 查看指定认知的版本历史（所有快照） |
| `/memory cognition-rollback <id> <version>` | 将指定认知回滚到某个版本 |

认知命令示例：
```
/memory cognitions                          → 列出所有认知
/memory cognition-history 3                 → 查看认知 ID=3 的版本历史
/memory cognition-rollback 3 2              → 将认知 ID=3 回滚到 v2
```

### 技能系统

| 命令 | 说明 |
|------|------|
| `/skills` | 列出所有已注册技能（名称、描述、触发词） |
| `/skill invoke <name> [json]` | 调用指定技能，可选传入 JSON 参数 |

技能调用示例：
```
/skills                                     → 查看可用技能
/skill invoke file_manager                  → 调用技能（无参数）
/skill invoke file_manager {"path": "/tmp"} → 调用技能（JSON 参数）
/skill invoke calculator 2+3               → 非 JSON 自动包装为 {"input": "2+3"}
```

### 系统诊断

| 命令 | 说明 |
|------|------|
| `/model` | 显示模型信息（名称、状态、运行时间、GPU） |
| `/agent` | 显示当前 Agent 状态（Idle / Thinking / Acting） |
| `/doctor` | 系统诊断（Engine 状态 + GPU + 模型） |
| `/cache` | KV Cache 统计（命中率、块数、大小） |
| `/version` | 显示 openLoom 版本号 |

### 配置管理

| 命令 | 说明 |
|------|------|
| `/config` | 显示完整配置（JSON 格式） |
| `/config <key>` | 显示指定配置项的值 |
| `/config set <key>=<value>` | 修改配置项并持久化到 config.toml |
| `/config reload` | 从磁盘重新加载 config.toml 到 Engine |

配置操作示例：
```
/config                                     → 查看全部配置
/config models                              → 查看 models 配置块
/config set rate_limit.interval_ms=200      → 修改限流间隔
/config reload                              → 重新加载配置文件
```

### 显示与统计

| 命令 | 说明 |
|------|------|
| `/theme` | 列出可用主题 |
| `/theme <name>` | 切换主题（`dark` / `light` / `high-contrast` / `hc`） |
| `/cost` | Token 成本看板（总 prompt/completion tokens + 估算费用） |
| `/usage` | Token 用量看板（当前轮次、上下文窗口占用率、费用） |
| `/diff` | 对比最近两次 assistant 回复的差异（弹出 diff 面板） |
| `/keymap` | 显示快捷键列表 |

---

## 历史搜索（Ctrl+R）

1. 按 `Ctrl+R` 进入搜索模式
2. 输入关键词，实时过滤匹配的输入历史
3. `↑` / `↓` 选择匹配项
4. `Enter` 确认选择，内容填入输入框
5. `Esc` 取消搜索

---

## Diff 查看器

执行 `/diff` 后弹出 diff 面板，对比最近两次 assistant 回复的逐行差异：

| 操作 | 功能 |
|------|------|
| `↑` / `↓` | 逐行滚动 |
| `PgUp` / `PgDn` | 翻页滚动（10 行） |
| `Esc` | 关闭 diff 面板 |

绿色 = 新增行，红色 = 删除行，灰色 = 上下文行。

---

## 审批覆盖层

当 Agent Loop 执行工具需要用户审批时，弹出审批面板：

| 按键 | 操作 |
|------|------|
| `A` | 批准（Approve） |
| `D` | 拒绝（Deny） |
| `S` | 批准本次会话（ApproveSession） |
| `C` | 取消（Cancel） |
| `Enter` | 确认当前选择 |
| `←` / `→` | 切换选择项 |
| `Esc` | 关闭（等同取消） |

边框颜色指示风险等级：灰色 = 只读，黄色 = 文件写入，红色 = Shell/网络操作。

---

## 状态栏

屏幕底部状态栏从左到右显示：

| 区域 | 内容 |
|------|------|
| 状态 | `Ready` / `Waiting...` / `Streaming` / `Thinking...` / `Acting...`（带旋转动画） |
| 模型 | 模型名称缩写 |
| 分支 | 当前 Git 分支 |
| 上下文 | 上下文窗口占用百分比 + 已用/总量（>80% 变橙色） |
| Token | 总 token 数 + 估算费用 |

---

## 主题

| 主题名 | 别名 | 说明 |
|--------|------|------|
| `dark` | — | 深色背景（默认） |
| `light` | — | 浅色背景 |
| `high-contrast` | `hc` | 高对比度 |

切换示例：`/theme light`、`/theme hc`

---

## 欢迎界面

空会话时显示：
- openLOOM ASCII art
- 版本号、模型名称、当前工作目录、Git 分支
- 快捷操作提示（`/` 命令、`!` bash、`@` 文件、`Ctrl+C x2` 退出）
- 未配置云端 API Key 时显示警告

---

## 自动行为

| 场景 | 行为 |
|------|------|
| 发送消息 | 自动跳到底部，进入跟随模式 |
| 流式生成中 | 50ms 轮询间隔，光标闪烁 + 旋转动画 |
| 向上滚动 | 退出跟随模式，新消息不会自动跳转 |
| 滚回底部 | 自动恢复跟随模式 |
| 双击 Ctrl+C | 优雅关机：等待进行中请求完成（最多 5s），WAL checkpoint 后退出 |

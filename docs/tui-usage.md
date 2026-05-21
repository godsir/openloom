# openLoom TUI 使用文档

## 启动

```bash
# 交互模式（默认，新建会话）
openloom chat

# 继续上次对话（恢复最近一个会话）
openloom chat --continue
openloom chat -r

# 继续指定会话
openloom chat --continue <session-id>
openloom chat -r <session-id>

# 指定模型
openloom chat -m "anthropic:claude-sonnet-4-20250514"

# 单次执行（非交互）
openloom chat -c "解释什么是认知图谱"

# 跳过权限确认（危险）
openloom chat --dangerously-skip-permissions

# 指定配置文件
openloom chat --config /path/to/config.toml

# 组合使用
openloom chat -r -m "deepseek-chat"
```

### 其他命令

```bash
openloom serve              # 启动 HTTP/WebSocket 服务
openloom run "写一首诗"     # 单次执行并退出
openloom doctor             # 系统诊断
openloom memory persona     # 查看认知画像
openloom session list       # 列出所有会话
openloom config path        # 显示配置文件路径
openloom download-model     # 下载 GGUF 模型
```

## 界面布局

```
┌─────────────────────────────────────────────────┐
│                                                 │
│  ❯ you                                         │
│    你好                                         │
│                                                 │
│  ◆ openLoom                                    │
│    你好！有什么我可以帮你的？                      │
│    ▍                                            │
│                                                 │  ← 消息区域（可滚动）
│                                                 │
│                                       ↓ 3 new  │  ← 新消息指示
├─────────────────────────────────────────────────┤
│ ⠹ claude-sonnet │ main │ 1M ctx    F:/openLoom │  ← 状态栏（带动画）
├─ streaming ─────────────────────────────────────┤  ← 分隔线（显示状态）
│ > _                                             │  ← 输入区域
└─────────────────────────────────────────────────┘
```

## 快捷键

### 输入模式

| 按键 | 功能 |
|------|------|
| `Enter` | 发送消息 / 选择弹窗命令 |
| `Shift+Enter` | 插入换行 |
| `Ctrl+J` | 插入换行（备选） |
| `Tab` | 自动补全 / 循环弹窗选项 |
| `↑` / `↓` | 浏览历史 / 导航弹窗 |
| `Ctrl+G` | 打开外部编辑器（$EDITOR） |
| `Ctrl+R` | 搜索历史 |
| `Esc` | 关闭命令弹窗 |

### 全局

| 按键 | 功能 |
|------|------|
| `Ctrl+C` | 取消流式输出（第一次）/ 退出（第二次，2秒内） |
| `Ctrl+L` | 重绘屏幕 |
| `PageUp` / `PageDown` | 滚动消息视口（25行/次） |

### 流式状态

| 按键 | 功能 |
|------|------|
| `Ctrl+C` | 取消当前生成 |
| `Esc` | 取消当前生成 |

### Overlay 模式

| 按键 | 功能 |
|------|------|
| `Esc` / `q` | 关闭 |
| `j` / `k` / `↑` / `↓` | 上下滚动 |
| `PageUp` / `PageDown` | 翻页 |
| `Home` / `End` | 跳到顶/底 |

## 斜杠命令

输入 `/` 触发命令弹窗，支持：
- **↑/↓ 键导航** — 移动高亮选择
- **Tab** — 循环并填充到输入框
- **Enter** — 选择高亮命令
- **Esc** — 关闭弹窗
- **继续输入** — 实时过滤匹配

| 命令 | 说明 |
|------|------|
| `/help` | 显示帮助面板 |
| `/model` | 显示模型详细信息（名称、后端、URL、context、API key 状态） |
| `/model set <backend> <model> [key_env]` | 配置云端模型 |
| `/token` | 当前会话 token 累计用量 + 费用 |
| `/token summary` | 全局按模型分组统计 |
| `/token today` | 今日用量 |
| `/token session [id]` | 指定会话的逐条明细 |
| `/token history [N]` | 最近 N 条请求记录（默认 10） |
| `/cost` | `/token` 的别名 |
| `/health` | 引擎状态、GPU、缓存诊断 |
| `/clear` | 清空所有消息 |
| `/theme dark` | 切换暗色主题 |
| `/theme light` | 切换亮色主题 |
| `/session new` | 创建新会话 |
| `/session list` | 列出所有会话 |
| `/memory persona` | 显示人格摘要 |
| `/memory events [N]` | 列出最近事件 |
| `/memory cognitions [subject]` | 列出认知（带 [G]/[P] scope 标记） |
| `/memory search <query>` | FTS5 全文搜索 |
| `/skills list` | 列出已注册技能 |
| `/skills invoke <name> [params]` | 直接调用技能 |
| `/local status` | 本地模型状态 + LM Studio/Ollama 连通性 |
| `/local test` | 发送测试消息验证推理可用性 |
| `/config get [key]` | 查看配置 |
| `/config set <key> <value>` | 修改配置 |

> 提示：以 `//` 开头的消息会作为普通文本发送，不触发命令。

## 状态栏说明

```
 ● model │ branch │ 1M ctx              1.2k / 3.4k used  $0.0012
```

- **状态指示器**：`●` 绿色=空闲，⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏ 黄色=思考中，蓝色=流式输出
- **model**：当前使用的模型名称
- **branch**：当前 git 分支（如果在 git 仓库内）
- **ctx**：模型上下文窗口大小
- **右侧**：会话累计 prompt/completion token 数 + cache 命中率 + 估算费用；无 token 时显示当前目录

## 主题

内置两套主题：

- **dark**（默认）— 纯黑背景 `#0d0d0d`，低饱和蓝 accent `#6ea0ff`
- **light** — 白色背景，深蓝 accent

通过 `/theme dark` 或 `/theme light` 切换。

## 消息角色标记

| 标记 | 角色 |
|------|------|
| `❯ you` | 用户输入 |
| `◆ openLoom` | 助手回复 |
| `○ thinking` | 思考过程 |
| `▸ tool` | 工具调用 |
| `◇ result` | 工具结果 |
| `✖ error` | 错误信息 |

## 滚动行为

- **鼠标滚轮**：终端原生滚动，直接滚动终端窗口内容
- **鼠标左键拖拽**：终端原生文本选择，可复制内容
- **PageUp / PageDown**：应用内滚动消息视口（25行/次）
- 新消息到达时自动滚动到底部
- 手动 PageUp 后停止自动滚动
- PageDown 回到底部时重新启用自动滚动
- 右下角显示未读消息数量（`↓ N new`）

## 外部编辑器

按 `Ctrl+G` 打开外部编辑器编写长文本。优先使用 `$EDITOR` 环境变量，其次 `$VISUAL`，Windows 默认 `notepad`，其他系统默认 `vi`。保存退出后内容自动填入输入框。

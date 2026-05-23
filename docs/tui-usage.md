# Loom CLI 使用文档

> 基于 Codex CLI 移植，命令体系与 Codex 兼容。

## 安装

```bash
cargo build -p loom-cli --release
# 二进制路径: ./target/release/loom.exe (Windows) / ./target/release/loom (Linux/macOS)
```

## 启动

```bash
# 交互模式（默认，启动 TUI）
loom

# 带初始提示词启动
loom "帮我重构这个函数"

# 非交互执行
loom exec "解释这段代码"
loom e "解释这段代码"          # 别名

# 代码审查
loom review

# 继续上次会话
loom resume
loom resume --last

# 分叉会话
loom fork
loom fork --last

# 应用最近的 diff
loom apply
loom a                          # 别名

# Shell 补全
loom completion bash
loom completion zsh
loom completion fish
```

## 其他命令

```bash
loom doctor              # 诊断：配置、环境、存储
loom mcp                 # MCP 服务器管理
loom plugin              # 插件管理
loom sandbox             # 沙箱执行
loom debug               # 调试工具
loom execpolicy          # 执行策略检查
```

## 快捷键（Codex TUI）

Codex TUI 使用 Vim 风格的快捷键体系。

### 输入

| 按键 | 功能 |
|------|------|
| `Enter` | 发送消息 |
| `Shift+Enter` | 换行 |
| `Esc` | 退出当前模式 / 取消 |
| `Ctrl+C` | 中断生成 |
| `Ctrl+R` | 搜索历史 |
| `Ctrl+G` | 打开外部编辑器 |
| `Tab` | 补全 |

### 导航

| 按键 | 功能 |
|------|------|
| `j` / `k` / `↑` / `↓` | 上下滚动 |
| `Ctrl+D` / `Ctrl+U` | 半页滚动 |
| `Ctrl+F` / `Ctrl+B` | 全页滚动 |
| `gg` / `G` | 跳转到开头/结尾 |

### 模式

| 按键 | 功能 |
|------|------|
| `/code` | 切换到编码模式（默认） |
| `/chat` | 切换到陪伴模式 |
| `Esc` | 退出弹窗/对话框 |

## 模式系统

两种核心模式通过命令切换：

- **编码模式（默认）**：完整工具链——文件读写、diff 预览、shell 执行、沙箱。`loom` 启动默认进入。
- **陪伴模式**：全局记忆、人格交互、日常对话。`/chat` 切换。

编码时的洞察可写入全局记忆，陪伴时可调用。

## 权限审批

高风险操作（文件写入、shell 执行）弹出确认对话框：

- **A** — 批准本次
- **D** — 拒绝
- **S** — 本次会话全部批准
- **C** — 取消

## 配置文件

```toml
# ~/.loom/config.toml 或项目 .loom/config.toml

[model]
model = "anthropic:claude-sonnet-4-20250514"

[features]
code_mode = true
memories = true
plugins = true
```

## 功能开关

```bash
loom -c features.memories=true           # 启用记忆
loom -c features.code_mode=false         # 禁用编码模式
```

## 外部技能

```
<data_dir>/skills/*/SKILL.md             # 全局技能
<cwd>/.loom/skills/*/SKILL.md            # 项目技能
```

## 项目指令

| 文件 | 作用 |
|------|------|
| `<data_dir>/loom.md` | 全局指令 |
| `<cwd>/loom.md` | 项目级指令 |
| `<cwd>/CLAUDE.md` | 兼容格式 |

## 数据目录

| 平台 | 路径 |
|------|------|
| Windows | `%APPDATA%/openLoom/` |
| macOS | `~/Library/Application Support/openLoom/` |
| Linux | `~/.local/share/openLoom/` |

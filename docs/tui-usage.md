# Loom CLI 使用文档

## 安装

```bash
cargo build -p loom-cli --release
cp target/release/loom ~/.cargo/bin/
```

## 快速开始

```bash
loom                          # 陪伴模式（Chat，纯对话无工具）
loom code                     # 编码模式（Code，完整 agent + 工具）
loom "帮我看看这个项目"        # 带初始提示词启动
```

## 模式系统

Loom 有四种引擎模式，通过 `/mode` 命令或 CLI 子命令切换：

| 模式 | CLI | 斜杠命令 | 说明 |
|------|-----|---------|------|
| Chat（陪伴） | `loom`（默认） | `/mode chat` | 纯对话，不调工具 |
| Plan（规划） | — | `/mode plan` | 只读探索，不改文件 |
| Code（编码） | `loom code` | `/mode code` | 完整 agent 循环 + 全工具 |
| Assistant | — | `/mode assistant` | 读文件 + 记忆 + 技能，不写不执行 |

## 模型偏好

控制后端选择优先级的全局开关：

```bash
# TUI 内斜杠命令
/model use local     # 强制本地模型（LM Studio / Ollama）
/model use cloud     # 优先云端模型（Anthropic / OpenAI / DeepSeek）
/model use auto      # 自动（云端优先，本地兜底，默认值）
/model               # 打开模型选择弹窗
```

### 路由逻辑

| 偏好 | 优先级 |
|------|--------|
| `local` | 本地模型 → 本地 GGUF 兜底 |
| `cloud` | 云端模型 → 本地兜底 → GGUF 兜底 |
| `auto` | 同 cloud（默认） |

## 全部 CLI 参数

```
loom [OPTIONS] [PROMPT]
loom [OPTIONS] code     # 编码模式
```

### 模型和配置

| 参数 | 说明 |
|------|------|
| `-m, --model <MODEL>` | 指定模型 |
| `--oss` | 使用开源本地 provider |
| `--local-provider <lmstudio\|ollama>` | 指定本地 provider |
| `-p, --profile <NAME>` | 加载 `$LOOM_HOME/<name>.config.toml` 配置层 |
| `-c, --config <key=value>` | 覆盖配置项（可重复）。例：`-c model="o3"` |
| `--strict-config` | 严格模式：config.toml 里有未知字段就报错 |

### 沙箱和权限

| 参数 | 说明 |
|------|------|
| `-s, --sandbox <MODE>` | 沙箱策略：`read-only` / `workspace-write` / `danger-full-access` |
| `--dangerously-bypass-approvals-and-sandbox` | 跳过所有确认和沙箱（极度危险） |
| `--dangerously-bypass-hook-trust` | 跳过 hook 信任检查（仅自动化场景） |

### 审批

| 参数 | 说明 |
|------|------|
| `-a, --ask-for-approval <POLICY>` | 审批策略：`untrusted` / `on-failure` / `on-request` / `never` |

### 工作目录

| 参数 | 说明 |
|------|------|
| `-C, --cd <DIR>` | 指定工作根目录 |
| `--add-dir <DIR>` | 额外的可写目录（可重复） |

### 输入

| 参数 | 说明 |
|------|------|
| `[PROMPT]` | 初始提示词（位置参数） |
| `-i, --image <FILE>` | 附加图片（可重复，逗号分隔） |
| `--search` | 启用实时网络搜索 |

### 连接

| 参数 | 说明 |
|------|------|
| `--remote <ADDR>` | 连接到远程 app server |
| `--remote-auth-token-env <ENV>` | 远程连接 bearer token 环境变量名 |

### 显示

| 参数 | 说明 |
|------|------|
| `--no-alt-screen` | 不切换备用屏幕（行内模式，保留终端滚动历史） |
| `--enable <FEATURE>` | 启用功能开关（可重复） |
| `--disable <FEATURE>` | 禁用功能开关（可重复） |

### 示例

```bash
# 编码模式 + 指定模型 + 工作区写权限
loom code -m claude-sonnet-4-20250514 -s workspace-write

# 自动化场景：跳过所有确认（CI/CD）
loom exec "修复所有 clippy 警告" --dangerously-bypass-approvals-and-sandbox

# 用本地模型启动陪伴模式
loom --local-provider lmstudio

# 用云端模型覆盖 config
loom code -c model="deepseek-v4-pro[1m]"

# 传递初始提示词 + 图片
loom -i screenshot.png "这个报错怎么修"

# 多张图片
loom -i a.png,b.png "对比两张截图"

# 带配置文件层
loom -p work "今天的任务是什么"
```

## 子命令

```bash
loom exec "解释这段代码"       # 非交互执行
loom review                    # 代码审查
loom resume                    # 继续之前的会话
loom resume --last             # 继续最近会话
loom fork                      # 分叉当前会话
loom apply                     # 应用最近 diff
loom doctor                    # 诊断安装状态
loom mcp                       # MCP 服务器管理
loom plugin                    # 插件管理
loom sandbox                   # 沙箱执行
loom completion bash           # 生成 shell 补全脚本
```

## TUI 斜杠命令

### 模式和偏好

| 命令 | 说明 |
|------|------|
| `/mode chat` | 切换陪伴模式 |
| `/mode plan` | 切换规划模式 |
| `/mode code` | 切换编码模式 |
| `/mode assistant` | 切换助理模式 |
| `/mode` | 循环切换四种模式 |
| `/model use local` | 强制本地模型 |
| `/model use cloud` | 强制云端模型 |
| `/model use auto` | 自动选择（默认） |
| `/model` | 打开模型选择弹窗 |

### 会话

| 命令 | 说明 |
|------|------|
| `/new` | 新建会话 |
| `/resume` | 恢复历史会话 |
| `/fork` | 分叉当前会话 |
| `/rename <name>` | 重命名当前线程 |
| `/clear` | 清空终端，开新会话 |
| `/compact` | 压缩上下文防超限 |
| `/init` | 创建 loom.md 项目指令文件 |

### 显示

| 命令 | 说明 |
|------|------|
| `/status` | 显示当前会话配置和 token 用量 |
| `/statusline` | 配置底部状态栏显示项 |
| `/title` | 配置终端标题栏显示项 |
| `/theme` | 选择语法高亮主题 |
| `/diff` | 显示 git diff |
| `/raw [on\|off]` | 切换纯文本滚动模式 |
| `/copy` | 复制最后回复为 markdown |

### 权限和安全

| 命令 | 说明 |
|------|------|
| `/permissions` | 配置许可策略 |
| `/approve` | 批准被拒绝的操作重试一次 |

### 其他

| 命令 | 说明 |
|------|------|
| `/review` | 审查当前改动 |
| `/skills` | 管理技能 |
| `/hooks` | 管理生命周期钩子 |
| `/memories` | 配置记忆生成 |
| `/personality` | 选择沟通风格 |
| `/feedback` | 发送反馈 |
| `/quit` `/exit` | 退出 Loom |

## 状态栏配置

通过 `/statusline` 命令打开配置面板，可自由选择和排序以下项：

- `model-with-reasoning` — 模型名 + 推理级别
- `model` — 模型名
- `current-dir` — 当前工作目录
- `project-name` — 项目根目录名
- `git-branch` — Git 分支名
- `run-state` — 运行状态（Ready / Working / Thinking）
- `permissions` — 权限配置
- `approval-mode` — 命令审批模式
- `context-used` — 上下文窗口占用百分比
- `context-remaining` — 上下文窗口剩余百分比
- `used-tokens` — 已用 token 总数
- `loom-version` — Loom 版本号
- `fast-mode` — Fast 模式状态
- `thread-id` — 线程 UUID

配置持久化到 `%APPDATA%/openLoom/config.toml`：

```toml
[tui]
status_line = ["model-with-reasoning", "current-dir", "run-state"]
status_line_use_colors = true
```

## 快捷键

### 输入

| 按键 | 功能 |
|------|------|
| `Enter` | 发送消息 |
| `Shift+Enter` | 换行 |
| `Ctrl+G` | 打开外部编辑器 |
| `Ctrl+R` | 增量历史搜索 |
| `Tab` | 补全 `/` 命令 |

### 控制

| 按键 | 功能 |
|------|------|
| `Esc` | 取消 / 关闭弹窗 |
| `Ctrl+C`（一次） | 中断当前生成 |
| `Ctrl+C`（两次，2s 内） | 强制退出 |

### 导航

| 按键 | 功能 |
|------|------|
| `↑` `↓` / `j` `k` | 上下滚动 |
| `PgUp` `PgDn` | 翻页 |
| `Ctrl+D` `Ctrl+U` | 半页滚动 |
| `gg` / `G` | 跳开头/结尾 |

### 审批弹窗

| 按键 | 功能 |
|------|------|
| `A` | 批准本次 |
| `D` | 拒绝 |
| `S` | 本次会话全部批准 |
| `Enter` | 确认 |

## 配置文件

路径：`%APPDATA%/openLoom/config.toml`（Windows）/ `~/.loom/config.toml`（Linux/macOS）

```toml
[[models]]
name = "local"
backend = "LmStudio"
model = "qwen3-8b"
context_size = 32000
base_url = "http://localhost:1234/v1"

[[models]]
name = "cloud"
backend = "OpenAI"
model = "deepseek-v4-pro[1m]"
api_key_env = "OPENLOOM_API_KEY"
base_url = "https://your-api-endpoint/v1"

[router]
keyword_threshold = 0.85
fallback_threshold = 0.7

[agent]
max_iterations = 15
timeout_secs = 120

[persona]
top_n = 5
recency_decay_days = 30

[tui]
status_line = ["model-with-reasoning", "current-dir", "run-state", "used-tokens"]
status_line_use_colors = true
```

## 数据目录

| 平台 | 路径 |
|------|------|
| Windows | `%APPDATA%/openLoom/` |
| macOS | `~/Library/Application Support/openLoom/` |
| Linux | `~/.local/share/openLoom/` |

## 项目指令文件

| 文件 | 作用 |
|------|------|
| `<cwd>/loom.md` | 项目级指令（Loom 优先读取） |
| `<cwd>/CLAUDE.md` | 兼容格式 |
| `<cwd>/AGENTS.md` | 兼容格式 |
| `<data_dir>/loom.md` | 全局指令 |

## 外部技能

```
<data_dir>/skills/*/SKILL.md     # 全局技能
<cwd>/.loom/skills/*/SKILL.md    # 项目技能
```

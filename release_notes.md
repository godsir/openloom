# openLoom v0.3.21

本地优先的私有 AI 助手 — 运行在你本机，拥有真实系统访问能力。

## 🧠 Loom.md — Agent 纪律文件

类似 Claude Code 的 CLAUDE.md，openLoom 支持 **Loom.md** 优先级加载，作为对所有 Agent 生效的共享纪律基座：

- `$WORKSPACE/Loom.md` → 工作区级，完全替代默认系统提示
- `~/.loom/Loom.md` → 全局级，首次启动自动创建
- 硬编码默认 → 兜底

Loom.md 对**所有 Agent**（含自定义 Agent）的每次对话生效；Agent 的 `system_prompt_override` 作为特定补充叠加在其后，而非取代整个纪律文件。文件名大小写不敏感（`Loom.md` / `loom.md` 均可）。空文件视为用户主动禁用，不会被默认内容覆盖。设置 → Loom.md 标签页可直接编辑 `~/.loom/Loom.md`，保存后下一轮对话即刻生效。

> 🔧 修复：此前自定义 Agent 的 `system_prompt_override` 非空时会完全跳过 Loom.md，导致 Loom.md 对任何自定义 Agent 都不生效；同时修复了空文件被默认内容覆盖、以及区分大小写系统上小写 `loom.md` 读不到的问题。

## ⚡ 后端稳定性

- 推理引擎流式响应可靠性修复 (streaming + KV-cache)
- MCP/LSP 协议并发修复 (id-correlation, sub-agent cancellation)
- 定时任务调度器改用 AI 提示词（替代 Shell 命令）
- FTS5 全文搜索特殊字符转义
- 沙箱 deny-floor + 路径/工具权限策略加固
- 消息序列清理逻辑支持工具调用链

## 🎨 前端改进

- 渲染器 P2 正确性：流结束、重连、按键、Mermaid
- XSS 加固 + IPC listener 泄漏修复
- Slash 命令菜单 + Cron 检测对话框
- UI 重构：更多下拉框替代按钮、缩放持久化

## ⌨️ 快捷键系统

- 全局快捷键注册表 + useKeybinding hook
- Settings → Shortcuts 标签页支持自定义绑定
- KeyCaptureModal 可视化捕获按键
- 移除 AppShell 硬编码 Ctrl+B

## 🌐 国际化

- 支持 zh-CN / zh-TW / en-US 三语言切换

## 📦 下载

| 平台 | 文件 |
|------|------|
| Windows | `openLoom.Setup.0.3.21.exe` |

**共 69 个提交** 自 v0.2.20 以来。

> [!NOTE]
> 当前仅提供 Windows 版本。macOS/Linux 可自行从源码编译。

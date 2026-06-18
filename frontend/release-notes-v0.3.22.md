# openLoom v0.3.22

本地优先的私有 AI 助手 — 运行在你本机，拥有真实系统访问能力。

## 🎨 亮色主题重配色

- **紫蓝全新配色**：亮色主题从旧配色全面升级为紫蓝色系
- 主题预览改为动态读取 CSS 变量，修复亮色主题预览色不匹配问题
- 右键菜单完整主题跟随，不再出现亮/暗混搭

## ✍️ 写作模式 (Write Mode) — 五阶段全面优化

全新 Write 模块从零重构，历时 50+ 个提交：

### 阶段 1：模块化架构骨架
- Zustand Store + 12 核心组件 + 双引擎依赖配置

### 阶段 2：双引擎编辑器
- **TipTap 富文本编辑器** + **CodeMirror 6 Live 预览** + 投影装饰层
- 支持 Markdown 实时渲染和源码编辑双模式

### 阶段 3：AI 写作管道
- **WriteInlineAgent**：CM6 选中文本时自动弹出浮动工具栏
- 块类型切换 + 格式化 + 快捷操作 + AI 内联编辑
- Ghost 补全 + 写作人格切换
- WriteChatPanel 复用解析管线，修复初次打开不显示历史

### 阶段 4+5：文件系统 + 导出 + 高级特性
- **文件树右键菜单**：新建文件/文件夹、重命名、删除
- **ExportMenu**：真导出 HTML / PDF / DOCX
- **文件监听**：2s 轮询检测外部变更并自动同步内容
- **Live 预览模式**：CM6 装饰层隐藏 Markdown 语法标记
- PDF Worker + HTML sanitize 安全预览
- 双栏/分屏/设置面板完整集成

## 📋 Todo 功能 + Continue

- **TodoPanel**：任务管理面板，支持创建/编辑/完成待办事项
- **ContinueButton**：消息因预算或迭代限制中断时，一键继续生成
- AssistantMessage 截断提示 + 状态区分
- 后端 SQLite 表结构 + 增删改查完整支持
- zh-CN / zh-TW / en-US 三语言国际化

## 🧠 上下文总结引擎

- **SummaryEngine**：六维度总结 prompt + 1024 token 上限
- **80% token 阈值**自动触发增量总结
- 分段增量 `build_prompt_segmented` + 游标推进
- `truncate_history` 分层组装：已总结消息丢弃，近期消息保留
- mid-turn 安全截断兜底（90% 阈值 / file_read 豁免）
- 历史预算动态化：`context_window × 25%` 取代硬编码
- 废除 `compact_for_llm` 跨轮抹除，保留工具上下文

## 🔧 Loom.md 修复

- 自定义 Agent 正确读取 Loom.md
- 新增编辑入口（一键打开 Loom.md）
- 修复大小写敏感和文件覆盖问题

## 🖥️ 后端

- VFS API 格式兼容：`list_directory` 返回 kind/extension/path，`read_file` 返回 size/truncated
- 新增 `write_rag` 调度处理器（BM25 检索）
- VFS 文件监听桩

## 📦 下载

| 平台 | 文件 |
|------|------|
| Windows | `openLoom.Setup.0.3.22.exe` |

**共 53 个提交** 自 v0.3.21 以来。

> [!NOTE]
> 当前仅提供 Windows 版本。macOS/Linux 可自行从源码编译。

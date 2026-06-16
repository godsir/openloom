# Write 模式全面优化 — 设计规格文档

> 日期：2026-06-16
> 状态：设计阶段
> 参考项目：F:\DeepSeek-GUI (Kun)

## 1. 概述

### 1.1 背景

OpenLoom 的 Write 模式目前处于 MVP 阶段：一个 565 行的单体组件 `WriteWorkspaceView.tsx`，所有状态散落在 `useState`/`useRef` 中，功能集有限（3 种预览模式、平铺文件列表、基础 AI 聊天、FIM 补全）。

DeepSeek-GUI（Kun）的 Write 模式有 40+ 个专用文件、800 行 Zustand Store、5 种视图模式、选区内联编辑、Ghost 补全、RAG 检索、PDF 查看、多格式导出等成熟功能。

本设计旨在将 OpenLoom 的 Write 模式从 MVP 升级到接近 DeepSeek-GUI 的完整写作工作室。

### 1.2 目标

- **架构模块化**：从单体组件拆分为 18+ 独立组件 + 专用 Zustand Store
- **双引擎编辑器**：CodeMirror 6 + TipTap（所见即所得），支持 5 种视图模式
- **AI 写作能力**：选区内联编辑 + Diff 审阅、Ghost 补全、写作人格、RAG 检索
- **文件系统升级**：递归文件树、PDF/图片查看器
- **多格式导出**：HTML / PDF / DOCX / 富文本剪贴板
- **完整设置面板**：排版、AI 补全、写作人格、快捷操作

### 1.3 非目标

- 实时协作编辑
- 版本历史/Git 集成
- 信息图/原型生成（DeepSeek-GUI 特有功能，依赖独立的图片生成服务）
- 移动端支持

---

## 2. 实施策略

### 2.1 策略：架构先重构

**第 1 步（阶段 1）**：建立模块化架构骨架——拆分组件、创建 Zustand Store，功能保持现状不变。

**第 2-N 步（阶段 2-5）**：在模块化骨架上逐步添加新功能。每个新功能都有清晰的组件边界和状态归属。

### 2.2 五阶段计划

| 阶段 | 内容 | 预估工时 | 新建文件 |
|------|------|----------|----------|
| 1 | 架构骨架（Store + 组件拆分） | 8-12h | 12 |
| 2 | 双引擎编辑器（TipTap + Live 模式） | 9-12h | 6 |
| 3 | AI 写作能力（内联编辑 + Ghost + RAG） | 17-25h | 10 |
| 4 | 文件系统 + 导出 | 7-11h | 6 |
| 5 | 设置面板 + 高级特性 | 5-7h | 5 |
| **总计** | | **46-67h** | **39** |

---

## 3. 架构设计

### 3.1 Zustand Store：四切片结构

新建 `frontend/src/renderer/src/stores/write.ts`，替代当前散落在 `WriteWorkspaceView.tsx` 中的所有 `useState`/`useRef`。

**共享类型定义**：

```typescript
interface WorkspaceEntry {
  name: string;
  path: string;       // 相对于 workspaceRoot
  kind: 'file' | 'directory';
  extension?: string; // 文件扩展名（不含点）
  children?: WorkspaceEntry[];
}

interface WriteEditorSelectionState {
  text: string;
  from: number;
  to: number;
  lineFrom: number;
  lineTo: number;
  blockType: string | null;  // 'paragraph' | 'heading' | 'list' | 'code' 等
  containsImage: boolean;
}

interface QuotedSelection {
  id: string;
  text: string;
  filePath: string;
  lineFrom: number;
  lineTo: number;
  timestamp: number;
}

interface RecentEdit {
  instruction: string;
  originalText: string;
  editedText: string;
  filePath: string;
  timestamp: number;
}

interface DiffChunk {
  id: string;
  originalText: string;
  modifiedText: string;
  fromA: number; toA: number;  // 原文位置
  fromB: number; toB: number;  // 修改后位置
  accepted: boolean | null;     // null = 待审阅
}
```

#### writeSettingsSlice — 工作区 & 编辑器配置

```typescript
interface WriteSettingsSlice {
  // 工作区
  workspaceRoot: string | null;
  defaultWorkspaceRoot: string | null;
  // 编辑器
  previewMode: 'rich' | 'source' | 'live' | 'split' | 'preview';
  fontSize: number;          // 12-28
  lineHeight: number;        // 1.2-2.5
  fontFamily: string;
  fileSidebarOpen: boolean;
  // 内联补全
  inlineCompletionEnabled: boolean;
  inlineCompletionModel: string | null;  // null = 继承聊天模型
  shortDebounceMs: number;   // 150-5000
  longDebounceMs: number;    // 1000-15000
  minAcceptScore: number;
  shortMaxTokens: number;
  longMaxTokens: number;
  // 工作区
  retrievalEnabled: boolean;
  imageStoragePath: string;  // 默认 ".assets/"
  // 自动保存
  autoSaveIntervalMs: number; // 默认 900
}
```

#### writeFilesSlice — 文件 CRUD & 目录树

```typescript
interface WriteFilesSlice {
  entriesByDir: Record<string, WorkspaceEntry[]>;
  expandedDirs: Set<string>;
  activeFilePath: string | null;
  activeFileKind: 'text' | 'image' | 'pdf';
  fileContent: string;
  saveStatus: 'saved' | 'dirty' | 'saving' | 'error';
  fileLoading: boolean;
  fileError: string | null;
  fileSize: number;
  fileTruncated: boolean;
}
```

#### writeUiSlice — 临时 UI 状态

```typescript
interface WriteUiSlice {
  assistantOpen: boolean;
  inlineAgentVisible: boolean;
  inlineAgentPosition: { x: number; y: number; placement: 'above' | 'below' };
  modalState: 'none' | 'newFile' | 'rename' | 'delete' | 'export';
  toastMessage: { type: 'success' | 'error' | 'info'; text: string } | null;
}
```

#### writeAiSlice — AI 写作特性状态

```typescript
interface WriteAiSlice {
  selection: WriteEditorSelectionState | null;
  quotedSelections: QuotedSelection[];
  recentEdits: RecentEdit[];
  pendingAgentReview: DiffChunk[] | null;
  reviewActive: boolean;
  agentPresetId: string | null;
  fileThreads: Record<string, string>; // filePath → threadId
}
```

### 3.2 与 DeepSeek-GUI 的关键差异适配

| 维度 | DeepSeek-GUI | OpenLoom 适配 |
|------|-------------|--------------|
| LLM 通信 | IPC → Main Process HTTP | 复用现有 JSON-RPC `completion.fim` / `chat.send` 管道 |
| Session 管理 | Thread per workspace | 保持现有 per-file session（已验证可行） |
| RAG 检索 | BM25 内存索引（主进程） | 后端 JSON-RPC 实现，复用 Embedding 基础设施 |
| 设置持久化 | browser localStorage | 复用现有 config 系统 + localStorage |
| 图片生成 | 独立 AI 图像服务 | **不实现**（非目标） |

### 3.3 组件树

```
WriteWorkspaceView.tsx (~80行，编排层)
├── WriteSidebar.tsx
│   └── WriteFileTree.tsx (递归文件树)
├── WriteToolbar.tsx
│   ├── WritePreviewModeSelector.tsx
│   ├── WriteFontSizeControl.tsx
│   └── WriteExportMenu.tsx
├── WriteDocumentPane.tsx
│   ├── WriteMarkdownEditor.tsx (CM6 — 现有，重构)
│   ├── WriteRichEditor.tsx (TipTap — 新建)
│   ├── WriteMarkdownPreview.tsx (独立预览 — 新建)
│   ├── WriteImagePreview.tsx (图片查看 — 新建)
│   └── WritePdfViewer.tsx (PDF 查看 — 新建)
├── WriteInlineAgent.tsx (选区浮动工具栏 — 新建)
├── WriteAssistantPanel.tsx (AI 聊天 — 现有，增强)
├── WriteWorkspaceStart.tsx (着陆页 — 新建)
└── WriteFileDialogs.tsx (模态框 — 新建)
```

### 3.4 逻辑模块目录

新建 `frontend/src/renderer/src/write/` 目录，集中管理写作相关的纯逻辑模块：

```
write/
├── write-selection.ts          # 选区状态工具函数
├── write-thread-registry.ts    # 文件→Thread 映射管理
├── write-render-safety.ts      # 大文件安全渲染
├── write-file-watch.ts         # 外部文件变更监听
├── markdown-live-preview.ts    # CM6 Live 装饰模式
├── markdown-live-widgets.ts    # CM6 自定义 Widget
├── inline-edit.ts              # AI 内联编辑管道
├── inline-completion/          # Ghost 文本补全
│   └── ghost-text-plugin.ts    # CM6 ViewPlugin 实现
├── inline-format.ts            # 内联格式化（粗/斜/删除线/代码）
├── block-type.ts               # 块类型检测/转换
├── quick-actions.ts            # 快速操作（润色/翻译/扩写等）
├── quoted-selection.ts         # 选区引用管理
├── recent-edits.ts             # 最近编辑追踪
├── agent-presets.ts            # 写作人格解析
├── term-propagation.ts         # 术语变更传播
├── template-shortcuts.ts       # @date 等模板展开
└── tiptap/
    ├── WriteRichEditor.tsx      # TipTap 编辑器组件
    ├── markdown-projection.ts   # TipTap JSON ↔ Markdown
    ├── markdown-sync.ts         # 模式切换同步
    └── paste-image.ts           # 图片粘贴处理
```

---

## 4. 编辑器设计

### 4.1 五种视图模式

| 模式 | 引擎 | 说明 |
|------|------|------|
| **Rich** | TipTap | 所见即所得，默认模式。支持图片粘贴/拖拽 |
| **Source** | CodeMirror 6 | 原始 Markdown 编辑，语法高亮 |
| **Live** | CodeMirror 6 + Decorations | 隐藏 Markdown 标记语法，内联渲染标题/图片/表格 |
| **Split** | CM6（左）+ react-markdown（右） | 分屏编辑+预览，同步滚动 |
| **Preview** | react-markdown | 纯预览，只读 |

- 模式选择器位于工具栏，持久化到 localStorage
- 大文件（>300K 字符）自动禁用 Rich/Live，强制 Source 模式
- 超大文件（>1MB）只读打开

### 4.2 TipTap 集成

**依赖**：`@tiptap/react`、`@tiptap/starter-kit`、`@tiptap/extension-placeholder`、`@tiptap/extension-image`、`@tiptap/extension-dropcursor`

**初始化**：
- TipTap 从当前文件 Markdown 内容生成初始文档 JSON
- 通过 `markdown-projection.ts` 维护 Markdown 投影

### 4.3 Markdown 投影层（双引擎同步核心）

**设计原则**：Markdown 是"真相源"（Source of Truth）。文件在磁盘上始终存储为 Markdown。所有 AI 操作只理解 Markdown。

**markdown-projection.ts**：
- TipTap JSON → Markdown：遍历 ProseMirror 文档节点树输出 Markdown 语法
- Markdown → TipTap JSON：解析 Markdown 生成 TipTap 兼容 JSON
- 增量更新：局部修改时只转换变更节点范围

**markdown-sync.ts**：
- Rich → Source：提取 Markdown 投影写入 CM6
- Source → Rich：读取 CM6 内容重新生成 TipTap 文档
- 尽量保留光标位置（行/列映射）

**paste-image.ts**：
- 拦截粘贴事件中的图片数据
- 保存到工作区 `.assets/` 子目录
- 插入 `![alt](.assets/image-xxx.png)` 到文档

### 4.4 Live 装饰模式

**markdown-live-preview.ts**：
- CM6 Decoration API 隐藏 Markdown 语法标记（`#`、`*`、`-` 等）
- 标题行：隐藏 `#` 前缀，应用对应字号/粗细
- 粗体/斜体：隐藏 `**`/`*` 包裹符，应用对应样式
- 代码块：添加背景色，隐藏 fence

**markdown-live-widgets.ts**：
- 图片：inline `![alt](url)` 替换为实际渲染的 `<img>` Widget
- 表格：添加边框和对齐
- 非活跃行（光标不在的行）应用更强的装饰效果

---

## 5. AI 写作管道

### 5.1 选区浮动工具栏（WriteInlineAgent）

**触发**：用户在编辑器中选择文字时，在选区上方/下方弹出浮动工具栏。

**工具栏内容**：
- 块类型选择器：段落 / H1 / H2 / H3 / 引用 / 无序列表 / 有序列表 / 代码块
- 内联格式化：粗体 / 斜体 / 删除线 / 行内代码（Toggle 按钮）
- 写作人格切换：快速选择预设人格
- 快速操作：润色 / 翻译 / 扩写 / 总结 / 正式化（可配置）
- "引用到助手"按钮：将选区加入 quotedSelections
- AI 编辑输入框：自由文本输入，Enter 提交内联编辑，Ctrl+Enter 发送到侧边栏

**定位**：`useLayoutEffect` 测量工具栏尺寸，自动选择 above/below 位置，避免视口溢出。

**交互保护**：`mousedown` 时 `preventDefault` 防止编辑器选区折叠。

### 5.2 内联 AI 编辑 + Diff 审阅

**管道流程**：

1. **选区捕获**：记录选中文本、文件路径、行号
2. **编辑范围计算**：确定编辑粒度（段落 / 精确选区 / 行范围）
3. **构造 Prompt**：
   ```
   <<<PREFIX
   {选区之前的全文}
   <<<EDIT_SCOPE
   {被选中的文本}
   <<<SUFFIX
   {选区之后的全文}
   ---
   用户指令：{自然语言指令}
   只修改 EDIT_SCOPE 范围内的内容。
   ```
4. **API 调用**：JSON-RPC `completion.fim`（复用现有后端管道）
5. **响应解析**：提取模型返回的编辑后文本。模型通过 FIM 格式返回，prefix = 原文前半部分，suffix = 原文后半部分，模型填充 middle 部分作为编辑结果。若模型支持结构化输出，则通过 `<<<EDIT` / `<<</EDIT` 标记精确提取
6. **Diff 显示**：使用 `@codemirror/merge` 的 `unifiedMergeView`，将原文与编辑结果并排展示：红色高亮为删除内容，绿色高亮为新增内容
7. **逐块审阅**：用户 Accept/Reject 每个差异块
8. **错误处理**：文档在 API 调用期间被修改时，重新定位编辑范围；文本消失或不再唯一时提示"文档已变更"

### 5.3 Ghost 文本补全

**实现**：CM6 `ViewPlugin`，在光标位置渲染灰色建议文本（Ghost Text）。

- 光标停止 300ms（可配置）后自动请求
- 发送 prefix/suffix 到 `completion.fim`
- Tab 键接受，继续输入则忽略
- 可选启用 RAG 检索增强
- 与现有 FIM 共享后端管道，仅前端展示不同

### 5.4 侧边栏 AI 聊天（增强）

相比当前版本的改进：

| 组件 | 当前 | 目标 |
|------|------|------|
| 文件内容 | 全文拼接 | 结构化注入：[当前文件] + [选区引用] |
| 选区引用 | 不支持 | `[引用原文]...[/引用原文]` 块 |
| RAG 检索 | 不支持 | `[相关文献]...[/相关文献]` 块 |
| 写作人格 | 无 | 系统提示词注入 |
| 快速操作 | 5 个固定按钮 | 可配置，最多 12 个 |

### 5.5 RAG 工作区检索

**方案**：后端 JSON-RPC 实现。

- **索引**：扫描工作区 .md/.txt 文件，分块（~900 chars/块），向量嵌入
- **存储**：内存索引，TTL 30s，最多 160 个文件，每文件最多 600KB
- **检索**：当前编辑行 / 用户查询 → Top-K 相似块
- **注入**：侧边栏聊天和内联编辑 Prompt 中注入 `[相关文献上下文]...[/相关文献上下文]`
- **增量更新**：文件保存后自动重新索引已变更文件

### 5.6 写作人格

**内置模板**（中英文本地化）：
- 情节统筹（Plot Coordinator）
- 文字编辑（Line Editor）
- 伏笔追踪（Foreshadowing Tracker）
- 连贯性检查（Continuity Checker）

**自定义人格**：用户可在设置中创建（emoji + 名称 + 人格 Prompt），最多 50 个。

**使用方式**：在侧边栏助手或选区工具栏中选择，人格 Prompt 折叠进系统提示词。

---

## 6. 文件系统

### 6.1 递归文件树

**WriteFileTree.tsx**：
- 递归展示目录结构（展开/折叠）
- 支持文件类型：`.md`、`.txt`、`.pdf`、`.png`、`.jpg`、`.jpeg`、`.gif`、`.webp`、`.svg`
- 右键菜单：新建文件 / 新建目录 / 重命名 / 删除
- 目录懒加载（展开时才加载子项）
- 隐藏 dotfile/dotdir
- 当前活动文件高亮

### 6.2 PDF 查看器

**WritePdfViewer.tsx**：
- 依赖：`pdfjs-dist`
- 页码导航、缩放（50%-200%）
- 文本搜索
- 文本选择 → 触发选区工具栏 → 引用到助手
- 按页懒加载 Canvas 渲染
- 文本提取用于 RAG 索引

### 6.3 图片预览器

**WriteImagePreview.tsx**：
- 图片全尺寸预览
- 缩放适配模式：fit / 100% / 200%
- 深色背景遮罩

### 6.4 渲染安全

- >300K 字符：自动关闭 Rich/Live 预览，强制 Source
- >1MB：只读打开，显示截断警告
- PDF 按页懒加载

### 6.5 外部文件监听

**write-file-watch.ts**：
- 监听活动文件的外部修改（通过 `chokidar` 在主进程实现）
- 外部变更时自动同步内容到编辑器
- 图片文件变化时重新加载 base64

---

## 7. 导出系统

### 7.1 统一入口

工具栏 "导出" 下拉菜单，四通道：

| 通道 | 格式 | 实现 |
|------|------|------|
| 复制富文本 | HTML Clipboard | 渲染 Markdown → HTML → `clipboard.write()` (text/html MIME) |
| 导出 HTML | .html（自包含） | 主进程 `write:export-html`，嵌入打印友好 CSS + 图片 base64 |
| 导出 PDF | .pdf | 渲染 HTML → Electron `BrowserWindow.webContents.printToPDFAsync()` |
| 导出 DOCX | .docx | `html-to-docx` 库，Markdown → HTML → DOCX |

### 7.2 IPC 设计

在 `frontend/src/main/ipc/write.ts` 中新增/增强：
- `write:export-html` — 增强：添加 CSS 样式
- `write:export-pdf` — 新建：PDF 导出
- `write:export-docx` — 新建：DOCX 导出
- `write:copy-rich-text` — 增强：富文本剪贴板

---

## 8. 设置面板

复用现有 Settings 路由（`appMode === 'settings'`），新增 "写作" 标签页。

### 8.1 排版设置
- 字体预设（系统 / 思源黑体 / 微软雅黑 / 苹方 / 黑体 / 宋体 / 楷体 / 自定义）
- 字号滑块（12-28px）
- 行高滑块（1.2-2.5）
- 默认预览模式

### 8.2 AI 补全设置
- 内联补全开关
- 模型选择（继承聊天模型 / 自定义）
- 短补全 debounce（150-5000ms）
- 长补全 debounce（1000-15000ms）
- 接受分数阈值
- Token 限制（短/长）

### 8.3 写作人格管理
- 内置模板列表
- 自定义人格 CRUD（最多 50 个）
- 默认助手人格选择

### 8.4 快速操作
- 预制：润色 / 翻译 / 扩写 / 总结 / 正式化
- 自定义操作（最多 12 个）
- 拖拽排序
- 自定义 Prompt

### 8.5 工作区设置
- 默认工作区目录
- 已注册工作区列表
- RAG 检索开关
- 图片存储路径

### 8.6 高级
- 自动保存间隔
- 大文件渲染阈值
- 文件编码
- 补全调试日志查看器

---

## 9. 后端变更

### 9.1 VFS 增强

现有 `backend/crates/loom-server/src/dispatch/vfs.rs` 已有 6 个 JSON-RPC 方法，阶段 4 需增加：
- `vfs.watch_file` / `vfs.unwatch_file` — 文件变更通知（通过现有的通知通道推送）

### 9.2 RAG 检索端点（新建）

新增 JSON-RPC 方法（或在现有 embedding 端点基础上扩展）：
- `write.index_workspace` — 索引工作区文件
- `write.search_workspace` — 搜索相关片段
- `write.reindex_file` — 单文件重新索引

利用现有 `loom-embeddings` crate 进行向量嵌入。

### 9.3 无其他后端变更

内联编辑、Ghost 补全、侧边栏聊天均复用现有 `completion.fim` 和 `chat.send` JSON-RPC 管道，不需要新的 AI 后端。

---

## 10. 依赖变更

### 10.1 新增前端依赖

```json
{
  "@tiptap/react": "^2.x",
  "@tiptap/starter-kit": "^2.x",
  "@tiptap/extension-placeholder": "^2.x",
  "@tiptap/extension-image": "^2.x",
  "@tiptap/extension-dropcursor": "^2.x",
  "@codemirror/merge": "^6.x",
  "pdfjs-dist": "^4.x",
  "html-to-docx": "^1.x"
}
```

### 10.2 新增后端依赖

无新增后端依赖。RAG 检索复用现有 `loom-embeddings`。

---

## 11. 风险与缓解

| 风险 | 严重度 | 缓解措施 |
|------|--------|----------|
| TipTap Markdown 投影层复杂度高 | 高 | 参考 DeepSeek-GUI 已验证实现，阶段 2 充分测试 |
| 双引擎同步不一致 | 中 | Markdown 为唯一真相源，TipTap 投影只读不写磁盘 |
| 大文件性能问题 | 中 | 渲染安全阈值 + 按页懒加载 + 增量索引 |
| 重构破坏现有功能 | 中 | 阶段 1 功能不变，仅拆分；每个阶段独立测试 |
| pdfjs-dist 体积大 | 低 | 动态 import，仅在打开 PDF 时加载 |
| 依赖冲突（@codemirror/merge） | 低 | 项目已使用 CM6，版本兼容性高 |

---

## 12. 国际化

所有新增文案需覆盖 `zh-CN`、`zh-TW`、`en-US` 三种语言。预估新增约 80-100 个翻译 key。

---

## 13. 测试策略

- **阶段 1**：回归测试——所有现有 Write 功能正常工作
- **阶段 2**：5 种模式切换测试、Markdown 投影双向转换测试、图片粘贴测试
- **阶段 3**：内联编辑 Prompt 准确性测试、Diff 审阅流程测试、Ghost 补全接受率测试
- **阶段 4**：文件树展开/折叠测试、PDF 渲染测试、四通道导出测试
- **阶段 5**：设置持久化测试、术语传播正确性测试

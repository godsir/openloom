# Frontend UI Restoration — Cyan Edge 1:1 复原

**Date:** 2026-05-28  
**Scope:** `frontend/src/renderer/src/` — 5 个组件文件，零跨文件副作用

---

## 目标

将 `frontend/` 包的运行时 UI 完整对齐 "Cyan Edge 主界面" 设计稿（Image #1），修复 3 天来困扰用户的视觉崩坏问题。

---

## 变更清单

### 1. `AppShell.tsx` — 运行时崩溃修复 (CRITICAL)

**问题：** 标题栏 JSX 使用了 `wsState` 变量，但组件内从未从 store 订阅该值，导致 `ReferenceError` 崩溃，连接状态指示器无法渲染。

**修复：** 在现有 `useStore` 订阅块末尾追加：
```tsx
const wsState = useStore(s => s.wsState)
```

---

### 2. `WelcomeScreen.tsx` — Logo 尺寸 + 按钮图标

**问题：** Logo 盒子 48px（`w-12 h-12`），目标设计为 ~72px；按钮缺少 `+` 前缀图标。

**修复：**
- Logo 盒子：`w-12 h-12` → `w-[72px] h-[72px]`，字号 `text-xl` → `text-2xl`
- 按钮：追加 `<IconPlus size={13} />` 图标（与侧边栏"新建会话"样式一致）

---

### 3. `Sidebar.tsx` — 日期分组

**问题：** 所有会话均归入"今天"分组，无日期分层。

**修复：** 用 `session.modified`（已有 ISO 8601 字段）将会话分为三组：
- **今天**（当天）
- **昨天**（前一天）  
- **更早**（显示实际月/日，如"5月26日"）

只渲染非空分组。置顶会话（`pinnedIds`）独立一组，不参与日期分组。

---

### 4. `SessionItem.tsx` — 相对时间 + 消息数

**问题：** 会话行只显示标题，无时间/消息数辅助信息。

**修复：** 标题下方加一行 `text-[10px] text-[var(--text-muted)]`：
- 左：相对时间（`modified` → "3分钟前"/"1小时前"/"2天前"）
- 右：`messageCount > 0` 时显示"N条消息"

格式对齐设计稿："3分钟前·12条消息"。`relativeTime` 为内联纯函数，无外部依赖。

---

### 5. `ModelSelector.tsx` — 视觉指示修复

**问题：** `appearance-none` 隐藏了原生下拉箭头，文字颜色 `rgba(0,227,199,0.3)` 过浅。

**修复：**
- 移除 `appearance-none`，或在 select 右侧叠加一个 `▼` 字符（SVG chevron）
- 文字颜色提升至 `rgba(0,227,199,0.55)`

---

## 受影响文件（5 个）

```
frontend/src/renderer/src/components/app/AppShell.tsx
frontend/src/renderer/src/components/shared/WelcomeScreen.tsx
frontend/src/renderer/src/components/app/Sidebar.tsx
frontend/src/renderer/src/components/app/SessionItem.tsx
frontend/src/renderer/src/components/input/ModelSelector.tsx
```

## 不在范围内

- 后端会话标题自动生成（已有逻辑，显示依赖后端返回非空 `title`）
- 其他页面/组件
- 样式系统或 CSS 变量修改

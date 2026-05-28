# Frontend UI Cyan Edge 1:1 复原 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 修复 `frontend/` 包中 5 个组件文件，使运行时 UI 完整对齐 Cyan Edge 设计稿（Image #1）。

**Architecture:** 所有改动限定在 `frontend/src/renderer/src/components/` 下的 5 个组件文件，无跨文件副作用，无新依赖引入。每个 Task 独立可提交。

**Tech Stack:** React 19, TypeScript, Tailwind CSS 4, Zustand 5, Vite 6 (electron-vite)

---

## File Map

| 文件 | 操作 |
|------|------|
| `frontend/src/renderer/src/components/app/AppShell.tsx` | Modify (add wsState) |
| `frontend/src/renderer/src/components/shared/WelcomeScreen.tsx` | Modify (logo size + button icon) |
| `frontend/src/renderer/src/components/app/Sidebar.tsx` | Modify (date grouping logic) |
| `frontend/src/renderer/src/components/app/SessionItem.tsx` | Modify (relative time + message count) |
| `frontend/src/renderer/src/components/input/ModelSelector.tsx` | Modify (chevron + opacity) |

---

## Task 1: 修复 AppShell.tsx wsState 崩溃

**Files:**
- Modify: `frontend/src/renderer/src/components/app/AppShell.tsx:9-13`

- [ ] **Step 1: 找到 store 订阅块并添加 wsState**

  当前第 9–13 行：
  ```tsx
  const sidebarOpen = useStore(s => s.sidebarOpen)
  const toggleSidebar = useStore(s => s.toggleSidebar)
  const currentSessionId = useStore(s => s.currentSessionId)
  const sessions = useStore(s => s.sessions)
  ```

  替换为：
  ```tsx
  const sidebarOpen = useStore(s => s.sidebarOpen)
  const toggleSidebar = useStore(s => s.toggleSidebar)
  const currentSessionId = useStore(s => s.currentSessionId)
  const sessions = useStore(s => s.sessions)
  const wsState = useStore(s => s.wsState)
  ```

- [ ] **Step 2: 验证 TypeScript 编译无报错**

  ```powershell
  cd frontend
  npx tsc --noEmit 2>&1 | Select-String "AppShell"
  ```
  Expected: 无输出（无 AppShell 相关错误）。

- [ ] **Step 3: Commit**

  ```powershell
  git add frontend/src/renderer/src/components/app/AppShell.tsx
  git commit -m "fix: AppShell 补上 wsState store 订阅，修复连接状态崩溃"
  ```

---

## Task 2: WelcomeScreen Logo 尺寸 + 按钮图标

**Files:**
- Modify: `frontend/src/renderer/src/components/shared/WelcomeScreen.tsx`

- [ ] **Step 1: 添加 IconPlus import**

  当前第 1 行：
  ```tsx
  import { useStore } from '../../stores'
  ```

  替换为：
  ```tsx
  import { useStore } from '../../stores'
  import { IconPlus } from '../../utils/icons'
  ```

- [ ] **Step 2: 放大 Logo 盒子**

  当前第 15 行：
  ```tsx
  <div className="w-12 h-12 mx-auto mb-6 rounded-[var(--r-lg)] bg-[rgba(0,227,199,0.06)] border border-[rgba(0,227,199,0.12)] flex items-center justify-center shadow-[0_0_30px_rgba(0,227,199,0.06)]">
    <span className="text-xl font-bold text-[var(--accent)]">L</span>
  </div>
  ```

  替换为：
  ```tsx
  <div className="w-[72px] h-[72px] mx-auto mb-6 rounded-[var(--r-lg)] bg-[rgba(0,227,199,0.06)] border border-[rgba(0,227,199,0.12)] flex items-center justify-center shadow-[0_0_30px_rgba(0,227,199,0.06)]">
    <span className="text-2xl font-bold text-[var(--accent)]">L</span>
  </div>
  ```

- [ ] **Step 3: 按钮添加 + 图标**

  当前第 32–37 行：
  ```tsx
  <button
    onClick={handleStart}
    className="px-6 py-2.5 rounded-[var(--r-md)] bg-[var(--accent-light)] text-[var(--accent)] hover:bg-[rgba(var(--accent-rgb),.25)] border border-[var(--border-accent)] text-[13px] font-medium transition-colors"
  >
    开始新对话
  </button>
  ```

  替换为：
  ```tsx
  <button
    onClick={handleStart}
    className="inline-flex items-center gap-1.5 px-6 py-2.5 rounded-[var(--r-md)] bg-[var(--accent-light)] text-[var(--accent)] hover:bg-[rgba(var(--accent-rgb),.25)] border border-[var(--border-accent)] text-[13px] font-medium transition-colors"
  >
    <IconPlus size={13} />
    开始新对话
  </button>
  ```

- [ ] **Step 4: 验证编译**

  ```powershell
  npx tsc --noEmit 2>&1 | Select-String "WelcomeScreen"
  ```
  Expected: 无输出。

- [ ] **Step 5: Commit**

  ```powershell
  git add frontend/src/renderer/src/components/shared/WelcomeScreen.tsx
  git commit -m "fix: WelcomeScreen Logo 放大到 72px + 按钮加 + 图标"
  ```

---

## Task 3: Sidebar 日期分组

**Files:**
- Modify: `frontend/src/renderer/src/components/app/Sidebar.tsx`

- [ ] **Step 1: 添加 getDateGroup 辅助函数**

  在文件末尾（`export default` 之前，或文件末尾）追加：
  ```tsx
  function getDateGroup(modified: string): string {
    if (!modified) return '今天'
    const d = new Date(modified)
    const now = new Date()
    const today = new Date(now.getFullYear(), now.getMonth(), now.getDate())
    const yesterday = new Date(today)
    yesterday.setDate(yesterday.getDate() - 1)
    const day = new Date(d.getFullYear(), d.getMonth(), d.getDate())
    if (day >= today) return '今天'
    if (day >= yesterday) return '昨天'
    return `${d.getMonth() + 1}月${d.getDate()}日`
  }
  ```

- [ ] **Step 2: 替换 useMemo 分组逻辑**

  当前第 18–25 行：
  ```tsx
  const filtered = useMemo(() => {
    if (!query.trim()) return sessions
    const q = query.toLowerCase()
    return sessions.filter(s => (s.title||'').toLowerCase().includes(q) || s.path.toLowerCase().includes(q))
  }, [sessions, query])

  const pinned = filtered.filter(s => pinnedIds.has(s.path))
  const unpinned = filtered.filter(s => !pinnedIds.has(s.path))
  ```

  替换为：
  ```tsx
  const filtered = useMemo(() => {
    if (!query.trim()) return sessions
    const q = query.toLowerCase()
    return sessions.filter(s => (s.title||'').toLowerCase().includes(q) || s.path.toLowerCase().includes(q))
  }, [sessions, query])

  const pinned = useMemo(() => filtered.filter(s => pinnedIds.has(s.path)), [filtered, pinnedIds])

  const dateGroups = useMemo(() => {
    const unpinned = filtered.filter(s => !pinnedIds.has(s.path))
    const map = new Map<string, typeof unpinned>()
    const order: string[] = []
    for (const s of unpinned) {
      const label = getDateGroup(s.modified)
      if (!map.has(label)) { map.set(label, []); order.push(label) }
      map.get(label)!.push(s)
    }
    return order.map(label => ({ label, sessions: map.get(label)! }))
  }, [filtered, pinnedIds])
  ```

- [ ] **Step 3: 替换 JSX 渲染部分**

  找到当前的 session list JSX（第 66–94 行），将整个 `<div className="flex-1 overflow-y-auto px-2">` 内容替换为：
  ```tsx
  <div className="flex-1 overflow-y-auto px-2">
    {filtered.length === 0 ? (
      <div className="px-4 py-16 text-center">
        <p className="text-[12px] text-[var(--text-muted)]">
          {query ? '无匹配会话' : '暂无会话'}
        </p>
      </div>
    ) : (
      <>
        {pinned.length > 0 && (
          <div>
            <div className="px-4 pt-2 pb-1 text-[10px] font-semibold uppercase tracking-[1.2px] text-[var(--text-muted)]">
              已置顶
            </div>
            {pinned.map(s => <SessionItem key={s.path} session={s} />)}
          </div>
        )}
        {dateGroups.map(({ label, sessions: gs }) => (
          <div key={label}>
            <div className="px-4 pt-2 pb-1 text-[10px] font-semibold uppercase tracking-[1.2px] text-[var(--text-muted)]">
              {label}
            </div>
            {gs.map(s => <SessionItem key={s.path} session={s} />)}
          </div>
        ))}
      </>
    )}
  </div>
  ```

- [ ] **Step 4: 验证编译**

  ```powershell
  npx tsc --noEmit 2>&1 | Select-String "Sidebar"
  ```
  Expected: 无输出。

- [ ] **Step 5: Commit**

  ```powershell
  git add frontend/src/renderer/src/components/app/Sidebar.tsx
  git commit -m "feat: Sidebar 会话按日期分组（今天/昨天/更早）"
  ```

---

## Task 4: SessionItem 相对时间 + 消息数

**Files:**
- Modify: `frontend/src/renderer/src/components/app/SessionItem.tsx`

- [ ] **Step 1: 在文件末尾追加 relativeTime 辅助函数**

  在文件末尾（`export default` 之前或末尾）追加：
  ```tsx
  function relativeTime(iso: string): string {
    if (!iso) return ''
    const diff = Date.now() - new Date(iso).getTime()
    const mins = Math.floor(diff / 60000)
    if (mins < 1) return '刚刚'
    if (mins < 60) return `${mins}分钟前`
    const hrs = Math.floor(mins / 60)
    if (hrs < 24) return `${hrs}小时前`
    const days = Math.floor(hrs / 24)
    return `${days}天前`
  }
  ```

- [ ] **Step 2: 替换标题展示 span 为双行 div**

  当前第 48–50 行（非 renaming 状态下的 span）：
  ```tsx
  <span className="flex-1 truncate text-[12px] leading-snug">
    {session.title || `会话 ${sid.slice(0, 8)}`}
  </span>
  ```

  替换为：
  ```tsx
  <div className="flex-1 min-w-0">
    <div className="truncate text-[12px] leading-snug">
      {session.title || `会话 ${sid.slice(0, 8)}`}
    </div>
    {(session.modified || session.messageCount > 0) && (
      <div className="flex items-center gap-1 text-[10px] text-[var(--text-muted)] mt-0.5">
        {session.modified && <span>{relativeTime(session.modified)}</span>}
        {session.modified && session.messageCount > 0 && <span>·</span>}
        {session.messageCount > 0 && <span>{session.messageCount}条消息</span>}
      </div>
    )}
  </div>
  ```

- [ ] **Step 3: 微调外层行高（py-1 → py-1.5）**

  当前第 36 行 `className` 里：
  ```
  mx-2 px-2 py-1 cursor-pointer
  ```
  改为：
  ```
  mx-2 px-2 py-1.5 cursor-pointer
  ```

- [ ] **Step 4: 验证编译**

  ```powershell
  npx tsc --noEmit 2>&1 | Select-String "SessionItem"
  ```
  Expected: 无输出。

- [ ] **Step 5: Commit**

  ```powershell
  git add frontend/src/renderer/src/components/app/SessionItem.tsx
  git commit -m "feat: SessionItem 显示相对时间和消息数"
  ```

---

## Task 5: ModelSelector 视觉修复

**Files:**
- Modify: `frontend/src/renderer/src/components/input/ModelSelector.tsx`

- [ ] **Step 1: 用包裹 div + 绝对定位 ▼ 替换裸 select**

  当前第 31–41 行（return 内容）：
  ```tsx
  return (
    <select
      value={currentModel || undefined}
      onChange={(e) => {
        setCurrentModel(e.target.value)
        loomRpc('model.switch', { model: e.target.value }).catch(() => {})
      }}
      className="bg-transparent text-[11px] text-[rgba(0,227,199,0.3)] hover:text-[rgba(0,227,199,0.5)] outline-none border-0 cursor-pointer transition-colors appearance-none"
    >
      {models.map((m) => <option key={m} value={m}>{m}</option>)}
    </select>
  )
  ```

  替换为：
  ```tsx
  return (
    <div className="relative flex items-center">
      <select
        value={currentModel || undefined}
        onChange={(e) => {
          setCurrentModel(e.target.value)
          loomRpc('model.switch', { model: e.target.value }).catch(() => {})
        }}
        className="bg-transparent text-[11px] text-[rgba(0,227,199,0.55)] hover:text-[rgba(0,227,199,0.75)] outline-none border-0 cursor-pointer transition-colors appearance-none pr-4"
      >
        {models.map((m) => <option key={m} value={m}>{m}</option>)}
      </select>
      <span className="pointer-events-none absolute right-0 text-[rgba(0,227,199,0.4)] text-[9px] leading-none">▼</span>
    </div>
  )
  ```

- [ ] **Step 2: 验证编译**

  ```powershell
  npx tsc --noEmit 2>&1 | Select-String "ModelSelector"
  ```
  Expected: 无输出。

- [ ] **Step 3: Commit**

  ```powershell
  git add frontend/src/renderer/src/components/input/ModelSelector.tsx
  git commit -m "fix: ModelSelector 添加 ▼ 指示器，提升文字可读性"
  ```

---

## 全量验证（所有 Task 完成后）

- [ ] **运行全量 TypeScript 检查**

  ```powershell
  cd frontend
  npx tsc --noEmit
  ```
  Expected: 0 errors.

- [ ] **重启 Electron 应用，目视验证对齐点**

  | 检查项 | 预期 |
  |--------|------|
  | 标题栏右侧 | 绿点 + "已连接" 文字正常显示 |
  | Welcome 中心 | Logo 更大，按钮有 + 图标 |
  | 侧边栏 | 今天/昨天/5月N日 分组标题 |
  | 会话行 | 标题下方显示"N分钟前·N条消息" |
  | 输入框右侧 | 模型名称有 ▼ 指示，颜色更清晰 |

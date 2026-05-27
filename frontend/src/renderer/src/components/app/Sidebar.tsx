import { useState, useMemo, useRef, useEffect } from 'react'
import { useStore } from '../../stores'
import SessionItem from './SessionItem'
import { IconPlus, IconSearch, IconSettings, IconPanelLeftClose } from '../../utils/icons'

export default function Sidebar() {
  const sessions = useStore((s) => s.sessions)
  const pinnedIds = useStore((s) => s.pinnedIds)
  const createSession = useStore((s) => s.createSession)
  const switchSession = useStore((s) => s.switchSession)
  const setSettingsOpen = useStore((s) => s.setSettingsOpen)
  const setSidebarOpen = useStore((s) => s.setSidebarOpen)
  const [query, setQuery] = useState('')
  const inputRef = useRef<HTMLInputElement>(null)

  useEffect(() => { inputRef.current?.focus() }, [])

  const filtered = useMemo(() => {
    if (!query.trim()) return sessions
    const q = query.toLowerCase()
    return sessions.filter(s => (s.title||'').toLowerCase().includes(q) || s.path.toLowerCase().includes(q))
  }, [sessions, query])

  const pinned = filtered.filter(s => pinnedIds.has(s.path))
  const unpinned = filtered.filter(s => !pinnedIds.has(s.path))

  const handleCreate = async () => {
    const id = await createSession()
    if (id) await switchSession(id)
  }

  return (
    <aside className="flex flex-col w-[var(--sidebar-w)] h-full bg-[rgba(0,227,199,0.01)] border-r border-[var(--border)]">
      {/* Header — matches titlebar height */}
      <div className="flex items-center justify-between h-[var(--titlebar-h)] px-3 border-b border-[var(--border)]">
        <div className="flex items-center gap-2">
          <div className="w-5 h-5 rounded-[4px] bg-[var(--accent)] flex items-center justify-center">
            <span className="text-[9px] font-extrabold text-[var(--bg)]">L</span>
          </div>
          <span className="text-[13px] font-semibold text-[var(--text)] tracking-tight">openLoom</span>
        </div>
        <button
          onClick={() => setSidebarOpen(false)}
          className="flex items-center justify-center w-6 h-6 rounded-[var(--r-sm)] text-[var(--text-muted)] hover:text-[var(--accent)] hover:bg-[rgba(0,227,199,0.06)] transition-colors"
          title="收起侧边栏 (⌘B)"
        >
          <IconPanelLeftClose size={14} />
        </button>
      </div>

      {/* Search */}
      <div className="px-3 py-2">
        <div className="flex items-center gap-2 h-[28px] px-2.5 rounded-[var(--r-md)] bg-[rgba(0,227,199,0.03)] border border-[rgba(0,227,199,0.06)]">
          <IconSearch size={12} className="text-[var(--text-muted)] shrink-0" />
          <input
            ref={inputRef}
            value={query}
            onChange={e => setQuery(e.target.value)}
            onKeyDown={e => e.key==='Escape'&&setQuery('')}
            placeholder="搜索会话..."
            className="flex-1 bg-transparent text-[var(--text)] text-[12px] outline-none placeholder:text-[var(--text-muted)]"
          />
        </div>
      </div>

      {/* Session list */}
      <div className="flex-1 overflow-y-auto">
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
            {unpinned.length > 0 && (
              <div>
                <div className="px-4 pt-2 pb-1 text-[10px] font-semibold uppercase tracking-[1.2px] text-[var(--text-muted)]">
                  {pinned.length > 0 ? '全部' : '今天'}
                </div>
                {unpinned.map(s => <SessionItem key={s.path} session={s} />)}
              </div>
            )}
          </>
        )}
      </div>

      {/* Bottom actions */}
      <div className="flex items-center gap-2 px-3 py-2.5 border-t border-[var(--border)]">
        <button onClick={handleCreate}
          className="flex items-center gap-1.5 flex-1 h-[28px] px-3 text-[12px] font-medium text-[var(--accent)] bg-[rgba(0,227,199,0.08)] hover:bg-[rgba(0,227,199,0.12)] border border-[rgba(0,227,199,0.12)] rounded-[var(--r-md)] transition-colors justify-center">
          <IconPlus size={13} /> 新建会话
        </button>
        <button onClick={() => setSettingsOpen(true)}
          className="w-[28px] h-[28px] flex items-center justify-center rounded-[var(--r-md)] text-[var(--text-muted)] hover:text-[var(--text)] hover:bg-[rgba(255,255,255,0.04)] transition-colors">
          <IconSettings size={14} />
        </button>
      </div>
    </aside>
  )
}

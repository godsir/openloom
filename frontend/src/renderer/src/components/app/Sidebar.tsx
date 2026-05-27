import { useState, useMemo, useRef, useEffect } from 'react'
import { useStore } from '../../stores'
import type { SessionSummary } from '../../stores/session'
import SessionItem from './SessionItem'

export default function Sidebar() {
  const sessions = useStore((s) => s.sessions)
  const pinnedIds = useStore((s) => s.pinnedIds)
  const createSession = useStore((s) => s.createSession)
  const switchSession = useStore((s) => s.switchSession)
  const setSettingsOpen = useStore((s) => s.setSettingsOpen)
  const [search, setSearch] = useState(false)
  const [query, setQuery] = useState('')
  const inputRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    if (search) inputRef.current?.focus()
  }, [search])

  const filtered = useMemo(() => {
    if (!query.trim()) return sessions
    const q = query.toLowerCase()
    return sessions.filter(
      (s) =>
        (s.title || '').toLowerCase().includes(q) ||
        s.path.toLowerCase().includes(q),
    )
  }, [sessions, query])

  const pinned = filtered.filter((s) => pinnedIds.has(s.path))
  const unpinned = filtered.filter((s) => !pinnedIds.has(s.path))

  const handleCreate = async () => {
    const id = await createSession()
    if (id) await switchSession(id)
  }

  return (
    <div className="flex flex-col h-full bg-zinc-950 border-r border-zinc-800 w-[280px]">
      {/* Header */}
      <div className="flex items-center justify-between px-3 py-2 border-b border-zinc-800">
        {search ? (
          <input
            ref={inputRef}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => e.key === 'Escape' && (setSearch(false), setQuery(''))}
            placeholder="搜索会话..."
            className="flex-1 bg-zinc-800 text-zinc-200 text-sm rounded px-2 py-1 outline-none focus:ring-1 focus:ring-blue-500/50 placeholder:text-zinc-600"
          />
        ) : (
          <span className="font-semibold text-sm text-zinc-200">openLoom</span>
        )}
        <div className="flex gap-1">
          <button
            onClick={() => { setSearch(!search); setQuery('') }}
            className={`w-7 h-7 flex items-center justify-center rounded text-sm transition-colors ${search ? 'bg-blue-600 text-white' : 'hover:bg-zinc-800 text-zinc-400'}`}
            title="搜索"
          >
            &#128269;
          </button>
          <button
            onClick={handleCreate}
            className="w-7 h-7 flex items-center justify-center rounded hover:bg-zinc-800 text-zinc-400 text-lg"
            title="新建会话"
          >
            +
          </button>
          <button
            onClick={() => setSettingsOpen(true)}
            className="w-7 h-7 flex items-center justify-center rounded hover:bg-zinc-800 text-zinc-400 text-sm"
            title="设置"
          >
            &#9881;
          </button>
        </div>
      </div>

      {/* Session list */}
      <div className="flex-1 overflow-y-auto py-1">
        {filtered.length === 0 && (
          <div className="px-4 py-8 text-center">
            <p className="text-sm text-zinc-600">
              {query ? '无匹配会话' : '暂无会话'}
            </p>
            {!query && (
              <button
                onClick={handleCreate}
                className="mt-2 px-3 py-1.5 bg-blue-600 text-white text-sm rounded-lg hover:bg-blue-500 transition-colors"
              >
                新建会话
              </button>
            )}
          </div>
        )}

        {pinned.length > 0 && (
          <div className="mb-1">
            <div className="px-3 py-1 text-[10px] text-zinc-600 uppercase tracking-wider font-medium">
              已置顶
            </div>
            {pinned.map((s) => (
              <SessionItem key={s.path} session={s} />
            ))}
          </div>
        )}

        {unpinned.length > 0 && (
          <div>
            {pinned.length > 0 && (
              <div className="px-3 py-1 text-[10px] text-zinc-600 uppercase tracking-wider font-medium">
                全部
              </div>
            )}
            {unpinned.map((s) => (
              <SessionItem key={s.path} session={s} />
            ))}
          </div>
        )}
      </div>
    </div>
  )
}

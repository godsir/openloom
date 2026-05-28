import { useState, useMemo, useRef, useEffect } from 'react'
import { useStore } from '../../stores'
import SessionItem from './SessionItem'
import { IconPlus, IconSearch, IconSettings } from '../../utils/icons'
import styles from './Sidebar.module.css'

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

export default function Sidebar() {
  const sessions = useStore((s) => s.sessions)
  const pinnedIds = useStore((s) => s.pinnedIds)
  const createSession = useStore((s) => s.createSession)
  const switchSession = useStore((s) => s.switchSession)
  const setSettingsOpen = useStore((s) => s.setSettingsOpen)
  const [query, setQuery] = useState('')
  const inputRef = useRef<HTMLInputElement>(null)

  useEffect(() => { inputRef.current?.focus() }, [])

  const filtered = useMemo(() => {
    if (!query.trim()) return sessions
    const q = query.toLowerCase()
    return sessions.filter(s => (s.title||'').toLowerCase().includes(q) || (s.firstMessage||'').toLowerCase().includes(q) || s.path.toLowerCase().includes(q))
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

  const handleCreate = async () => {
    const id = await createSession()
    if (id) await switchSession(id)
  }

  return (
    <aside className={styles.sidebar}>
      <div className={styles.search}>
        <div className={styles.searchBox}>
          <IconSearch size={13} className={styles.searchIcon} />
          <input
            ref={inputRef}
            value={query}
            onChange={e => setQuery(e.target.value)}
            onKeyDown={e => e.key === 'Escape' && setQuery('')}
            placeholder="搜索会话..."
            className={styles.searchInput}
          />
        </div>
      </div>

      <div className={styles.sessionList}>
        {filtered.length === 0 ? (
          <div className={styles.emptyState}>
            <p className={styles.emptyText}>
              {query ? '无匹配会话' : '暂无会话'}
            </p>
          </div>
        ) : (
          <>
            {pinned.length > 0 && (
              <div className={styles.dateGroup}>
                <div className={styles.dateLabel}>已置顶</div>
                {pinned.map(s => <SessionItem key={s.path} session={s} />)}
              </div>
            )}
            {dateGroups.map(({ label, sessions: gs }) => (
              <div key={label} className={styles.dateGroup}>
                <div className={styles.dateLabel}>{label}</div>
                {gs.map(s => <SessionItem key={s.path} session={s} />)}
              </div>
            ))}
          </>
        )}
      </div>

      <div className={styles.bottom}>
        <button onClick={handleCreate} className={styles.createBtn}>
          <IconPlus size={13} /> 新建会话
        </button>
        <button onClick={() => setSettingsOpen(true)} className={styles.settingsBtn}>
          <IconSettings size={15} />
        </button>
      </div>
    </aside>
  )
}

import { useState, useMemo, useRef, useEffect, useCallback } from 'react'
import { useStore } from '../../stores'
import SessionItem from './SessionItem'
import { IconPlus, IconSearch, IconTrash, IconPin, IconPinOff, IconCheck, IconX, IconCalendarClock, IconPlay, IconPause } from '../../utils/icons'
import { loomRpc } from '../../services/jsonrpc'
import { useLocale, t as i18nT } from '../../i18n'
import type { CronJobSummary } from '../../stores/cron'
import styles from './Sidebar.module.css'

function getDateGroup(modified: string): string {
  if (!modified) return i18nT('sidebar.today')
  const d = new Date(modified)
  const now = new Date()
  const today = new Date(now.getFullYear(), now.getMonth(), now.getDate())
  const yesterday = new Date(today)
  yesterday.setDate(yesterday.getDate() - 1)
  const day = new Date(d.getFullYear(), d.getMonth(), d.getDate())
  if (day >= today) return i18nT('sidebar.today')
  if (day >= yesterday) return i18nT('sidebar.yesterday')
  const month = d.getMonth() + 1
  const date = d.getDate()
  return i18nT('sidebar.dateFormat', { month, day: date })
}

function describeCron(expr: string): string {
  const p = expr.trim().split(/\s+/)
  if (p.length !== 7) return expr
  if (p[0] === '0' && p[1] === '*' && p.slice(2).every(f => f === '*')) return i18nT('sidebar.everyMinute')
  if (p[0] === '0' && p[1].startsWith('*/') && p.slice(2).every(f => f === '*')) return i18nT('sidebar.everyNMinutes', { n: p[1].slice(2) })
  if (p[0] === '0' && p[1] === '0' && p.slice(2).every(f => f === '*')) return i18nT('sidebar.everyHour')
  if (p[0] === '0' && p[1] === '0' && /^\d+$/.test(p[2]) && p.slice(3).every(f => f === '*')) return i18nT('sidebar.everyDayAt', { hour: p[2] })
  if (p[0] === '0' && p[1] !== '*' && /^\d+$/.test(p[1]) && p[5] !== '*' && /^\d+$/.test(p[5])) {
    const dowMap = [i18nT('sidebar.dowSun'), i18nT('sidebar.dowMon'), i18nT('sidebar.dowTue'), i18nT('sidebar.dowWed'), i18nT('sidebar.dowThu'), i18nT('sidebar.dowFri'), i18nT('sidebar.dowSat')]
    const dow = dowMap[parseInt(p[5])] || ''
    return i18nT('sidebar.everyWeekAt', { dow, hour: p[2], minute: p[1].padStart(2, '0') })
  }
  return expr
}

function relativeTime(ts: number | null): string {
  if (!ts) return ''
  const diff = Date.now() - ts * 1000
  const mins = Math.floor(diff / 60000)
  if (mins < 1) return i18nT('time.justNow')
  if (mins < 60) return i18nT('time.minutesAgo', { n: mins })
  const hrs = Math.floor(mins / 60)
  if (hrs < 24) return i18nT('time.hoursAgo', { n: hrs })
  return i18nT('time.daysAgo', { n: Math.floor(hrs / 24) })
}

export default function Sidebar() {
  const { t } = useLocale()
  const sessions = useStore((s) => s.sessions)
  const pinnedIds = useStore((s) => s.pinnedIds)
  const selectedSessionIds = useStore((s) => s.selectedSessionIds)
  const createSession = useStore((s) => s.createSession)
  const switchSession = useStore((s) => s.switchSession)
  const deleteSessions = useStore((s) => s.deleteSessions)
  const pinSessions = useStore((s) => s.pinSessions)
  const unpinSessions = useStore((s) => s.unpinSessions)
  const selectAllSessions = useStore((s) => s.selectAllSessions)
  const deselectAllSessions = useStore((s) => s.deselectAllSessions)
  const setScheduledTasksOpen = useStore((s) => s.setScheduledTasksOpen)
  const scheduledTasksOpen = useStore((s) => s.scheduledTasksOpen)
  const [query, setQuery] = useState('')
  const inputRef = useRef<HTMLInputElement>(null)

  // Cron jobs for sidebar display
  const [cronJobs, setCronJobs] = useState<CronJobSummary[]>([])
  const [cronLoading, setCronLoading] = useState(false)

  const loadCronJobs = useCallback(async () => {
    setCronLoading(true)
    try {
      const res = await loomRpc<CronJobSummary[]>('cron.list')
      setCronJobs(Array.isArray(res) ? res : [])
    } catch { /* ignore if cron scheduler not initialized */ }
    finally { setCronLoading(false) }
  }, [])

  useEffect(() => { loadCronJobs() }, [loadCronJobs])

  // Refresh cron jobs when modal closes
  useEffect(() => {
    if (!scheduledTasksOpen) loadCronJobs()
  }, [scheduledTasksOpen, loadCronJobs])

  const openScheduledTasks = () => {
    setScheduledTasksOpen(true)
    loadCronJobs()
  }

  useEffect(() => { inputRef.current?.focus() }, [])

  const selectedCount = selectedSessionIds.size
  const selectionMode = selectedCount > 0

  const filtered = useMemo(() => {
    if (!query.trim()) return sessions
    const q = query.toLowerCase()
    return sessions.filter(s => (s.title||'').toLowerCase().includes(q) || (s.firstMessage||'').toLowerCase().includes(q) || s.path.toLowerCase().includes(q))
  }, [sessions, query])

  const pinned = useMemo(() => {
    const list = filtered.filter(s => pinnedIds.has(s.path))
    list.sort((a, b) => new Date(b.modified).getTime() - new Date(a.modified).getTime())
    return list
  }, [filtered, pinnedIds])

  const dateGroups = useMemo(() => {
    const unpinned = filtered.filter(s => !pinnedIds.has(s.path))
    unpinned.sort((a, b) => new Date(b.modified).getTime() - new Date(a.modified).getTime())
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

  const handleBatchDelete = async () => {
    const ids = [...selectedSessionIds]
    const ok = await useStore.getState().showConfirm(t('sidebar.batchDelete'), t('sidebar.batchDeleteConfirm', { n: ids.length }), true)
    if (ok) await deleteSessions(ids)
  }

  const selectedPinned = useMemo(() => {
    return [...selectedSessionIds].filter(id => pinnedIds.has(id))
  }, [selectedSessionIds, pinnedIds])

  const selectedUnpinned = useMemo(() => {
    return [...selectedSessionIds].filter(id => !pinnedIds.has(id))
  }, [selectedSessionIds, pinnedIds])

  const activeCronEditId = useStore((s) => s.cronEditJobId)

  return (
    <aside className={styles.sidebar}>
      {selectionMode ? (
        <div className={styles.batchBar}>
          <span className={styles.batchCount}>{t('common.selected', { count: selectedCount })}</span>
          <div className={styles.batchActions}>
            <button onClick={selectAllSessions} className={styles.batchBtn} title={t('common.selectAll')}>
              <IconCheck size={13} />
            </button>
            <button onClick={deselectAllSessions} className={styles.batchBtn} title={t('common.deselect')}>
              <IconX size={13} />
            </button>
            {selectedUnpinned.length > 0 && (
              <button onClick={() => pinSessions(selectedUnpinned)} className={styles.batchBtn} title={t('sidebar.pin')}>
                <IconPin size={13} />
              </button>
            )}
            {selectedPinned.length > 0 && (
              <button onClick={() => unpinSessions(selectedPinned)} className={styles.batchBtn} title={t('sidebar.unpin')}>
                <IconPinOff size={13} />
              </button>
            )}
            <button onClick={handleBatchDelete} className={`${styles.batchBtn} ${styles.batchDanger}`} title={t('common.delete')}>
              <IconTrash size={13} />
            </button>
          </div>
        </div>
      ) : (
        <div className={styles.search}>
          <div className={styles.searchBox}>
            <IconSearch size={13} className={styles.searchIcon} />
            <input
              ref={inputRef}
              value={query}
              onChange={e => setQuery(e.target.value)}
              onKeyDown={e => e.key === 'Escape' && setQuery('')}
              placeholder={t('sidebar.searchPlaceholder')}
              className={styles.searchInput}
            />
            {query && (
              <button onClick={() => setQuery('')} className={styles.searchClear}>
                <IconX size={11} />
              </button>
            )}
          </div>
        </div>
      )}

      {/* ── 定时任务 section ─────────────────────────────── */}
      {!selectionMode && (
        <div className={styles.cronSection}>
          <div className={styles.dateGroup}>
            <div className={`${styles.dateLabel} ${styles.cronSectionLabel}`}>
              <span>{t('sidebar.cronTasks')}</span>
              <button className={styles.sectionAddBtn} onClick={openScheduledTasks} title={t('sidebar.newCronTask')}>
                <IconPlus size={15} />
              </button>
            </div>
            {cronLoading ? (
              <div className={styles.cronItem}>
                <span className={styles.cronItemText}>{t('common.loading')}</span>
              </div>
            ) : cronJobs.length === 0 ? (
              <div className={styles.cronItem} onClick={openScheduledTasks}>
                <IconCalendarClock size={13} className={styles.cronItemIcon} />
                <span className={styles.cronItemMuted}>{t('sidebar.noCronTasks')}</span>
              </div>
            ) : (
              cronJobs.map(job => (
                <div
                  key={job.id}
                  className={`${styles.cronItem} ${activeCronEditId === job.id ? styles.cronItemActive : ''}`}
                  onClick={() => { useStore.getState().setCronEditJobId(job.id); setScheduledTasksOpen(true) }}
                >
                  <IconCalendarClock size={13} className={styles.cronItemIcon} />
                  <div className={styles.cronItemContent}>
                    <div className={styles.cronItemName}>
                      {job.name}
                      {!job.enabled && <IconPause size={10} className={styles.cronItemPaused} />}
                    </div>
                    <div className={styles.cronItemDesc}>
                      {describeCron(job.cron_expression)}
                      {job.last_run && <span className={styles.cronItemTime}> · {relativeTime(job.last_run)}</span>}
                    </div>
                  </div>
                </div>
              ))
            )}
          </div>
        </div>
      )}

      {/* ── 会话 header（不随列表滚动）── */}
      {!selectionMode && (
        <div className={styles.sessionHeader}>
          <span>{t('sidebar.sessions')}</span>
          <button className={styles.sectionAddBtn} onClick={handleCreate} title={t('sidebar.newSession')}>
            <IconPlus size={15} />
          </button>
        </div>
      )}

      <div className={styles.sessionList}>
        {filtered.length === 0 ? (
          <div className={styles.emptyState}>
            <p className={styles.emptyText}>
              {query ? t('sidebar.noResults') : t('sidebar.noSessions')}
            </p>
          </div>
        ) : (
          <>
            {pinned.length > 0 && (
              <div className={styles.dateGroup}>
                <div className={styles.dateLabel}>{t('sidebar.pinned')}</div>
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

    </aside>
  )
}

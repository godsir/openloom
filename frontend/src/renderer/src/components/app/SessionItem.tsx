import { useState, useRef, useEffect } from 'react'
import { useStore } from '../../stores'
import type { SessionSummary } from '../../stores/session'
import { useIMStore } from '../../stores/im'
import PlatformIcon from '../shared/PlatformIcon'
import { rpc } from '../../services/rpc-toast'
import ContextMenu, { ContextMenuItem } from '../shared/ContextMenu'
import { IconPin, IconPinOff } from '../../utils/icons'
import { useLocale, t as i18nT } from '../../i18n'
import styles from './SessionItem.module.css'

export default function SessionItem({ session }: { session: SessionSummary }) {
  const { t } = useLocale()
  const currentId = useStore(s => s.currentSessionId)
  const switchSession = useStore(s => s.switchSession)
  const renameSession = useStore(s => s.renameSession)
  const deleteSession = useStore(s => s.deleteSession)
  const pinSession = useStore(s => s.pinSession)
  const unpinSession = useStore(s => s.unpinSession)
  const toggleSessionSelect = useStore(s => s.toggleSessionSelect)
  const selectedSessionIds = useStore(s => s.selectedSessionIds)
  const streamingIds = useStore(s => s.streamingSessionIds)
  const sid = session.path || ''
  const imSource = useIMStore(s => sid ? s.imSessionSources[sid] : undefined)
  const isActive = sid === currentId
  const isStreaming = sid ? streamingIds.has(sid) : false
  const isPinned = useStore(s => sid ? s.pinnedIds.has(sid) : false)
  const isSelected = selectedSessionIds.has(sid)
  const selectionMode = selectedSessionIds.size > 0
  const [menuOpen, setMenuOpen] = useState(false)
  const [menuPos, setMenuPos] = useState({ x: 0, y: 0 })
  const [renaming, setRenaming] = useState(false)
  const [titleDraft, setTitleDraft] = useState(session.title || '')
  const inputRef = useRef<HTMLInputElement>(null)

  useEffect(() => { if (renaming) inputRef.current?.focus() }, [renaming])

  const submitRename = async () => {
    if (titleDraft.trim() && titleDraft !== session.title) await renameSession(sid, titleDraft.trim())
    setRenaming(false)
  }

  const handleClick = () => {
    if (selectionMode) {
      toggleSessionSelect(sid)
    } else if (isActive) {
      useStore.getState().setCurrentSessionId(null)
    } else {
      switchSession(sid)
    }
  }

  if (!sid) return null

  return (
    <div
      onClick={handleClick}
      onContextMenu={e => {
        if (selectionMode) return
        e.preventDefault(); setMenuPos({ x: e.clientX, y: e.clientY }); setMenuOpen(true)
      }}
      className={`${styles.item} ${isActive ? styles.active : ''} ${isSelected ? styles.selected : ''}`}
    >
      <div
        className={`${styles.checkbox} ${(selectionMode || isSelected) ? styles.checkboxVisible : ''}`}
        onClick={e => { e.stopPropagation(); toggleSessionSelect(sid) }}
      >
        <div className={`${styles.checkmark} ${isSelected ? styles.checked : ''}`}>
          {isSelected && (
            <svg width="10" height="10" viewBox="0 0 10 10" fill="none">
              <path d="M2 5L4 7L8 3" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round"/>
            </svg>
          )}
        </div>
      </div>
      {renaming ? (
        <input ref={inputRef} value={titleDraft} onChange={e => setTitleDraft(e.target.value)}
          onKeyDown={e => { if(e.key==='Enter')submitRename(); if(e.key==='Escape')setRenaming(false) }}
          onBlur={submitRename} onClick={e => e.stopPropagation()}
          className={styles.renameInput} />
      ) : (
        <div className={styles.content}>
          <div className={styles.title}>
            {isStreaming && <span className={styles.streamingDot} />}
            {imSource && <PlatformIcon platform={imSource.platform} size={14} />}
            {session.title || session.firstMessage?.slice(0, 40) || i18nT('sidebar.sessionDefault', { id: sid.slice(0, 8) })}
          </div>
          {(session.modified || session.messageCount > 0) && (
            <div className={styles.meta}>
              {session.modified && <span>{relativeTime(session.modified)}</span>}
              {session.modified && session.messageCount > 0 && <span>·</span>}
              {session.messageCount > 0 && <span>{i18nT('sidebar.messageCount', { n: session.messageCount })}</span>}
              {session.createdAt && <>
                <span>·</span>
                <span className={styles.createTime}>{formatDate(session.createdAt)}</span>
              </>}
            </div>
          )}
        </div>
      )}
      {!renaming && (
        <button onClick={e => { e.stopPropagation(); isPinned ? unpinSession(sid) : pinSession(sid) }}
          className={`${styles.pinBtn} ${isPinned ? styles.pinned : ''}`}>
          {isPinned ? <IconPinOff size={11} /> : <IconPin size={11} />}
        </button>
      )}
      {!selectionMode && (
        <ContextMenu open={menuOpen} x={menuPos.x} y={menuPos.y} onClose={() => setMenuOpen(false)}>
          <ContextMenuItem onClick={()=>{setMenuOpen(false);setRenaming(true);setTitleDraft(session.title||'')}}>{t('sidebar.rename')}</ContextMenuItem>
          <ContextMenuItem onClick={async ()=>{
            setMenuOpen(false)
            const path = await window.loom.selectFolder()
            if (path) {
              useStore.getState().setSessionWorkspace(sid, path)
              await rpc('workspace.set_session', { session_id: sid, path }, i18nT('sidebar.setWorkspace') + ' (' + i18nT('common.done') + ')')
            }
          }}>{t('sidebar.setWorkspace')}</ContextMenuItem>
          <ContextMenuItem onClick={async ()=>{setMenuOpen(false); const ok = await useStore.getState().showConfirm(t('sidebar.deleteSession'), t('sidebar.deleteConfirm'), true); if(ok)deleteSession(sid)}} danger>{t('common.delete')}</ContextMenuItem>
        </ContextMenu>
      )}
    </div>
  )
}

function relativeTime(iso: string): string {
  if (!iso) return ''
  const diff = Date.now() - new Date(iso).getTime()
  const mins = Math.floor(diff / 60000)
  if (mins < 1) return i18nT('time.justNow')
  if (mins < 60) return i18nT('time.minutesAgo', { n: mins })
  const hrs = Math.floor(mins / 60)
  if (hrs < 24) return i18nT('time.hoursAgo', { n: hrs })
  const days = Math.floor(hrs / 24)
  if (days < 30) return i18nT('time.daysAgo', { n: days })
  const months = Math.floor(days / 30)
  if (months < 12) return i18nT('time.monthsAgo', { n: months })
  return i18nT('time.yearsAgo', { n: Math.floor(months / 12) })
}

function formatDate(iso: string): string {
  if (!iso) return ''
  const d = new Date(iso)
  const now = new Date()
  const year = d.getFullYear()
  const month = d.getMonth() + 1
  const day = d.getDate()
  if (year === now.getFullYear()) {
    return i18nT('sidebar.dateFormat', { month, day })
  }
  return i18nT('sidebar.dateFormatYear', { year, month, day })
}

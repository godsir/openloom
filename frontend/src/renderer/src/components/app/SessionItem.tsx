import { useState, useRef, useEffect } from 'react'
import { useStore } from '../../stores'
import type { SessionSummary } from '../../stores/session'
import ContextMenu, { ContextMenuItem } from '../shared/ContextMenu'
import { IconPin, IconPinOff } from '../../utils/icons'
import styles from './SessionItem.module.css'

export default function SessionItem({ session }: { session: SessionSummary }) {
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
            {session.title || session.firstMessage?.slice(0, 40) || `会话 ${sid.slice(0, 8)}`}
          </div>
          {(session.modified || session.messageCount > 0) && (
            <div className={styles.meta}>
              {session.modified && <span>{relativeTime(session.modified)}</span>}
              {session.modified && session.messageCount > 0 && <span>·</span>}
              {session.messageCount > 0 && <span>{session.messageCount}条消息</span>}
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
          <ContextMenuItem onClick={()=>{setMenuOpen(false);setRenaming(true);setTitleDraft(session.title||'')}}>重命名</ContextMenuItem>
          <ContextMenuItem onClick={async ()=>{setMenuOpen(false); const ok = await useStore.getState().showConfirm('删除会话', '确定删除此会话？', true); if(ok)deleteSession(sid)}} danger>删除</ContextMenuItem>
        </ContextMenu>
      )}
    </div>
  )
}

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

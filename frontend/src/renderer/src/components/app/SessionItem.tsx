import { useState, useRef, useEffect } from 'react'
import { useStore } from '../../stores'
import type { SessionSummary } from '../../stores/session'
import ContextMenu, { ContextMenuItem } from '../shared/ContextMenu'
import { IconPin, IconPinOff } from '../../utils/icons'

export default function SessionItem({ session }: { session: SessionSummary }) {
  const currentId = useStore(s => s.currentSessionId)
  const switchSession = useStore(s => s.switchSession)
  const renameSession = useStore(s => s.renameSession)
  const deleteSession = useStore(s => s.deleteSession)
  const pinSession = useStore(s => s.pinSession)
  const unpinSession = useStore(s => s.unpinSession)
  const sid = session.path || ''
  const isActive = sid === currentId
  const isPinned = useStore(s => sid ? s.pinnedIds.has(sid) : false)
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

  if (!sid) return null

  return (
    <div
      onClick={() => switchSession(sid)}
      onContextMenu={e => { e.preventDefault(); setMenuPos({ x: e.clientX, y: e.clientY }); setMenuOpen(true) }}
      className={`group relative flex items-center gap-2 mx-2 px-2 py-1 cursor-pointer rounded-[var(--r-sm)] transition-all duration-[var(--dur-fast)] ${
        isActive
          ? 'bg-[rgba(0,227,199,0.05)] border-l-2 border-l-[var(--accent)] pl-1.5'
          : 'border-l-2 border-l-transparent text-[var(--text-light)] hover:bg-[rgba(0,227,199,0.03)] hover:text-[var(--text)]'
      }`}
    >
      {renaming ? (
        <input ref={inputRef} value={titleDraft} onChange={e => setTitleDraft(e.target.value)}
          onKeyDown={e => { if(e.key==='Enter')submitRename(); if(e.key==='Escape')setRenaming(false) }}
          onBlur={submitRename} onClick={e => e.stopPropagation()}
          className="flex-1 bg-[rgba(255,255,255,0.06)] text-[var(--text)] text-[12px] rounded-[var(--r-sm)] px-1.5 py-0.5 outline-none" />
      ) : (
        <span className="flex-1 truncate text-[12px] leading-snug">
          {session.title || `会话 ${sid.slice(0, 8)}`}
        </span>
      )}
      {!renaming && (
        <button onClick={e => { e.stopPropagation(); isPinned ? unpinSession(sid) : pinSession(sid) }}
          className={`shrink-0 ${isPinned ? 'text-[var(--accent)] opacity-100' : 'text-[var(--text-muted)] opacity-0 group-hover:opacity-100'} transition-opacity`}>
          {isPinned ? <IconPinOff size={11} /> : <IconPin size={11} />}
        </button>
      )}
      <ContextMenu open={menuOpen} x={menuPos.x} y={menuPos.y} onClose={() => setMenuOpen(false)}>
        <ContextMenuItem onClick={()=>{setMenuOpen(false);setRenaming(true);setTitleDraft(session.title||'')}}>重命名</ContextMenuItem>
        <ContextMenuItem onClick={()=>{setMenuOpen(false);if(confirm('确定删除此会话？'))deleteSession(sid)}} danger>删除</ContextMenuItem>
      </ContextMenu>
    </div>
  )
}

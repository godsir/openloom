import { useState, useRef, useEffect } from 'react'
import { useStore } from '../../stores'
import type { SessionSummary } from '../../stores/session'
import ContextMenu, { ContextMenuItem, ContextMenuDivider } from '../shared/ContextMenu'

export default function SessionItem({ session }: { session: SessionSummary }) {
  const currentId = useStore((s) => s.currentSessionId)
  const switchSession = useStore((s) => s.switchSession)
  const renameSession = useStore((s) => s.renameSession)
  const deleteSession = useStore((s) => s.deleteSession)
  const pinSession = useStore((s) => s.pinSession)
  const unpinSession = useStore((s) => s.unpinSession)

  const sid = session.path || ''
  const isActive = sid === currentId
  const isPinned = useStore((s) => (sid ? s.pinnedIds.has(sid) : false))

  const [menuOpen, setMenuOpen] = useState(false)
  const [menuPos, setMenuPos] = useState({ x: 0, y: 0 })
  const [renaming, setRenaming] = useState(false)
  const [titleDraft, setTitleDraft] = useState(session.title || '')
  const inputRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    if (renaming) inputRef.current?.focus()
  }, [renaming])

  const handleRename = () => {
    setMenuOpen(false)
    setRenaming(true)
    setTitleDraft(session.title || '')
  }

  const submitRename = async () => {
    if (titleDraft.trim() && titleDraft !== session.title) {
      await renameSession(sid, titleDraft.trim())
    }
    setRenaming(false)
  }

  const handleDelete = async () => {
    setMenuOpen(false)
    if (confirm('确定删除此会话？')) {
      await deleteSession(sid)
    }
  }

  if (!sid) return null

  return (
    <div
      onClick={() => switchSession(sid)}
      onContextMenu={(e) => {
        e.preventDefault()
        setMenuPos({ x: e.clientX, y: e.clientY })
        setMenuOpen(true)
      }}
      className={`group relative flex items-center gap-2 px-3 py-1.5 cursor-pointer rounded-md mx-1 transition-colors ${
        isActive
          ? 'bg-zinc-700 text-white'
          : 'text-zinc-400 hover:bg-zinc-800 hover:text-zinc-200'
      }`}
    >
      {renaming ? (
        <input
          ref={inputRef}
          value={titleDraft}
          onChange={(e) => setTitleDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') submitRename()
            if (e.key === 'Escape') setRenaming(false)
          }}
          onBlur={submitRename}
          onClick={(e) => e.stopPropagation()}
          className="flex-1 bg-zinc-600 text-zinc-200 text-sm rounded px-1 py-0.5 outline-none"
        />
      ) : (
        <span className="flex-1 truncate text-sm">
          {session.title || `会话 ${sid.slice(0, 8)}`}
        </span>
      )}

      {!renaming && (
        <>
          <span className="text-[10px] text-zinc-600 tabular-nums">
            {session.messageCount ?? 0}
          </span>
          <button
            onClick={(e) => {
              e.stopPropagation()
              isPinned ? unpinSession(sid) : pinSession(sid)
            }}
            className={`shrink-0 opacity-0 group-hover:opacity-100 transition-opacity text-xs ${
              isPinned ? 'text-yellow-500 opacity-100' : 'text-zinc-500'
            }`}
          >
            {isPinned ? '★' : '☆'}
          </button>
        </>
      )}

      {/* Context menu */}
      <ContextMenu open={menuOpen} x={menuPos.x} y={menuPos.y} onClose={() => setMenuOpen(false)}>
        <ContextMenuItem onClick={handleRename}>重命名</ContextMenuItem>
        <ContextMenuItem onClick={handleDelete} danger>删除</ContextMenuItem>
      </ContextMenu>
    </div>
  )
}

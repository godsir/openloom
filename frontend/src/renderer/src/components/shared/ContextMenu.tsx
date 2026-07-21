import React, { useEffect, useRef, useState, useLayoutEffect } from 'react'
import styles from './ContextMenu.module.css'

interface ContextMenuProps {
  open: boolean
  x: number
  y: number
  onClose: () => void
  children: React.ReactNode
}

export default function ContextMenu({ open, x, y, onClose, children }: ContextMenuProps) {
  const menuRef = useRef<HTMLDivElement>(null)
  const [pos, setPos] = useState({ left: x, top: y })

  // Clamp position so the menu never overflows the viewport
  useLayoutEffect(() => {
    if (!open) return
    const el = menuRef.current
    if (!el) return
    const r = el.getBoundingClientRect()
    const vw = window.innerWidth
    const vh = window.innerHeight
    let left = x
    let top = y
    if (left + r.width > vw) left = Math.max(0, x - r.width)
    if (top + r.height > vh) top = Math.max(0, y - r.height)
    setPos({ left, top })
  }, [open, x, y])

  useEffect(() => {
    if (!open) return
    const handle = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        onClose()
      }
    }
    const handleKey = (e: KeyboardEvent) => { if (e.key === 'Escape') onClose() }
    // Small delay so the same click that opened it doesn't close it.
    // 用 pointerdown 而非 click：在另一条目上右键触发的是 contextmenu 而非 click，
    // 只听 click 会让旧菜单不关闭、屏幕上叠加两个右键菜单。pointerdown 对左/右键
    // 都触发，可在打开新菜单前关闭旧菜单（菜单项在 menuRef 内，不受影响）。
    const id = setTimeout(() => {
      window.addEventListener('pointerdown', handle)
      window.addEventListener('keydown', handleKey)
    }, 0)
    return () => {
      clearTimeout(id)
      window.removeEventListener('pointerdown', handle)
      window.removeEventListener('keydown', handleKey)
    }
  }, [open, onClose])

  if (!open) return null

  return (
    <div
      ref={menuRef}
      className={styles.menu}
      style={{ position: 'fixed', zIndex: 50000, left: pos.left, top: pos.top }}
      onContextMenu={(e) => e.preventDefault()}
      onClick={(e) => e.stopPropagation()}
    >
      {children}
    </div>
  )
}

// ── ContextMenuItem ──────────────────────────────────────────────

interface ContextMenuItemProps {
  children: React.ReactNode
  onClick: () => void
  danger?: boolean
}

export const ContextMenuItem: React.FC<ContextMenuItemProps> = ({ children, onClick, danger }) => (
  <button
    className={`${styles.item} ${danger ? styles.danger : ''}`}
    onClick={onClick}
  >
    {children}
  </button>
)

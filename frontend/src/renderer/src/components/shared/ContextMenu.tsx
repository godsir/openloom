import { useEffect, useRef, useState, type ReactNode } from 'react'
import { createPortal } from 'react-dom'

interface ContextMenuProps {
  open: boolean
  x: number
  y: number
  onClose: () => void
  children: ReactNode
}

export default function ContextMenu({
  open,
  x,
  y,
  onClose,
  children,
}: ContextMenuProps) {
  const ref = useRef<HTMLDivElement>(null)
  const [adjustX, setAdjustX] = useState(0)
  const [adjustY, setAdjustY] = useState(0)

  useEffect(() => {
    if (!open) {
      setAdjustX(0)
      setAdjustY(0)
      return
    }
    // Clamp position after render so menu stays within viewport
    const raf = requestAnimationFrame(() => {
      if (!ref.current) return
      const rect = ref.current.getBoundingClientRect()
      const vw = window.innerWidth
      const vh = window.innerHeight
      let ax = 0, ay = 0
      if (x + rect.width > vw - 8) ax = x + rect.width - vw + 8
      if (y + rect.height > vh - 8) ay = y + rect.height - vh + 8
      setAdjustX(ax)
      setAdjustY(ay)
    })
    return () => cancelAnimationFrame(raf)
  }, [open, x, y])

  useEffect(() => {
    if (!open) return
    const close = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) onClose()
    }
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose()
    }
    document.addEventListener('mousedown', close)
    document.addEventListener('keydown', onKey)
    return () => {
      document.removeEventListener('mousedown', close)
      document.removeEventListener('keydown', onKey)
    }
  }, [open, onClose])

  if (!open) return null

  return createPortal(
    <div
      ref={ref}
      className="fixed z-[9999] min-w-[160px] bg-[var(--bg-card)] border border-[var(--border)] rounded-[var(--r-md)] shadow-[var(--shadow-lg)] py-1 animate-fade-in"
      style={{ left: x - adjustX, top: y - adjustY }}
    >
      {children}
    </div>,
    document.body,
  )
}

export function ContextMenuItem({
  onClick,
  danger,
  children,
}: {
  onClick: () => void
  danger?: boolean
  children: ReactNode
}) {
  return (
    <button
      onClick={onClick}
      className={`w-full text-left px-3.5 py-2 text-[13px] transition-colors ${
        danger
          ? 'text-[var(--red)] hover:bg-[var(--red-light)]'
          : 'text-[var(--text-secondary)] hover:bg-[rgba(255,255,255,0.04)] hover:text-[var(--text)]'
      }`}
    >
      {children}
    </button>
  )
}

export function ContextMenuDivider() {
  return <div className="my-1 border-t border-[var(--border)]" />
}

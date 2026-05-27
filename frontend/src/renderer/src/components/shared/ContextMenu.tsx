import { useEffect, useRef, type ReactNode } from 'react'

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

  return (
    <div
      ref={ref}
      className="fixed z-50 min-w-[150px] bg-[var(--bg)] border border-[var(--border-accent)] rounded-[var(--r-md)] shadow-xl py-1 animate-fade-in backdrop-blur-xl"
      style={{ left: x, top: y }}
    >
      {children}
    </div>
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
      className={`w-full text-left px-3.5 py-2 text-[13px] transition-colors-fast ${
        danger
          ? 'text-[var(--red)] hover:bg-[var(--red-light)]'
          : 'text-[var(--text-light)] hover:bg-[rgba(0,227,199,0.04)] hover:text-[var(--text)]'
      }`}
    >
      {children}
    </button>
  )
}

export function ContextMenuDivider() {
  return <div className="my-1 border-t border-[var(--border)]" />
}

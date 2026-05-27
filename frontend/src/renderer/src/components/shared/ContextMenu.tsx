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
      className="fixed z-50 min-w-[140px] bg-zinc-800 border border-zinc-700 rounded-lg shadow-xl py-1"
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
      className={`w-full text-left px-3 py-1.5 text-xs transition-colors ${
        danger
          ? 'text-red-400 hover:bg-red-900/20'
          : 'text-zinc-300 hover:bg-zinc-700'
      }`}
    >
      {children}
    </button>
  )
}

export function ContextMenuDivider() {
  return <div className="my-1 border-t border-zinc-700" />
}

import { useEffect, useRef, useState, type ReactNode } from 'react'
import { createPortal } from 'react-dom'
import styles from './ContextMenu.module.css'

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
      className={styles.menu}
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
      className={`${styles.item} ${danger ? styles.itemDanger : ''}`}
    >
      {children}
    </button>
  )
}

export function ContextMenuDivider() {
  return <div className={styles.divider} />
}

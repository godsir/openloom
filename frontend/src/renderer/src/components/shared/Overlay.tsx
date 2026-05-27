import { useEffect, useRef, type ReactNode } from 'react'
import { IconX } from '../../utils/icons'

interface OverlayProps {
  open: boolean
  onClose: () => void
  children: ReactNode
  title?: string
}

export default function Overlay({ open, onClose, children, title }: OverlayProps) {
  const overlayRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (!open) return
    const handleEsc = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose()
    }
    document.addEventListener('keydown', handleEsc)
    return () => document.removeEventListener('keydown', handleEsc)
  }, [open, onClose])

  if (!open) return null

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <div
        ref={overlayRef}
        className="absolute inset-0 bg-black/70 backdrop-blur-sm"
        onClick={onClose}
      />
      <div className="relative bg-[var(--bg)] border border-[var(--border-accent)] rounded-[var(--r-lg)] shadow-[var(--shadow-lg)] max-w-xl w-full max-h-[80vh] overflow-y-auto m-4 animate-fade-up">
        {title && (
          <div className="flex items-center justify-between px-5 py-3.5 border-b border-[var(--border)]">
            <h2 className="text-sm font-semibold text-[var(--text)]">{title}</h2>
            <button
              onClick={onClose}
              className="text-[var(--text-muted)] hover:text-[var(--text)] transition-colors-fast"
            >
              <IconX size={16} />
            </button>
          </div>
        )}
        <div className="p-5">{children}</div>
      </div>
    </div>
  )
}

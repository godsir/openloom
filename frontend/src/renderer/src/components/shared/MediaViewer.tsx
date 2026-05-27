import { useEffect, useState } from 'react'
import { IconX } from '../../utils/icons'

interface MediaViewerProps { open: boolean; src: string; alt?: string; onClose: () => void }

export default function MediaViewer({ open, src, alt, onClose }: MediaViewerProps) {
  const [scale, setScale] = useState(1)

  useEffect(() => {
    if (!open) return
    const handleKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose()
      if (e.key === '+' || e.key === '=') setScale((s) => Math.min(s + 0.25, 5))
      if (e.key === '-') setScale((s) => Math.max(s - 0.25, 0.25))
      if (e.key === '0') setScale(1)
    }
    document.addEventListener('keydown', handleKey)
    return () => document.removeEventListener('keydown', handleKey)
  }, [open, onClose])

  useEffect(() => { setScale(1) }, [src])

  if (!open) return null

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/85 backdrop-blur-sm"
      onClick={onClose}
    >
      <div className="absolute top-4 right-4 flex items-center gap-2 text-xs">
        <span className="px-2.5 py-1 bg-[var(--bg-card)]/80 rounded-[var(--r-sm)] text-[var(--text-light)] font-mono backdrop-blur-sm">
          {Math.round(scale * 100)}%
        </span>
        <button
          onClick={onClose}
          className="flex items-center gap-1 px-2.5 py-1 bg-[var(--bg-card)]/80 rounded-[var(--r-sm)] text-[var(--text-light)] hover:text-[var(--text)] transition-colors-fast backdrop-blur-sm"
        >
          <IconX size={12} /> 关闭
        </button>
      </div>

      <img
        src={src}
        alt={alt || ''}
        onClick={(e) => e.stopPropagation()}
        style={{ transform: `scale(${scale})`, maxWidth: '90vw', maxHeight: '90vh', objectFit: 'contain' }}
        className="transition-transform duration-200 rounded-[var(--r-sm)]"
        draggable={false}
      />
    </div>
  )
}

import { useEffect, useState, useCallback } from 'react'

interface MediaViewerProps {
  open: boolean
  src: string
  alt?: string
  onClose: () => void
}

export default function MediaViewer({
  open,
  src,
  alt,
  onClose,
}: MediaViewerProps) {
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
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/80 backdrop-blur-sm"
      onClick={onClose}
    >
      <div className="absolute top-3 right-3 flex gap-2 text-xs text-zinc-400">
        <span className="px-2 py-1 bg-zinc-800/50 rounded">{Math.round(scale * 100)}%</span>
        <button onClick={onClose} className="px-2 py-1 bg-zinc-800/50 rounded hover:bg-zinc-700">
          × 关闭
        </button>
      </div>

      <img
        src={src}
        alt={alt || ''}
        onClick={(e) => e.stopPropagation()}
        style={{ transform: `scale(${scale})`, maxWidth: '90vw', maxHeight: '90vh', objectFit: 'contain' }}
        className="transition-transform duration-200 cursor-zoom-in rounded"
        draggable={false}
      />
    </div>
  )
}

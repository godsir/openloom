import { useState, useEffect, useRef, useMemo } from 'react'
import { IconImage, IconFileText, IconPaperclip } from '../../utils/icons'

interface FileItem { path: string; name: string; kind: string }

interface Props {
  query: string
  onSelect: (file: FileItem) => void
  onClose: () => void
}

export default function FileMentionMenu({ query, onSelect, onClose }: Props) {
  const [files, setFiles] = useState<FileItem[]>([])
  const menuRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    const close = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) onClose()
    }
    document.addEventListener('mousedown', close)
    return () => document.removeEventListener('mousedown', close)
  }, [onClose])

  const filtered = useMemo(() => {
    if (!query.trim()) return files
    const q = query.toLowerCase()
    return files.filter((f) => f.name.toLowerCase().includes(q))
  }, [files, query])

  return (
    <div
      ref={menuRef}
      className="absolute bottom-full left-0 mb-1 w-64 bg-[var(--bg)] border border-[var(--border-accent)] rounded-[var(--r-sm)] shadow-xl overflow-hidden z-20 animate-fade-in"
    >
      {filtered.length === 0 ? (
        <div className="px-3.5 py-3 text-xs text-[var(--text-muted)]">
          {query ? '无匹配文件' : '输入文件名搜索...'}
        </div>
      ) : (
        filtered.slice(0, 10).map((f) => (
          <button
            key={f.path}
            onClick={() => onSelect(f)}
            className="w-full text-left px-3.5 py-2.5 hover:bg-[rgba(255,255,255,0.04)] transition-colors-fast flex items-center gap-2"
          >
            <span className="opacity-50">
              {f.kind === 'image' ? <IconImage size={14} /> : f.kind === 'code' ? <IconFileText size={14} /> : <IconPaperclip size={14} />}
            </span>
            <span className="text-[13px] text-[var(--text-light)] truncate">{f.name}</span>
          </button>
        ))
      )}
    </div>
  )
}

import { useState, useEffect, useRef, useMemo } from 'react'

interface FileItem {
  path: string
  name: string
  kind: string
}

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
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        onClose()
      }
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
      className="absolute bottom-full left-0 mb-1 w-64 bg-zinc-800 border border-zinc-700 rounded-lg shadow-xl overflow-hidden z-20"
    >
      {filtered.length === 0 ? (
        <div className="px-3 py-2 text-xs text-zinc-500">
          {query ? '无匹配文件' : '输入文件名搜索...'}
        </div>
      ) : (
        filtered.slice(0, 10).map((f) => (
          <button
            key={f.path}
            onClick={() => onSelect(f)}
            className="w-full text-left px-3 py-2 hover:bg-zinc-700 transition-colors flex items-center gap-2"
          >
            <span className="text-xs text-zinc-500">
              {f.kind === 'image' ? '🖼' : f.kind === 'code' ? '📄' : '📎'}
            </span>
            <span className="text-sm text-zinc-300 truncate">{f.name}</span>
          </button>
        ))
      )}
    </div>
  )
}

import { IconX, IconImage, IconPaperclip } from '../../utils/icons'

interface Props {
  files: { name: string; path: string; size: number; mimeType: string }[]
  onRemove: (index: number) => void
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
}

export default function AttachedFiles({ files, onRemove }: Props) {
  if (files.length === 0) return null

  return (
    <div className="flex flex-wrap gap-1.5">
      {files.map((f, i) => (
        <div
          key={i}
          className="flex items-center gap-1.5 px-2.5 py-1.5 bg-[var(--bg-card)] border border-[var(--border)] rounded-[var(--r-sm)] text-xs group"
          title={f.path}
        >
          <span className="opacity-50">
            {f.mimeType?.startsWith('image/') ? <IconImage size={14} /> : <IconPaperclip size={14} />}
          </span>
          <span className="text-[var(--text-light)] truncate max-w-[120px]">{f.name}</span>
          <span className="text-[10px] font-mono text-[var(--text-muted)]">{formatSize(f.size)}</span>
          <button
            onClick={() => onRemove(i)}
            className="text-[var(--text-muted)] hover:text-[var(--text)] shrink-0 transition-colors-fast"
          >
            <IconX size={12} />
          </button>
        </div>
      ))}
    </div>
  )
}

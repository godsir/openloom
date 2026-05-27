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
          className="flex items-center gap-1.5 px-2 py-1 bg-zinc-800 border border-zinc-700/50 rounded-md text-xs group"
          title={f.path}
        >
          <span className="text-zinc-500">
            {f.mimeType?.startsWith('image/') ? '🖼' : '📎'}
          </span>
          <span className="text-zinc-300 truncate max-w-[120px]">{f.name}</span>
          <span className="text-[10px] text-zinc-600">{formatSize(f.size)}</span>
          <button
            onClick={() => onRemove(i)}
            className="text-zinc-500 hover:text-zinc-300 shrink-0"
          >
            ×
          </button>
        </div>
      ))}
    </div>
  )
}

import type { ContentBlock } from '../../stores/chat'

export default function FileBlock({ block }: { block: ContentBlock }) {
  const name = (block.name as string) || 'unknown'
  const filePath = (block.path as string) || ''
  const size = block.size as number | undefined

  const formatSize = (bytes: number) => {
    if (bytes < 1024) return `${bytes} B`
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
  }

  return (
    <div className="border border-zinc-700/50 rounded-md px-3 py-2 flex items-center gap-3 text-sm">
      <span className="text-zinc-500">&#128196;</span>
      <div className="flex-1 min-w-0">
        <p className="text-zinc-300 truncate">{name}</p>
        {size != null && (
          <p className="text-xs text-zinc-600">{formatSize(size)}</p>
        )}
      </div>
      {filePath && (
        <button
          onClick={() => window.hana.openFile(filePath)}
          className="text-xs text-blue-400 hover:text-blue-300 shrink-0"
        >
          打开
        </button>
      )}
    </div>
  )
}

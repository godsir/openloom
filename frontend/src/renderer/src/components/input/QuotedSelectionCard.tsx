interface Props {
  text: string
  filePath?: string
  onRemove: () => void
}

export default function QuotedSelectionCard({
  text,
  filePath,
  onRemove,
}: Props) {
  if (!text) return null

  const preview = text.length > 200 ? text.slice(0, 200) + '...' : text

  return (
    <div className="flex items-start gap-2 px-3 py-2 bg-zinc-800/50 border border-zinc-700/50 rounded-lg text-xs">
      <div className="flex-1 min-w-0">
        {filePath && (
          <p className="text-zinc-500 text-[10px] mb-0.5 truncate">
            {filePath}
          </p>
        )}
        <p className="text-zinc-400 whitespace-pre-wrap line-clamp-3">
          {preview}
        </p>
      </div>
      <button
        onClick={onRemove}
        className="shrink-0 text-zinc-500 hover:text-zinc-300"
      >
        ×
      </button>
    </div>
  )
}

import { IconX } from '../../utils/icons'

interface Props {
  text: string
  filePath?: string
  onRemove?: () => void
}

export default function QuotedSelectionCard({ text, filePath, onRemove }: Props) {
  if (!text) return null

  const preview = text.length > 200 ? text.slice(0, 200) + '...' : text

  return (
    <div className="flex items-start gap-2 px-3 py-2.5 bg-[var(--bg-card)] border border-[var(--border)] rounded-[var(--r-sm)] text-xs animate-fade-in">
      <div className="flex-1 min-w-0">
        {filePath && (
          <p className="text-[var(--text-muted)] text-[10px] font-mono mb-0.5 truncate">
            {filePath}
          </p>
        )}
        <p className="text-[var(--text-light)] whitespace-pre-wrap line-clamp-3 leading-relaxed">
          {preview}
        </p>
      </div>
      {onRemove && (
        <button
          onClick={onRemove}
          className="shrink-0 text-[var(--text-muted)] hover:text-[var(--text)] transition-colors-fast"
        >
          <IconX size={12} />
        </button>
      )}
    </div>
  )
}

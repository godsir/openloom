import { useState } from 'react'
import { IconX } from '../../utils/icons'
import { useLocale } from '../../i18n'

interface Props {
  text: string
  filePath?: string
  onRemove?: () => void
}

/** 退场动画时长，与 base.css 的 .animate-fade-out 保持一致 */
const EXIT_MS = 120

export default function QuotedSelectionCard({ text, filePath, onRemove }: Props) {
  const { t } = useLocale()
  const [exiting, setExiting] = useState(false)

  if (!text) return null

  const preview = text.length > 200 ? text.slice(0, 200) + '...' : text

  // 移除时先播淡出再真正删除，与入场（fade-in）节奏对称
  const handleRemove = () => {
    if (exiting || !onRemove) return
    setExiting(true)
    setTimeout(() => onRemove(), EXIT_MS)
  }

  return (
    <div className={`flex items-start gap-2 px-3 py-2.5 bg-[var(--bg-card)] border border-[var(--border)] rounded-[var(--r-sm)] text-xs ${exiting ? 'animate-fade-out' : 'animate-fade-in'}`}>
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
          onClick={handleRemove}
          aria-label={t('common.removeQuote')}
          title={t('common.removeQuote')}
          className="shrink-0 text-[var(--text-muted)] hover:text-[var(--text)] transition-colors-fast"
        >
          <IconX size={12} />
        </button>
      )}
    </div>
  )
}

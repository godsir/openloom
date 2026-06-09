import type { ContentBlock } from '../../stores/chat'
import { useLocale } from '../../i18n'
import { IconFile, IconExternalLink } from '../../utils/icons'

export default function FileBlock({ block }: { block: ContentBlock }) {
  const { t } = useLocale()
  const name = (block.name as string) || 'file'
  const filePath = (block.path as string) || ''
  const size = block.size as number | undefined

  const fmt = (b: number) => b < 1024 ? `${b}B` : b < 1024**2 ? `${(b/1024).toFixed(1)}KB` : `${(b/1024**2).toFixed(1)}MB`

  return (
    <div className="inline-flex items-center gap-2.5 px-3 py-2 rounded-[var(--r-md)] bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.04)] text-[12px]">
      <IconFile size={12} className="text-[var(--accent)] opacity-60 shrink-0" />
      <span className="text-[var(--text)] truncate max-w-[200px]">{name}</span>
      {size != null && <span className="text-[10px] text-[var(--text-muted)] tabular-nums">{fmt(size)}</span>}
      {filePath && (
        <button onClick={() => window.loom.openFile(filePath)}
          className="flex items-center gap-1 text-[10px] text-[var(--accent)] hover:text-[var(--accent-hover)] transition-colors">
          <IconExternalLink size={9} /> {t('common.open')}
        </button>
      )}
    </div>
  )
}

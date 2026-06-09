import type { ContentBlock } from '../../stores/chat'
import { useLocale } from '../../i18n'
import { IconZap, IconCheck, IconLoader } from '../../utils/icons'

export default function SubagentCard({ block }: { block: ContentBlock }) {
  const { t } = useLocale()
  const name = (block.name as string) || t('chat.subAgent')
  const status = (block.streamStatus as string) || 'running'
  const summary = (block.summary as string) || ''

  return (
    <div className="bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.04)] rounded-[var(--r-md)] overflow-hidden">
      <div className="flex items-center gap-2 px-3 py-2 bg-[rgba(0,227,199,0.02)]">
        <IconZap size={10} className="text-[var(--accent)]" />
        <span className="text-[11px] font-medium text-[var(--accent)]">{name}</span>
        <span className="ml-auto">
          {status === 'done' ? <IconCheck size={10} className="text-[var(--green)]" /> : <IconLoader size={10} className="text-[var(--amber)] animate-spin" />}
        </span>
      </div>
      {summary && (
        <div className="px-3 py-2.5 text-[12px] text-[var(--text-light)] border-t border-[rgba(255,255,255,0.04)] leading-relaxed">
          {summary}
        </div>
      )}
    </div>
  )
}

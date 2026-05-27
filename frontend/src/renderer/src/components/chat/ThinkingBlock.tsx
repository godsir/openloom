import { useState } from 'react'
import type { ContentBlock } from '../../stores/chat'
import { IconChevronRight, IconChevronDown } from '../../utils/icons'

export default function ThinkingBlock({ block }: { block: ContentBlock }) {
  const [expanded, setExpanded] = useState(false)
  const sealed = block.sealed as boolean
  const content = block.content as string
  const elapsed = block.elapsed as number | undefined

  return (
    <div className="bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.04)] rounded-[var(--r-md)] overflow-hidden">
      <button
        onClick={() => setExpanded(!expanded)}
        className="flex items-center gap-2 w-full px-3 py-2 text-[11px] text-[var(--text-light)] hover:bg-[rgba(255,255,255,0.02)] transition-colors"
      >
        {expanded ? <IconChevronDown size={10} /> : <IconChevronRight size={10} />}
        <span className="font-medium">思考过程</span>
        {elapsed != null && <span className="text-[var(--text-muted)] ml-1">· {elapsed}s</span>}
        {!sealed && (
          <span className="w-1.5 h-1.5 rounded-full bg-[var(--accent)] animate-pulse-dot ml-auto" />
        )}
      </button>
      {expanded && (
        <div className="px-3 py-2.5 text-[12px] text-[var(--text-light)] border-t border-[rgba(255,255,255,0.04)] whitespace-pre-wrap max-h-56 overflow-y-auto leading-relaxed">
          {content}
        </div>
      )}
    </div>
  )
}

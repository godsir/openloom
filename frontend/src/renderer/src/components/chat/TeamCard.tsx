import { useState } from 'react'
import type { ContentBlock } from '../../stores/chat'
import { useLocale } from '../../i18n'
import { IconUsers, IconCheck, IconLoader, IconX, IconChevronDown } from '../../utils/icons'

interface TeamMemberStatus {
  name: string
  status: 'running' | 'done' | 'errored'
  summary?: string
}

interface TeamCardProps {
  block: ContentBlock
}

function StatusIcon({ status }: { status: TeamMemberStatus['status'] }) {
  switch (status) {
    case 'done': return <IconCheck size={10} className="text-[var(--green)]" />
    case 'errored': return <IconX size={10} className="text-[var(--red)]" />
    default: return <IconLoader size={10} className="text-[var(--amber)] animate-spin" />
  }
}

export default function TeamCard({ block }: TeamCardProps) {
  const { t } = useLocale()
  const teamName = (block.teamName as string) || t('chat.team')
  const members = (block.members as TeamMemberStatus[]) || []
  const [expandedIdx, setExpandedIdx] = useState<number | null>(null)

  return (
    <div className="bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.04)] rounded-[var(--r-md)] overflow-hidden my-2">
      <div className="flex items-center gap-2 px-3 py-2 bg-[rgba(99,102,241,0.08)]">
        <IconUsers size={10} style={{ color: '#6366f1' }} />
        <span className="text-[11px] font-medium" style={{ color: '#6366f1' }}>{teamName}</span>
        <span className="ml-auto text-[10px] text-[var(--text-muted)]">{members.length} members</span>
      </div>
      {members.map((member, i) => (
        <div key={`${member.name}-${i}`}>
          <div
            className="flex items-center gap-2 px-3 py-2 hover:bg-[var(--bg-active)] cursor-pointer"
            onClick={() => setExpandedIdx(expandedIdx === i ? null : i)}
          >
            <span className="flex-shrink-0"><StatusIcon status={member.status} /></span>
            <span className="text-[11px] text-[var(--text-light)]">{member.name}</span>
            {member.summary && (
              <span className="ml-auto text-[10px] text-[var(--text-muted)] truncate max-w-[160px]">
                {member.summary.slice(0, 60)}{member.summary.length > 60 ? '…' : ''}
              </span>
            )}
            <IconChevronDown
              size={10}
              style={{ color: 'var(--text-muted)', transform: expandedIdx === i ? 'rotate(180deg)' : undefined, transition: 'transform 0.15s' }}
            />
          </div>
          {expandedIdx === i && member.summary && (
            <div className="px-3 pb-3 pl-8">
              <pre className="text-[10px] text-[var(--text-secondary)] font-mono whitespace-pre-wrap leading-relaxed m-0">
                {member.summary}
              </pre>
            </div>
          )}
        </div>
      ))}
    </div>
  )
}

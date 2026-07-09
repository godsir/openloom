import type { ContentBlock } from '../../stores/chat'
import { useLocale } from '../../i18n'
import { IconUsers, IconCheck, IconLoader, IconX } from '../../utils/icons'

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
    case 'done':
      return <IconCheck size={10} className="text-[var(--green)]" />
    case 'errored':
      return <IconX size={10} className="text-[var(--red)]" />
    case 'running':
    default:
      return <IconLoader size={10} className="text-[var(--amber)] animate-spin" />
  }
}

export default function TeamCard({ block }: TeamCardProps) {
  const { t } = useLocale()
  const teamName = (block.teamName as string) || t('chat.team')
  const members = (block.members as TeamMemberStatus[]) || []

  return (
    <div className="bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.04)] rounded-[var(--r-md)] overflow-hidden">
      {/* Header */}
      <div className="flex items-center gap-2 px-3 py-2 bg-[rgba(99,102,241,0.08)]">
        <IconUsers size={10} style={{ color: '#6366f1' }} />
        <span className="text-[11px] font-medium" style={{ color: '#6366f1' }}>{teamName}</span>
      </div>

      {/* Member list */}
      {members.length > 0 && (
        <div className="divide-y divide-[rgba(255,255,255,0.04)]">
          {members.map((member, i) => (
            <div key={`${member.name}-${i}`} className="flex items-center gap-2 px-3 py-2">
              <span className="flex-shrink-0">
                <StatusIcon status={member.status} />
              </span>
              <span className="text-[11px] text-[var(--text-light)]">{member.name}</span>
              {member.summary && (
                <span className="ml-auto text-[10px] text-[var(--text-muted)] truncate max-w-[140px]">
                  {member.summary}
                </span>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  )
}

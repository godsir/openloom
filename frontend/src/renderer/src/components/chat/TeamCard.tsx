import { useState } from 'react'
import type { ContentBlock } from '../../stores/chat'
import { useLocale } from '../../i18n'
import { IconUsers, IconCheck, IconLoader, IconX, IconChevronDown } from '../../utils/icons'
import styles from './TeamCard.module.css'

interface TeamMemberStatus {
  name: string
  status: 'running' | 'done' | 'errored'
  summary?: string
}

interface TeamCardProps {
  block: ContentBlock
}

function StatusIcon({ status }: { status: TeamMemberStatus['status'] }) {
  const cls = status === 'done' ? styles.iconGreen : status === 'errored' ? styles.iconRed : styles.iconAmber
  const spin = status === 'running' ? styles.spin : undefined
  switch (status) {
    case 'done': return <IconCheck size={10} className={cls} />
    case 'errored': return <IconX size={10} className={cls} />
    default: return <IconLoader size={10} className={`${cls} ${spin}`} />
  }
}

export default function TeamCard({ block }: TeamCardProps) {
  const { t } = useLocale()
  const teamName = (block.teamName as string) || t('chat.team')
  const members = (block.members as TeamMemberStatus[]) || []
  const [expandedIdx, setExpandedIdx] = useState<number | null>(null)

  return (
    <div className={styles.card}>
      <div className={styles.header}>
        <IconUsers size={10} className={styles.iconIndigo} />
        <span className={styles.teamName}>{teamName}</span>
        <span className={styles.memberCount}>{members.length} members</span>
      </div>
      {members.map((member, i) => (
        <div key={`${member.name}-${i}`}>
          <div
            className={styles.memberRow}
            onClick={() => setExpandedIdx(expandedIdx === i ? null : i)}
          >
            <span className={styles.statusIcon}><StatusIcon status={member.status} /></span>
            <span className={styles.memberName}>{member.name}</span>
            {member.summary && (
              <span className={styles.memberSummary}>
                {member.summary.slice(0, 60)}{member.summary.length > 60 ? '…' : ''}
              </span>
            )}
            <IconChevronDown
              size={10}
              className={expandedIdx === i ? styles.chevronUp : styles.chevronDown}
            />
          </div>
          {expandedIdx === i && member.summary && (
            <div className={styles.expandedBody}>
              <pre className={styles.expandedPre}>{member.summary}</pre>
            </div>
          )}
        </div>
      ))}
    </div>
  )
}

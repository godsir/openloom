import { useState } from 'react'
import type { ContentBlock } from '../../stores/chat'
import { useLocale } from '../../i18n'
import { IconZap, IconCheck, IconLoader, IconChevronDown } from '../../utils/icons'
import { renderMarkdown } from '../../utils/markdown'
import { sanitizeHtml } from '../../utils/markdown-sanitizer'
import styles from './SubagentCard.module.css'

export default function SubagentCard({ block }: { block: ContentBlock }) {
  const { t } = useLocale()
  const name = (block.name as string) || t('chat.subAgent')
  const status = (block.streamStatus as string) || 'running'
  const summary = (block.summary as string) || ''
  const body = (block.body as string) || ''
  const promptTokens = (block.promptTokens as number) || 0
  const completionTokens = (block.completionTokens as number) || 0
  const totalTokens = promptTokens + completionTokens
  const [expanded, setExpanded] = useState(true)

  function fmtTokens(n: number): string {
    if (n >= 1_000_000) return (n / 1_000_000).toFixed(1) + 'M'
    if (n >= 1_000) return (n / 1_000).toFixed(1) + 'K'
    return String(n)
  }

  const displayHtml = body
    ? sanitizeHtml(renderMarkdown(body
        .replace(/\x02[^\x02]*\x02/g, '')
        .replace(/\[思考\]/g, '')
        .replace(/(```(?:json)?\s*\n[\s\S]*?```|(?<=[\n\r]|^)\s*(\{[\s\S]{20,}\}|\[[\s\S]{20,}\])\s*(?=[\n\r]|$))/g, (m: string) => {
          if (m.startsWith('```')) return m
          try { JSON.parse(m); return '```json\n' + m + '\n```' } catch { return m }
        })
      ))
    : summary
      ? sanitizeHtml(renderMarkdown(summary))
      : ''

  return (
    <div className={styles.card}>
      <div
        onClick={() => setExpanded(!expanded)}
        className={`${styles.header} ${expanded ? styles.headerExpanded : ''}`}
      >
        <IconZap size={11} className={styles.accentColor} />
        <span className={styles.name}>{name}</span>
        {summary && (
          <span className={styles.summaryText}>
            {summary.slice(0, 60)}{summary.length > 60 ? '…' : ''}
          </span>
        )}
        <span className={styles.spacer} />
        {totalTokens > 0 && (
          <span className={styles.tokensBadge}>{fmtTokens(totalTokens)} tokens</span>
        )}
        <span>
          {status === 'done'
            ? <IconCheck size={11} className={styles.iconGreen} />
            : <IconLoader size={11} className={`${styles.iconAmber} ${styles.spin}`} />
          }
        </span>
        <IconChevronDown
          size={11}
          className={expanded ? styles.chevronUp : styles.chevronDown}
        />
      </div>
      {expanded && (displayHtml ? (
        <div className={`${styles.body} markdown-preview`} dangerouslySetInnerHTML={{ __html: displayHtml }} />
      ) : (
        <div className={styles.empty}>{t('chat.thinking')}</div>
      ))}
    </div>
  )
}

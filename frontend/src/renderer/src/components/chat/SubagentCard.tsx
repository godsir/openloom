import { useState } from 'react'
import type { ContentBlock } from '../../stores/chat'
import { useLocale } from '../../i18n'
import { IconZap, IconCheck, IconLoader, IconChevronDown } from '../../utils/icons'
import { renderMarkdown } from '../../utils/markdown'
import { sanitizeHtml } from '../../utils/markdown-sanitizer'

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
        .replace(/\x02[^\x02]*\x02/g, '')  // strip control signals
        .replace(/\[思考\]/g, '')            // strip leftover reasoning fragments
        // Syntax-highlight inline JSON blocks
        .replace(/(```(?:json)?\s*\n[\s\S]*?```|(?<=[\n\r]|^)\s*(\{[\s\S]{20,}\}|\[[\s\S]{20,}\])\s*(?=[\n\r]|$))/g, (m: string) => {
          if (m.startsWith('```')) return m
          try { JSON.parse(m); return '```json\n' + m + '\n```' } catch { return m }
        })
      ))
    : summary
      ? sanitizeHtml(renderMarkdown(summary))
      : ''

  return (
    <div style={{
      background: 'var(--bg-active)',
      border: '1px solid var(--border)',
      borderRadius: 'var(--r-md)',
      overflow: 'hidden',
      marginBottom: 8,
    }}>
      <div
        onClick={() => setExpanded(!expanded)}
        style={{
          display: 'flex', alignItems: 'center', gap: 8,
          padding: '8px 12px', cursor: 'pointer',
          borderBottom: expanded ? '1px solid var(--border)' : 'none',
        }}
      >
        <IconZap size={11} style={{ color: 'var(--accent)' }} />
        <span style={{ fontSize: 12, fontWeight: 500, color: 'var(--text)' }}>{name}</span>
        {summary && (
          <span style={{ fontSize: 10, color: 'var(--text-muted)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', maxWidth: 140, flex: 1 }}>
            {summary.slice(0, 60)}{summary.length > 60 ? '…' : ''}
          </span>
        )}
        <span style={{ flex: 1 }} />
        {totalTokens > 0 && (
          <span style={{ fontSize: 10, color: 'var(--text-muted)', padding: '1px 6px', borderRadius: 'var(--r-xs)', background: 'var(--bg-subtle)', fontWeight: 500, whiteSpace: 'nowrap' }}>
            {fmtTokens(totalTokens)} tokens
          </span>
        )}
        <span>
          {status === 'done'
            ? <IconCheck size={11} style={{ color: 'var(--green)' }} />
            : <IconLoader size={11} style={{ color: 'var(--amber)' }} className="animate-spin" />
          }
        </span>
        <IconChevronDown
          size={11}
          style={{ color: 'var(--text-muted)', transform: expanded ? 'rotate(180deg)' : undefined, transition: 'transform var(--dur-fast) var(--ease-out)' }}
        />
      </div>
      {expanded && (displayHtml ? (
        <div
          className="markdown-preview"
          style={{ padding: '10px 12px', maxHeight: 400, overflowY: 'auto', background: 'var(--bg-surface)', fontSize: 12, lineHeight: 1.7 }}
          dangerouslySetInnerHTML={{ __html: displayHtml }}
        />
      ) : (
        <div style={{ padding: '20px 12px', fontSize: 11, color: 'var(--text-muted)', textAlign: 'center' }}>
          {t('chat.thinking')}
        </div>
      ))}
    </div>
  )
}

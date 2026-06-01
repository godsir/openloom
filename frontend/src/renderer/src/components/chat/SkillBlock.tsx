import { useState, useRef, useEffect } from 'react'
import type { ContentBlock } from '../../stores/chat'
import { IconChevronRight, IconChevronDown, IconSparkles } from '../../utils/icons'
import { renderMarkdown } from '../../utils/markdown'
import { sanitizeHtml } from '../../utils/markdown-sanitizer'
import styles from './SkillBlock.module.css'

export default function SkillBlock({ block }: { block: ContentBlock }) {
  const [expanded, setExpanded] = useState(false)
  const bodyRef = useRef<HTMLDivElement>(null)
  const sealed = block.sealed as boolean
  const skillName = block.name as string
  const status = block.status as string
  const rawResult = block.result as string | undefined

  // Strip the "## Skill: {name}\n\n" prefix from use_skill result
  const content = rawResult
    ?.replace(/^## Skill: [^\n]*\n\n?/, '')  // Accept 1 or 2 newlines
    ?.replace(/^### Skill: [^\n]*\n\n?/, '')  // Also handle ### variant
    || rawResult  // Fallback: if no header matched, use raw result
    || ''
  const renderedHtml = content ? sanitizeHtml(renderMarkdown(content)) : ''

  // Auto-scroll when streaming
  useEffect(() => {
    if (expanded && bodyRef.current) {
      bodyRef.current.scrollTop = bodyRef.current.scrollHeight
    }
  }, [content, expanded])

  // Auto-expand when skill starts loading
  useEffect(() => {
    if (status === 'running') setExpanded(true)
  }, [status])

  return (
    <div className={styles.block}>
      <button
        onClick={() => setExpanded(!expanded)}
        className={styles.toggle}
      >
        {expanded ? <IconChevronDown size={10} /> : <IconChevronRight size={10} />}
        <IconSparkles size={11} className={styles.icon} />
        <span className={styles.label}>Skill: {skillName}</span>
        {!sealed && <span className={styles.dot} />}
      </button>
      {expanded && (
        <div ref={bodyRef} className={styles.body}>
          {status === 'running' && !content && (
            <span className={styles.loading}>Loading skill...</span>
          )}
          {renderedHtml && (
            <div dangerouslySetInnerHTML={{ __html: renderedHtml }} />
          )}
        </div>
      )}
    </div>
  )
}

import { useState, useRef, useEffect } from 'react'
import type { ContentBlock } from '../../stores/chat'
import { IconChevronRight, IconChevronDown, IconBrain } from '../../utils/icons'
import styles from './ThinkingBlock.module.css'

export default function ThinkingBlock({ block }: { block: ContentBlock }) {
  const [expanded, setExpanded] = useState(false)
  const bodyRef = useRef<HTMLDivElement>(null)
  const sealed = block.sealed as boolean
  const content = block.content as string
  const elapsed = block.elapsed as number | undefined

  // Load default expand preference
  useEffect(() => {
    window.loom.getPreference('thinkingExpandDefault', false).then(v => setExpanded(v))
  }, [])

  // Auto-scroll thinking body to bottom when content grows during streaming
  useEffect(() => {
    if (expanded && bodyRef.current) {
      bodyRef.current.scrollTop = bodyRef.current.scrollHeight
    }
  }, [content, expanded])

  return (
    <div className={styles.block}>
      <button
        onClick={() => setExpanded(!expanded)}
        className={styles.toggle}
      >
        {expanded ? <IconChevronDown size={10} /> : <IconChevronRight size={10} />}
        <IconBrain size={12} className={styles.icon} />
        <span className={styles.label}>思考过程</span>
        {elapsed != null && <span className={styles.label}>· {elapsed}s</span>}
        {!sealed && <span className={styles.dot} />}
      </button>
      {expanded && (
        <div ref={bodyRef} className={styles.body}>{content}</div>
      )}
    </div>
  )
}

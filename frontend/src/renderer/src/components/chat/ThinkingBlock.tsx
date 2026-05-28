import { useState } from 'react'
import type { ContentBlock } from '../../stores/chat'
import { IconChevronRight, IconChevronDown } from '../../utils/icons'
import styles from './ThinkingBlock.module.css'

export default function ThinkingBlock({ block }: { block: ContentBlock }) {
  const [expanded, setExpanded] = useState(false)
  const sealed = block.sealed as boolean
  const content = block.content as string
  const elapsed = block.elapsed as number | undefined

  return (
    <div className={styles.block}>
      <button
        onClick={() => setExpanded(!expanded)}
        className={styles.toggle}
      >
        {expanded ? <IconChevronDown size={10} /> : <IconChevronRight size={10} />}
        <span className={styles.label}>思考过程</span>
        {elapsed != null && <span className={styles.label}>· {elapsed}s</span>}
        {!sealed && <span className={styles.dot} />}
      </button>
      {expanded && (
        <div className={styles.body}>{content}</div>
      )}
    </div>
  )
}

import { useState } from 'react'
import type { ContentBlock } from '../../stores/chat'
import { IconZap, IconCheck, IconLoader, IconXCircle, IconChevronRight, IconChevronDown } from '../../utils/icons'
import styles from './ToolGroupBlock.module.css'

interface ToolCall {
  id: string; name: string; status: 'running' | 'done' | 'error'
  elapsed: number; args: Record<string, unknown>; result?: string
}

function toolStatusIcon(s: string) {
  if (s === 'done') return <IconCheck size={10} className={styles.iconZap} />
  if (s === 'running') return <IconLoader size={10} className={styles.iconZap} />
  return <IconXCircle size={10} className={styles.iconZap} />
}

export default function ToolGroupBlock({ block }: { block: ContentBlock }) {
  const [expandedId, setExpandedId] = useState<string | null>(null)
  const tools = (block.tools as ToolCall[]) || []

  return (
    <div className={styles.block}>
      {tools.map((tool, idx) => (
        <div key={tool.id} className={styles.row}>
          <button
            onClick={() => setExpandedId(expandedId === tool.id ? null : tool.id)}
            className={styles.toggle}
          >
            <IconZap size={10} className={styles.iconZap} />
            <span className={styles.toolName}>{tool.name}</span>
            <span className={styles.statusIcon}>{toolStatusIcon(tool.status)}</span>
            {expandedId !== tool.id
              ? <IconChevronRight size={9} className={styles.chevron} />
              : <IconChevronDown size={9} className={styles.chevron} />}
          </button>
          {expandedId === tool.id && (
            <div className={styles.body}>
              {Object.keys(tool.args).length > 0 && (
                <pre className={styles.args}>
                  {JSON.stringify(tool.args, null, 2)}
                </pre>
              )}
              {tool.result && (
                <pre className={styles.result}>
                  {tool.result}
                </pre>
              )}
            </div>
          )}
        </div>
      ))}
    </div>
  )
}

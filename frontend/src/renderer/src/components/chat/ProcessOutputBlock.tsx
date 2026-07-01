import { useState, useRef, useEffect } from 'react'
import styles from './ShellBlock.module.css'

interface ProcessLine {
  stream: string
  text: string
}

export default function ProcessOutputBlock({ block }: { block: { pid?: string; lines?: ProcessLine[]; sealed?: boolean } }) {
  const [expanded, setExpanded] = useState(true)
  const bodyRef = useRef<HTMLDivElement>(null)
  const pid = (block.pid as string) || '?'
  const lines = (block.lines as ProcessLine[]) || []
  const sealed = block.sealed as boolean

  useEffect(() => {
    window.loom.getPreference('toolExpandDefault', true).then(v => setExpanded(v))
  }, [])

  useEffect(() => {
    if (expanded && bodyRef.current) {
      bodyRef.current.scrollTop = bodyRef.current.scrollHeight
    }
  }, [lines, expanded])

  return (
    <div className={styles.block}>
      <button
        onClick={() => setExpanded(!expanded)}
        className={styles.toggle}
      >
        <span className={styles.label}>PID:{pid.slice(0, 8)}</span>
        <span className={styles.detail}>{lines.length} lines</span>
        {!sealed && <span className={styles.dot} />}
      </button>
      {expanded && (
        <div ref={bodyRef} className={styles.body}>
          <pre className={styles.output}>
            {lines.map((l, i) => {
              const cls = l.stream === 'stderr' ? styles.stderrLine :
                l.stream === 'system' ? styles.systemLine :
                styles.stdoutLine
              return (
                <div key={i} className={cls}>{l.text}</div>
              )
            })}
            {!sealed && lines.length === 0 && (
              <span className={styles.cursor} />
            )}
          </pre>
        </div>
      )}
    </div>
  )
}

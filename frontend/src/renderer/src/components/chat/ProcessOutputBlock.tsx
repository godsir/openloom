import { useState, useRef, useEffect, useMemo } from 'react'
import styles from './ShellBlock.module.css'

interface ProcessLine {
  stream: string
  text: string
}

interface GameStateBadge {
  phase: string
  name: string
  alive: number
}

function tryParseJson(text: string): { obj: any; ok: true } | { ok: false } {
  try {
    const trimmed = text.trim()
    if (!trimmed.startsWith('{')) return { ok: false }
    return { obj: JSON.parse(trimmed), ok: true }
  } catch {
    return { ok: false }
  }
}

function extractBadge(obj: any): GameStateBadge | null {
  if (!obj || typeof obj !== 'object') return null
  const state = obj.state
  if (typeof state !== 'string') return null
  const phase = state.split(';')[0] || state
  const name = (obj.you || obj.summary?.you?.name) ?? null
  const alive = typeof obj.alive === 'number' ? obj.alive
    : typeof obj.summary?.alive === 'number' ? obj.summary.alive
    : null
  if (name || alive != null) return { phase, name: name ?? '?', alive: alive ?? 0 }
  return null
}

/** Render a single clawclaw JSON line as structured content. */
function ClawLine({ obj }: { obj: any }) {
  if (!obj || typeof obj !== 'object') return <span>{JSON.stringify(obj)}</span>

  const msgs: string[] = Array.isArray(obj.messages) ? obj.messages : []
  const events: string[] = Array.isArray(obj.events) ? obj.events : []
  const nextStep: string = obj.next_step || ''
  const state: string = obj.state || ''
  const speaker: string = obj.speaker || ''
  const exitReason: string[] = Array.isArray(obj.exit_reason) ? obj.exit_reason : []

  return (
    <div className={styles.clawLine}>
      {/* State + speaker bar */}
      {(state || speaker) && (
        <div className={styles.clawHeader}>
          {speaker && <span className={styles.clawSpeaker}>{speaker} 发言</span>}
          {state && <span className={styles.clawState}>{state}</span>}
        </div>
      )}

      {/* Messages — primary content */}
      {msgs.map((m, i) => (
        <div key={i} className={styles.clawMsg}>
          {m}
        </div>
      ))}

      {/* Next step — highlighted call to action */}
      {nextStep && <div className={styles.clawNext}>{nextStep}</div>}

      {/* Events — pill tags */}
      {events.length > 0 && (
        <div className={styles.clawTags}>
          {events.map((e, i) => (
            <span key={i} className={styles.clawTag}>{e}</span>
          ))}
        </div>
      )}

      {/* Exit reason */}
      {exitReason.length > 0 && (
        <div className={styles.clawExit}>Exit: {exitReason.join(', ')}</div>
      )}

      {/* Fallback: unknown JSON structure */}
      {msgs.length === 0 && !nextStep && events.length === 0 && exitReason.length === 0 && !state && (
        <div className={styles.clawRaw}>
          {JSON.stringify(obj, null, 2)}
        </div>
      )}
    </div>
  )
}

export default function ProcessOutputBlock({ block }: {
  block: { pid?: string; lines?: ProcessLine[]; sealed?: boolean }
}) {
  const [expanded, setExpanded] = useState(true)
  const bodyRef = useRef<HTMLDivElement>(null)
  const pid = (block.pid as string) || '?'
  const lines = (block.lines as ProcessLine[]) || []
  const sealed = block.sealed as boolean

  const parsedLines = useMemo(() =>
    lines.map(l => {
      const parsed = tryParseJson(l.text)
      return { parsed: parsed.ok ? parsed.obj : null, text: l.text }
    }), [lines])

  const badge = useMemo(() => {
    for (let i = parsedLines.length - 1; i >= 0; i--) {
      const b = extractBadge(parsedLines[i].parsed)
      if (b) return b
    }
    return null
  }, [parsedLines])

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
      <button onClick={() => setExpanded(!expanded)} className={styles.toggle}>
        <span className={styles.label}>PID:{pid.slice(0, 8)}</span>
        {badge && (
          <span className={styles.detail}>
            {badge.name} · {badge.phase} · 存活 {badge.alive}
          </span>
        )}
        {!badge && <span className={styles.detail}>{lines.length} lines</span>}
        {!sealed && <span className={styles.dot} />}
      </button>
      {expanded && (
        <div ref={bodyRef} className={styles.body}>
          <div className={styles.clawOutput}>
            {parsedLines.map((l, i) =>
              l.parsed
                ? <ClawLine key={i} obj={l.parsed} />
                : <div key={i} className={styles.stdoutLine}>{l.text}</div>
            )}
          </div>
        </div>
      )}
    </div>
  )
}

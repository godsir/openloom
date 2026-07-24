import { useState, useRef, useEffect, useMemo, useCallback } from 'react'
import type { ContentBlock } from '../../stores/chat'
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

      {/* Messages */}
      {msgs.map((m, i) => (
        <div key={i} className={styles.clawMsg}>
          {typeof m === 'string' ? m : JSON.stringify(m)}
        </div>
      ))}
      {/* Next step */}
      {nextStep && <div className={styles.clawNext}>{typeof nextStep === 'string' ? nextStep : JSON.stringify(nextStep)}</div>}
      {/* Events */}
      {events.length > 0 && (
        <div className={styles.clawTags}>
          {events.map((e, i) => (
            <span key={i} className={styles.clawTag}>{typeof e === 'string' ? e : JSON.stringify(e)}</span>
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

export default function ProcessOutputBlock({ block }: { block: ContentBlock }) {
  const [expanded, setExpanded] = useState(true)
  const bodyRef = useRef<HTMLDivElement>(null)
  const pid = (block.pid as string) || '?'
  const lines = (block.lines as ProcessLine[]) || []
  const sealed = block.sealed as boolean
  // 终态徽章：进程崩溃(exit≠0)红色 / 正常退出(exit 0)弱化。此前退出码被丢弃，
  // 崩溃与正常退出视觉上无区别。
  const exitCode = block.exitCode as number | null | undefined
  const failed = sealed && exitCode != null && exitCode !== 0

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
    window.loom.getPreference('toolExpandDefault', true).then((v: boolean) => setExpanded(v))
    const handler = (e: Event) => {
      const d = (e as CustomEvent).detail
      if (d?.key === 'tool_expand') setExpanded(d.val)
    }
    window.addEventListener('loom-pref-changed', handler)
    return () => window.removeEventListener('loom-pref-changed', handler)
  }, [])

  // 智能自动滚动：仅在用户未上翻时跟随新输出（与 ShellBlock 同语义）。
  // 长驻监控进程持续输出时，用户需要能自由上翻阅读早期日志而不被拽回。
  const userScrolledUp = useRef(false)
  const handleScroll = useCallback(() => {
    if (!bodyRef.current) return
    const el = bodyRef.current
    userScrolledUp.current = el.scrollHeight - el.scrollTop - el.clientHeight > 40
  }, [])

  useEffect(() => {
    if (expanded && bodyRef.current && !userScrolledUp.current) {
      bodyRef.current.scrollTop = bodyRef.current.scrollHeight
    }
  }, [lines, expanded])

  return (
    <div className={styles.block}>
      <button onClick={() => setExpanded(!expanded)} className={styles.toggle}>
        <span className={`${styles.label} ${failed ? styles.labelFailed : ''}`}>PID:{pid.slice(0, 8)}</span>
        {badge && (
          <span className={styles.detail}>
            {badge.name} · {badge.phase} · 存活 {badge.alive}
          </span>
        )}
        {!badge && <span className={styles.detail}>{lines.length} lines</span>}
        {!sealed && <span className={styles.dot} />}
        {failed && <span className={styles.exitBadgeFail}>exit {exitCode}</span>}
        {!failed && sealed && exitCode === 0 && <span className={styles.exitBadgeOk}>exit 0</span>}
      </button>
      {expanded && (
        <div ref={bodyRef} onScroll={handleScroll} className={styles.body}>
          <div className={styles.clawOutput}>
            {parsedLines.map((l, i) =>
              l.parsed
                ? <ClawLine key={i} obj={l.parsed} />
                : <div key={i} className={styles.stdoutLine}>{typeof l.text === 'string' ? l.text : JSON.stringify(l.text)}</div>
            )}
          </div>
        </div>
      )}
    </div>
  )
}

import { useState, useEffect, useCallback, useRef, memo } from 'react'
import type { TimelineAnchor } from './timeline-anchors'
import styles from './ChatTimelineNavigator.module.css'

interface LayoutEntry {
  targetTop: number
}

interface Props {
  anchors: TimelineAnchor[]
  scrollRef: React.RefObject<HTMLDivElement | null>
}

const ChatTimelineNavigator = memo(function ChatTimelineNavigator({ anchors, scrollRef }: Props) {
  const [layouts, setLayouts] = useState<Record<string, LayoutEntry>>({})
  const [activeId, setActiveId] = useState<string | null>(null)
  const rafRef = useRef<number>(0)

  // Measure DOM positions of all anchored messages
  const measure = useCallback(() => {
    const panel = scrollRef.current
    if (!panel) return
    const panelRect = panel.getBoundingClientRect()
    const next: Record<string, LayoutEntry> = {}
    for (const a of anchors) {
      const el = panel.querySelector(`[data-message-id="${a.messageId}"]`) as HTMLElement | null
      if (!el) continue
      const rect = el.getBoundingClientRect()
      const targetTop = panel.scrollTop + rect.top - panelRect.top - 12
      next[a.messageId] = { targetTop: Math.max(0, targetTop) }
    }
    setLayouts(next)
  }, [anchors, scrollRef])

  // Re-measure on anchors change and resize
  useEffect(() => {
    measure()
    const panel = scrollRef.current
    if (!panel) return
    const ro = new ResizeObserver(() => measure())
    ro.observe(panel)
    return () => ro.disconnect()
  }, [measure, scrollRef])

  // Track which marker is active based on scroll position
  const updateActive = useCallback(() => {
    const panel = scrollRef.current
    if (!panel) return
    const st = panel.scrollTop + 96
    let active: string | null = null
    for (const a of anchors) {
      const layout = layouts[a.messageId]
      if (layout && layout.targetTop <= st) {
        active = a.messageId
      }
    }
    setActiveId(active)
  }, [anchors, layouts, scrollRef])

  useEffect(() => {
    const panel = scrollRef.current
    if (!panel) return
    const onScroll = () => {
      cancelAnimationFrame(rafRef.current)
      rafRef.current = requestAnimationFrame(updateActive)
    }
    panel.addEventListener('scroll', onScroll, { passive: true })
    return () => {
      panel.removeEventListener('scroll', onScroll)
      cancelAnimationFrame(rafRef.current)
    }
  }, [updateActive, scrollRef])

  // Update active on initial layout measurement
  useEffect(() => {
    updateActive()
  }, [layouts, updateActive])

  // Jump to a message
  const jumpTo = useCallback((anchor: TimelineAnchor) => {
    const panel = scrollRef.current
    const layout = layouts[anchor.messageId]
    if (!panel || !layout) return
    panel.scrollTo({ top: layout.targetTop, behavior: 'smooth' })
  }, [layouts, scrollRef])

  if (anchors.length === 0) return null

  return (
    <nav className={styles.nav} aria-label="对话跳转导航">
      <div className={styles.card}>
        {anchors.map(a => (
          <button
            key={a.messageId}
            className={`${styles.marker} ${a.messageId === activeId ? styles.markerActive : ''}`}
            onClick={() => jumpTo(a)}
            aria-label={`跳转到 ${a.label}`}
          >
            <span className={styles.label}>{a.label}</span>
            <span
              className={styles.line}
              style={{ width: `${a.markerWidthEm}em` }}
            />
          </button>
        ))}
      </div>
    </nav>
  )
})

export default ChatTimelineNavigator

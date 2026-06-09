import { useState, useEffect, useCallback, memo } from 'react'
import { useLocale } from '../../i18n'
import type { TimelineAnchor } from './timeline-anchors'
import styles from './ChatTimelineNavigator.module.css'

interface Props {
  anchors: TimelineAnchor[]
  scrollRef: React.RefObject<HTMLDivElement | null>
  /** Callback to pause auto-scroll when user manually navigates */
  onManualNavigate?: () => void
}

const ChatTimelineNavigator = memo(function ChatTimelineNavigator({
  anchors,
  scrollRef,
  onManualNavigate,
}: Props) {
  const { t } = useLocale()
  const [activeId, setActiveId] = useState<string | null>(null)

  // Track visible anchor based on scroll position (which anchor is above the viewport top)
  useEffect(() => {
    const el = scrollRef.current
    if (!el || anchors.length === 0) return

    const findActive = () => {
      const scrollTop = el.scrollTop
      let active: string | null = null
      for (const a of anchors) {
        const msgEl = el.querySelector(`[data-message-id="${a.messageId}"]`)
        if (!msgEl) continue
        const rect = msgEl.getBoundingClientRect()
        const containerRect = el.getBoundingClientRect()
        // Active if the message top is at or above the container top
        if (rect.top - containerRect.top <= 60) {
          active = a.messageId
        }
      }
      if (active !== activeId) setActiveId(active)
    }

    findActive()
    const onScroll = () => findActive()
    el.addEventListener('scroll', onScroll, { passive: true })
    return () => el.removeEventListener('scroll', onScroll)
  }, [anchors, scrollRef, activeId])

  // Jump to a message
  // Smooth-scroll to a message using rAF animation (scrollIntoView unreliable in child containers)
  const jumpTo = useCallback(
    (anchor: TimelineAnchor) => {
      const container = scrollRef.current
      if (!container) return
      const msgEl = container.querySelector(`[data-message-id="${anchor.messageId}"]`) as HTMLElement | null
      if (!msgEl) return
      onManualNavigate?.()

      const start = container.scrollTop
      // msgEl.offsetTop is relative to .chatScroll (now position:relative), subtract 28px padding
      const target = msgEl.offsetTop - 28
      if (Math.abs(target - start) < 4) return
      const duration = 420
      const startTime = performance.now()

      const animate = (now: number) => {
        const t = Math.min((now - startTime) / duration, 1)
        const eased = t < 0.5 ? 4 * t * t * t : 1 - Math.pow(-2 * t + 2, 3) / 2
        container.scrollTop = start + (target - start) * eased
        if (t < 1) requestAnimationFrame(animate)
      }
      requestAnimationFrame(animate)
    },
    [scrollRef, onManualNavigate],
  )

  if (anchors.length === 0) return null

  return (
    <nav className={styles.nav} aria-label={t('chat.timelineNav')}>
      <div className={styles.card}>
        {anchors.map(a => (
          <button
            key={a.messageId}
            className={`${styles.marker} ${a.messageId === activeId ? styles.markerActive : ''}`}
            onClick={() => jumpTo(a)}
            aria-label={t('chat.jumpTo', { label: a.label })}
          >
            <span className={styles.label}>{a.label}</span>
            <span className={styles.line} style={{ width: `${a.markerWidthEm}em` }} />
          </button>
        ))}
      </div>
    </nav>
  )
})

export default ChatTimelineNavigator

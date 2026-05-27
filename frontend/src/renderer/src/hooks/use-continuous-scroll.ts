import { useRef, useEffect, useCallback } from 'react'

// Auto-scroll with exponential easing, sticky detection, and ResizeObserver.
export function useContinuousBottomScroll(options?: {
  enabled?: boolean
  reducedMotion?: boolean
}) {
  const { enabled = true, reducedMotion = false } = options || {}
  const scrollRef = useRef<HTMLDivElement>(null)
  const stickyRef = useRef(true)

  const scrollToBottom = useCallback(
    (smooth = true) => {
      const el = scrollRef.current
      if (!el || !enabled) return

      if (reducedMotion || !smooth) {
        el.scrollTop = el.scrollHeight
      } else {
        el.scrollTo({ top: el.scrollHeight, behavior: 'smooth' })
      }
    },
    [enabled, reducedMotion],
  )

  useEffect(() => {
    const el = scrollRef.current
    if (!el) return

    const handleScroll = () => {
      if (!el) return
      const threshold = 80
      const distanceFromBottom = el.scrollHeight - el.scrollTop - el.clientHeight
      stickyRef.current = distanceFromBottom <= threshold
    }

    el.addEventListener('scroll', handleScroll, { passive: true })
    return () => el.removeEventListener('scroll', handleScroll)
  }, [])

  return { scrollRef, stickyRef, scrollToBottom }
}

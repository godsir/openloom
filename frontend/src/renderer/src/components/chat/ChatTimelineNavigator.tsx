import { useState, useEffect, useCallback, useRef, memo } from 'react'
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

  // Track visible anchor based on scroll position (which anchor is above the viewport top).
  // 优化：① rAF 节流，避免每个 scroll 事件都对全部锚点做 querySelector +
  // getBoundingClientRect 强制布局；② 缓存锚点元素引用（脱离则重查）；③ 用
  // 函数式 setState 比较，activeId 不再进依赖数组（此前 activeId 一变就卸载
  // 重绑 scroll 监听，形成"滚动→高亮变→重绑"的自激循环）。
  useEffect(() => {
    const el = scrollRef.current
    if (!el || anchors.length === 0) return

    let rafId: number | null = null
    const elCache = new Map<string, Element>()
    const getMsgEl = (id: string): Element | null => {
      let node: Element | null | undefined = elCache.get(id)
      if (!node || !node.isConnected) {
        node = el.querySelector(`[data-message-id="${id}"]`)
        if (node) elCache.set(id, node)
      }
      return node ?? null
    }

    const findActive = () => {
      rafId = null
      const containerTop = el.getBoundingClientRect().top
      let active: string | null = null
      for (const a of anchors) {
        const msgEl = getMsgEl(a.messageId)
        if (!msgEl) continue
        if (msgEl.getBoundingClientRect().top - containerTop <= 60) {
          active = a.messageId
        }
      }
      setActiveId(prev => (prev === active ? prev : active))
    }

    const onScroll = () => {
      if (rafId !== null) return // 已有待执行的 rAF，节流
      rafId = requestAnimationFrame(findActive)
    }

    findActive()
    el.addEventListener('scroll', onScroll, { passive: true })
    return () => {
      el.removeEventListener('scroll', onScroll)
      if (rafId !== null) cancelAnimationFrame(rafId)
    }
  }, [anchors, scrollRef])

  // Jump to a message
  // Smooth-scroll to a message using rAF animation (scrollIntoView unreliable in child containers)
  const animRef = useRef<number | null>(null)
  useEffect(() => () => {
    if (animRef.current !== null) cancelAnimationFrame(animRef.current)
  }, [])

  const jumpTo = useCallback(
    (anchor: TimelineAnchor) => {
      const container = scrollRef.current
      if (!container) return
      const msgEl = container.querySelector(`[data-message-id="${anchor.messageId}"]`) as HTMLElement | null
      if (!msgEl) return

      const start = container.scrollTop
      // msgEl.offsetTop is relative to .chatScroll (now position:relative), subtract 28px padding
      const target = msgEl.offsetTop - 28
      // 提前 return 上移到 onManualNavigate 之前：点击距当前位置很近的锚点
      // 不应关闭自动滚动（此前"点了等于没点"且流式不再跟随）
      if (Math.abs(target - start) < 4) return
      onManualNavigate?.()

      // 取消上一次未完成的跳转动画，避免快速连点时多条动画争抢 scrollTop 抖动
      if (animRef.current !== null) cancelAnimationFrame(animRef.current)
      const duration = 420
      const startTime = performance.now()

      const animate = (now: number) => {
        const t = Math.min((now - startTime) / duration, 1)
        const eased = t < 0.5 ? 4 * t * t * t : 1 - Math.pow(-2 * t + 2, 3) / 2
        container.scrollTop = start + (target - start) * eased
        animRef.current = t < 1 ? requestAnimationFrame(animate) : null
      }
      animRef.current = requestAnimationFrame(animate)
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

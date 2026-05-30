import { useStore } from '../../stores'
import { useRef, useEffect, useLayoutEffect, useMemo } from 'react'
import AssistantMessage from './AssistantMessage'
import UserMessage from './UserMessage'
import ImageLightbox from '../shared/ImageLightbox'
import ChatTimelineNavigator from './ChatTimelineNavigator'
import { buildTimelineAnchors } from './timeline-anchors'
import styles from './ChatArea.module.css'

const EMPTY: never[] = []

export default function ChatArea() {
  const sessionId = useStore(s => s.currentSessionId)
  const messagesBySession = useStore(s => s.messagesBySession)
  const messages = sessionId ? (messagesBySession.get(sessionId) ?? EMPTY) : EMPTY
  const streamingIds = useStore(s => s.streamingSessionIds)
  const isStreaming = sessionId ? streamingIds.has(sessionId) : false
  const inlineErrors = useStore(s => s.inlineErrors)
  const error = sessionId ? inlineErrors.get(sessionId)?.text : null
  const scrollRef = useRef<HTMLDivElement>(null)
  const timelineAnchors = useMemo(() => buildTimelineAnchors(messages as any[]), [messages])
  const lightboxSrc = useStore(s => s.lightbox.lightboxSrc)
  const openLightbox = useStore(s => s.openLightbox)
  const closeLightbox = useStore(s => s.closeLightbox)

  useLayoutEffect(() => {
    const el = scrollRef.current
    if (el) el.scrollTop = el.scrollHeight
  }, [messages.length, isStreaming])

  // Keep scrolled to bottom as content grows during streaming
  useEffect(() => {
    if (!isStreaming) return
    const el = scrollRef.current
    if (!el) return
    const observer = new ResizeObserver(() => {
      el.scrollTop = el.scrollHeight
    })
    observer.observe(el)
    return () => observer.disconnect()
  }, [isStreaming])

  useEffect(() => {
    // Use document-level delegation so we don't depend on scrollRef being
    // mounted at the time this effect runs (the empty-state branch returns
    // early before scrollRef is attached).
    const docHandler = (e: MouseEvent) => {
      const target = e.target as HTMLElement
      if (!target || target.tagName !== 'IMG') return
      // Only react to images inside the chat area
      const inChat = target.closest('[class*="chatScroll"]') || target.closest('[class*="message"]')
      if (!inChat) return
      const img = target as HTMLImageElement
      const blocked = img.getAttribute('data-blocked-src')
      if (blocked) {
        img.src = blocked
        img.removeAttribute('data-blocked-src')
        img.classList.remove('blocked-image')
        img.removeAttribute('title')
        return
      }
      if (img.src) openLightbox(img.src)
    }
    document.addEventListener('click', docHandler, true)
    return () => document.removeEventListener('click', docHandler, true)
  }, [openLightbox])

  return (
    <div className={styles.chatWrapper}>
      {messages.length === 0 && !isStreaming ? (
        <div className={styles.emptyState}>
          <div className={styles.emptyContent}>
            <div className={styles.emptyLogo}>
              <span className={styles.emptyLogoText}>L</span>
            </div>
            <p className={styles.emptyHint}>发送消息开始对话</p>
          </div>
        </div>
      ) : (
        <div ref={scrollRef} className={styles.chatScroll}>
          <div className={styles.messageList}>
            {messages.map(msg =>
              msg.role === 'user'
                ? <UserMessage key={msg.id} message={msg} />
                : <AssistantMessage key={msg.id} message={msg} sessionId={sessionId} />
            )}

            {error && (
              <div className={styles.errorBlock}>
                <span className={styles.errorIcon}>!</span>
                <span>{error}</span>
              </div>
            )}
          </div>
        </div>
      )}
      <ChatTimelineNavigator
        anchors={timelineAnchors}
        scrollRef={scrollRef as React.RefObject<HTMLDivElement | null>}
      />
      <ImageLightbox src={lightboxSrc} onClose={closeLightbox} />
    </div>
  )
}

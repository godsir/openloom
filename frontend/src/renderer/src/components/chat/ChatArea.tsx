import { useStore } from '../../stores'
import { useRef, useEffect, useMemo, useCallback, useState } from 'react'
import AssistantMessage from './AssistantMessage'
import UserMessage from './UserMessage'
import ImageLightbox from '../shared/ImageLightbox'
import ChatTimelineNavigator from './ChatTimelineNavigator'
import { buildTimelineAnchors } from './timeline-anchors'
import { IconChevronDown } from '../../utils/icons'
import styles from './ChatArea.module.css'

const EMPTY: never[] = []

export default function ChatArea() {
  const sessionId = useStore(s => s.currentSessionId)
  // Use a stable selector: only trigger re-render when the array reference or
  // its element references actually change (shallow compare).
  const messagesBySession = useStore(s => s.messagesBySession)
  const messages: any[] = useMemo(() => {
    return sessionId ? (messagesBySession.get(sessionId) ?? EMPTY) : EMPTY
  }, [sessionId, messagesBySession])
  const streamingIds = useStore(s => s.streamingSessionIds)
  const isStreaming = sessionId ? streamingIds.has(sessionId) : false
  const inlineErrors = useStore(s => s.inlineErrors)
  const error = sessionId ? inlineErrors.get(sessionId)?.text : null
  const scrollRef = useRef<HTMLDivElement>(null)
  const autoScrollRef = useRef(true)
  const [showScrollBtn, setShowScrollBtn] = useState(false)

  // Track message count for efficient auto-scroll (avoids full array dep)
  const msgCount = messages.length
  const lastMsgBlocksLen = messages.length > 0 ? messages[messages.length - 1].blocks?.length ?? 0 : 0

  const timelineAnchors = useMemo(() => buildTimelineAnchors(messages as any[]), [msgCount])
  const lightboxSrc = useStore(s => s.lightbox.lightboxSrc)
  const openLightbox = useStore(s => s.openLightbox)
  const closeLightbox = useStore(s => s.closeLightbox)

  // Auto-scroll to bottom on new messages when at bottom
  useEffect(() => {
    if (!autoScrollRef.current || !scrollRef.current) return
    scrollRef.current.scrollTop = scrollRef.current.scrollHeight
  }, [msgCount, lastMsgBlocksLen])

  // Reset auto-scroll flag on session switch
  useEffect(() => {
    autoScrollRef.current = true
    setShowScrollBtn(false)
  }, [sessionId])

  const handleScroll = useCallback(() => {
    const el = scrollRef.current
    if (!el) return
    const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 80
    autoScrollRef.current = atBottom
    setShowScrollBtn(!atBottom)
  }, [])

  const scrollToBottom = useCallback(() => {
    const el = scrollRef.current
    if (!el) return
    autoScrollRef.current = true
    setShowScrollBtn(false)
    el.scrollTo({ top: el.scrollHeight, behavior: 'smooth' })
  }, [])

  // Lightbox click handler — only opens for successfully loaded images
  useEffect(() => {
    const docHandler = (e: MouseEvent) => {
      const target = e.target as HTMLElement
      if (!target || target.tagName !== 'IMG') return
      // Skip clicks inside the lightbox overlay (prevent close → reopen loop)
      if (target.closest('[class*="overlay"]')) return
      const inChat = target.closest('[class*="chatWrapper"]') || target.closest('[class*="message"]')
      if (!inChat) return
      const img = target as HTMLImageElement
      if (img.naturalWidth < 20 || img.naturalHeight < 20) return
      const blocked = img.getAttribute('data-blocked-src')
      if (blocked) {
        img.src = blocked
        img.removeAttribute('data-blocked-src')
        img.classList.remove('blocked-image')
        img.removeAttribute('title')
        return
      }
      if (img.complete && img.naturalWidth > 0 && img.src) {
        openLightbox(img.src)
      }
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
        <div className={styles.chatScroll} ref={scrollRef} onScroll={handleScroll}>
          {messages.map((msg, idx) => (
            <div key={msg.id} className={styles.messageGap} data-message-id={msg.id}>
              {msg.role === 'user'
                ? <UserMessage message={msg} />
                : <AssistantMessage
                    message={msg}
                    sessionId={sessionId}
                    isStreaming={isStreaming}
                    isStreamingActive={isStreaming && idx === messages.length - 1}
                  />
              }
            </div>
          ))}
          {error && (
            <div className={styles.errorBlock}>
              <span className={styles.errorIcon}>!</span>
              <span>{error}</span>
            </div>
          )}
        </div>
      )}
      <ChatTimelineNavigator anchors={timelineAnchors} scrollRef={scrollRef} onManualNavigate={() => { autoScrollRef.current = false }} />
      {showScrollBtn && messages.length > 0 && (
        <button className={styles.scrollToBottom} onClick={scrollToBottom} title="回到底部">
          <IconChevronDown size={16} />
        </button>
      )}
      <ImageLightbox src={lightboxSrc} onClose={closeLightbox} />
    </div>
  )
}

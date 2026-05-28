import { useStore } from '../../stores'
import { useRef, useEffect, useState, useCallback } from 'react'
import AssistantMessage from './AssistantMessage'
import UserMessage from './UserMessage'
import ImageLightbox from '../shared/ImageLightbox'
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
  const [lightboxSrc, setLightboxSrc] = useState<string | null>(null)

  useEffect(() => {
    const el = scrollRef.current
    if (el) el.scrollTop = el.scrollHeight
  }, [messages[messages.length - 1]?.id, isStreaming])

  const handleImageClick = useCallback((e: MouseEvent) => {
    const target = e.target as HTMLElement
    if (target.tagName === 'IMG') {
      const src = (target as HTMLImageElement).src
      if (src) setLightboxSrc(src)
    }
  }, [])

  useEffect(() => {
    const el = scrollRef.current
    if (!el) return
    el.addEventListener('click', handleImageClick)
    return () => el.removeEventListener('click', handleImageClick)
  }, [handleImageClick])

  if (messages.length === 0 && !isStreaming) {
    return (
      <div className={styles.emptyState}>
        <div className={styles.emptyContent}>
          <div className={styles.emptyLogo}>
            <span className={styles.emptyLogoText}>L</span>
          </div>
          <p className={styles.emptyHint}>发送消息开始对话</p>
        </div>
      </div>
    )
  }

  return (
    <div ref={scrollRef} className={styles.chatScroll}>
      <div className={styles.messageList}>
        {messages.map(msg =>
          msg.role === 'user'
            ? <UserMessage key={msg.id} message={msg} />
            : <AssistantMessage key={msg.id} message={msg} />
        )}

        {error && (
          <div className={styles.errorBlock}>
            <span className={styles.errorIcon}>!</span>
            <span>{error}</span>
          </div>
        )}
      </div>
      <ImageLightbox src={lightboxSrc} onClose={() => setLightboxSrc(null)} />
    </div>
  )
}

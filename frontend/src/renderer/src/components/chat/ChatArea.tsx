import { useStore } from '../../stores'
import { useRef, useEffect } from 'react'
import AssistantMessage from './AssistantMessage'
import UserMessage from './UserMessage'
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

  useEffect(() => {
    const el = scrollRef.current
    if (el) el.scrollTop = el.scrollHeight
  }, [messages[messages.length - 1]?.id, isStreaming])

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
    </div>
  )
}

import { useStore } from '../../stores'
import { useLocale } from '../../i18n'
import { useRef, useEffect, useMemo, useCallback, useState } from 'react'
import AssistantMessage from './AssistantMessage'
import UserMessage from './UserMessage'
import ChatTimelineNavigator from './ChatTimelineNavigator'
import ReviewPanel from './ReviewPanel'
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
  // 切会话后历史消息加载中的标志：此间展示 skeleton 而非"空会话"引导页，
  // 避免内容"啪"地替换出来的误导。
  const messagesLoading = useStore(s => s.messagesLoading)
  const switchSession = useStore(s => s.switchSession)
  const inlineErrors = useStore(s => s.inlineErrors)
  const error = sessionId ? inlineErrors.get(sessionId)?.text : null
  const scrollRef = useRef<HTMLDivElement>(null)
  const autoScrollRef = useRef(true)
  const [showScrollBtn, setShowScrollBtn] = useState(false)

  // Track message count for efficient auto-scroll (avoids full array dep)
  const msgCount = messages.length

  const timelineAnchors = useMemo(() => buildTimelineAnchors(messages as any[]), [msgCount])
  const openLightbox = useStore(s => s.openLightbox)
  const { t } = useLocale()

  // Auto-scroll to bottom on new messages when at bottom.
  // 依赖 messages 引用本身：流式 flush 每次都替换消息数组（块内容增长，但消息数
  // 与末条块数都不变），若只依赖 [msgCount, lastMsgBlocksLen] 会在长回复流式
  // 输出时永不触发 → 视口被内容"甩开"且回底按钮不出现。滚到底后同步隐藏回底按钮。
  useEffect(() => {
    const el = scrollRef.current
    if (!el || !autoScrollRef.current) return
    el.scrollTop = el.scrollHeight
    setShowScrollBtn(false)
  }, [messages])

  // Reset auto-scroll flag on session switch
  useEffect(() => {
    autoScrollRef.current = true
    setShowScrollBtn(false)
  }, [sessionId])

  // 会话切换时短暂禁用单条消息入场动画，避免整列错位动画；整体用列表淡入
  const [switchFade, setSwitchFade] = useState(false)
  useEffect(() => {
    setSwitchFade(true)
    const timer = setTimeout(() => setSwitchFade(false), 350)
    return () => clearTimeout(timer)
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
        // 收集聊天区内全部可见图片，打开时传入整组，灯箱内可用 ←/→ 切换
        const container = scrollRef.current
        const imgs = container
          ? Array.from(container.querySelectorAll('img')).filter(
              i => i.naturalWidth >= 20 && i.src && !i.getAttribute('data-blocked-src'))
          : [img]
        const list = imgs.map(i => i.src)
        const index = Math.max(0, list.indexOf(img.src))
        openLightbox(img.src, list, index)
      }
    }
    document.addEventListener('click', docHandler, true)
    return () => document.removeEventListener('click', docHandler, true)
  }, [openLightbox])

  return (
    <div className={styles.chatWrapper}>
      {messages.length === 0 && !isStreaming && messagesLoading ? (
        <div className={styles.skeletonList} aria-busy="true">
          <div className={styles.skeletonMsg} />
          <div className={styles.skeletonMsg} />
          <div className={styles.skeletonMsg} />
        </div>
      ) : messages.length === 0 && !isStreaming ? (
        <div className={styles.emptyState}>
          <div className={styles.emptyContent}>
            <div className={styles.emptyLogo}>
              <span className={styles.emptyLogoText}>L</span>
            </div>
            <p className={styles.emptyHint}>{t('chat.empty')}</p>
          </div>
        </div>
      ) : (
        <div className={styles.chatScroll} ref={scrollRef} onScroll={handleScroll} data-switch={switchFade ? 'true' : 'false'}>
          {messages.map((msg, idx) => (
            <div
              key={msg.id}
              className={styles.messageGap}
              data-message-id={msg.id}
              data-streaming={isStreaming && idx === messages.length - 1 && msg.role === 'assistant' ? 'true' : 'false'}
              data-switch={switchFade ? 'true' : 'false'}
            >
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
              <span className={styles.errorText}>{error}</span>
              {/* 错误不再是"终点"：给出重试入口，重新拉取当前会话 */}
              <button
                className={styles.errorRetry}
                onClick={() => { if (sessionId) switchSession(sessionId) }}
              >
                {t('common.retry')}
              </button>
            </div>
          )}
        </div>
      )}
      <ChatTimelineNavigator anchors={timelineAnchors} scrollRef={scrollRef} onManualNavigate={() => { autoScrollRef.current = false }} />
      {showScrollBtn && messages.length > 0 && (
        <button className={styles.scrollToBottom} onClick={scrollToBottom} title={t('chat.scrollToBottom')}>
          <IconChevronDown size={16} />
        </button>
      )}
      <ReviewPanel />
    </div>
  )
}

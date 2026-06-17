import { useRef, useEffect, useCallback, useState } from 'react'
import { useStore } from '../../stores'
import { useLocale } from '../../i18n'
import { loomRpc } from '../../services/jsonrpc'
import AssistantMessage from '../chat/AssistantMessage'
import UserMessage from '../chat/UserMessage'
import ImageLightbox from '../shared/ImageLightbox'
import { IconChevronDown, IconFileText, IconExternalLink, IconPlus } from '../../utils/icons'
import styles from './WriteChatPanel.module.css'

const EMPTY: never[] = []

interface WriteChatPanelProps {
  sessionId: string | null
  activeFileName: string | null
  /** Renders quick suggestions when no messages exist yet */
  quickSuggestions: { key: string; text: string }[]
  onSuggestionClick: (text: string) => void
  /** Clear the current file's conversation and start a new session */
  onNewChat: () => void
  /** Called when the stored session no longer exists on the backend */
  onStaleSession?: (deadSessionId: string) => void
}

/**
 * Embedded chat panel for WriteWorkspaceView's AI assistant sidebar.
 * Displays the per-file conversation inline — no mode switching needed.
 *
 * Subscribes to messagesBySession (same store as ChatWorkspace).
 * Streaming data flows through the same streamBufferManager pipeline.
 * Note: No React.memo — the parent (WriteWorkspaceView) does NOT subscribe to
 * messagesBySession, so streaming updates won't cause parent re-renders.
 */
export default function WriteChatPanel({
  sessionId,
  activeFileName,
  quickSuggestions,
  onSuggestionClick,
  onNewChat,
  onStaleSession,
}: WriteChatPanelProps) {
  const { t } = useLocale()
  const messagesBySession = useStore(s => s.messagesBySession)
  const messages = sessionId ? (messagesBySession.get(sessionId) ?? EMPTY) : EMPTY
  const streamingIds = useStore(s => s.streamingSessionIds)
  const isStreaming = sessionId ? streamingIds.has(sessionId) : false
  const inlineErrors = useStore(s => s.inlineErrors)
  const error = sessionId ? inlineErrors.get(sessionId)?.text : null

  const lightboxSrc = useStore(s => s.lightbox.lightboxSrc)
  const closeLightbox = useStore(s => s.closeLightbox)

  const setAppMode = useStore(s => s.setAppMode)
  const switchSession = useStore(s => s.switchSession)
  const evictSession = useStore(s => s.evictSession)

  const scrollRef = useRef<HTMLDivElement>(null)
  const autoScrollRef = useRef(true)
  const [showScrollBtn, setShowScrollBtn] = useState(false)

  // Hydrate messages from backend — mirrors switchSession() in session.ts.
  // Only skips when streaming is active AND we already have cached messages.
  // Otherwise always loads from backend, even if ensureSession() created an
  // empty entry in messagesBySession.
  useEffect(() => {
    if (!sessionId) return

    // Don't interfere with an active streaming session that already has data
    const store = useStore.getState()
    const isStreamingNow = store.streamingSessionIds.has(sessionId)
    const hasCached = store.messagesBySession.has(sessionId)
    if (isStreamingNow && hasCached) return

    let cancelled = false
    ;(async () => {
      try {
        // Activate the session on the backend (best-effort, like switchSession does)
        try { await loomRpc('session.switch', { session_id: sessionId }) } catch {}

        if (cancelled) return

        const result = await loomRpc<{ messages: any[] }>('session.messages', { session_id: sessionId })
        if (cancelled) return

        const allMsgs = result.messages || []
        if (allMsgs.length === 0) return

        // Merge tool_result content into the preceding assistant message
        for (let i = 0; i < allMsgs.length; i++) {
          const m = allMsgs[i]
          const role = typeof m.role === 'string' ? m.role.toLowerCase() : ''
          if (role === 'tool' && i > 0) {
            let prev = i - 1
            while (prev >= 0) {
              const pr = typeof allMsgs[prev].role === 'string' ? allMsgs[prev].role.toLowerCase() : ''
              if (pr === 'assistant') break
              prev--
            }
            if (prev >= 0) {
              const ac = allMsgs[prev].content
              if (Array.isArray(ac)) {
                const tp = Array.isArray(m.content) ? m.content : []
                ac.push(...tp)
              }
            }
          }
        }

        // Filter out tool messages and normalize
        const rawMsgs = allMsgs
          .filter((m: any) => typeof m.role === 'string' && m.role.toLowerCase() !== 'tool')
          .map((m: any) => ({
            id: m.id || crypto.randomUUID(),
            role: typeof m.role === 'string' ? m.role.toLowerCase() : m.role,
            timestamp: m.timestamp || new Date().toISOString(),
            blocks: Array.isArray(m.content) ? m.content.map((p: any) => {
              if (typeof p === 'string') return { type: 'text', text: p }
              return p
            }) : (typeof m.content === 'string' ? [{ type: 'text', text: m.content }] : []),
            usage: m.usage ? {
              prompt: m.usage.prompt_tokens || 0,
              completion: m.usage.completion_tokens || 0,
              cached: m.usage.cached_tokens || 0,
              cacheRead: m.usage.cache_read_tokens || 0,
              cacheWrite: m.usage.cache_write_tokens || 0,
              contextWindow: m.usage.context_window || 0,
            } : undefined,
          }))

        // Merge consecutive assistant messages
        const msgs = rawMsgs.reduce((acc: typeof rawMsgs, msg) => {
          if (msg.role === 'assistant' && acc.length > 0 && acc[acc.length - 1].role === 'assistant') {
            const prev = acc[acc.length - 1]
            prev.blocks = [...prev.blocks, ...msg.blocks]
            if (msg.usage) prev.usage = msg.usage
            prev.timestamp = msg.timestamp
            return acc
          }
          acc.push(msg)
          return acc
        }, [] as typeof rawMsgs)

        if (!cancelled) {
          useStore.getState().hydrateMessages(sessionId, msgs)
        }
      } catch (err: any) {
        if (cancelled) return
        const msg = err?.message ?? String(err)
        const isNotFound =
          msg.includes('not found') ||
          msg.includes('does not exist') ||
          msg.includes('no such session') ||
          (err?.code != null && err.code === -32000)
        if (isNotFound) {
          onStaleSession?.(sessionId)
        }
        useStore.getState().setInlineError(
          sessionId,
          isNotFound
            ? t('write.sessionDeleted')
            : t('write.loadFailed')
        )
      }
    })()

    return () => { cancelled = true }
  }, [sessionId])

  const msgCount = messages.length
  const lastMsgBlocksLen = messages.length > 0 ? messages[messages.length - 1].blocks?.length ?? 0 : 0

  // Auto-scroll to bottom on new messages when at bottom
  useEffect(() => {
    if (!autoScrollRef.current || !scrollRef.current) return
    scrollRef.current.scrollTop = scrollRef.current.scrollHeight
  }, [msgCount, lastMsgBlocksLen])

  // Reset auto-scroll when session changes
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

  const openInChat = useCallback(() => {
    if (!sessionId) return
    setAppMode('chat')
    switchSession(sessionId)
  }, [sessionId, setAppMode, switchSession])

  // No file open — show prompt
  if (!activeFileName) {
    return (
      <div className={styles.panel}>
        <div className={styles.emptyState}>
          <IconFileText size={32} className={styles.emptyIcon} />
          <span>{t('write.aiContextNoFile')}</span>
        </div>
      </div>
    )
  }

  // No session or no messages yet — show context + quick suggestions
  if (!sessionId || messages.length === 0) {
    return (
      <div className={styles.panel}>
        <div className={styles.emptyState}>
          <div className={styles.contextFile}>
            <IconFileText size={11} />{activeFileName}
          </div>
          <span className={styles.contextHint}>{t('write.aiContextWithFile')}</span>

          <div className={styles.quickLabel}>{t('write.quickCommands')}</div>
          <div className={styles.quickButtons}>
            {quickSuggestions.map(s => (
              <button key={s.key} className={styles.suggestionBtn}
                onClick={() => onSuggestionClick(s.text)}>
                {s.text}
              </button>
            ))}
          </div>
        </div>
      </div>
    )
  }

  // Messages exist — render conversation
  return (
    <div className={styles.panel}>
      {/* Toolbar: file badge, new-chat, open-in-chat */}
      <div className={styles.panelToolbar}>
        <span className={styles.panelFileBadge}>
          <IconFileText size={10} />{activeFileName}
        </span>
        <div className={styles.toolbarActions}>
          <button className={styles.toolbarActionBtn} onClick={onNewChat} title={t('write.newChat')}>
            <IconPlus size={12} />
          </button>
          <button className={styles.toolbarActionBtn} onClick={openInChat} title={t('write.openInChat')}>
            <IconExternalLink size={12} />
          </button>
        </div>
      </div>

      <div className={styles.scrollArea} ref={scrollRef} onScroll={handleScroll}>
        {messages.map((msg: any, idx: number) => (
          <div key={msg.id} className={styles.messageItem}>
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

        {/* Bottom anchor for auto-scroll */}
        <div ref={(el) => { if (el && autoScrollRef.current) el.scrollIntoView?.({ block: 'end' }) }} />
      </div>

      {showScrollBtn && messages.length > 0 && (
        <button className={styles.scrollToBottom} onClick={scrollToBottom} title={t('chat.scrollToBottom')}>
          <IconChevronDown size={16} />
        </button>
      )}

      <ImageLightbox src={lightboxSrc} onClose={closeLightbox} />
    </div>
  )
}

import { useStore } from '../../stores'
import { useRef, useEffect } from 'react'
import AssistantMessage from './AssistantMessage'
import UserMessage from './UserMessage'
import TypingIndicator from '../shared/TypingIndicator'

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

  if (!sessionId) {
    return (
      <div className="flex-1 flex items-center justify-center">
        <div className="text-center animate-fade-up">
          <div className="w-14 h-14 mx-auto mb-5 rounded-[var(--r-lg)] bg-[rgba(0,227,199,0.06)] border border-[rgba(0,227,199,0.12)] flex items-center justify-center shadow-[0_0_30px_rgba(0,227,199,0.06)]">
            <span className="text-2xl font-bold text-[var(--accent)]">L</span>
          </div>
          <h1 className="text-[22px] font-semibold text-[var(--text)] tracking-tight">openLoom</h1>
          <p className="text-[14px] text-[rgba(0,227,199,0.35)] mt-2">你的私人 AI 助理</p>
        </div>
      </div>
    )
  }

  if (messages.length === 0 && !isStreaming) {
    return (
      <div className="flex-1 flex items-center justify-center">
        <div className="text-center animate-fade-up">
          <div className="w-12 h-12 mx-auto mb-4 rounded-[var(--r-lg)] bg-[rgba(0,227,199,0.06)] border border-[rgba(0,227,199,0.12)] flex items-center justify-center shadow-[0_0_30px_rgba(0,227,199,0.06)]">
            <span className="text-xl font-bold text-[var(--accent)]">L</span>
          </div>
          <p className="text-[14px] text-[rgba(0,227,199,0.35)]">发送消息开始对话</p>
        </div>
      </div>
    )
  }

  return (
    <div ref={scrollRef} className="flex-1 overflow-y-auto" style={{ padding: '24px 1.25rem 120px' }}>
      <div className="max-w-[680px] mx-auto space-y-5">
        {messages.map(msg =>
          msg.role === 'user'
            ? <UserMessage key={msg.id} message={msg} />
            : <AssistantMessage key={msg.id} message={msg} />
        )}

        {isStreaming && (
          <div className="flex items-center gap-2 text-[13px] text-[var(--text-muted)] animate-fade-in">
            <span>AI 回复中</span>
            <TypingIndicator />
          </div>
        )}

        {error && (
          <div className="flex items-start gap-2 px-3.5 py-2.5 rounded-[var(--r-md)] border border-[rgba(239,68,68,0.15)] bg-[var(--red-light)] text-[13px] text-[var(--red)]">
            <span className="font-bold shrink-0">!</span>
            <span>{error}</span>
          </div>
        )}
      </div>
    </div>
  )
}

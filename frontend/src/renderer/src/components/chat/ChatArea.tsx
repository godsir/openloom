import { useStore } from '../../stores'
import { useRef, useEffect } from 'react'
import { loomRpc } from '../../services/jsonrpc'
import AssistantMessage from './AssistantMessage'
import UserMessage from './UserMessage'

const EMPTY: never[] = []

export default function ChatArea() {
  const sessionId = useStore((s) => s.currentSessionId)
  const messagesBySession = useStore((s) => s.messagesBySession)
  const messages = sessionId ? (messagesBySession.get(sessionId) ?? EMPTY) : EMPTY
  const streamingIds = useStore((s) => s.streamingSessionIds)
  const isStreaming = sessionId ? streamingIds.has(sessionId) : false
  const inlineErrors = useStore((s) => s.inlineErrors)
  const error = sessionId ? inlineErrors.get(sessionId)?.text : null
  const scrollRef = useRef<HTMLDivElement>(null)

  // Auto-scroll on new messages
  useEffect(() => {
    const el = scrollRef.current
    if (!el) return
    el.scrollTop = el.scrollHeight
  }, [messages[messages.length - 1]?.id, isStreaming])

  if (!sessionId) {
    return (
      <div className="flex-1 flex items-center justify-center">
        <div className="text-center text-zinc-500">
          <p className="text-lg mb-1">openLoom</p>
          <p className="text-sm">选择一个会话或创建新会话开始</p>
        </div>
      </div>
    )
  }

  return (
    <div ref={scrollRef} className="flex-1 overflow-y-auto">
      <div className="max-w-3xl mx-auto px-4 py-4 space-y-4">
        {messages.length === 0 && !isStreaming && (
          <div className="text-center text-zinc-600 py-16">
            <p className="text-sm">发送一条消息开始对话</p>
          </div>
        )}

        {messages.map((msg) =>
          msg.role === 'user' ? (
            <UserMessage key={msg.id} message={msg} />
          ) : (
            <AssistantMessage key={msg.id} message={msg} />
          ),
        )}

        {isStreaming && !messages.length && (
          <div className="flex items-center gap-2 text-zinc-500 text-sm px-1">
            <span className="animate-pulse">AI 正在回复</span>
            <span className="animate-[ping_1.5s_infinite]">...</span>
          </div>
        )}

        {error && (
          <div className="flex items-center gap-2 px-3 py-2 bg-red-900/30 border border-red-800/50 rounded-lg text-sm text-red-300">
            <span className="shrink-0">!</span>
            <span className="flex-1">{error}</span>
          </div>
        )}
      </div>
    </div>
  )
}

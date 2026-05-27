import { useState, useRef, useCallback } from 'react'
import { useStore } from '../../stores'
import { loomRpc } from '../../services/jsonrpc'
import ContextRing from './ContextRing'
import ModelSelector from './ModelSelector'
import ThinkingLevelButton from './ThinkingLevelButton'
import PermissionModeButton from './PermissionModeButton'

export default function InputArea() {
  const [text, setText] = useState('')
  const [sending, setSending] = useState(false)
  const textareaRef = useRef<HTMLTextAreaElement>(null)
  const sessionId = useStore((s) => s.currentSessionId)
  const createSession = useStore((s) => s.createSession)
  const switchSession = useStore((s) => s.switchSession)
  const isStreaming = useStore((s) =>
    sessionId ? s.streamingSessionIds.has(sessionId) : false,
  )
  const wsState = useStore((s) => s.wsState)

  const ensureSession = useCallback(async (): Promise<string> => {
    if (sessionId) return sessionId
    const id = await createSession()
    if (id) await switchSession(id)
    return id
  }, [sessionId, createSession, switchSession])

  const handleSend = async () => {
    const content = text.trim()
    if (!content || sending || isStreaming) return

    setSending(true)
    setText('')

    const sid = await ensureSession()
    if (!sid) {
      setSending(false)
      setText(content)
      return
    }

    // Optimistically add user message
    const msgId = crypto.randomUUID()
    useStore.getState().ensureSession(sid)
    useStore.getState().appendMessage(sid, {
      id: msgId,
      role: 'user',
      blocks: [{ type: 'text', html: escapeHtml(content), source: content }],
      timestamp: new Date().toISOString(),
    })

    try {
      await loomRpc('chat.send', { session_id: sid, content })
    } catch (e: any) {
      useStore
        .getState()
        .setInlineError(sid, e.message || '发送失败，请检查连接后重试')
      // Remove the optimistic message on failure
      useStore.getState().deleteMessage(sid, msgId)
    } finally {
      setSending(false)
    }
  }

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault()
      handleSend()
    }
  }

  const isConnected = wsState === 'connected'
  const placeholder = !isConnected
    ? '正在连接引擎...'
    : !sessionId
      ? '新建会话后开始对话...'
      : isStreaming
        ? 'AI 回复中...'
        : '输入消息... (Enter 发送, Shift+Enter 换行)'

  return (
    <div className="border-t border-zinc-800 px-4 py-3">
      <div className="max-w-3xl mx-auto space-y-2">
        <div className="flex gap-2">
          <textarea
            ref={textareaRef}
            value={text}
            onChange={(e) => setText(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder={placeholder}
            rows={2}
            disabled={!isConnected || isStreaming}
            className="flex-1 bg-zinc-800 text-zinc-200 text-sm rounded-lg px-3 py-2 resize-none outline-none focus:ring-1 focus:ring-blue-500/50 placeholder:text-zinc-600 disabled:opacity-50"
          />
          <button
            onClick={handleSend}
            disabled={!text.trim() || !isConnected || isStreaming}
            className="px-4 py-2 bg-blue-600 text-white text-sm rounded-lg hover:bg-blue-500 disabled:opacity-40 disabled:cursor-not-allowed transition-colors shrink-0 font-medium"
          >
            {isStreaming ? '...' : '发送'}
          </button>
        </div>

        {/* Control bar */}
        <div className="flex items-center gap-1">
          <PermissionModeButton />
          <ThinkingLevelButton />
          <div className="flex-1" />
          <ModelSelector />
          <ContextRing />
        </div>
      </div>
    </div>
  )
}

function escapeHtml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
}

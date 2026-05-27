import { useState, useRef, useCallback, useEffect } from 'react'
import { useStore } from '../../stores'
import { loomRpc } from '../../services/jsonrpc'
import ContextRing from './ContextRing'
import ModelSelector from './ModelSelector'
import ThinkingLevelButton from './ThinkingLevelButton'
import PermissionModeButton from './PermissionModeButton'
import TypingIndicator from '../shared/TypingIndicator'
import { IconSend } from '../../utils/icons'

export default function InputArea() {
  const [text, setText] = useState('')
  const [sending, setSending] = useState(false)
  const textareaRef = useRef<HTMLTextAreaElement>(null)
  const sessionId = useStore(s => s.currentSessionId)
  const createSession = useStore(s => s.createSession)
  const switchSession = useStore(s => s.switchSession)
  const isStreaming = useStore(s => sessionId ? s.streamingSessionIds.has(sessionId) : false)
  const wsState = useStore(s => s.wsState)
  const { saveDraft, restoreDraft } = useStore.getState()

  useEffect(() => {
    if (sessionId) { const d = restoreDraft(sessionId); setText(d?.text ?? '') }
    else setText('')
  }, [sessionId])

  useEffect(() => {
    if (sessionId && text) {
      const t = setTimeout(() => saveDraft(sessionId, { text, attachedFiles: [] }), 300)
      return () => clearTimeout(t)
    }
  }, [text, sessionId])

  const ensureSession = useCallback(async (): Promise<string> => {
    if (sessionId) return sessionId
    const id = await createSession()
    if (id) await switchSession(id)
    return id
  }, [sessionId, createSession, switchSession])

  const handleSend = async () => {
    const content = text.trim()
    if (!content || sending || isStreaming) return
    setSending(true); setText('')
    const sid = await ensureSession()
    if (!sid) { setSending(false); setText(content); return }
    const msgId = crypto.randomUUID()
    useStore.getState().ensureSession(sid)
    useStore.getState().appendMessage(sid, {
      id: msgId, role: 'user',
      blocks: [{ type: 'text', html: escapeHtml(content), source: content }],
      timestamp: new Date().toISOString(),
    })
    try { await loomRpc('chat.send', { session_id: sid, content }) }
    catch (e: any) { useStore.getState().setInlineError(sid, e.message||'发送失败'); useStore.getState().deleteMessage(sid, msgId) }
    finally { setSending(false) }
  }

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); handleSend() }
  }

  const isConnected = wsState === 'connected'
  const placeholder = !isConnected ? '正在连接...' : !sessionId ? '新建会话后开始对话' : isStreaming ? 'AI 回复中...' : '输入消息，⏎ 发送'

  return (
    <div className="absolute bottom-0 left-0 right-0 z-5 px-4 pb-4 pointer-events-none">
      <div className="max-w-[680px] mx-auto pointer-events-auto">
        <div className="flex flex-col bg-[rgba(0,227,199,0.025)] backdrop-blur-[24px] border border-[rgba(0,227,199,0.07)] rounded-[var(--r-xl)] shadow-[var(--shadow-glass)]">
          <textarea
            ref={textareaRef}
            value={text}
            onChange={e => setText(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder={placeholder}
            rows={2}
            disabled={!isConnected || isStreaming}
            className="w-full bg-transparent text-[var(--text)] text-[0.875rem] leading-relaxed resize-none outline-none placeholder:text-[var(--text-muted)] placeholder:italic px-3.5 pt-3 disabled:opacity-40"
          />
          <div className="flex items-center gap-2 px-3.5 pb-2.5 pt-1 border-t border-[rgba(255,255,255,0.03)]">
            <PermissionModeButton />
            <ThinkingLevelButton />
            <div className="flex-1" />
            <ModelSelector />
            <ContextRing />
            <button
              onClick={handleSend}
              disabled={!text.trim() || !isConnected || isStreaming}
              className="inline-flex items-center justify-center gap-1.5 h-[26px] px-3.5 text-[12px] font-semibold text-[var(--bg)] bg-[var(--accent)] hover:bg-[var(--accent-hover)] disabled:opacity-25 disabled:cursor-not-allowed rounded-[var(--r-md)] transition-all shrink-0"
            >
              {isStreaming ? <TypingIndicator /> : <><IconSend size={12} />发送</>}
            </button>
          </div>
        </div>
      </div>
    </div>
  )
}

function escapeHtml(s: string): string { return s.replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;') }

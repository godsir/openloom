import { useState, useRef, useCallback, useEffect } from 'react'
import { useStore } from '../../stores'
import { loomRpc } from '../../services/jsonrpc'
import { streamBufferManager } from '../../services/stream-buffer'
import ContextRing from './ContextRing'
import ModelSelector from './ModelSelector'
import AgentSelector from './AgentSelector'
import ThinkingLevelButton from './ThinkingLevelButton'
import PermissionModeButton from './PermissionModeButton'
import TypingIndicator from '../shared/TypingIndicator'
import { IconSend } from '../../utils/icons'
import styles from './InputArea.module.css'

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
    // Create assistant placeholder immediately so user sees feedback
    const aiMsgId = crypto.randomUUID()
    useStore.getState().addStreamingSession(sid)
    useStore.getState().appendMessage(sid, {
      id: aiMsgId, role: 'assistant',
      blocks: [],
      timestamp: new Date().toISOString(),
    })
    // Wire the stream buffer to this placeholder
    streamBufferManager.startStream(sid, aiMsgId)
    try {
      const { currentModel, thinkingLevel } = useStore.getState()
      await loomRpc('chat.send', {
        session_id: sid,
        content,
        model: currentModel || undefined,
        thinking_level: thinkingLevel || 'off',
      })
    }
    catch (e: any) {
      useStore.getState().setInlineError(sid, e.message||'发送失败')
      // Clear the streaming placeholder on error
      useStore.getState().removeStreamingSession(sid)
      streamBufferManager.clear(sid)
    }
    finally { setSending(false) }
  }

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); handleSend() }
  }

  const isConnected = wsState === 'connected'
  const placeholder = !isConnected ? '正在连接...' : !sessionId ? '开始新对话...' : isStreaming ? 'AI 回复中...' : '输入消息，Enter 发送'

  return (
    <div className={styles.wrapper}>
      <div className={styles.container}>
        <div className={styles.composer}>
          <textarea
            ref={textareaRef}
            value={text}
            onChange={e => setText(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder={placeholder}
            rows={2}
            disabled={!isConnected || isStreaming}
            className={styles.textarea}
          />
          <div className={styles.toolbar}>
            <PermissionModeButton />
            <ThinkingLevelButton />
            <ModelSelector />
            <AgentSelector />
            <div className={styles.spacer} />
            <ContextRing />
            <button
              onClick={handleSend}
              disabled={!text.trim() || !isConnected || isStreaming}
              className={styles.sendBtn}
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

import { useState, useRef, useCallback, useEffect } from 'react'
import { useStore } from '../../stores'
import { loomRpc } from '../../services/jsonrpc'
import { streamBufferManager } from '../../services/stream-buffer'
import ContextRing from './ContextRing'
import ModelSelector from './ModelSelector'
import AgentSelector from './AgentSelector'
import ThinkingLevelButton from './ThinkingLevelButton'
import PermissionModeButton from './PermissionModeButton'
import AttachedFiles from './AttachedFiles'
import { IconSend, IconImage, IconPaperclip } from '../../utils/icons'
import { escapeHtml } from '../../utils/format'
import type { AttachedFile } from '../../stores/input'
import styles from './InputArea.module.css'


export default function InputArea() {
  const [text, setText] = useState('')
  const [attachedFiles, setAttachedFiles] = useState<AttachedFile[]>([])
  const [isDragOver, setIsDragOver] = useState(false)
  const sendingRef = useRef(false)
  const pasteCounterRef = useRef(0)
  const textareaRef = useRef<HTMLTextAreaElement>(null)
  const imageInputRef = useRef<HTMLInputElement>(null)
  const fileInputRef = useRef<HTMLInputElement>(null)
  const sessionId = useStore(s => s.currentSessionId)
  const createSession = useStore(s => s.createSession)
  const switchSession = useStore(s => s.switchSession)
  const wsState = useStore(s => s.wsState)
  const { saveDraft, restoreDraft } = useStore.getState()

  useEffect(() => {
    if (sessionId) {
      const d = restoreDraft(sessionId)
      setText(d?.text ?? '')
      setAttachedFiles(d?.attachedFiles ?? [])
    } else {
      setText('')
      setAttachedFiles([])
    }
  }, [sessionId])

  useEffect(() => {
    if (sessionId && (text || attachedFiles.length > 0)) {
      const t = setTimeout(() => saveDraft(sessionId, { text, attachedFiles }), 300)
      return () => clearTimeout(t)
    }
  }, [text, attachedFiles, sessionId])

  const ensureSession = useCallback(async (): Promise<string> => {
    if (sessionId) return sessionId
    const id = await createSession()
    if (id) await switchSession(id)
    return id
  }, [sessionId, createSession, switchSession])

  const handlePaste = useCallback((e: React.ClipboardEvent) => {
    const items = e.clipboardData?.items
    if (!items) return

    const imageItems: DataTransferItem[] = []
    for (let i = 0; i < items.length; i++) {
      if (items[i].type.startsWith('image/')) {
        imageItems.push(items[i])
      }
    }

    if (imageItems.length === 0) return

    e.preventDefault()

    for (const item of imageItems) {
      const blob = item.getAsFile()
      if (!blob) continue

      pasteCounterRef.current += 1
      const idx = pasteCounterRef.current
      const ext = blob.type.split('/')[1] || 'png'
      const reader = new FileReader()
      reader.onload = () => {
        setAttachedFiles(prev => [...prev, {
          path: '',
          name: `pasted-image-${Date.now()}-${idx}.${ext}`,
          size: blob.size,
          mimeType: blob.type,
          thumbnail: reader.result as string,
        }])
      }
      reader.readAsDataURL(blob)
    }
  }, [])

  const processFiles = async (files: FileList | null) => {
    if (!files || files.length === 0) return

    for (let i = 0; i < files.length; i++) {
      const file = files[i]
      let thumbnail: string | undefined

      if (file.type.startsWith('image/')) {
        thumbnail = await new Promise<string>((resolve) => {
          const reader = new FileReader()
          reader.onload = () => resolve(reader.result as string)
          reader.readAsDataURL(file)
        })
      }

      setAttachedFiles(prev => [...prev, {
        path: (file as any).path ?? '',
        name: file.name,
        size: file.size,
        mimeType: file.type || 'application/octet-stream',
        thumbnail,
      }])
    }

    // reset input so the same file can be re-selected
    if (imageInputRef.current) imageInputRef.current.value = ''
    if (fileInputRef.current) fileInputRef.current.value = ''
  }

  const handleRemoveFile = useCallback((index: number) => {
    setAttachedFiles(prev => prev.filter((_, i) => i !== index))
  }, [])

  const handleDragOver = (e: React.DragEvent) => {
    e.preventDefault()
    e.stopPropagation()
    if (e.dataTransfer.types.some(t => t === 'Files' || t === 'application/x-moz-file')) {
      e.dataTransfer.dropEffect = 'copy'
      setIsDragOver(true)
    }
  }

  const handleDragLeave = (e: React.DragEvent) => {
    e.preventDefault()
    e.stopPropagation()
    if (e.currentTarget === e.target || !e.currentTarget.contains(e.relatedTarget as Node)) {
      setIsDragOver(false)
    }
  }

  const handleDrop = (e: React.DragEvent) => {
    e.preventDefault()
    e.stopPropagation()
    setIsDragOver(false)
    if (e.dataTransfer.files && e.dataTransfer.files.length > 0) {
      processFiles(e.dataTransfer.files)
    }
  }

  const handleStop = async () => {
    if (!sessionId) return
    // Optimistically clear streaming state immediately so UI unlocks
    useStore.getState().removeStreamingSession(sessionId)
    streamBufferManager.clear(sessionId)
    sendingRef.current = false
    // Replace any empty assistant placeholder so "思考中" doesn't linger
    const store = useStore.getState()
    const msgs = store.messagesBySession.get(sessionId)
    if (msgs) {
      const updated = msgs.map(m => {
        if (m.role === 'assistant' && m.blocks.length === 0) {
          return { ...m, blocks: [{ type: 'text', html: '<em>已停止生成</em>', source: '' }] as any }
        }
        return m
      })
      if (updated.some((m, i) => m !== msgs[i])) {
        const next = new Map(store.messagesBySession)
        next.set(sessionId, updated)
        useStore.setState({ messagesBySession: next })
      }
    }
    try {
      await loomRpc('chat.stop', { session_id: sessionId })
    } catch {
      // Already cleaned up above
    }
  }

  const handleSend = async () => {
    const content = text.trim()
    if ((!content && attachedFiles.length === 0) || sendingRef.current || (sessionId && useStore.getState().streamingSessionIds.has(sessionId))) return
    sendingRef.current = true
    setText('')
    const filesToSend = attachedFiles
    setAttachedFiles([])
    const sid = await ensureSession()
    if (!sid) { sendingRef.current = false; setText(content); setAttachedFiles(filesToSend); return }

    const msgId = crypto.randomUUID()
    useStore.getState().ensureSession(sid)

    const blocks: any[] = []
    if (content) {
      blocks.push({ type: 'text', html: escapeHtml(content), source: content })
    }
    for (const f of filesToSend) {
      if (f.mimeType.startsWith('image/')) {
        blocks.push({ type: 'image', path: f.path, name: f.name, mimeType: f.mimeType, thumbnail: f.thumbnail })
      } else {
        blocks.push({ type: 'file', path: f.path, name: f.name, mimeType: f.mimeType, size: f.size })
      }
    }

    useStore.getState().appendMessage(sid, {
      id: msgId, role: 'user',
      blocks,
      timestamp: new Date().toISOString(),
    })

    const aiMsgId = crypto.randomUUID()
    useStore.getState().addStreamingSession(sid)
    useStore.getState().appendMessage(sid, {
      id: aiMsgId, role: 'assistant',
      blocks: [],
      timestamp: new Date().toISOString(),
    })
    streamBufferManager.startStream(sid, aiMsgId)

    // Safety timeout: if stream_end never arrives (e.g. backend deadlock),
    // auto-unlock the input after 3 minutes so the user isn't permanently stuck.
    const safetyTimer = setTimeout(() => {
      const buf = streamBufferManager.snapshot(sid)
      if (buf && buf.messageId === aiMsgId) {
        useStore.getState().removeStreamingSession(sid)
        streamBufferManager.clear(sid)
      }
      sendingRef.current = false
    }, 180_000)

    try {
      const { currentModel, thinkingLevel } = useStore.getState()
      await loomRpc('chat.send', {
        session_id: sid,
        content,
        model: currentModel || undefined,
        thinking_level: thinkingLevel || 'off',
        attached_files: filesToSend.map(f => ({
          path: f.path,
          name: f.name,
          size: f.size,
          mime_type: f.mimeType,
          thumbnail: f.thumbnail,
        })),
      })
    }
    catch (e: any) {
      useStore.getState().setInlineError(sid, e.message || '发送失败')
    }
    finally {
      clearTimeout(safetyTimer)
      // Only clean up if a subsequent send hasn't already started a new stream
      // for this session. A slow chat.send RPC (e.g. entity extraction after
      // image messages) can complete after the user has already sent the next
      // message — without this guard, it would destroy the new stream's state
      // and the next message would appear stuck in "thinking".
      const buf = streamBufferManager.snapshot(sid)
      if (buf && buf.messageId === aiMsgId) {
        useStore.getState().removeStreamingSession(sid)
        streamBufferManager.clear(sid)
      }
      sendingRef.current = false
    }
  }

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); handleSend() }
  }

  const streaming = useStore(s => sessionId ? s.streamingSessionIds.has(sessionId) : false)

  // When streaming ends (stream_end event fires before chat.send RPC returns),
  // reset sendingRef so the user can send the next message immediately.
  const prevStreamingRef = useRef(false)
  useEffect(() => {
    if (prevStreamingRef.current && !streaming) {
      sendingRef.current = false
    }
    prevStreamingRef.current = streaming
  }, [streaming])

  const isConnected = wsState === 'connected'
  const placeholder = !isConnected ? '正在连接...' : !sessionId ? '开始新对话...' : streaming ? 'AI 回复中...' : '输入消息，Enter 发送'

  return (
    <div
      className={`${styles.wrapper} ${isDragOver ? styles.dragOver : ''}`}
      onDragOver={handleDragOver}
      onDragLeave={handleDragLeave}
      onDrop={handleDrop}
    >
      <div className={styles.container}>
        <div className={styles.composer}>
          {attachedFiles.length > 0 && (
            <div className={styles.attachmentsArea}>
              <AttachedFiles files={attachedFiles} onRemove={handleRemoveFile} />
            </div>
          )}
          <textarea
            ref={textareaRef}
            value={text}
            onChange={e => setText(e.target.value)}
            onKeyDown={handleKeyDown}
            onPaste={handlePaste}
            placeholder={placeholder}
            rows={2}
            disabled={!isConnected || streaming}
            className={styles.textarea}
          />
          <div className={styles.toolbar}>
            <input
              ref={imageInputRef}
              type="file"
              accept="image/*"
              multiple
              hidden
              onChange={e => processFiles(e.target.files)}
            />
            <input
              ref={fileInputRef}
              type="file"
              multiple
              hidden
              onChange={e => processFiles(e.target.files)}
            />
            <button
              onClick={() => imageInputRef.current?.click()}
              disabled={!isConnected || streaming}
              className={styles.fileActionBtn}
              title="插入图片"
            >
              <IconImage size={15} />
            </button>
            <button
              onClick={() => fileInputRef.current?.click()}
              disabled={!isConnected || streaming}
              className={styles.fileActionBtn}
              title="添加附件"
            >
              <IconPaperclip size={15} />
            </button>
            <PermissionModeButton />
            <ThinkingLevelButton />
            <ModelSelector />
            <AgentSelector />
            <div className={styles.spacer} />
            <ContextRing />
            {streaming ? (
              <button
                onClick={handleStop}
                className={`${styles.sendBtn} ${styles.stopBtn}`}
              >
                <IconSend size={12} />停止
              </button>
            ) : (
              <button
                onClick={handleSend}
                disabled={(!text.trim() && attachedFiles.length === 0) || !isConnected}
                className={styles.sendBtn}
              >
                <IconSend size={12} />发送
              </button>
            )}
          </div>
        </div>
      </div>
    </div>
  )
}

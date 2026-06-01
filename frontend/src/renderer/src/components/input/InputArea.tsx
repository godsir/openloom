import { useState, useRef, useCallback, useEffect } from 'react'
import { useStore } from '../../stores'
import { loomRpc } from '../../services/jsonrpc'
import { streamBufferManager } from '../../services/stream-buffer'
import { sendMessage } from '../../services/sendMessage'
import ContextRing from './ContextRing'
import ModelSelector from './ModelSelector'
import AgentSelector from './AgentSelector'
import ThinkingLevelButton from './ThinkingLevelButton'
import PermissionModeButton from './PermissionModeButton'
import AttachedFiles from './AttachedFiles'
import { IconSend, IconImage, IconPaperclip, IconSparkles, IconX, IconCheck } from '../../utils/icons'
import type { AttachedFile } from '../../stores/input'
import styles from './InputArea.module.css'

interface SkillInfo {
  name: string
  description?: string
  path?: string
  version?: string
  user_invocable?: boolean
  always_active?: boolean
}


export default function InputArea() {
  const [text, setText] = useState('')
  const [attachedFiles, setAttachedFiles] = useState<AttachedFile[]>([])
  const [isDragOver, setIsDragOver] = useState(false)
  const [selectedSkills, setSelectedSkills] = useState<string[]>([])
  const [availableSkills, setAvailableSkills] = useState<SkillInfo[]>([])
  const [showSkillPopover, setShowSkillPopover] = useState(false)
  const sendingRef = useRef(false)
  const pasteCounterRef = useRef(0)
  const textareaRef = useRef<HTMLTextAreaElement>(null)
  const imageInputRef = useRef<HTMLInputElement>(null)
  const fileInputRef = useRef<HTMLInputElement>(null)
  const skillPopoverRef = useRef<HTMLDivElement>(null)
  const sessionId = useStore(s => s.currentSessionId)
  const sessionWorkspaces = useStore(s => s.sessionWorkspaces)
  const createSession = useStore(s => s.createSession)
  const switchSession = useStore(s => s.switchSession)
  const wsState = useStore(s => s.wsState)
  const { saveDraft, restoreDraft } = useStore.getState()
  const sessionWorkspace = sessionId ? sessionWorkspaces[sessionId] : undefined

  // Load available skills (deduplicate by name)
  const refreshSkills = useCallback(async () => {
    try {
      const res = await loomRpc<{ skills: SkillInfo[] }>('skills.list')
      const seen = new Set<string>()
      const deduped = (res.skills ?? []).filter(s => {
        if (seen.has(s.name)) return false
        seen.add(s.name)
        return true
      })
      setAvailableSkills(deduped)
      // Prune stale selections
      const names = new Set(deduped.map(s => s.name))
      setSelectedSkills(prev => prev.filter(n => names.has(n)))
    } catch {}
  }, [])

  // Load on mount
  useEffect(() => { refreshSkills() }, [refreshSkills])

  // Refresh when popover opens (catches add/delete from Settings)
  useEffect(() => {
    if (showSkillPopover) refreshSkills()
  }, [showSkillPopover, refreshSkills])

  // Close popover on outside click
  useEffect(() => {
    if (!showSkillPopover) return
    const handleClick = (e: MouseEvent) => {
      if (skillPopoverRef.current && !skillPopoverRef.current.contains(e.target as Node)) {
        setShowSkillPopover(false)
      }
    }
    document.addEventListener('mousedown', handleClick)
    return () => document.removeEventListener('mousedown', handleClick)
  }, [showSkillPopover])

  const toggleSkill = useCallback((name: string) => {
    setSelectedSkills(prev =>
      prev.includes(name) ? prev.filter(s => s !== name) : [...prev, name]
    )
  }, [])

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

    try {
      const sid = await ensureSession()
      if (!sid) { sendingRef.current = false; setText(content); setAttachedFiles(filesToSend); return }
      await sendMessage({ sessionId: sid, content, attachedFiles: filesToSend, skills: selectedSkills.length > 0 ? selectedSkills : undefined })
    } finally {
      sendingRef.current = false
    }
  }

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      if (e.ctrlKey || e.metaKey) {
        // Insert newline at cursor position
        e.preventDefault()
        const el = e.currentTarget as HTMLTextAreaElement
        const { selectionStart, selectionEnd, value } = el
        const next = value.slice(0, selectionStart) + '\n' + value.slice(selectionEnd)
        setText(next)
        // Restore cursor after the inserted newline
        requestAnimationFrame(() => {
          el.selectionStart = el.selectionEnd = selectionStart + 1
        })
        return
      }
      e.preventDefault()
      handleSend()
    }
  }

  const streaming = useStore(s => sessionId ? s.streamingSessionIds.has(sessionId) : false)

  // Auto-resize textarea
  useEffect(() => {
    const el = textareaRef.current
    if (!el) return
    el.style.height = 'auto'
    el.style.height = Math.min(el.scrollHeight, 200) + 'px'
  }, [text])

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
  const placeholder = !isConnected ? '正在连接...' : !sessionId ? '开始新对话...' : '输入消息，Enter 发送 · Ctrl+Enter 换行'

  return (
    <div
      className={`${styles.wrapper} ${isDragOver ? styles.dragOver : ''}`}
      onDragOver={handleDragOver}
      onDragLeave={handleDragLeave}
      onDrop={handleDrop}
    >
      <div className={styles.container}>
        <div className={styles.composer}>
          {selectedSkills.length > 0 && (
            <div className={styles.skillBar}>
              {selectedSkills.map(name => (
                <span key={name} className={styles.skillChip}>
                  {name}
                  <button
                    onClick={() => toggleSkill(name)}
                    className={styles.skillChipRemove}
                  >
                    <IconX size={10} />
                  </button>
                </span>
              ))}
            </div>
          )}
          {sessionWorkspace && (
            <div className={styles.workspaceBar}>
              <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
                <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/>
              </svg>
              <span className={styles.workspacePath}>{sessionWorkspace}</span>
            </div>
          )}
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
            disabled={!isConnected}
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
            <div className={styles.skillBtnWrap} ref={skillPopoverRef}>
              <button
                onClick={() => setShowSkillPopover(v => !v)}
                disabled={!isConnected || streaming}
                className={`${styles.skillBtn} ${selectedSkills.length > 0 ? styles.skillBtnActive : ''}`}
                title="加载技能"
              >
                <IconSparkles size={13} />
                {selectedSkills.length > 0 && <span>{selectedSkills.length}</span>}
              </button>
              {showSkillPopover && (
                <div className={styles.skillPopover}>
                  <div className={styles.skillPopoverHeader}>可用技能</div>
                  {availableSkills.length === 0 ? (
                    <div className={styles.skillPopoverEmpty}>暂无技能，可在设置中导入</div>
                  ) : (
                    availableSkills.map(s => {
                      const isSelected = selectedSkills.includes(s.name)
                      return (
                        <button
                          key={s.name}
                          onClick={() => toggleSkill(s.name)}
                          className={`${styles.skillPopoverItem} ${isSelected ? styles.skillPopoverItemSelected : ''}`}
                        >
                          <span className={styles.skillPopoverItemName}>{s.name}</span>
                          {s.description && (
                            <span className={styles.skillPopoverItemDesc}>{s.description}</span>
                          )}
                          {isSelected && <IconCheck size={12} className={styles.skillPopoverCheck} />}
                        </button>
                      )
                    })
                  )}
                </div>
              )}
            </div>
            <div className={styles.toolbarDivider} />
            <PermissionModeButton />
            <ThinkingLevelButton />
            <ModelSelector />
            <AgentSelector />
            <div className={styles.spacer} />
            <div className={styles.toolbarDivider} />
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

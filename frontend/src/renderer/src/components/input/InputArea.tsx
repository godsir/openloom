import { useState, useRef, useCallback, useEffect } from 'react'
import { useStore } from '../../stores'
import type { SendShortcut } from '../../stores/input'
import { loomRpc } from '../../services/jsonrpc'
import { streamBufferManager } from '../../services/stream-buffer'
import { sendMessage } from '../../services/sendMessage'
import { useLocale } from '../../i18n'
import ContextRing from './ContextRing'
import ModelSelector from './ModelSelector'
import AgentSelector from './AgentSelector'
import ThinkingLevelButton from './ThinkingLevelButton'
import PermissionModeButton from './PermissionModeButton'
import AttachedFiles from './AttachedFiles'
import QuotedSelectionCard from './QuotedSelectionCard'
import { IconImage, IconPaperclip, IconSparkles, IconX, IconCheck } from '../../utils/icons'
import { CodeMirrorInput } from './CodeMirrorInput'
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
  const { t } = useLocale()
  const [text, setText] = useState('')

  // 点击外部关闭发送快捷键下拉
  useEffect(() => {
    const handler = (e: MouseEvent) => {
      const target = e.target as HTMLElement
      if (!target.closest(`.${styles.sendSplit}`)) {
        document.querySelectorAll(`.${styles.sendSplitCaret}`).forEach(el => {
          (el as HTMLElement).dataset.open = '0'
        })
      }
    }
    document.addEventListener('mousedown', handler)
    return () => document.removeEventListener('mousedown', handler)
  }, [])
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
  const sendShortcut = useStore(s => s.sendShortcut)
  const fimEnabled = useStore(s => s.fimEnabled)
  const quotedSelections = useStore(s => s.quotedSelections)
  const removeQuotedSelection = useStore(s => s.removeQuotedSelection)
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
    if (sessionId) {
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

  const handlePaste = useCallback((e: React.ClipboardEvent | ClipboardEvent) => {
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

    const MAX_FILE_SIZE = 512_000 // 500 KB, matches backend

    for (let i = 0; i < files.length; i++) {
      const file = files[i]
      let thumbnail: string | undefined
      let content: string | undefined

      // Resolve real file path — Electron sandbox strips File.path,
      // so we also read content in the renderer as the primary data channel.
      const filePath: string = (file as any).path ?? ''

      if (file.type.startsWith('image/')) {
        thumbnail = await new Promise<string>((resolve) => {
          const reader = new FileReader()
          reader.onload = () => resolve(reader.result as string)
          reader.readAsDataURL(file)
        })
      } else if (file.size > 0) {
        // Read non-image file content in the renderer so the backend
        // doesn't need to access the file on disk (sandbox-safe).
        if (file.size <= MAX_FILE_SIZE) {
          try {
            content = await new Promise<string>((resolve, reject) => {
              const reader = new FileReader()
              reader.onload = () => resolve(reader.result as string)
              reader.onerror = () => reject(reader.error)
              reader.readAsText(file)
            })
          } catch {
            // File read failed; still attach the file metadata so the
            // backend can try to read from path as fallback.
            content = undefined
          }
        }
        // Files over MAX_FILE_SIZE have content=undefined; the backend
        // will skip them with a visible message.
      }

      setAttachedFiles(prev => [...prev, {
        path: filePath,
        name: file.name,
        size: file.size,
        mimeType: file.type || 'application/octet-stream',
        thumbnail,
        content,
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
          return { ...m, blocks: [{ type: 'text', html: `<em>${t('chat.stopped')}</em>`, source: '' }] as any }
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
    const { quotedSelections } = useStore.getState()
    const hasContent = content || attachedFiles.length > 0 || quotedSelections.length > 0
    if (!hasContent || sendingRef.current || (sessionId && useStore.getState().streamingSessionIds.has(sessionId))) return
    sendingRef.current = true
    setText('')
    const filesToSend = attachedFiles
    setAttachedFiles([])
    const selectionsToSend = [...quotedSelections]
    useStore.getState().clearQuotedSelections()

    try {
      const sid = await ensureSession()
      if (!sid) { sendingRef.current = false; setText(content); setAttachedFiles(filesToSend); return }
      await sendMessage({ sessionId: sid, content, attachedFiles: filesToSend, skills: selectedSkills.length > 0 ? selectedSkills : undefined, quotedSelections: selectionsToSend })
    } finally {
      sendingRef.current = false
    }
  }

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key !== 'Enter') return

    const ctrlOrMeta = e.ctrlKey || e.metaKey
    const shift = e.shiftKey

    let shouldSend = false
    switch (sendShortcut) {
      case 'ctrl+enter':
        shouldSend = ctrlOrMeta && !shift
        break
      case 'shift+enter':
        shouldSend = shift && !ctrlOrMeta
        break
      default: // 'enter'
        shouldSend = !ctrlOrMeta && !shift
        break
    }

    if (shouldSend) {
      e.preventDefault()
      handleSend()
      return
    }

    // Insert newline manually for modifier combos that need it
    if (ctrlOrMeta && !shift) {
      e.preventDefault()
      const el = e.currentTarget as HTMLTextAreaElement
      const { selectionStart, selectionEnd, value } = el
      const next = value.slice(0, selectionStart) + '\n' + value.slice(selectionEnd)
      setText(next)
      requestAnimationFrame(() => {
        el.selectionStart = el.selectionEnd = selectionStart + 1
      })
    }
    // Enter without modifiers in ctrl+enter / shift+enter mode:
    // let browser insert newline naturally (don't preventDefault)
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
  const placeholder = !isConnected
    ? t('app.connecting')
    : !sessionId
      ? t('input.startChat')
      : sendShortcut === 'ctrl+enter'
        ? t('chat.placeholderCtrlEnter')
        : sendShortcut === 'shift+enter'
          ? t('chat.placeholder', { modifier: 'Shift+Enter', other: 'Enter' })
          : t('chat.placeholderEnter')

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
          {quotedSelections.length > 0 && (
            <div className={styles.attachmentsArea}>
              {quotedSelections.map(qs => (
                <QuotedSelectionCard
                  key={qs.id}
                  text={qs.text}
                  filePath={qs.filePath}
                  onRemove={() => removeQuotedSelection(qs.id)}
                />
              ))}
            </div>
          )}
          {fimEnabled ? (
            <CodeMirrorInput
              value={text}
              onChange={setText}
              onSend={handleSend}
              onPaste={handlePaste}
              placeholder={placeholder}
              disabled={!isConnected}
            />
          ) : (
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
          )}
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
              title={t('input.insertImage')}
            >
              <IconImage size={15} />
            </button>
            <button
              onClick={() => fileInputRef.current?.click()}
              disabled={!isConnected || streaming}
              className={styles.fileActionBtn}
              title={t('input.addAttachment')}
            >
              <IconPaperclip size={15} />
            </button>
            <div className={styles.skillBtnWrap} ref={skillPopoverRef}>
              <button
                onClick={() => setShowSkillPopover(v => !v)}
                disabled={!isConnected || streaming}
                className={`${styles.skillBtn} ${selectedSkills.length > 0 ? styles.skillBtnActive : ''}`}
                title={t('input.loadSkills')}
              >
                <IconSparkles size={13} />
                {selectedSkills.length > 0 && <span>{selectedSkills.length}</span>}
              </button>
              {showSkillPopover && (
                <div className={styles.skillPopover}>
                  <div className={styles.skillPopoverHeader}>{t('input.availableSkills')}</div>
                  {availableSkills.length === 0 ? (
                    <div className={styles.skillPopoverEmpty}>{t('input.noSkills')}</div>
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
                {t('chat.stop')}
              </button>
            ) : (
              <div className={styles.sendSplit}>
                <button
                  onClick={handleSend}
                  disabled={(!text.trim() && attachedFiles.length === 0 && quotedSelections.length === 0) || !isConnected}
                  className={styles.sendSplitMain}
                >
                  {t('chat.send')}
                </button>
                <button
                  className={styles.sendSplitCaret}
                  title={t('input.sendShortcut')}
                  onClick={(e) => {
                    e.stopPropagation()
                    const btn = e.currentTarget
                    const rect = btn.getBoundingClientRect()
                    // Toggle dropdown via a data attribute on the button
                    const open = btn.dataset.open === '1'
                    btn.dataset.open = open ? '0' : '1'
                  }}
                >
                  <svg width="8" height="5" viewBox="0 0 8 5"><path d="M0 0l4 5 4-5z" fill="currentColor"/></svg>
                </button>
                <div className={styles.sendShortcutMenu} onMouseDown={(e) => e.preventDefault()}>
                  {(['enter', 'ctrl+enter', 'shift+enter'] as SendShortcut[]).map(k => (
                    <div
                      key={k}
                      className={`${styles.sendShortcutItem} ${sendShortcut === k ? styles.sendShortcutItemActive : ''}`}
                      onClick={(e) => {
                        e.stopPropagation()
                        useStore.getState().setSendShortcut(k)
                        // Close the dropdown
                        const caret = (e.currentTarget.closest(`.${styles.sendSplit}`) as HTMLElement)?.querySelector(`.${styles.sendSplitCaret}`) as HTMLElement | null
                        if (caret) { caret.dataset.open = '0'; caret.blur() }
                      }}
                    >
                      {k === 'enter' ? '↵ Enter' : k === 'ctrl+enter' ? '⌃ Ctrl+Enter' : '⇧ Shift+Enter'}
                      {sendShortcut === k && <IconCheck size={11} style={{ marginLeft: 'auto' }} />}
                    </div>
                  ))}
                </div>
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  )
}

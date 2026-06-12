import React, { useRef, useEffect, useState, useCallback } from 'react'
import { useStore } from '../../stores'
import { useLocale } from '../../i18n'
import { IconFilePlus, IconFileText, IconEdit, IconTrash, IconSend, IconFolderOpen, IconPlus, IconSparkles, IconExternalLink } from '../../utils/icons'
import { renderMarkdown } from '../../utils/markdown'
import { sanitizeHtml } from '../../utils/markdown-sanitizer'
import Select from '../shared/Select'
import styles from './WriteWorkspaceView.module.css'
import { CodeMirrorEditor } from './CodeMirrorEditor'
import WriteChatPanel from './WriteChatPanel'
import { sendMessage } from '../../services/sendMessage'
import { loomRpc } from '../../services/jsonrpc'
import { streamBufferManager } from '../../services/stream-buffer'

interface FileEntry { name: string; is_directory: boolean }
type PreviewMode = 'source' | 'split' | 'preview'
type ModalKind = 'none' | 'newFile' | 'rename' | 'delete'

const FILE_EXT_OPTIONS = [
  { value: '.md', label: '.md' },
  { value: '.txt', label: '.txt' },
]

export const WriteWorkspaceView: React.FC = () => {
  const appMode = useStore(s => s.appMode)
  const createSession = useStore(s => s.createSession)
  const sessionWorkspaces = useStore(s => s.sessionWorkspaces)
  const defaultWorkspace = useStore(s => s.defaultWorkspace)
  const writeFileSidebarOpen = useStore(s => s.writeFileSidebarOpen)
  const setFimEnabled = useStore(s => s.setFimEnabled)
  const fimEnabled = useStore(s => s.fimEnabled)
  const evictSession = useStore(s => s.evictSession)

  const { t } = useLocale()

  const previewOptions = [
    { value: 'source' as const, label: t('write.previewEdit') },
    { value: 'split' as const, label: t('write.previewSplit') },
    { value: 'preview' as const, label: t('write.previewPreview') },
  ]

  const quickSuggestions = [
    { key: 'write.suggestionPolish', text: t('write.suggestionPolish') },
    { key: 'write.suggestionTranslate', text: t('write.suggestionTranslate') },
    { key: 'write.suggestionExpand', text: t('write.suggestionExpand') },
    { key: 'write.suggestionSummarize', text: t('write.suggestionSummarize') },
    { key: 'write.suggestionFormal', text: t('write.suggestionFormal') },
  ]

  const [workspaceRoot, setWorkspaceRoot] = useState<string | null>(null)
  const [activeFilePath, setActiveFilePath] = useState<string | null>(null)
  const [fileContent, setFileContent] = useState('')
  const [lastSaved, setLastSaved] = useState('')
  const [dirty, setDirty] = useState(false)
  const [previewMode, setPreviewMode] = useState<PreviewMode>('source')

  const [files, setFiles] = useState<FileEntry[]>([])
  const [loadingFiles, setLoadingFiles] = useState(false)
  const [toast, setToast] = useState<string | null>(null)

  const [modal, setModal] = useState<{ kind: ModalKind; targetName?: string }>({ kind: 'none' })
  const [modalInput, setModalInput] = useState('')
  const [fileExt, setFileExt] = useState('.md')
  const modalInputRef = useRef<HTMLInputElement>(null)

  const timerRef = useRef<ReturnType<typeof setTimeout>>()
  const toastTimerRef = useRef<ReturnType<typeof setTimeout>>()
  // Ref to deduplicate in-flight ensureSession calls (prevents race-condition duplicate sessions)
  const pendingSessions = useRef<Record<string, Promise<string>>>({})
  // Ref to avoid stale fileContent in callbacks without re-creating them on every keystroke
  const fileContentRef = useRef(fileContent)
  fileContentRef.current = fileContent

  // 写作专用会话 — 按文件隔离，每个文件独立会话
  const [writeFileSessions, setWriteFileSessions] = useState<Record<string, string>>(() => {
    try {
      const raw = localStorage.getItem('loom:writeFileSessions')
      return raw ? JSON.parse(raw) : {}
    } catch { return {} }
  })
  const [assistantPanelOpen, setAssistantPanelOpen] = useState(true)
  const [assistantText, setAssistantText] = useState('')
  const [editorFontSize, setEditorFontSize] = useState(() => {
    const saved = localStorage.getItem('loom:writeFontSize')
    return saved ? Number(saved) : 14
  })

  const showToast = useCallback((msg: string) => {
    setToast(msg)
    clearTimeout(toastTimerRef.current)
    toastTimerRef.current = setTimeout(() => setToast(null), 2500)
  }, [])

  // 初始化工作区
  useEffect(() => {
    (async () => {
      try { const v = await (window as any).loom?.getPreference?.('writeWorkspace', '') || ''; if (v) setWorkspaceRoot(v) } catch {}
      // Re-evaluate when defaultWorkspace/sessionWorkspaces become available (loaded after mount)
      if (!workspaceRoot) {
        const ws = defaultWorkspace || Object.values(sessionWorkspaces)[0]
        if (ws) setWorkspaceRoot(ws)
      }
    })()
  }, [workspaceRoot, defaultWorkspace, sessionWorkspaces])

  // 确保文件有对应的写作会话（按文件隔离，带竞态去重）
  const ensureSession = useCallback(async (filePath: string) => {
    // 已有会话，直接返回
    if (writeFileSessions[filePath]) return writeFileSessions[filePath]
    // 已有进行中的请求，复用
    if (pendingSessions.current[filePath]) return pendingSessions.current[filePath]

    // 创建并缓存 Promise
    const promise = (async () => {
      const sid = await createSession()
      const sessionTitle = '[写] ' + (filePath.split('/').pop() || filePath)
      try { await loomRpc('session.rename', { session_id: sid, title: sessionTitle }) } catch {}
      // Use functional updater to avoid stale closure — concurrent ensureSession
      // calls for different files won't lose each other's mappings.
      setWriteFileSessions(prev => {
        const updated = { ...prev, [filePath]: sid }
        localStorage.setItem('loom:writeFileSessions', JSON.stringify(updated))
        return updated
      })
      return sid
    })()

    pendingSessions.current[filePath] = promise
    try {
      return await promise
    } finally {
      delete pendingSessions.current[filePath]
    }
  }, [writeFileSessions, createSession])

  useEffect(() => {
    if (modal.kind !== 'none') setTimeout(() => modalInputRef.current?.focus(), 50)
  }, [modal.kind])

  // 文件列表
  const loadFiles = useCallback(async () => {
    if (!workspaceRoot) return
    setLoadingFiles(true)
    try {
      const result = await loomRpc<{ ok: boolean; entries: FileEntry[] }>('vfs.list_directory', { workspace_root: workspaceRoot, path: '.' })
      if (result.ok) {
        const textFiles = result.entries.filter(e => !e.is_directory && /\.(md|txt|markdown)$/i.test(e.name))
        setFiles(textFiles.sort((a, b) => a.name.localeCompare(b.name)))
      }
    } catch (e: any) { showToast(t('write.readDirFailed', { error: String(e).slice(0, 40) })) }
    setLoadingFiles(false)
  }, [workspaceRoot, showToast, t])

  useEffect(() => { if (workspaceRoot) loadFiles() }, [workspaceRoot, loadFiles])

  // 打开文件
  const openFile = useCallback(async (name: string) => {
    if (!workspaceRoot) return
    if (dirty && activeFilePath) await saveFile(activeFilePath, fileContent)
    try {
      const result = await loomRpc<{ ok: boolean; content: string }>('vfs.read_file', { workspace_root: workspaceRoot, path: name })
      if (result.ok) {
        setActiveFilePath(name); setFileContent(result.content); setLastSaved(result.content); setDirty(false)
      } else { showToast(t('write.readFailed')) }
    } catch (e: any) { showToast(t('write.openFailed', { error: String(e).slice(0, 40) })) }
  }, [workspaceRoot, dirty, activeFilePath, fileContent, showToast, t])

  // 保存
  const saveFile = useCallback(async (path: string, content: string) => {
    if (!workspaceRoot) return
    try {
      await loomRpc('vfs.write_file', { workspace_root: workspaceRoot, path, content })
      setLastSaved(content); setDirty(false)
    } catch (e: any) { showToast(t('write.saveFailed', { error: String(e).slice(0, 40) })) }
  }, [workspaceRoot, showToast, t])

  // 自动保存
  useEffect(() => {
    if (dirty && activeFilePath) {
      timerRef.current = setTimeout(() => saveFile(activeFilePath, fileContent), 650)
      return () => clearTimeout(timerRef.current)
    }
  }, [fileContent, dirty, activeFilePath, saveFile])

  // Ctrl+滚轮缩放字体 (仅 write 模式生效)
  useEffect(() => {
    if (appMode !== 'write') return
    const h = (e: WheelEvent) => {
      if (!e.ctrlKey && !e.metaKey) return
      e.preventDefault()
      setEditorFontSize(prev => {
        const next = prev - Math.sign(e.deltaY)
        const clamped = Math.max(10, Math.min(32, next))
        localStorage.setItem('loom:writeFontSize', String(clamped))
        return clamped
      })
    }
    window.addEventListener('wheel', h, { passive: false })
    return () => window.removeEventListener('wheel', h)
  }, [appMode])

  // Ctrl+S (仅 write 模式生效)
  useEffect(() => {
    if (appMode !== 'write') return
    const h = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key === 's') {
        e.preventDefault()
        if (activeFilePath && dirty) saveFile(activeFilePath, fileContent)
      }
    }
    window.addEventListener('keydown', h)
    return () => window.removeEventListener('keydown', h)
  }, [appMode, activeFilePath, dirty, fileContent, saveFile])

  // 弹窗操作
  const confirmModal = useCallback(async () => {
    if (!workspaceRoot) return
    try {
      if (modal.kind === 'newFile') {
        const raw = modalInput.trim()
        if (!raw) return
        const name = /\.(md|txt|markdown)$/i.test(raw) ? raw : raw + fileExt
        const title = raw.replace(/\.(md|txt|markdown)$/i, '')
        const content = '# ' + title + '\n\n'
        await loomRpc('vfs.write_file', { workspace_root: workspaceRoot, path: name, content })
        setActiveFilePath(name); setFileContent(content); setLastSaved(content); setDirty(false)
        loadFiles(); showToast(t('write.fileCreated'))
      } else if (modal.kind === 'rename' && modal.targetName) {
        const newName = modalInput.trim()
        if (!newName || newName === modal.targetName) return
        await loomRpc('vfs.rename', { workspace_root: workspaceRoot, path: modal.targetName, new_name: newName })
        if (activeFilePath === modal.targetName) setActiveFilePath(newName)
        loadFiles(); showToast(t('write.fileRenamed'))
      } else if (modal.kind === 'delete' && modal.targetName) {
        await loomRpc('vfs.delete', { workspace_root: workspaceRoot, path: modal.targetName })
        if (activeFilePath === modal.targetName) { setActiveFilePath(null); setFileContent(''); setDirty(false) }
        // Clean up orphaned session for the deleted file
        const sessions = { ...writeFileSessions }
        if (sessions[modal.targetName]) {
          delete sessions[modal.targetName]
          setWriteFileSessions(sessions)
          localStorage.setItem('loom:writeFileSessions', JSON.stringify(sessions))
        }
        loadFiles(); showToast(t('write.fileDeleted'))
      }
      setModal({ kind: 'none' })
    } catch (e: any) { showToast(t('write.operationFailed', { error: (e?.message || String(e)).slice(0, 40) })) }
  }, [modal, modalInput, fileExt, workspaceRoot, activeFilePath, writeFileSessions, loadFiles, showToast, t])

  // 选目录
  const pickWorkspace = useCallback(async () => {
    try {
      const path = await (window as any).loom?.selectFolder?.()
      if (path) {
        setWorkspaceRoot(path); (window as any).loom?.setPreference?.('writeWorkspace', path)
        setActiveFilePath(null); setFileContent(''); setDirty(false)
      }
    } catch { /* non-critical */ }
  }, [])

  // AI 助手 — 按文件隔离会话
  const handleAssistantSend = useCallback(async (text?: string) => {
    const msg = (text || assistantText).trim()
    if (!msg) return
    // 需要先打开文件，才能使用 AI 助手
    if (!activeFilePath) {
      showToast(t('write.aiNeedFile'))
      return
    }
    const sid = await ensureSession(activeFilePath)

    // Only clear input on success; on failure, keep the text
    const wasSuggestion = !!text
    if (!wasSuggestion) setAssistantText('')

    try {
      // LLM-facing prompt — 包含当前文件内容作为上下文
      const currentContent = fileContentRef.current
      const content = `[写作上下文]\n当前文件: ${activeFilePath}\n\n${currentContent}\n\n[用户指令]\n${msg}`
      await sendMessage({ sessionId: sid, content, permissionMode: 'operate' })
      // Response will appear inline in WriteChatPanel — no mode switch needed
    } catch {
      // Restore text on failure (only for manual input, not suggestion buttons)
      if (!wasSuggestion) setAssistantText(msg)
      showToast(t('write.sendFailed'))
    }
  }, [assistantText, activeFilePath, ensureSession, fileContentRef, showToast, t])

  // 清空当前文件的对话，下次发送时自动创建新会话
  const handleNewChat = useCallback(() => {
    if (!activeFilePath) return
    const sid = writeFileSessions[activeFilePath]
    if (sid) {
      evictSession(sid) // clear in-memory messages + streaming state
      // Also clear any in-flight stream buffer for this session
      try { streamBufferManager.clear(sid) } catch {}
    }
    // Use functional updater to avoid stale closure
    setWriteFileSessions(prev => {
      const updated = { ...prev }
      delete updated[activeFilePath]
      localStorage.setItem('loom:writeFileSessions', JSON.stringify(updated))
      return updated
    })
  }, [activeFilePath, writeFileSessions, evictSession])

  // Clean up stale session mappings (called by WriteChatPanel when backend session was deleted)
  const handleStaleSession = useCallback((deadSessionId: string) => {
    setWriteFileSessions(prev => {
      const updated = { ...prev }
      let changed = false
      for (const [fp, sid] of Object.entries(updated)) {
        if (sid === deadSessionId) {
          delete updated[fp]
          changed = true
        }
      }
      if (changed) {
        localStorage.setItem('loom:writeFileSessions', JSON.stringify(updated))
      }
      return changed ? updated : prev
    })
  }, [])

  if (appMode !== 'write') return null

  const previewHtml = previewMode !== 'source' && fileContent ? sanitizeHtml(renderMarkdown(fileContent)) : ''
  const editorPlaceholder = activeFilePath ? t('write.startWriting') : t('write.selectOrNewFile')

  return (
    <div className={styles.root}>

      {/* ===== 工具栏 ===== */}
      <div className={styles.toolbar}>
        {!workspaceRoot ? (
          <button className={styles.toolbarBtn} onClick={pickWorkspace}>
            <IconFolderOpen size={12} />{t('write.selectDirectory')}
          </button>
        ) : (
          <div className={styles.toolbarGroup}>
            <span className={styles.workspacePath} onClick={pickWorkspace} title={t('write.clickSwitchDir')}>
              {workspaceRoot.split(/[/\\]/).pop() || workspaceRoot}
            </span>
            <button className={styles.toolbarBtnGhost}
              onClick={() => (window as any).loom?.openFolder?.(workspaceRoot)}
              title={t('write.openInExplorer')} style={{ padding: '2px 4px' }}>
              <IconFolderOpen size={13} />
            </button>
          </div>
        )}

        {activeFilePath && (
          <>
            <div className={styles.toolbarDivider} />
            <span className={styles.fileName}>{activeFilePath.split('/').pop()}</span>
          </>
        )}

        <div className={styles.spacer} />

        {activeFilePath && (
          <button
            className={fimEnabled ? styles.fimBtnOn : styles.fimBtnOff}
            onClick={() => setFimEnabled(!fimEnabled)}
            title={fimEnabled ? t('write.fimDisable') : t('write.fimEnable')}>
            {fimEnabled ? t('write.fimOn') : t('write.fimOff')}
          </button>
        )}

        <Select value={previewMode} options={previewOptions} onChange={setPreviewMode} variant="pill" />

        {workspaceRoot && (
          <button
            className={assistantPanelOpen ? styles.toolbarBtnAccent : styles.toolbarBtnGhost}
            onClick={() => setAssistantPanelOpen(o => !o)}
            title={assistantPanelOpen ? t('write.collapseAIPanel') : t('write.expandAIPanel')}>
            <IconSparkles size={13} />
          </button>
        )}
      </div>

      {/* ===== 主体 ===== */}
      <div className={styles.body}>
        {/* 文件侧栏 */}
        {workspaceRoot && (
          <div className={`${styles.fileSidebar} ${!writeFileSidebarOpen ? styles.fileSidebarCollapsed : ''}`}>
            <div className={styles.fileSidebarHeader}>
              <span>{t('write.fileList')}</span>
              <button className={styles.fileSidebarNewBtn}
                onClick={() => { setModalInput(''); setFileExt('.md'); setModal({ kind: 'newFile' }) }}
                title={t('write.newFile')}>
                <IconPlus size={13} />
              </button>
            </div>
            <div className={styles.fileList}>
              {loadingFiles ? (
                <div className={styles.fileListHint}>{t('common.loading')}</div>
              ) : files.length === 0 ? (
                <div className={styles.fileListHint}>
                  {t('write.noFiles')}<br />
                  <span style={{ fontSize: 10, opacity: 0.5 }}>{t('write.clickPlusNew')}</span>
                </div>
              ) : (
                files.map(f => (
                  <div key={f.name}
                    className={activeFilePath === f.name ? styles.fileItemActive : styles.fileItem}
                    onClick={() => openFile(f.name)}>
                    <IconFileText size={13} className={styles.fileItemIcon} />
                    <span className={styles.fileItemName}>{f.name}</span>
                    <div className={styles.fileItemActions}>
                      <button className={styles.fileItemAction}
                        onClick={e => { e.stopPropagation(); setModalInput(f.name); setModal({ kind: 'rename', targetName: f.name }) }}
                        title={t('common.rename')}>
                        <IconEdit size={11} />
                      </button>
                      <button className={styles.fileItemAction}
                        onClick={e => { e.stopPropagation(); setModal({ kind: 'delete', targetName: f.name }) }}
                        title={t('common.delete')}>
                        <IconTrash size={11} />
                      </button>
                    </div>
                  </div>
                ))
              )}
            </div>
          </div>
        )}

        {/* 内容区 */}
        {!workspaceRoot ? (
          <div className={styles.emptyState}>
            <IconFolderOpen size={48} className={styles.emptyIcon} />
            <span>{t('write.selectDirStart')}</span>
            <button className={styles.workspacePromptBtn} onClick={pickWorkspace}>
              <IconFolderOpen size={16} />{t('write.selectDirectory')}
            </button>
          </div>
        ) : !activeFilePath ? (
          <div className={styles.emptyState}>
            <IconFileText size={40} className={styles.emptyIcon} />
            <span>{t('write.selectOrNewFilePrompt')}</span>
            <button className={styles.workspacePromptBtn}
              onClick={() => { setModalInput(''); setFileExt('.md'); setModal({ kind: 'newFile' }) }}>
              <IconFilePlus size={16} />{t('write.newFile')}
            </button>
          </div>
        ) : previewMode === 'preview' ? (
          <div className={styles.editorArea}>
            <div className={styles.preview} dangerouslySetInnerHTML={{ __html: previewHtml }} />
          </div>
        ) : previewMode === 'split' ? (
          <div className={styles.editorArea}>
            <div style={{ width: '50%', height: '100%', borderRight: '1px solid var(--border)' }}>
              <CodeMirrorEditor
                value={fileContent}
                onChange={v => { setFileContent(v); setDirty(true) }}
                placeholder={editorPlaceholder}
                fontSize={editorFontSize}
              />
            </div>
            <div className={styles.preview} dangerouslySetInnerHTML={{ __html: previewHtml }} />
          </div>
        ) : (
          <div className={styles.editorArea}>
            <CodeMirrorEditor
              value={fileContent}
              onChange={v => { setFileContent(v); setDirty(true) }}
              placeholder={editorPlaceholder}
              fontSize={editorFontSize}
            />
          </div>
        )}

        {/* AI 右侧面板 — 常显 */}
        {workspaceRoot && (
          <div className={`${styles.assistantPanel} ${!assistantPanelOpen ? styles.assistantPanelCollapsed : ''}`}>
            <div className={styles.assistantPanelHeader}>
              <IconSparkles size={13} />
              <span>{t('write.aiWritingAssistant')}</span>
            </div>

            {/* WriteChatPanel — inline conversation for the current file */}
            <WriteChatPanel
              sessionId={activeFilePath ? writeFileSessions[activeFilePath] || null : null}
              activeFileName={activeFilePath ? activeFilePath.split('/').pop() || null : null}
              quickSuggestions={quickSuggestions}
              onSuggestionClick={(text: string) => handleAssistantSend(text)}
              onNewChat={handleNewChat}
              onStaleSession={handleStaleSession}
            />

            <div className={styles.assistantPanelFooter}>
              <div className={styles.assistantInputRow}>
                <input className={styles.assistantInput}
                  value={assistantText}
                  onChange={e => setAssistantText(e.target.value)}
                  onKeyDown={e => {
                    if (e.key !== 'Enter' || e.shiftKey) return
                    e.preventDefault(); handleAssistantSend()
                  }}
                  placeholder={t('write.inputInstruction')}
                />
                <button className={styles.assistantSendBtn}
                  onClick={() => handleAssistantSend()}
                  disabled={!assistantText.trim()} title={t('chat.send')}>
                  <IconSend size={13} />
                </button>
              </div>
            </div>
          </div>
        )}
      </div>

      {/* ===== 弹窗 ===== */}
      {modal.kind !== 'none' && (
        <div className={styles.modalBackdrop} onClick={() => setModal({ kind: 'none' })} />
      )}
      {modal.kind !== 'none' && (
        <div className={styles.modalDialog}>
          <div className={styles.modalTitle}>
            {modal.kind === 'newFile' ? t('write.newFile') : modal.kind === 'rename' ? t('common.rename') : t('write.confirmDeleteTitle')}
          </div>
          {modal.kind === 'delete' ? (
            <>
              <div style={{ fontSize: 13, color: 'var(--text-secondary)', marginBottom: 16, lineHeight: 1.5 }}>
                {t('write.deleteConfirmMsg', { name: modal.targetName || '' })}
              </div>
              <div className={styles.modalFooter}>
                <button className={styles.modalBtnCancel} onClick={() => setModal({ kind: 'none' })}>{t('common.cancel')}</button>
                <button className={styles.modalBtnDanger} onClick={confirmModal}>{t('common.delete')}</button>
              </div>
            </>
          ) : (
            <>
              {modal.kind === 'newFile' ? (
                <div style={{ display: 'flex', gap: 8, marginBottom: 12 }}>
                  <input ref={modalInputRef} className={styles.modalInput}
                    style={{ flex: 1, marginBottom: 0 }}
                    value={modalInput}
                    onChange={e => setModalInput(e.target.value)}
                    onKeyDown={e => { if (e.key === 'Enter') confirmModal(); if (e.key === 'Escape') setModal({ kind: 'none' }) }}
                    placeholder={t('write.fileNamePlaceholder')} />
                  <Select value={fileExt} options={FILE_EXT_OPTIONS} onChange={setFileExt} variant="pill" />
                </div>
              ) : (
                <input ref={modalInputRef} className={styles.modalInput}
                  value={modalInput}
                  onChange={e => setModalInput(e.target.value)}
                  onKeyDown={e => { if (e.key === 'Enter') confirmModal(); if (e.key === 'Escape') setModal({ kind: 'none' }) }}
                  placeholder={t('write.newFileName')} />
              )}
              <div className={styles.modalFooter}>
                <button className={styles.modalBtnCancel} onClick={() => setModal({ kind: 'none' })}>{t('common.cancel')}</button>
                <button className={styles.modalBtnConfirm} onClick={confirmModal}>{t('common.confirm')}</button>
              </div>
            </>
          )}
        </div>
      )}

      {toast && <div className={styles.toast}>{toast}</div>}
    </div>
  )
}

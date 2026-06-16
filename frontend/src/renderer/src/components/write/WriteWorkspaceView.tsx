import React, { useEffect, useRef, useCallback } from 'react'
import { useStore } from '../../stores'
import { useWriteStore } from '../../stores/write'
import { useLocale } from '../../i18n'
import { loomRpc } from '../../services/jsonrpc'
import { sendMessage } from '../../services/sendMessage'
import { streamBufferManager } from '../../services/stream-buffer'
import { startWatchingFile, stopWatchingFile } from '../../write/write-file-watch'
import { WriteSidebar } from './WriteSidebar'
import WriteToolbar from './WriteToolbar'
import { WriteDocumentPane } from './WriteDocumentPane'
import { WriteAssistantPanel } from './WriteAssistantPanel'
import { WriteFileDialogs } from './WriteFileDialogs'
import { WriteInlineAgent } from './WriteInlineAgent'
import styles from './WriteWorkspaceView.module.css'

export const WriteWorkspaceView: React.FC = () => {
  const { t } = useLocale()
  const appMode = useStore(s => s.appMode)
  const writeFileSidebarOpen = useStore(s => s.writeFileSidebarOpen)
  const toggleWriteFileSidebar = useStore(s => s.toggleWriteFileSidebar)
  const createSession = useStore(s => s.createSession)
  const evictSession = useStore(s => s.evictSession)

  const {
    workspaceRoot, setWorkspaceRoot, autoSaveIntervalMs, fontSize, setFontSize,
    activeFilePath, fileContent, saveStatus, setSaveStatus,
    assistantOpen, toggleAssistant, setModalState, showToast, clearToast, toastMessage,
    fileThreads, setFileThread, removeFileThread,
    setFileContent, setFileSize, setFileTruncated,
  } = useWriteStore()

  // ── Non-state refs ──
  const fileContentRef = useRef(fileContent); fileContentRef.current = fileContent
  const pendingSessions = useRef<Record<string, Promise<string>>>({})

  // ── Quick suggestions ──
  const suggestions = [
    { key: 'polish', text: t('write.suggestionPolish') },
    { key: 'translate', text: t('write.suggestionTranslate') },
    { key: 'expand', text: t('write.suggestionExpand') },
    { key: 'summarize', text: t('write.suggestionSummarize') },
    { key: 'formal', text: t('write.suggestionFormal') },
  ]

  // ── Handlers ──

  const handleSelectWorkspace = useCallback(async () => {
    try { const p = await (window as any).loom?.selectFolder?.(); if (p) setWorkspaceRoot(p) } catch {}
  }, [setWorkspaceRoot])

  const handleNewFile = useCallback(() => setModalState('newFile'), [setModalState])

  const handleSave = useCallback(async () => {
    if (!workspaceRoot || !activeFilePath) return
    try {
      setSaveStatus('saving')
      await loomRpc('vfs.write_file', { workspace_root: workspaceRoot, path: activeFilePath, content: fileContentRef.current })
      setSaveStatus('saved')
    } catch { setSaveStatus('error'); showToast('error', t('write.saveFailed')) }
  }, [workspaceRoot, activeFilePath, setSaveStatus, showToast, t])

  const ensureSession = useCallback(async (filePath: string): Promise<string> => {
    if (fileThreads[filePath]) return fileThreads[filePath]
    if (pendingSessions.current[filePath]) return pendingSessions.current[filePath]
    const p = (async () => {
      const sid = await createSession()
      try { await loomRpc('session.rename', { session_id: sid, title: '[写] ' + (filePath.split('/').pop() || filePath) }) } catch {}
      setFileThread(filePath, sid); return sid
    })()
    pendingSessions.current[filePath] = p
    try { return await p } finally { delete pendingSessions.current[filePath] }
  }, [fileThreads, createSession, setFileThread])

  const handleAssistantSend = useCallback(async (text: string) => {
    if (!activeFilePath || !text.trim()) return
    const sid = await ensureSession(activeFilePath)
    const content = `[写作上下文]\n当前文件: ${activeFilePath}\n\n${fileContentRef.current}\n\n[用户指令]\n${text}`
    await sendMessage({ sessionId: sid, content, permissionMode: 'operate' })
  }, [activeFilePath, ensureSession])

  const handleNewChat = useCallback(() => {
    if (!activeFilePath) return
    const sid = fileThreads[activeFilePath]
    if (sid) { evictSession(sid); try { streamBufferManager.clear(sid) } catch {} }
    removeFileThread(activeFilePath)
  }, [activeFilePath, fileThreads, evictSession, removeFileThread])

  const handleStaleSession = useCallback((dead: string) => {
    for (const [fp, sid] of Object.entries(fileThreads)) { if (sid === dead) removeFileThread(fp) }
  }, [fileThreads, removeFileThread])

  // ── Effects ──

  // Autosave debounce
  useEffect(() => {
    if (saveStatus !== 'dirty' || !activeFilePath || !workspaceRoot) return
    const timer = setTimeout(async () => {
      try {
        setSaveStatus('saving')
        await loomRpc('vfs.write_file', { workspace_root: workspaceRoot, path: activeFilePath, content: fileContentRef.current })
        setSaveStatus('saved')
      } catch { setSaveStatus('error'); showToast('error', t('write.saveFailed')) }
    }, autoSaveIntervalMs)
    return () => clearTimeout(timer)
  }, [fileContent, saveStatus, activeFilePath, workspaceRoot, autoSaveIntervalMs, setSaveStatus, showToast, t])

  // Ctrl+S
  useEffect(() => {
    if (appMode !== 'write') return
    const h = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key === 's') { e.preventDefault(); if (activeFilePath && saveStatus === 'dirty') handleSave() }
    }
    window.addEventListener('keydown', h); return () => window.removeEventListener('keydown', h)
  }, [appMode, activeFilePath, saveStatus, handleSave])

  // Ctrl+Scroll font zoom
  useEffect(() => {
    if (appMode !== 'write') return
    const h = (e: WheelEvent) => {
      if (!e.ctrlKey && !e.metaKey) return
      e.preventDefault(); setFontSize(Math.max(10, Math.min(32, fontSize - Math.sign(e.deltaY))))
    }
    window.addEventListener('wheel', h, { passive: false }); return () => window.removeEventListener('wheel', h)
  }, [appMode, fontSize, setFontSize])

  // Auto-clear toast
  useEffect(() => {
    if (!toastMessage) return
    const t = setTimeout(() => clearToast(), 2800); return () => clearTimeout(t)
  }, [toastMessage, clearToast])

  // External file change watcher — reloads content when another program edits the active file
  useEffect(() => {
    if (!workspaceRoot || !activeFilePath) { stopWatchingFile(); return }
    startWatchingFile({
      filePath: activeFilePath,
      workspaceRoot,
      fileKind: 'text',
      onContentChanged: (content, size, truncated) => {
        setFileContent(content)
        setFileSize(size)
        setFileTruncated(truncated)
      },
      onImageChanged: () => {},
      onError: () => {},
    })
    return () => { stopWatchingFile() }
  }, [workspaceRoot, activeFilePath, setFileContent, setFileSize, setFileTruncated])

  // ── Render ──

  if (appMode !== 'write') return null

  return (
    <div className={styles.root}>
      {workspaceRoot && (
        <WriteSidebar onSelectWorkspace={handleSelectWorkspace} onNewFile={handleNewFile} />
      )}
      {/* Collapsed sidebar toggle strip — reads from main UI store, same as titlebar button */}
      {workspaceRoot && !writeFileSidebarOpen && (
        <div
          onClick={toggleWriteFileSidebar}
          style={{
            width: 4, cursor: 'pointer', background: 'var(--border)',
            transition: 'background 0.15s',
          }}
          onMouseEnter={(e) => (e.currentTarget.style.background = 'var(--accent)')}
          onMouseLeave={(e) => (e.currentTarget.style.background = 'var(--border)')}
          title={t('write.expandSidebar', 'Expand Sidebar')}
        />
      )}
      <div className={styles.body}>
        <div style={{ flex: 1, display: 'flex', flexDirection: 'column', minWidth: 0 }}>
          {workspaceRoot && (
            <WriteToolbar onNewFile={handleNewFile} onSave={handleSave} onToggleAssistant={toggleAssistant} />
          )}
          <WriteDocumentPane onSelectWorkspace={handleSelectWorkspace} />
        </div>
        {workspaceRoot && assistantOpen && activeFilePath && (
          <WriteAssistantPanel
            quickSuggestions={suggestions} onSend={handleAssistantSend}
            onNewChat={handleNewChat} onStaleSession={handleStaleSession}
          />
        )}
      </div>
      <WriteInlineAgent
        editorValue={fileContent}
        onApplyEdit={(newContent) => {
          useWriteStore.getState().setFileContent(newContent)
          useWriteStore.getState().setSaveStatus('dirty')
        }}
        onSendToAssistant={handleAssistantSend}
      />
      <WriteFileDialogs />
      {toastMessage && <div className={styles.toast}>{toastMessage.text}</div>}
    </div>
  )
}

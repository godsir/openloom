import React, { useEffect, useRef, useCallback, useState } from 'react'
import { useStore } from '../../stores'
import { getWriteThreadKey, useWriteStore } from '../../stores/write'
import { useLocale } from '../../i18n'
import { loomRpc } from '../../services/jsonrpc'
import { sendMessage } from '../../services/sendMessage'
import { streamBufferManager } from '../../services/stream-buffer'
import { startWatchingFile, stopWatchingFile } from '../../write/write-file-watch'
import { WriteSidebar } from './WriteSidebar'
import WriteToolbar from './WriteToolbar'
import { WriteDocumentPane } from './WriteDocumentPane'
import { WriteAssistantPanel } from './WriteAssistantPanel'
import { WriteAssistantWindow } from './WriteAssistantWindow'
import { WriteFileDialogs } from './WriteFileDialogs'
import styles from './WriteWorkspaceView.module.css'
import { composeWritePrompt, limitWriteContext } from '../../write/quoted-selection'
import { resolveAgentPreset } from '../../write/agent-presets'
import { guardWriteNavigation } from '../../write/navigation-guard'
import { openWriteAssistantWindow } from '../../write/write-assistant-window'

export const WriteWorkspaceView: React.FC = () => {
  const { t } = useLocale()
  const appMode = useStore(s => s.appMode)
  const writeFileSidebarOpen = useStore(s => s.writeFileSidebarOpen)
  const toggleWriteFileSidebar = useStore(s => s.toggleWriteFileSidebar)
  const createSession = useStore(s => s.createSession)
  const evictSession = useStore(s => s.evictSession)

  const {
    workspaceRoot, setWorkspaceRoot, autoSaveIntervalMs, fontSize, setFontSize,
    activeFilePath, fileContent, saveStatus, setSaveStatus, fileTruncated,
    assistantOpen, setModalState, showToast, clearToast, toastMessage,
    fileThreads, setFileThread, removeFileThread, writingAgentName, retrievalEnabled,
    setFileContent, setFileSize, setFileTruncated,
  } = useWriteStore()

  // ── Non-state refs ──
  const fileContentRef = useRef(fileContent); fileContentRef.current = fileContent
  const pendingSessions = useRef<Record<string, Promise<string>>>({})
  const [assistantWindow, setAssistantWindow] = useState<Window | null>(null)

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
    try {
      const p = await (window as any).loom?.selectFolder?.()
      if (p && p !== workspaceRoot && await guardWriteNavigation()) setWorkspaceRoot(p)
    } catch {}
  }, [workspaceRoot, setWorkspaceRoot])

  const handleNewFile = useCallback(() => setModalState('newFile'), [setModalState])

  const handleSave = useCallback(async () => {
    if (!workspaceRoot || !activeFilePath || useWriteStore.getState().fileTruncated) return
    const contentSnapshot = fileContentRef.current
    try {
      setSaveStatus('saving')
      await loomRpc('vfs.write_file', { workspace_root: workspaceRoot, path: activeFilePath, content: contentSnapshot })
      setSaveStatus(fileContentRef.current === contentSnapshot ? 'saved' : 'dirty')
    } catch { setSaveStatus('error'); showToast('error', t('write.saveFailed')) }
  }, [workspaceRoot, activeFilePath, setSaveStatus, showToast, t])

  const ensureSession = useCallback(async (filePath: string): Promise<string> => {
    if (!workspaceRoot) throw new Error('No writing workspace selected')
    const threadKey = getWriteThreadKey(workspaceRoot, filePath)
    if (fileThreads[threadKey]) return fileThreads[threadKey]
    if (pendingSessions.current[threadKey]) return pendingSessions.current[threadKey]
    const p = (async () => {
      const sid = await createSession()
      try { await loomRpc('session.rename', { session_id: sid, title: '[写] ' + (filePath.split('/').pop() || filePath) }) } catch {}
      const agentName = writingAgentName || 'default'
      await loomRpc('session.bind_agent', { session_id: sid, agent_config_name: agentName })
      useStore.getState().setSessionAgentBinding(sid, agentName)
      setFileThread(threadKey, sid); return sid
    })()
    pendingSessions.current[threadKey] = p
    try { return await p } finally { delete pendingSessions.current[threadKey] }
  }, [workspaceRoot, fileThreads, createSession, setFileThread, writingAgentName])

  const handleAssistantSend = useCallback(async (text: string) => {
    const writeState = useWriteStore.getState()
    const appState = useStore.getState()
    const snapshot = {
      workspaceRoot: writeState.workspaceRoot,
      filePath: writeState.activeFilePath,
      fileContent: writeState.fileContent,
      presetId: writeState.agentPresetId,
      quotes: [...writeState.quotedSelections],
      permissionMode: appState.permissionMode,
      writingAgentName: writeState.writingAgentName,
      retrievalEnabled: writeState.retrievalEnabled,
    }
    if (!snapshot.workspaceRoot || !snapshot.filePath || !text.trim()) return
    const sid = await ensureSession(snapshot.filePath)
    const agentName = snapshot.writingAgentName || 'default'
    await loomRpc('session.bind_agent', { session_id: sid, agent_config_name: agentName })
    useStore.getState().setSessionAgentBinding(sid, agentName)
    const persona = resolveAgentPreset(snapshot.presetId)
    let retrievalContext: string | undefined
    if (snapshot.retrievalEnabled) {
      type RagResult = { ok: boolean; results?: Array<{ file_path: string; text: string; score: number }> }
      let rag = await loomRpc<RagResult>('write.search_workspace', {
        workspace_root: snapshot.workspaceRoot,
        query: text.trim(),
        top_k: 4,
      })
      if (!rag.ok) {
        await loomRpc('write.index_workspace', { workspace_root: snapshot.workspaceRoot })
        rag = await loomRpc<RagResult>('write.search_workspace', {
          workspace_root: snapshot.workspaceRoot,
          query: text.trim(),
          top_k: 4,
        })
      }
      if (rag.ok && rag.results?.length) {
        retrievalContext = rag.results
          .map((item) => `[${item.file_path}]\n${item.text}`)
          .join('\n\n')
          .slice(0, 8_000)
      }
    }
    const content = composeWritePrompt(
      text.trim(),
      snapshot.filePath,
      limitWriteContext(snapshot.fileContent),
      snapshot.quotes.length ? snapshot.quotes : undefined,
      retrievalContext,
      persona?.persona,
    )
    await sendMessage({ sessionId: sid, content, permissionMode: snapshot.permissionMode })
    useWriteStore.getState().removeQuotedSelections(snapshot.quotes.map((q) => q.id))
  }, [ensureSession])

  useEffect(() => {
    if (!workspaceRoot || !retrievalEnabled) return
    let cancelled = false
    loomRpc('write.index_workspace', { workspace_root: workspaceRoot })
      .catch(() => {})
      .finally(() => { if (cancelled) return })
    return () => { cancelled = true }
  }, [workspaceRoot, retrievalEnabled])

  const handleNewChat = useCallback(() => {
    if (!workspaceRoot || !activeFilePath) return
    const threadKey = getWriteThreadKey(workspaceRoot, activeFilePath)
    const sid = fileThreads[threadKey]
    if (sid) {
      streamBufferManager.markCancelled(sid)
      loomRpc('chat.stop', { session_id: sid }).catch(() => {})
      evictSession(sid)
      try { streamBufferManager.clear(sid) } catch {}
    }
    removeFileThread(threadKey)
  }, [workspaceRoot, activeFilePath, fileThreads, evictSession, removeFileThread])

  const handleAssistantClose = useCallback(() => {
    setAssistantWindow(null)
    useWriteStore.getState().setAssistantOpen(false)
  }, [])

  const handleToggleAssistant = useCallback(() => {
    if (useWriteStore.getState().assistantOpen) {
      setAssistantWindow(null)
      useWriteStore.getState().setAssistantOpen(false)
      return
    }

    const popup = openWriteAssistantWindow()
    if (!popup) {
      showToast('error', '无法打开 AI 写作助手窗口')
      return
    }
    setAssistantWindow(popup)
    useWriteStore.getState().setAssistantOpen(true)
  }, [showToast])

  const handleStaleSession = useCallback((dead: string) => {
    for (const [fp, sid] of Object.entries(fileThreads)) { if (sid === dead) removeFileThread(fp) }
  }, [fileThreads, removeFileThread])

  // ── Effects ──

  // Autosave debounce
  useEffect(() => {
    if (saveStatus !== 'dirty' || !activeFilePath || !workspaceRoot || fileTruncated) return
    const timer = setTimeout(async () => {
      const contentSnapshot = fileContentRef.current
      try {
        setSaveStatus('saving')
        await loomRpc('vfs.write_file', { workspace_root: workspaceRoot, path: activeFilePath, content: contentSnapshot })
        setSaveStatus(fileContentRef.current === contentSnapshot ? 'saved' : 'dirty')
      } catch { setSaveStatus('error'); showToast('error', t('write.saveFailed')) }
    }, autoSaveIntervalMs)
    return () => clearTimeout(timer)
  }, [fileContent, saveStatus, activeFilePath, workspaceRoot, autoSaveIntervalMs, fileTruncated, setSaveStatus, showToast, t])

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
        const current = useWriteStore.getState()
        if (content === fileContentRef.current) return true
        if (
          current.workspaceRoot !== workspaceRoot ||
          current.activeFilePath !== activeFilePath ||
          current.saveStatus !== 'saved'
        ) return false
        setFileContent(content)
        setFileSize(size)
        setFileTruncated(truncated)
        return true
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
            <WriteToolbar onNewFile={handleNewFile} onSave={handleSave} onToggleAssistant={handleToggleAssistant} />
          )}
          <WriteDocumentPane onSelectWorkspace={handleSelectWorkspace} onSendToAssistant={handleAssistantSend} />
        </div>
      </div>
      {workspaceRoot && assistantOpen && assistantWindow && (
        <WriteAssistantWindow
          popup={assistantWindow}
          title={t('write.aiWritingAssistant')}
          onClose={handleAssistantClose}
        >
          <WriteAssistantPanel
            quickSuggestions={suggestions} onSend={handleAssistantSend}
            onNewChat={handleNewChat} onStaleSession={handleStaleSession}
          />
        </WriteAssistantWindow>
      )}
      <WriteFileDialogs />
      {toastMessage && <div className={styles.toast}>{toastMessage.text}</div>}
    </div>
  )
}

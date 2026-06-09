import React, { useRef, useEffect, useState, useCallback } from 'react'
import { useStore } from '../../stores'
import { IconFilePlus, IconFileText, IconEdit, IconTrash, IconSend, IconFolderOpen, IconPlus, IconSparkles } from '../../utils/icons'
import { renderMarkdown } from '../../utils/markdown'
import Select from '../shared/Select'
import styles from './WriteWorkspaceView.module.css'
import { sendMessage } from '../../services/sendMessage'
import { loomRpc } from '../../services/jsonrpc'

interface FileEntry { name: string; is_directory: boolean }
type PreviewMode = 'source' | 'split' | 'preview'
type ModalKind = 'none' | 'newFile' | 'rename' | 'delete'

const PREVIEW_OPTIONS = [
  { value: 'source' as const, label: '编辑' },
  { value: 'split' as const, label: '分屏' },
  { value: 'preview' as const, label: '预览' },
]

const FILE_EXT_OPTIONS = [
  { value: '.md', label: '.md' },
  { value: '.txt', label: '.txt' },
]

const QUICK_SUGGESTIONS = [
  '润色这段文字',
  '翻译成英文',
  '扩写到 500 字',
  '总结要点',
  '改写为更正式的语气',
]

export const WriteWorkspaceView: React.FC = () => {
  const appMode = useStore(s => s.appMode)
  const setAppMode = useStore(s => s.setAppMode)
  const createSession = useStore(s => s.createSession)
  const switchSession = useStore(s => s.switchSession)
  const sessionWorkspaces = useStore(s => s.sessionWorkspaces)
  const defaultWorkspace = useStore(s => s.defaultWorkspace)
  const writeFileSidebarOpen = useStore(s => s.writeFileSidebarOpen)

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

  // 写作专用会话 + AI 面板
  const [writeSessionId, setWriteSessionId] = useState<string | null>(
    () => localStorage.getItem('loom:writeSessionId')
  )
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
      if (!workspaceRoot) {
        const ws = defaultWorkspace || Object.values(sessionWorkspaces)[0]
        if (ws) setWorkspaceRoot(ws)
      }
    })()
  }, [])

  // 确保有写作会话
  const ensureSession = useCallback(async () => {
    if (writeSessionId) return writeSessionId
    const sid = await createSession()
    try { await loomRpc('session.rename', { session_id: sid, title: '写作助手' }) } catch {}
    setWriteSessionId(sid)
    localStorage.setItem('loom:writeSessionId', sid)
    return sid
  }, [writeSessionId, createSession])

  // 进入写模式时自动创建会话
  useEffect(() => {
    if (!writeSessionId) ensureSession()
  }, [])

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
    } catch (e: any) { showToast('读取目录失败: ' + String(e).slice(0, 40)) }
    setLoadingFiles(false)
  }, [workspaceRoot, showToast])

  useEffect(() => { if (workspaceRoot) loadFiles() }, [workspaceRoot, loadFiles])

  // 打开文件
  const openFile = useCallback(async (name: string) => {
    if (!workspaceRoot) return
    if (dirty && activeFilePath) await saveFile(activeFilePath, fileContent)
    try {
      const result = await loomRpc<{ ok: boolean; content: string }>('vfs.read_file', { workspace_root: workspaceRoot, path: name })
      if (result.ok) {
        setActiveFilePath(name); setFileContent(result.content); setLastSaved(result.content); setDirty(false)
      } else { showToast('读取失败') }
    } catch (e: any) { showToast('打开失败: ' + String(e).slice(0, 40)) }
  }, [workspaceRoot, dirty, activeFilePath, fileContent, showToast])

  // 保存
  const saveFile = useCallback(async (path: string, content: string) => {
    if (!workspaceRoot) return
    try {
      await loomRpc('vfs.write_file', { workspace_root: workspaceRoot, path, content })
      setLastSaved(content); setDirty(false)
    } catch (e: any) { showToast('保存失败: ' + String(e).slice(0, 40)) }
  }, [workspaceRoot, showToast])

  // 自动保存
  useEffect(() => {
    if (dirty && activeFilePath) {
      timerRef.current = setTimeout(() => saveFile(activeFilePath, fileContent), 650)
      return () => clearTimeout(timerRef.current)
    }
  }, [fileContent, dirty, activeFilePath, saveFile])

  // Ctrl+滚轮缩放字体
  useEffect(() => {
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
  }, [])

  // Ctrl+S
  useEffect(() => {
    const h = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key === 's') {
        e.preventDefault()
        if (activeFilePath && dirty) saveFile(activeFilePath, fileContent)
      }
    }
    window.addEventListener('keydown', h)
    return () => window.removeEventListener('keydown', h)
  }, [activeFilePath, dirty, fileContent, saveFile])

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
        loadFiles(); showToast('文件已创建')
      } else if (modal.kind === 'rename' && modal.targetName) {
        const newName = modalInput.trim()
        if (!newName || newName === modal.targetName) return
        await loomRpc('vfs.rename', { workspace_root: workspaceRoot, path: modal.targetName, new_name: newName })
        if (activeFilePath === modal.targetName) setActiveFilePath(newName)
        loadFiles(); showToast('已重命名')
      } else if (modal.kind === 'delete' && modal.targetName) {
        await loomRpc('vfs.delete', { workspace_root: workspaceRoot, path: modal.targetName })
        if (activeFilePath === modal.targetName) { setActiveFilePath(null); setFileContent(''); setDirty(false) }
        loadFiles(); showToast('已删除')
      }
      setModal({ kind: 'none' })
    } catch (e: any) { showToast('操作失败: ' + (e?.message || String(e)).slice(0, 40)) }
  }, [modal, modalInput, fileExt, workspaceRoot, activeFilePath, loadFiles, showToast])

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

  // AI 助手
  const handleAssistantSend = useCallback(async (text?: string) => {
    const msg = (text || assistantText).trim()
    if (!msg) return
    const sid = await ensureSession()
    setAssistantText('')
    try {
      const content = activeFilePath
        ? `[写作上下文]\n当前文件: ${activeFilePath}\n\n${fileContent}\n\n[用户指令]\n${msg}`
        : msg
      await sendMessage({ sessionId: sid, content })
      // 切换到对话模式查看 AI 回复
      setAppMode('chat')
      switchSession(sid)
    } catch { showToast('发送失败') }
  }, [assistantText, activeFilePath, fileContent, ensureSession, showToast, setAppMode, switchSession])

  if (appMode !== 'write') return null

  const previewHtml = previewMode !== 'source' && fileContent ? renderMarkdown(fileContent) : ''
  const editorPlaceholder = activeFilePath ? '开始写作...' : '选择或新建文件后开始写作'

  return (
    <div className={styles.root}>

      {/* ===== 工具栏 ===== */}
      <div className={styles.toolbar}>
        {!workspaceRoot ? (
          <button className={styles.toolbarBtn} onClick={pickWorkspace}>
            <IconFolderOpen size={12} />选择目录
          </button>
        ) : (
          <div className={styles.toolbarGroup}>
            <span className={styles.workspacePath} onClick={pickWorkspace} title="点击切换目录">
              {workspaceRoot.split(/[/\\]/).pop() || workspaceRoot}
            </span>
            <button className={styles.toolbarBtnGhost}
              onClick={() => (window as any).loom?.openFolder?.(workspaceRoot)}
              title="在文件管理器中打开" style={{ padding: '2px 4px' }}>
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

        <Select value={previewMode} options={PREVIEW_OPTIONS} onChange={setPreviewMode} variant="pill" />

        {workspaceRoot && (
          <button
            className={assistantPanelOpen ? styles.toolbarBtnAccent : styles.toolbarBtnGhost}
            onClick={() => setAssistantPanelOpen(o => !o)}
            title={assistantPanelOpen ? '收起AI面板' : '展开AI面板'}>
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
              <span>文件列表</span>
              <button className={styles.fileSidebarNewBtn}
                onClick={() => { setModalInput(''); setFileExt('.md'); setModal({ kind: 'newFile' }) }}
                title="新建文件">
                <IconPlus size={13} />
              </button>
            </div>
            <div className={styles.fileList}>
              {loadingFiles ? (
                <div className={styles.fileListHint}>加载中...</div>
              ) : files.length === 0 ? (
                <div className={styles.fileListHint}>
                  暂无文件<br />
                  <span style={{ fontSize: 10, opacity: 0.5 }}>点击 + 新建一个</span>
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
                        title="重命名">
                        <IconEdit size={11} />
                      </button>
                      <button className={styles.fileItemAction}
                        onClick={e => { e.stopPropagation(); setModal({ kind: 'delete', targetName: f.name }) }}
                        title="删除">
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
            <span>选择工作目录开始写作</span>
            <button className={styles.workspacePromptBtn} onClick={pickWorkspace}>
              <IconFolderOpen size={16} />选择目录
            </button>
          </div>
        ) : !activeFilePath ? (
          <div className={styles.emptyState}>
            <IconFileText size={40} className={styles.emptyIcon} />
            <span>选择文件或新建文件</span>
            <button className={styles.workspacePromptBtn}
              onClick={() => { setModalInput(''); setFileExt('.md'); setModal({ kind: 'newFile' }) }}>
              <IconFilePlus size={16} />新建文件
            </button>
          </div>
        ) : previewMode === 'preview' ? (
          <div className={styles.editorArea}>
            <div className={styles.preview} dangerouslySetInnerHTML={{ __html: previewHtml }} />
          </div>
        ) : previewMode === 'split' ? (
          <div className={styles.editorArea}>
            <textarea className={styles.editor}
              style={{ width: '50%', borderRight: '1px solid var(--border)', fontSize: editorFontSize }}
              value={fileContent}
              onChange={e => { setFileContent(e.target.value); setDirty(true) }}
              placeholder={editorPlaceholder} spellCheck={false} />
            <div className={styles.preview} dangerouslySetInnerHTML={{ __html: previewHtml }} />
          </div>
        ) : (
          <div className={styles.editorArea}>
            <textarea className={styles.editor}
              style={{ fontSize: editorFontSize }}
              value={fileContent}
              onChange={e => { setFileContent(e.target.value); setDirty(true) }}
              placeholder={editorPlaceholder} spellCheck={false} />
          </div>
        )}

        {/* AI 右侧面板 — 常显 */}
        {workspaceRoot && (
          <div className={`${styles.assistantPanel} ${!assistantPanelOpen ? styles.assistantPanelCollapsed : ''}`}>
            <div className={styles.assistantPanelHeader}>
              <IconSparkles size={13} />
              <span>AI 写作助手</span>
            </div>

            <div className={styles.assistantPanelBody}>
              {activeFilePath ? (
                <div className={styles.assistantContext}>
                  <div className={styles.assistantContextFile}>
                    <IconFileText size={11} />{activeFilePath.split('/').pop()}
                  </div>
                  <span>已打开，可在下方输入指令让 AI 帮你处理文本。</span>
                </div>
              ) : (
                <div className={styles.assistantContext}>
                  打开一个文件后，AI 可以帮你润色、翻译、扩写、总结等。
                </div>
              )}

              <div style={{ fontSize: 11, color: 'var(--text-muted)', padding: '4px 0 2px', fontWeight: 500 }}>
                快捷指令
              </div>
              {QUICK_SUGGESTIONS.map(s => (
                <button key={s} className={styles.assistantSuggestion}
                  onClick={() => handleAssistantSend(s)}>
                  {s}
                </button>
              ))}
            </div>

            <div className={styles.assistantPanelFooter}>
              <div className={styles.assistantInputRow}>
                <input className={styles.assistantInput}
                  value={assistantText}
                  onChange={e => setAssistantText(e.target.value)}
                  onKeyDown={e => {
                    if (e.key !== 'Enter' || e.shiftKey) return
                    e.preventDefault(); handleAssistantSend()
                  }}
                  placeholder="输入指令..."
                />
                <button className={styles.assistantSendBtn}
                  onClick={() => handleAssistantSend()}
                  disabled={!assistantText.trim()} title="发送">
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
            {modal.kind === 'newFile' ? '新建文件' : modal.kind === 'rename' ? '重命名' : '确认删除'}
          </div>
          {modal.kind === 'delete' ? (
            <>
              <div style={{ fontSize: 13, color: 'var(--text-secondary)', marginBottom: 16, lineHeight: 1.5 }}>
                确定要删除「{modal.targetName}」？此操作不可撤销。
              </div>
              <div className={styles.modalFooter}>
                <button className={styles.modalBtnCancel} onClick={() => setModal({ kind: 'none' })}>取消</button>
                <button className={styles.modalBtnDanger} onClick={confirmModal}>删除</button>
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
                    placeholder="文件名，如：笔记" />
                  <Select value={fileExt} options={FILE_EXT_OPTIONS} onChange={setFileExt} variant="pill" />
                </div>
              ) : (
                <input ref={modalInputRef} className={styles.modalInput}
                  value={modalInput}
                  onChange={e => setModalInput(e.target.value)}
                  onKeyDown={e => { if (e.key === 'Enter') confirmModal(); if (e.key === 'Escape') setModal({ kind: 'none' }) }}
                  placeholder="新文件名" />
              )}
              <div className={styles.modalFooter}>
                <button className={styles.modalBtnCancel} onClick={() => setModal({ kind: 'none' })}>取消</button>
                <button className={styles.modalBtnConfirm} onClick={confirmModal}>确定</button>
              </div>
            </>
          )}
        </div>
      )}

      {toast && <div className={styles.toast}>{toast}</div>}
    </div>
  )
}

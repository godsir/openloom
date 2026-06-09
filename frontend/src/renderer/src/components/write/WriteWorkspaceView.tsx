import React, { useRef, useEffect, useState, useCallback, useMemo } from 'react'
import { useStore } from '../../stores'
import type { SendShortcut } from '../../stores/input'
import { IconArrowLeft, IconFilePlus, IconFileText, IconEdit, IconTrash, IconCheck, IconSave, IconSend, IconFolderOpen, IconPlus, IconSparkles, IconX } from '../../utils/icons'
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

export const WriteWorkspaceView: React.FC = () => {
  const appMode = useStore(s => s.appMode)
  const setAppMode = useStore(s => s.setAppMode)
  const sidebarOpen = useStore(s => s.sidebarOpen)
  const toggleSidebar = useStore(s => s.toggleSidebar)
  const createSession = useStore(s => s.createSession)
  const sendShortcut = useStore(s => s.sendShortcut)
  const setSendShortcut = useStore(s => s.setSendShortcut)
  const sessionWorkspaces = useStore(s => s.sessionWorkspaces)
  const defaultWorkspace = useStore(s => s.defaultWorkspace)

  const [workspaceRoot, setWorkspaceRoot] = useState<string | null>(null)
  const [activeFilePath, setActiveFilePath] = useState<string | null>(null)
  const [fileContent, setFileContent] = useState('')
  const [lastSaved, setLastSaved] = useState('')
  const [dirty, setDirty] = useState(false)
  const [saveStatus, setSaveStatus] = useState<'idle' | 'saving' | 'error'>('idle')
  const [previewMode, setPreviewMode] = useState<PreviewMode>('source')

  const [files, setFiles] = useState<FileEntry[]>([])
  const [loadingFiles, setLoadingFiles] = useState(false)
  const [toast, setToast] = useState<string | null>(null)
  const [assistantOpen, setAssistantOpen] = useState(false)

  const [modal, setModal] = useState<{ kind: ModalKind; targetName?: string }>({ kind: 'none' })
  const [modalInput, setModalInput] = useState('')
  const [fileExt, setFileExt] = useState('.md')
  const modalInputRef = useRef<HTMLInputElement>(null)

  const timerRef = useRef<ReturnType<typeof setTimeout>>()
  const toastTimerRef = useRef<ReturnType<typeof setTimeout>>()

  // 写作专用会话
  const [writeSessionId, setWriteSessionId] = useState<string | null>(() => localStorage.getItem('loom:writeSessionId'))
  const effectiveSessionId = writeSessionId

  const showToast = useCallback((msg: string) => {
    setToast(msg)
    clearTimeout(toastTimerRef.current)
    toastTimerRef.current = setTimeout(() => setToast(null), 2500)
  }, [])

  // 初始化工作区：读偏好 > 会话绑定 > 全局默认 > 任意
  useEffect(() => {
    (async () => {
      try { const v = await (window as any).loom?.getPreference?.('writeWorkspace', '') || ''; if (v) setWorkspaceRoot(v) } catch {}
      if (!workspaceRoot) {
        const ws = defaultWorkspace || Object.values(sessionWorkspaces)[0]
        if (ws) setWorkspaceRoot(ws)
      }
    })()
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
    setSaveStatus('saving')
    try {
      await loomRpc('vfs.write_file', { workspace_root: workspaceRoot, path, content })
      setLastSaved(content); setDirty(false); setSaveStatus('idle')
    } catch (e: any) { setSaveStatus('error'); showToast('保存失败: ' + String(e).slice(0, 40)) }
  }, [workspaceRoot, showToast])

  // 自动保存 650ms
  useEffect(() => {
    if (dirty && activeFilePath) {
      timerRef.current = setTimeout(() => saveFile(activeFilePath, fileContent), 650)
      return () => clearTimeout(timerRef.current)
    }
  }, [fileContent, dirty, activeFilePath, saveFile])

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
        loadFiles(); showToast('文件已创�?)
      } else if (modal.kind === 'rename' && modal.targetName) {
        const newName = modalInput.trim()
        if (!newName || newName === modal.targetName) return
        await loomRpc('vfs.rename', { workspace_root: workspaceRoot, path: modal.targetName, new_name: newName })
        if (activeFilePath === modal.targetName) setActiveFilePath(newName)
        loadFiles(); showToast('已重命名')
      } else if (modal.kind === 'delete' && modal.targetName) {
        await loomRpc('vfs.delete', { workspace_root: workspaceRoot, path: modal.targetName })
        if (activeFilePath === modal.targetName) { setActiveFilePath(null); setFileContent(''); setDirty(false) }
        loadFiles(); showToast('已删�?)
      }
      setModal({ kind: 'none' })
    } catch (e: any) { showToast('操作失败: ' + (e?.message || String(e)).slice(0, 40)) }
  }, [modal, workspaceRoot, activeFilePath, loadFiles, showToast])

  // 选目�?  const pickWorkspace = useCallback(async () => {
    try {
      const path = await (window as any).loom?.selectFolder?.()
      if (path) {
        setWorkspaceRoot(path); (window as any).loom?.setPreference?.('writeWorkspace', path)
        setActiveFilePath(null); setFileContent(''); setDirty(false)
      }
    } catch { /* non-critical */ }
  }, [])

  // AI 助手
  const [assistantText, setAssistantText] = useState('')
  const handleAssistantSend = useCallback(async () => {
    const text = assistantText.trim()
    if (!text || !effectiveSessionId) return
    setAssistantText('')
    try {
      const content = activeFilePath
        ? `[写作上下文]\n当前文件: ${activeFilePath}\n\n${fileContent}\n\n[用户指令]\n${text}`
        : text
      await sendMessage({ sessionId: effectiveSessionId, content })
    } catch { showToast('发送失�?) }
  }, [assistantText, effectiveSessionId, activeFilePath, fileContent, showToast])

  if (appMode !== 'write') return null
  const previewHtml = previewMode !== 'source' && fileContent ? renderMarkdown(fileContent) : ''

  return (
    <div className={styles.root}>

      {/* ===== 工具�?===== */}
      <div className={styles.toolbar}>
        <button className={styles.toolbarBtnGhost} onClick={() => { setAppMode('chat'); if (!sidebarOpen) toggleSidebar() }}>
          <IconArrowLeft size={14} />返回
        </button>

        {!workspaceRoot ? (
          <button className={styles.toolbarBtn} onClick={pickWorkspace}><IconFolderOpen size={12} />选择目录</button>
        ) : (
          <>
            <button className={styles.toolbarBtnGhost} onClick={pickWorkspace} title="切换目录">
              <IconFolderOpen size={12} />
              <span style={{ fontSize: 11, color: 'var(--text-muted)', maxWidth: 120, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                {workspaceRoot.split(/[/\\]/).pop() || workspaceRoot}
              </span>
            </button>
            <button className={styles.toolbarBtnGhost} onClick={() => { setModalInput(''); setModal({ kind: 'newFile' }) }} title="新建">
              <IconFilePlus size={12} />新建
            </button>
          </>
        )}

        {activeFilePath && <span className={styles.fileName}>{activeFilePath.split('/').pop()}</span>}

        <div className={styles.spacer} />

        {dirty && activeFilePath && (
          <button className={styles.toolbarBtnGhost} onClick={() => saveFile(activeFilePath, fileContent)}
            style={{ color: '#f59e0b', fontSize: 11, gap: 3 }}>
            <IconSave size={11} />保存
          </button>
        )}
        {saveStatus === 'saving' && <span style={{ fontSize: 11, color: 'var(--text-muted)' }}>保存中�?/span>}
        {saveStatus === 'error' && <span style={{ fontSize: 11, color: 'var(--danger)' }}>保存失败</span>}

        <Select value={previewMode} options={PREVIEW_OPTIONS} onChange={setPreviewMode} variant="pill" />

        <button className={`${styles.toolbarBtnGhost} ${assistantOpen ? styles.toolbarBtnGhostActive : ''}`}
          onClick={async () => {
            if (!assistantOpen && !effectiveSessionId) {
              const sid = await createSession()
              try { await loomRpc('session.rename', { session_id: sid, title: '[写] 写作助手' }) } catch {}
              setWriteSessionId(sid); localStorage.setItem('loom:writeSessionId', sid)
            }
            setAssistantOpen(o => !o)
          }}>
          <IconSparkles size={13} />
        </button>
      </div>

      {/* ===== 主体 ===== */}
      <div className={styles.body}>
        {workspaceRoot && (
          <div className={styles.fileSidebar}>
            <div className={styles.fileSidebarHeader}>
              文件
              <button className={styles.toolbarBtnGhost} style={{ padding: '0 4px' }}
                onClick={() => { setModalInput(''); setModal({ kind: 'newFile' }) }}>
                <IconPlus size={12} />
              </button>
            </div>
            <div className={styles.fileList}>
              {loadingFiles ? <div className={styles.fileListHint}>加载中�?/div>
                : files.length === 0 ? <div className={styles.fileListHint}>暂无文件</div>
                  : files.map(f => (
                    <div key={f.name} className={activeFilePath === f.name ? styles.fileItemActive : styles.fileItem}
                      onClick={() => openFile(f.name)}>
                      <IconFileText size={12} style={{ flexShrink: 0 }} />
                      <span className={styles.fileItemName}>{f.name}</span>
                      <div className={styles.fileItemActions} onClick={e => e.stopPropagation()}>
                        <button className={styles.fileItemAction} onClick={() => { setModalInput(f.name); setModal({ kind: 'rename', targetName: f.name }) }} title="重命�?><IconEdit size={10} /></button>
                        <button className={styles.fileItemAction} onClick={() => setModal({ kind: 'delete', targetName: f.name })} title="删除"><IconTrash size={10} /></button>
                      </div>
                    </div>
                  ))}
            </div>
          </div>
        )}

        {!workspaceRoot ? (
          <div className={styles.emptyState}>
            <IconFolderOpen size={48} className={styles.emptyIcon} />
            <span>选择工作目录开始写�?/span>
            <button className={styles.workspacePromptBtn} onClick={pickWorkspace}><IconFolderOpen size={16} />选择目录</button>
          </div>
        ) : !activeFilePath ? (
          <div className={styles.emptyState}>
            <IconFileText size={40} className={styles.emptyIcon} />
            <span>选择文件或新建文�?/span>
            <button className={styles.workspacePromptBtn} onClick={() => { setModalInput(''); setModal({ kind: 'newFile' }) }}><IconFilePlus size={16} />新建文件</button>
          </div>
        ) : previewMode === 'preview' ? (
          <div className={styles.editorArea}><div className={styles.preview} dangerouslySetInnerHTML={{ __html: previewHtml }} /></div>
        ) : previewMode === 'split' ? (
          <div className={styles.editorArea}>
            <textarea className={styles.editor} style={{ width: '50%', borderRight: '1px solid var(--border)' }}
              value={fileContent}
              onChange={e => { setFileContent(e.target.value); setDirty(true) }}
              placeholder="开始写作�? spellCheck={false} />
            <div className={styles.preview} dangerouslySetInnerHTML={{ __html: previewHtml }} />
          </div>
        ) : (
          <div className={styles.editorArea}>
            <textarea className={styles.editor}
              value={fileContent}
              onChange={e => { setFileContent(e.target.value); setDirty(true) }}
              placeholder="开始写作�? spellCheck={false} />
          </div>
        )}
      </div>

      {/* ===== AI 助手 ===== */}
      {assistantOpen && <div className={styles.assistantBackdrop} onClick={() => setAssistantOpen(false)} />}
      {assistantOpen && (
        <div className={styles.assistantFloating}>
          <div className={styles.assistantFloatingHeader}>
            <IconSparkles size={13} /><span>AI 写作助手</span>
            <div className={styles.spacer} />
            <button className={styles.toolbarBtnGhost} onClick={() => setAssistantOpen(false)} style={{ padding: 2 }}><IconX size={14} /></button>
          </div>
          <div className={styles.assistantFloatingInput}>
            <input value={assistantText} onChange={e => setAssistantText(e.target.value)}
              onKeyDown={e => {
                if (e.key !== 'Enter') return
                const c = e.ctrlKey || e.metaKey; const s = e.shiftKey
                const send = sendShortcut === 'ctrl+enter' ? c && !s : sendShortcut === 'shift+enter' ? s && !c : !c && !s
                if (send) { e.preventDefault(); handleAssistantSend() }
              }}
              placeholder="润色这段 / 翻译成英�?/ 扩写�?500 字�?
              disabled={!effectiveSessionId} />
            <button className={`${styles.assistantSendBtn} ${assistantText.trim() ? '' : styles.assistantSendBtnDisabled}`}
              onClick={handleAssistantSend} disabled={!assistantText.trim()}>
              <IconSend size={12} />发�?            </button>
            <Select<SendShortcut> value={sendShortcut}
              options={[
                { value: 'enter', label: '�? },
                { value: 'ctrl+enter', label: '⌃↵' },
                { value: 'shift+enter', label: '⇧↵' },
              ]}
              onChange={setSendShortcut} variant="pill" />
          </div>
        </div>
      )}

      {/* ===== 弹窗 ===== */}
      {modal.kind !== 'none' && <div className={styles.modalBackdrop} onClick={() => setModal({ kind: 'none' })} />}
      {modal.kind !== 'none' && (
        <div className={styles.modalDialog}>
          <div className={styles.modalTitle}>
            {modal.kind === 'newFile' ? '新建文件' : modal.kind === 'rename' ? '重命�? : '确认删除'}
          </div>
          {modal.kind === 'delete' ? (
            <>
              <div style={{ fontSize: 13, color: 'var(--text)', marginBottom: 16 }}>确定删除「{modal.targetName}」？</div>
              <div style={{ display: 'flex', gap: 8, justifyContent: 'flex-end' }}>
                <button className={styles.toolbarBtn} onClick={() => setModal({ kind: 'none' })}>取消</button>
                <button className={styles.toolbarBtn} style={{ borderColor: 'var(--danger)', color: 'var(--danger)' }} onClick={confirmModal}>删除</button>
              </div>
            </>
          ) : (
            <>
              {modal.kind === 'newFile' ? (
                <div style={{ display: 'flex', gap: 8, marginBottom: 12 }}>
                  <input ref={modalInputRef} className={styles.modalInput} style={{ flex: 1, marginBottom: 0 }}
                    value={modalInput} onChange={e => setModalInput(e.target.value)}
                    onKeyDown={e => { if (e.key === 'Enter') confirmModal(); if (e.key === 'Escape') setModal({ kind: 'none' }) }}
                    placeholder="文件名，�?笔记" />
                  <Select value={fileExt} options={FILE_EXT_OPTIONS} onChange={setFileExt} variant="pill" />
                </div>
              ) : (
                <input ref={modalInputRef} className={styles.modalInput} value={modalInput}
                  onChange={e => setModalInput(e.target.value)}
                  onKeyDown={e => { if (e.key === 'Enter') confirmModal(); if (e.key === 'Escape') setModal({ kind: 'none' }) }}
                  placeholder="新文件名" />
              )}
              <div style={{ display: 'flex', gap: 8, justifyContent: 'flex-end' }}>
                <button className={styles.toolbarBtn} onClick={() => setModal({ kind: 'none' })}>取消</button>
                <button className={styles.toolbarBtn} style={{ borderColor: 'var(--accent)', color: 'var(--accent)' }} onClick={confirmModal}>确定</button>
              </div>
            </>
          )}
        </div>
      )}

      {toast && <div className={styles.toast}>{toast}</div>}
    </div>
  )
}

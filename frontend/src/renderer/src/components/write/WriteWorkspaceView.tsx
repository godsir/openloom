import React, { useRef, useEffect, useState, useCallback, useMemo } from 'react'
import { useStore } from '../../stores'
import type { SendShortcut } from '../../stores/input'
import { IconArrowLeft, IconFilePlus, IconFileText, IconEdit, IconTrash, IconSave, IconFolderOpen, IconPlus, IconRefresh, IconSparkles, IconX, IconSend, IconCheck } from '../../utils/icons'
import { renderMarkdown } from '../../utils/markdown'
import Select from '../shared/Select'
import styles from './WriteWorkspaceView.module.css'
import { sendMessage } from '../../services/sendMessage'
import { loomRpc } from '../../services/jsonrpc'

interface FileEntry { name: string; is_directory: boolean }

type SaveStatus = 'saved' | 'dirty' | 'saving' | 'error'
type PreviewMode = 'source' | 'split' | 'preview'
type ModalKind = 'none' | 'newFile' | 'rename' | 'delete' | 'workspace'

const PREVIEW_OPTIONS = [
  { value: 'source' as const, label: '纯编辑' },
  { value: 'split' as const, label: '分屏预览' },
  { value: 'preview' as const, label: '纯预览' },
]

export const WriteWorkspaceView: React.FC = () => {
  const appMode = useStore(s => s.appMode)
  const setAppMode = useStore(s => s.setAppMode)
  const sidebarOpen = useStore(s => s.sidebarOpen)
  const toggleSidebar = useStore(s => s.toggleSidebar)
  const activeSessionId = useStore(s => s.activeSessionId)
  const createSession = useStore(s => s.createSession)
  const sendShortcut = useStore(s => s.sendShortcut)
  const setSendShortcut = useStore(s => s.setSendShortcut)
  const sessionWorkspaces = useStore(s => s.sessionWorkspaces)
  const defaultWorkspace = useStore(s => s.defaultWorkspace)

  // 写作专用会话 ID — 启动时从偏好恢复，持久化到 localStorage
  const [writeSessionId, setWriteSessionId] = useState<string | null>(() => {
    try { return localStorage.getItem('loom:writeSessionId') }
    catch { return null }
  })

  const saveWriteSessionId = (id: string) => {
    try { localStorage.setItem('loom:writeSessionId', id) }
    catch { /* ignore */ }
    setWriteSessionId(id)
  }

  // 有效会话：优先写作会话 > 聊天当前会话
  const effectiveSessionId = writeSessionId || activeSessionId

  const configuredWorkspace = activeSessionId ? sessionWorkspaces[activeSessionId] : null

  const [manualWorkspace, setManualWorkspace] = useState<string | null>(null)
  const [globalWorkspace, setGlobalWorkspace] = useState('')

  useEffect(() => {
    (window as any).loom?.getPreference?.('writeWorkspace', '').then((v: string) => setGlobalWorkspace(v))
  }, [])

  // 手动选择 > 持久化偏好 > 会话配置 > 全局默认 > 任意会话兜底
  const workspaceRoot = useMemo(() => {
    if (manualWorkspace) return manualWorkspace
    if (globalWorkspace) return globalWorkspace
    if (configuredWorkspace) return configuredWorkspace
    if (defaultWorkspace) return defaultWorkspace
    const all = Object.values(sessionWorkspaces)
    if (all.length > 0) return all[0]
    return null
  }, [manualWorkspace, globalWorkspace, configuredWorkspace, defaultWorkspace, sessionWorkspaces])

  const [activeFilePath, setActiveFilePath] = useState<string | null>(null)
  const [fileContent, setFileContent] = useState('')
  const [lastSaved, setLastSaved] = useState('')
  const [saveStatus, setSaveStatus] = useState<SaveStatus>('saved')
  const [previewMode, setPreviewMode] = useState<PreviewMode>('source')

  const [files, setFiles] = useState<FileEntry[]>([])
  const [loadingFiles, setLoadingFiles] = useState(false)
  const [toast, setToast] = useState<string | null>(null)
  const [assistantOpen, setAssistantOpen] = useState(false)

  // 内联弹窗
  const [modal, setModal] = useState<{ kind: ModalKind; targetName?: string }>({ kind: 'none' })
  const [modalInput, setModalInput] = useState('')
  const modalInputRef = useRef<HTMLInputElement>(null)

  const timerRef = useRef<ReturnType<typeof setTimeout>>()
  const toastTimerRef = useRef<ReturnType<typeof setTimeout>>()

  const showToast = useCallback((msg: string) => {
    setToast(msg)
    if (toastTimerRef.current) clearTimeout(toastTimerRef.current)
    toastTimerRef.current = setTimeout(() => setToast(null), 2000)
  }, [])

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

  // ── 弹窗打开时自动聚焦 ──
  useEffect(() => {
    if (modal.kind !== 'none' && modalInputRef.current) {
      setTimeout(() => modalInputRef.current?.focus(), 50)
    }
  }, [modal.kind])

  // ── 文件列表 ──
  const loadFiles = useCallback(async () => {
    if (!workspaceRoot) return
    setLoadingFiles(true)
    try {
      const result = await loomRpc<{ ok: boolean; entries: FileEntry[] }>('vfs.list_directory', { workspace_root: workspaceRoot, path: '.' })
      if (result.ok) {
        const textFiles = result.entries.filter(e => !e.is_directory && (e.name.endsWith('.md') || e.name.endsWith('.txt') || e.name.endsWith('.markdown')))
        setFiles(textFiles.sort((a, b) => a.name.localeCompare(b.name)))
      }
    } catch { /* silence */ }
    setLoadingFiles(false)
  }, [workspaceRoot])

  useEffect(() => { if (workspaceRoot) loadFiles() }, [workspaceRoot, loadFiles])

  // ── 打开文件 ──
  const openFile = useCallback(async (name: string) => {
    if (!workspaceRoot) return
    if (saveStatus === 'dirty' && activeFilePath) {
      try { await loomRpc('vfs.write_file', { workspace_root: workspaceRoot, path: activeFilePath, content: fileContent }) } catch { /* */ }
    }
    try {
      const result = await loomRpc<{ ok: boolean; content: string }>('vfs.read_file', { workspace_root: workspaceRoot, path: name })
      if (result.ok) {
        setActiveFilePath(name); setFileContent(result.content); setLastSaved(result.content); setSaveStatus('saved')
      }
    } catch { showToast('打开文件失败') }
  }, [workspaceRoot, activeFilePath, saveStatus, fileContent, showToast])

  // ── 保存 ──
  const saveFile = useCallback(async (path: string, content: string) => {
    if (!workspaceRoot) return
    try {
      setSaveStatus('saving')
      await loomRpc('vfs.write_file', { workspace_root: workspaceRoot, path, content })
      setLastSaved(content); setSaveStatus('saved')
      if (timerRef.current) clearTimeout(timerRef.current)
    } catch { setSaveStatus('error') }
  }, [workspaceRoot])

  useEffect(() => {
    if (saveStatus === 'dirty' && activeFilePath && workspaceRoot) {
      timerRef.current = setTimeout(() => saveFile(activeFilePath, fileContent), 650)
      return () => clearTimeout(timerRef.current)
    }
  }, [fileContent, saveStatus, activeFilePath, workspaceRoot, saveFile])

  useEffect(() => {
    const h = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key === 's') {
        e.preventDefault()
        if (activeFilePath && workspaceRoot && saveStatus === 'dirty') saveFile(activeFilePath, fileContent)
      }
    }
    window.addEventListener('keydown', h)
    return () => window.removeEventListener('keydown', h)
  }, [activeFilePath, workspaceRoot, fileContent, saveStatus, saveFile])

  // ── 文件操作（通过内联弹窗） ──
  const startNewFile = () => { setModalInput(''); setModal({ kind: 'newFile' }) }
  const startRename = (name: string) => { setModalInput(name); setModal({ kind: 'rename', targetName: name }) }
  const startDelete = (name: string) => { setModal({ kind: 'delete', targetName: name }) }

  const confirmModal = useCallback(async () => {
    if (modal.kind === 'none' || !workspaceRoot) return
    try {
      if (modal.kind === 'newFile') {
        const raw = modalInput.trim()
        if (!raw) return
        const name = /\.(md|txt|markdown)$/i.test(raw) ? raw : raw + '.md'
        const content = '# ' + name.replace(/\.(md|txt|markdown)$/i, '') + '\n\n'
        await loomRpc('vfs.write_file', { workspace_root: workspaceRoot, path: name, content })
        setActiveFilePath(name); setFileContent(content); setLastSaved(content); setSaveStatus('saved')
        loadFiles(); showToast('文件已创建')
      } else if (modal.kind === 'rename' && modal.targetName) {
        const newName = modalInput.trim()
        if (!newName || newName === modal.targetName) return
        await loomRpc('vfs.rename', { workspace_root: workspaceRoot, path: modal.targetName, new_name: newName })
        if (activeFilePath === modal.targetName) setActiveFilePath(newName)
        loadFiles(); showToast('已重命名')
      } else if (modal.kind === 'delete' && modal.targetName) {
        await loomRpc('vfs.delete', { workspace_root: workspaceRoot, path: modal.targetName })
        if (activeFilePath === modal.targetName) { setActiveFilePath(null); setFileContent(''); setLastSaved(''); setSaveStatus('saved') }
        loadFiles(); showToast('已删除')
      }
    } catch { showToast('操作失败') }
    setModal({ kind: 'none' })
  }, [modal, modalInput, workspaceRoot, activeFilePath, loadFiles, showToast])

  // ── 切换工作区 ──
  const pickWorkspace = useCallback(async () => {
    try {
      const path = await (window as any).loom?.selectFolder?.()
      if (path) {
        setManualWorkspace(path)
        ;(window as any).loom?.setPreference?.('writeWorkspace', path)
        setActiveFilePath(null); setFileContent(''); setLastSaved(''); setSaveStatus('saved')
      }
    } catch {
      setModalInput(''); setModal({ kind: 'workspace' })
    }
  }, [])

  const confirmWorkspace = () => {
    const p = modalInput.trim()
    if (p) { setManualWorkspace(p); setActiveFilePath(null); setFileContent(''); setLastSaved(''); setSaveStatus('saved') }
    setModal({ kind: 'none' })
  }

  const backToChat = useCallback(() => {
    setAppMode('chat')
    if (!sidebarOpen) toggleSidebar()
  }, [setAppMode, sidebarOpen, toggleSidebar])

  // ── AI 助手 ──
  const [assistantText, setAssistantText] = useState('')
  const [assistantBusy, setAssistantBusy] = useState(false)
  const handleAssistantSend = useCallback(async () => {
    const text = assistantText.trim()
    if (!text || assistantBusy) return
    const sid = effectiveSessionId
    if (!sid) return
    setAssistantBusy(true)
    setAssistantText('')
    try {
      // 附带当前文件内容和路径，作为写作上下文
      const content = activeFilePath
        ? `[写作上下文]\n当前文件: ${activeFilePath}\n\n${fileContent}\n\n[用户指令]\n${text}`
        : text
      await sendMessage({ sessionId: sid, content })
    } catch {
      showToast('发送失败')
    }
    setAssistantBusy(false)
  }, [assistantText, assistantBusy, effectiveSessionId, activeFilePath, fileContent, showToast])

  const previewHtml = previewMode !== 'source' && fileContent ? renderMarkdown(fileContent) : ''

  const saveLabel = saveStatus === 'saved' ? '已保存' : saveStatus === 'dirty' ? '未保存' : saveStatus === 'saving' ? '保存中…' : '保存失败'
  const saveColor = saveStatus === 'saved' ? 'var(--success)' : saveStatus === 'dirty' ? '#f59e0b' : saveStatus === 'saving' ? 'var(--text-muted)' : 'var(--danger)'

  if (appMode !== 'write') return null

  return (
    <div className={styles.root}>
      {/* ── 工具栏 ── */}
      <div className={styles.toolbar}>
        <button className={styles.toolbarBtnGhost} onClick={backToChat} title="返回聊天"><IconArrowLeft size={14} /><span>返回聊天</span></button>

        {workspaceRoot ? (<>
          <button className={styles.toolbarBtnGhost} onClick={pickWorkspace} title="切换工作目录"><IconFolderOpen size={12} /><span className={styles.workspaceName}>{workspaceRoot.split(/[/\\]/).pop() || workspaceRoot}</span></button>
          <button className={styles.toolbarBtn} onClick={startNewFile} title="新建 Markdown 文件"><IconFilePlus size={12} /><span>新建</span></button>
          <button className={styles.toolbarBtnGhost} onClick={loadFiles} title="刷新文件列表"><IconRefresh size={12} /></button>
        </>) : (
          <button className={styles.toolbarBtn} onClick={pickWorkspace}><IconFolderOpen size={13} /><span>选择工作目录</span></button>
        )}

        {activeFilePath && (<span className={styles.fileName} title={activeFilePath}><IconFileText size={13} style={{ marginRight: 4 }} />{activeFilePath.split('/').pop()}</span>)}

        <div className={styles.spacer} />
        <span className={styles.saveStatus} style={{ color: saveColor }}>
          {saveStatus === 'dirty' ? <button className={styles.toolbarBtnGhost} onClick={() => activeFilePath && saveFile(activeFilePath, fileContent)} title="Ctrl+S"><IconSave size={12} /><span>{saveLabel}</span></button> : <span><IconSave size={11} style={{ marginRight: 2 }} />{saveLabel}</span>}
        </span>
        <Select value={previewMode} options={PREVIEW_OPTIONS} onChange={setPreviewMode} variant="pill" />
        <button className={`${styles.toolbarBtnGhost} ${assistantOpen ? styles.toolbarBtnGhostActive : ''}`}
          onClick={async () => {
            if (!assistantOpen && !effectiveSessionId) {
              // 自动创建写作专用会话
              const sid = await createSession()
              // 设标题为 [写] 前缀，侧边栏自动隐藏
              try { await loomRpc('session.rename', { session_id: sid, title: '[写] 写作助手' }) } catch {/* ignore */}
              saveWriteSessionId(sid)
            }
            setAssistantOpen(o => !o)
          }} title="AI 写作助手"><IconSparkles size={13} /></button>
      </div>

      {/* ── 主体 ── */}
      <div className={styles.body}>
        {workspaceRoot && (
          <div className={styles.fileSidebar}>
            <div className={styles.fileSidebarHeader}>
              <span>文件</span>
              <button className={styles.toolbarBtnGhost} onClick={startNewFile} title="新建" style={{ padding: '1px 4px' }}><IconPlus size={12} /></button>
            </div>
            <div className={styles.fileList}>
              {loadingFiles ? <div style={{ padding: 16, fontSize: 12, color: 'var(--text-muted)', textAlign: 'center' }}>加载中…</div>
                : files.length === 0 ? <div style={{ padding: 16, fontSize: 12, color: 'var(--text-muted)', textAlign: 'center' }}>暂无文件</div>
                  : files.map(f => (
                    <div key={f.name} className={activeFilePath === f.name ? styles.fileItemActive : styles.fileItem} onClick={() => openFile(f.name)}>
                      <IconFileText size={12} style={{ flexShrink: 0 }} />
                      <span className={styles.fileItemName}>{f.name}</span>
                      <div className={styles.fileItemActions} onClick={e => e.stopPropagation()}>
                        <button className={styles.fileItemAction} onClick={(e) => { e.stopPropagation(); startRename(f.name) }} title="重命名"><IconEdit size={10} /></button>
                        <button className={styles.fileItemAction} onClick={(e) => { e.stopPropagation(); startDelete(f.name) }} title="删除"><IconTrash size={10} /></button>
                      </div>
                    </div>
                  ))}
            </div>
          </div>
        )}

        {!workspaceRoot ? (
          <div className={styles.workspacePrompt}>
            <IconFolderOpen size={48} className={styles.emptyIcon} />
            <span>在设置中配置工作目录，或手动选择</span>
            <button className={styles.workspacePromptBtn} onClick={pickWorkspace}><IconFolderOpen size={16} /><span>选择工作目录</span></button>
          </div>
        ) : !activeFilePath ? (
          <div className={styles.emptyState}>
            <IconFileText size={40} className={styles.emptyIcon} />
            <span>从左侧选择一个文件，或新建一个开始写作</span>
            <button className={styles.workspacePromptBtn} onClick={startNewFile}><IconFilePlus size={16} /><span>新建文件</span></button>
          </div>
        ) : previewMode === 'preview' ? (
          <div className={styles.editorArea}><div className={styles.preview} dangerouslySetInnerHTML={{ __html: previewHtml }} /></div>
        ) : previewMode === 'split' ? (
          <div className={styles.editorArea}>
            <textarea className={styles.editor} style={{ width: '50%', borderRight: '1px solid var(--border)' }} value={fileContent}
              onChange={e => { setFileContent(e.target.value); setSaveStatus('dirty') }} placeholder="开始写作…" spellCheck={false} />
            <div className={styles.preview} dangerouslySetInnerHTML={{ __html: previewHtml }} />
          </div>
        ) : (
          <div className={styles.editorArea}>
            <textarea className={styles.editor} value={fileContent}
              onChange={e => { setFileContent(e.target.value); setSaveStatus('dirty') }} placeholder="开始写作…" spellCheck={false} />
          </div>
        )}
      </div>

      {/* ── AI 助手底部浮动弹窗 ── */}
      {assistantOpen && (
        <div className={styles.assistantBackdrop} onClick={() => setAssistantOpen(false)} />
      )}
      {assistantOpen && (
        <div className={styles.assistantFloating}>
          <div className={styles.assistantFloatingHeader}>
            <span><IconSparkles size={13} style={{ marginRight: 6 }} />AI 写作助手</span>
            <span className={styles.assistantStatus}>
              {effectiveSessionId ? '已就绪' : '创建会话中…'}
            </span>
            <div style={{ flex: 1 }} />
            <button className={styles.toolbarBtnGhost} onClick={() => setAssistantOpen(false)} style={{ padding: '2px' }}><IconX size={14} /></button>
          </div>
          <div className={styles.assistantHint}>
            按 <kbd>Ctrl+Shift+I</kbd> 对选中文本提问，或直接输入指令：
          </div>
          <div className={styles.assistantFloatingInput}>
            <input
              value={assistantText} onChange={e => setAssistantText(e.target.value)}
              onKeyDown={e => {
                if (e.key !== 'Enter') return
                const ctrlOrMeta = e.ctrlKey || e.metaKey
                const shift = e.shiftKey
                let shouldSend = false
                switch (sendShortcut) {
                  case 'ctrl+enter': shouldSend = ctrlOrMeta && !shift; break
                  case 'shift+enter': shouldSend = shift && !ctrlOrMeta; break
                  default: shouldSend = !ctrlOrMeta && !shift; break
                }
                if (shouldSend) { e.preventDefault(); handleAssistantSend() }
              }}
              placeholder="润色这段文字 / 翻译成英文 / 扩写到 500 字…"
              disabled={!effectiveSessionId || assistantBusy}
            />
            <div className={styles.sendSplit}>
              <button
                className={`${styles.sendSplitMain} ${assistantBusy ? styles.sendSplitStop : ''}`}
                onClick={handleAssistantSend}
                disabled={(!assistantText.trim() && !assistantBusy) || !effectiveSessionId}>
                {assistantBusy ? '停止' : '发送'}
              </button>
              <button
                className={styles.sendSplitCaret}
                title="发送快捷键"
                onClick={(e) => {
                  e.stopPropagation()
                  const caret = e.currentTarget
                  const open = caret.dataset.open === '1'
                  // Close all other open menus first
                  document.querySelectorAll(`.${styles.sendSplitCaret}`).forEach(el => {
                    (el as HTMLElement).dataset.open = '0'
                  })
                  caret.dataset.open = open ? '0' : '1'
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
                      setSendShortcut(k)
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
          </div>
        </div>
      )}

      {/* ── 内联弹窗 ── */}
      {modal.kind !== 'none' && <div className={styles.modalBackdrop} onClick={() => setModal({ kind: 'none' })} />}
      {modal.kind !== 'none' && (
        <div className={styles.modalDialog} onClick={e => e.stopPropagation()}>
          <div className={styles.modalTitle}>
            {modal.kind === 'newFile' ? '新建文件' : modal.kind === 'rename' ? '重命名' : modal.kind === 'delete' ? '确认删除' : '输入工作目录'}
          </div>
          {modal.kind === 'delete' ? (
            <>
              <div style={{ fontSize: 13, color: 'var(--text)', marginBottom: 16 }}>确定删除「{modal.targetName}」？此操作不可撤销。</div>
              <div style={{ display: 'flex', gap: 8, justifyContent: 'flex-end' }}>
                <button className={styles.toolbarBtn} onClick={() => setModal({ kind: 'none' })}>取消</button>
                <button className={styles.toolbarBtn} style={{ borderColor: 'var(--danger)', color: 'var(--danger)' }} onClick={confirmModal}>删除</button>
              </div>
            </>
          ) : (
            <>
              <input ref={modalInputRef} className={styles.modalInput} value={modalInput} onChange={e => setModalInput(e.target.value)}
                onKeyDown={e => { if (e.key === 'Enter') confirmModal(); if (e.key === 'Escape') setModal({ kind: 'none' }) }}
                placeholder={modal.kind === 'newFile' ? '文件名称（可省略 .md）' : modal.kind === 'workspace' ? '工作目录路径' : '新文件名'}
              />
              <div style={{ display: 'flex', gap: 8, justifyContent: 'flex-end' }}>
                <button className={styles.toolbarBtn} onClick={() => setModal({ kind: 'none' })}>取消</button>
                <button className={styles.toolbarBtn} style={{ borderColor: 'var(--accent)', color: 'var(--accent)' }}
                  onClick={modal.kind === 'workspace' ? confirmWorkspace : confirmModal}>确定</button>
              </div>
            </>
          )}
        </div>
      )}

      {toast && <div className={styles.toast}>{toast}</div>}
    </div>
  )
}

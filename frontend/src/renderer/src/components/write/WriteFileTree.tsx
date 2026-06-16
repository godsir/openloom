import React, { useEffect, useCallback } from 'react'
import { useWriteStore, type WorkspaceEntry, type WriteFileKind } from '../../stores/write'
import { loomRpc } from '../../services/jsonrpc'
import { useLocale } from '../../i18n'
import { IconChevronRight, IconChevronDown, IconFileText, IconFile, IconImage, IconFolder, IconLoader } from '../../utils/icons'
import styles from './WriteFileTree.module.css'

// ---- constants ----

// Show ALL files in the workspace. Only dotfiles are filtered.
const IMAGE_EXTS = new Set(['png', 'jpg', 'jpeg', 'gif', 'webp', 'svg', 'bmp', 'ico'])

function getFileKind(ext: string): WriteFileKind {
  if (ext === 'pdf') return 'pdf'
  if (IMAGE_EXTS.has(ext)) return 'image'
  return 'text'
}

function extOf(entry: WorkspaceEntry): string {
  if (entry.extension) return entry.extension.toLowerCase()
  const dot = entry.name.lastIndexOf('.')
  return dot > -1 ? entry.name.slice(dot + 1).toLowerCase() : ''
}

// ---- component ----

interface WriteFileTreeProps {
  onNewFile: () => void;
}

export const WriteFileTree: React.FC<WriteFileTreeProps> = ({ onNewFile }) => {
  const { t } = useLocale();

  // Context menu state
  const setModalState = useWriteStore(s => s.setModalState)
  const [ctxMenu, setCtxMenu] = React.useState<{ x: number; y: number; entry: WorkspaceEntry } | null>(null)

  // Close context menu on any click
  useEffect(() => {
    if (!ctxMenu) return
    const close = () => setCtxMenu(null)
    window.addEventListener('click', close)
    return () => window.removeEventListener('click', close)
  }, [ctxMenu])

  // read state
  const workspaceRoot = useWriteStore(s => s.workspaceRoot)
  const entriesByDir = useWriteStore(s => s.entriesByDir)
  const expandedDirs = useWriteStore(s => s.expandedDirs)
  const activeFilePath = useWriteStore(s => s.activeFilePath)
  const fileLoading = useWriteStore(s => s.fileLoading)

  // actions
  const setEntriesByDir = useWriteStore(s => s.setEntriesByDir)
  const toggleDir = useWriteStore(s => s.toggleDir)
  const setActiveFile = useWriteStore(s => s.setActiveFile)
  const setFileLoading = useWriteStore(s => s.setFileLoading)
  const setFileContent = useWriteStore(s => s.setFileContent)
  const setFileSize = useWriteStore(s => s.setFileSize)
  const setFileTruncated = useWriteStore(s => s.setFileTruncated)
  const setFileError = useWriteStore(s => s.setFileError)
  const showToast = useWriteStore(s => s.showToast)

  // ---- loadDir ----

  const loadDir = useCallback(
    async (dirPath: string) => {
      if (!workspaceRoot) return
      try {
        const result = await loomRpc<{ ok: boolean; entries: WorkspaceEntry[] }>(
          'vfs.list_directory',
          { path: dirPath, workspace_root: workspaceRoot },
        )
        if (!result.ok) {
          showToast('error', 'Failed to list directory')
          return
        }
        const filtered = result.entries
          .filter((e: WorkspaceEntry) => {
            if (e.name.startsWith('.')) return false
            if (e.kind === 'directory') return true
            // Show ALL files — no extension filtering
            return true
          })
          .sort((a: WorkspaceEntry, b: WorkspaceEntry) => {
            if (a.kind !== b.kind) return a.kind === 'directory' ? -1 : 1
            return a.name.localeCompare(b.name)
          })
        setEntriesByDir(dirPath, filtered)
      } catch (e: any) {
        showToast('error', String(e?.message ?? e).slice(0, 80))
      }
    },
    [workspaceRoot, setEntriesByDir, showToast],
  )

  // load root entries whenever workspaceRoot changes
  useEffect(() => {
    if (workspaceRoot) {
      loadDir('.')
    }
  }, [workspaceRoot])

  // Expose refresh for external use (called after create/rename/delete)
  const refreshRoot = useCallback(() => {
    if (workspaceRoot) loadDir('.')
  }, [workspaceRoot, loadDir])

  // Store refresh function globally for WriteFileDialogs to call
  useEffect(() => {
    (window as any).__writeRefreshFileTree = refreshRoot
    return () => { delete (window as any).__writeRefreshFileTree }
  }, [refreshRoot])

  // ---- handleFileClick ----

  const handleFileClick = useCallback(
    async (entry: WorkspaceEntry) => {
      if (entry.kind === 'directory') {
        if (!expandedDirs[entry.path] && !entriesByDir[entry.path]) {
          await loadDir(entry.path)
        }
        toggleDir(entry.path)
        return
      }

      // ---- file ----
      const ext = extOf(entry)
      const kind = getFileKind(ext)

      if (kind === 'text') {
        setFileLoading(true)
        setFileError(null)
        try {
          const result = await loomRpc<{
            ok: boolean
            content: string
            size?: number
            truncated?: boolean
          }>('vfs.read_file', {
            path: entry.path,
            workspace_root: workspaceRoot!,
          })
          if (result.ok) {
            setFileContent(result.content)
            if (result.size !== undefined) setFileSize(result.size)
            if (result.truncated !== undefined) setFileTruncated(result.truncated)
            setActiveFile(entry.path, kind)
          } else {
            setFileError('Failed to read file')
          }
        } catch (e: any) {
          setFileError(String(e?.message ?? e).slice(0, 120))
        } finally {
          setFileLoading(false)
        }
      } else {
        setActiveFile(entry.path, kind)
      }
    },
    [
      workspaceRoot,
      expandedDirs,
      entriesByDir,
      toggleDir,
      loadDir,
      setFileLoading,
      setFileError,
      setFileContent,
      setFileSize,
      setFileTruncated,
      setActiveFile,
    ],
  )

  // ---- render helpers ----

  const entryIcon = (entry: WorkspaceEntry) => {
    if (entry.kind === 'directory') {
      return <IconFolder size={14} />
    }
    const ext = extOf(entry)
    if (IMAGE_EXTS.has(ext)) return <IconImage size={14} />
    if (ext === 'pdf') return <IconFile size={14} />
    return <IconFileText size={14} />
  }

  const renderEntry = (entry: WorkspaceEntry, depth: number): React.ReactNode => {
    const isDir = entry.kind === 'directory'
    const isExpanded = !!expandedDirs[entry.path]
    const isActive = activeFilePath === entry.path
    const isLoadingThisFile = isActive && fileLoading
    const children = entriesByDir[entry.path] ?? []

    return (
      <div key={entry.path}>
        <div
          className={`${styles.entry} ${isActive ? styles.entryActive : ''}`}
          style={{ paddingLeft: 8 + depth * 16 }}
          onClick={() => handleFileClick(entry)}
          onContextMenu={(e) => {
            e.preventDefault()
            setCtxMenu({ x: e.clientX, y: e.clientY, entry })
          }}
          role="treeitem"
          aria-expanded={isDir ? isExpanded : undefined}
          aria-selected={isActive}
          tabIndex={0}
          onKeyDown={(e) => {
            if (e.key === 'Enter' || e.key === ' ') {
              e.preventDefault()
              handleFileClick(entry)
            }
          }}
        >
          <span className={styles.chevron}>
            {isDir ? (
              isExpanded ? <IconChevronDown size={10} /> : <IconChevronRight size={10} />
            ) : null}
          </span>

          <span className={styles.entryIcon}>
            {isLoadingThisFile ? (
              <IconLoader size={14} className={styles.spin} />
            ) : (
              entryIcon(entry)
            )}
          </span>

          <span className={styles.entryName} title={entry.name}>
            {entry.name}
          </span>
        </div>

        {isDir && isExpanded &&
          children.map(child => renderEntry(child, depth + 1))}
      </div>
    )
  }

  // ---- render ----

  const rootEntries = entriesByDir['.'] ?? []

  // empty: workspace has no supported files
  if (rootEntries.length === 0) {
    return (
      <div className={styles.empty}>
        <IconFileText size={32} className={styles.emptyIcon} />
        <span className={styles.emptyText}>{t('write.noFiles', 'No files yet')}</span>
        <button className={styles.newFileBtn} onClick={onNewFile} title={t('write.newFile', 'New File')}>
          + {t('write.newFile', 'New File')}
        </button>
      </div>
    )
  }

  return (
    <>
      <div className={styles.tree} role="tree" onContextMenu={(e) => {
        e.preventDefault()
        setCtxMenu({ x: e.clientX, y: e.clientY, entry: { name: '.', path: '.', kind: 'directory' } })
      }}>
        {rootEntries.map(entry => renderEntry(entry, 0))}
      </div>

      {/* Context menu */}
      {ctxMenu && (
        <div
          style={{
            position: 'fixed', zIndex: 5000, left: ctxMenu.x, top: ctxMenu.y,
            background: 'var(--bg-surface)', border: '1px solid var(--border)',
            borderRadius: 8, padding: 4, minWidth: 150,
            boxShadow: '0 8px 24px rgba(0,0,0,0.3)',
          }}
          onClick={(e) => e.stopPropagation()}
        >
          <ContextMenuItem onClick={() => { onNewFile(); setCtxMenu(null) }}>
            + {t('write.newFile')}
          </ContextMenuItem>
          {ctxMenu.entry.kind === 'directory' && (
            <ContextMenuItem onClick={() => {
              setModalState('newFolder')
              setCtxMenu(null)
            }}>
              + {t('write.newFolder', '新建文件夹')}
            </ContextMenuItem>
          )}
          {ctxMenu.entry.kind !== 'directory' && (
            <>
              <ContextMenuItem onClick={() => {
                setModalState('rename', ctxMenu.entry)
                setCtxMenu(null)
              }}>
                {t('common.rename', '重命名')}
              </ContextMenuItem>
              <ContextMenuItem onClick={() => {
                setModalState('delete', ctxMenu.entry)
                setCtxMenu(null)
              }}>
                {t('common.delete', '删除')}
              </ContextMenuItem>
            </>
          )}
        </div>
      )}
    </>
  )
}

export default WriteFileTree

// ── Context menu helper ──

const ContextMenuItem: React.FC<{ children: React.ReactNode; onClick: () => void }> = ({ children, onClick }) => (
  <div
    onClick={onClick}
    style={{
      padding: '6px 12px', fontSize: 12, cursor: 'pointer', borderRadius: 4,
      color: 'var(--text)', whiteSpace: 'nowrap',
    }}
    onMouseEnter={(e) => (e.currentTarget.style.background = 'var(--bg-hover)')}
    onMouseLeave={(e) => (e.currentTarget.style.background = 'transparent')}
  >
    {children}
  </div>
)

import React, { useState, useEffect, useRef } from 'react'
import { useWriteStore } from '../../stores/write'
import { loomRpc } from '../../services/jsonrpc'
import { useLocale } from '../../i18n'
import Select from '../shared/Select'
import styles from './WriteWorkspaceView.module.css'
import { guardWriteNavigation } from '../../write/navigation-guard'

const FILE_EXT_OPTIONS = [
  { value: '.md', label: '.md' },
  { value: '.txt', label: '.txt' },
]

export const WriteFileDialogs: React.FC = () => {
  const modalState = useWriteStore(s => s.modalState)
  const modalTarget = useWriteStore(s => s.modalTarget)
  const workspaceRoot = useWriteStore(s => s.workspaceRoot)
  const activeFilePath = useWriteStore(s => s.activeFilePath)
  const setModalState = useWriteStore(s => s.setModalState)
  const showToast = useWriteStore(s => s.showToast)
  const setActiveFile = useWriteStore(s => s.setActiveFile)
  const openCreatedTextFile = useWriteStore(s => s.openCreatedTextFile)
  const clearActiveFile = useWriteStore(s => s.clearActiveFile)
  const triggerRefresh = useWriteStore(s => s.triggerRefresh)

  const { t } = useLocale()

  const [inputValue, setInputValue] = useState('')
  const [selectedExt, setSelectedExt] = useState('.md')
  const inputRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    if (modalState !== 'none' && modalState !== 'delete') {
      setTimeout(() => inputRef.current?.focus(), 50)
    }
    // Reset form on open
    if (modalState === 'newFile') {
      setInputValue('')
      setSelectedExt('.md')
    } else if (modalState === 'rename' && modalTarget) {
      setInputValue(modalTarget.name)
    }
  }, [modalState, modalTarget])

  if (modalState === 'none') return null

  const close = () => setModalState('none')

  const handleCreateFolder = async () => {
    if (!workspaceRoot) return
    const raw = inputValue.trim()
    if (!raw) return
    try {
      await loomRpc('vfs.create_directory', { workspace_root: workspaceRoot, path: raw })
      showToast('success', t('write.folderCreated', '文件夹已创建'))
      triggerRefresh()
      close()
    } catch (e: any) {
      showToast('error', e?.message || String(e))
    }
  }

  const handleCreate = async () => {
    if (!workspaceRoot) return
    const raw = inputValue.trim()
    if (!raw) return
    const hasExt = /\.(md|txt|markdown)$/i.test(raw)
    const name = hasExt ? raw : raw + selectedExt
    const title = raw.replace(/\.(md|txt|markdown)$/i, '')
    const content = '# ' + title + '\n\n'
    try {
      if (!(await guardWriteNavigation())) return
      await loomRpc('vfs.write_file', { workspace_root: workspaceRoot, path: name, content })
      openCreatedTextFile(name, content)
      showToast('success', t('write.fileCreated'))
      triggerRefresh()
      close()
    } catch (e: any) {
      showToast('error', e?.message || String(e))
    }
  }

  const handleRename = async () => {
    if (!workspaceRoot || !modalTarget) return
    const newName = inputValue.trim()
    if (!newName || newName === modalTarget.name) return
    try {
      // vfs.rename uses new_name param, source path is in modalTarget.path
      await loomRpc('vfs.rename', { workspace_root: workspaceRoot, path: modalTarget.path, new_name: newName })
      // Update active file path if the renamed file was active
      if (activeFilePath === modalTarget.path) {
        const dir = modalTarget.path.includes('/') ? modalTarget.path.substring(0, modalTarget.path.lastIndexOf('/') + 1) : ''
        setActiveFile(dir + newName, 'text')
      }
      showToast('success', t('write.fileRenamed'))
      triggerRefresh()
      close()
    } catch (e: any) {
      showToast('error', e?.message || String(e))
    }
  }

  const handleDelete = async () => {
    if (!workspaceRoot || !modalTarget) return
    try {
      await loomRpc('vfs.delete', { workspace_root: workspaceRoot, path: modalTarget.path })
      if (activeFilePath === modalTarget.path) {
        clearActiveFile()
      }
      showToast('success', t('write.fileDeleted'))
      triggerRefresh()
      close()
    } catch (e: any) {
      showToast('error', e?.message || String(e))
    }
  }

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter') {
      if (modalState === 'newFile') handleCreate()
      else if (modalState === 'newFolder') handleCreateFolder()
      else if (modalState === 'rename') handleRename()
    }
    if (e.key === 'Escape') close()
  }

  return (
    <>
      <div className={styles.modalBackdrop} onClick={close} />
      <div className={styles.modalDialog} onClick={e => e.stopPropagation()}>
        <div className={styles.modalTitle}>
          {modalState === 'newFile'
            ? t('write.newFile')
            : modalState === 'newFolder'
              ? t('write.newFolder', '新建文件夹')
              : modalState === 'rename'
                ? t('common.rename')
                : t('write.confirmDeleteTitle')}
        </div>

        {modalState === 'delete' ? (
          <>
            <div style={{ fontSize: 13, color: 'var(--text-secondary)', marginBottom: 16, lineHeight: 1.5 }}>
              {t('write.deleteConfirmMsg', { name: modalTarget?.name || '' })}
            </div>
            <div className={styles.modalFooter}>
              <button className={styles.modalBtnCancel} onClick={close}>
                {t('common.cancel')}
              </button>
              <button className={styles.modalBtnDanger} onClick={handleDelete}>
                {t('common.delete')}
              </button>
            </div>
          </>
        ) : (
          <>
            {modalState === 'newFile' ? (
              <div style={{ display: 'flex', gap: 8, marginBottom: 12 }}>
                <input
                  ref={inputRef}
                  className={styles.modalInput}
                  style={{ flex: 1, marginBottom: 0 }}
                  value={inputValue}
                  onChange={e => setInputValue(e.target.value)}
                  onKeyDown={handleKeyDown}
                  placeholder={t('write.fileNamePlaceholder')}
                />
                <Select
                  value={selectedExt}
                  options={FILE_EXT_OPTIONS}
                  onChange={setSelectedExt}
                  variant="pill"
                />
              </div>
            ) : modalState === 'newFolder' ? (
              <input
                ref={inputRef}
                className={styles.modalInput}
                value={inputValue}
                onChange={e => setInputValue(e.target.value)}
                onKeyDown={(e) => { if (e.key === 'Enter') { handleCreateFolder() }; if (e.key === 'Escape') close() }}
                placeholder={t('write.folderNamePlaceholder', '文件夹名')}
              />
            ) : (
              <input
                ref={inputRef}
                className={styles.modalInput}
                value={inputValue}
                onChange={e => setInputValue(e.target.value)}
                onKeyDown={handleKeyDown}
                placeholder={t('write.newFileName')}
              />
            )}
            <div className={styles.modalFooter}>
              <button className={styles.modalBtnCancel} onClick={close}>
                {t('common.cancel')}
              </button>
              <button
                className={styles.modalBtnConfirm}
                onClick={modalState === 'newFile' ? handleCreate : modalState === 'newFolder' ? handleCreateFolder : handleRename}
              >
                {modalState === 'newFile' ? t('common.create') : t('common.rename')}
              </button>
            </div>
          </>
        )}
      </div>
    </>
  )
}

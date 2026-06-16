import React from 'react'
import { useWriteStore } from '../../stores/write'
import { useLocale } from '../../i18n'
import { IconFolderOpen, IconFile } from '../../utils/icons'
import styles from './WriteWorkspaceView.module.css'

interface WriteWorkspaceStartProps {
  onSelectWorkspace?: () => void
}

/**
 * 写作模块的着陆页 — 当没有打开任何文件时显示。
 *
 * 两种状态：
 * 1. 未选择工作目录：显示文件夹图标 + 提示文字 + "选择目录"按钮
 * 2. 已选择工作目录但未打开文件：显示文件图标 + 提示文字
 */
export const WriteWorkspaceStart: React.FC<WriteWorkspaceStartProps> = ({ onSelectWorkspace }) => {
  const workspaceRoot = useWriteStore(s => s.workspaceRoot)
  const activeFilePath = useWriteStore(s => s.activeFilePath)
  const { t } = useLocale()

  // 已有打开的文件时不渲染着陆页
  if (activeFilePath) return null

  // 状态 1：未选择工作目录
  if (!workspaceRoot) {
    return (
      <div className={styles.emptyState}>
        <IconFolderOpen size={48} className={styles.emptyIcon} />
        <span>{t('write.selectDirStart')}</span>
        <button className={styles.workspacePromptBtn} onClick={onSelectWorkspace}>
          <IconFolderOpen size={16} />{t('write.selectDirectory')}
        </button>
      </div>
    )
  }

  // 状态 2：已选择工作目录，但未打开文件
  return (
    <div className={styles.emptyState}>
      <IconFile size={40} className={styles.emptyIcon} />
      <span>{t('write.selectOrNewFilePrompt')}</span>
    </div>
  )
}

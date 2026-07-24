import React, { useMemo } from 'react'
import { useWriteStore } from '../../stores/write'
import WritePreviewModeSelector from './WritePreviewModeSelector'
import WriteFontSizeControl from './WriteFontSizeControl'
import WriteExportMenu from './WriteExportMenu'
import styles from './WriteToolbar.module.css'

const SAVE_STATUS_LABELS: Record<string, string> = {
  saved: '已保存',
  dirty: '未保存',
  saving: '保存中...',
  error: '保存失败',
}

interface WriteToolbarProps {
  onNewFile: () => void
  onSave: () => void
  onToggleAssistant: () => void
}

export default function WriteToolbar({ onNewFile, onSave, onToggleAssistant }: WriteToolbarProps) {
  const saveStatus = useWriteStore((s) => s.saveStatus)
  const activeFilePath = useWriteStore((s) => s.activeFilePath)
  const activeFileKind = useWriteStore((s) => s.activeFileKind)
  const assistantOpen = useWriteStore((s) => s.assistantOpen)
  const fileTruncated = useWriteStore((s) => s.fileTruncated)
  const fileContent = useWriteStore((s) => s.fileContent)

  // 字数统计：CJK 字符按字计，其余按词计（写作场景惯例）
  const wordCount = useMemo(() => {
    if (!activeFilePath || activeFileKind !== 'text') return null
    const cjk = (fileContent.match(/[一-鿿㐀-䶿　-〿＀-￯]/g) || []).length
    const words = (fileContent
      .replace(/[一-鿿㐀-䶿　-〿＀-￯]/g, ' ')
      .match(/[A-Za-z0-9_'-]+/g) || []).length
    return cjk + words
  }, [fileContent, activeFilePath, activeFileKind])

  const statusLabel = SAVE_STATUS_LABELS[saveStatus] || saveStatus
  const statusClass =
    saveStatus === 'saved'
      ? styles.saveStatusSaved
      : saveStatus === 'dirty'
        ? styles.saveStatusDirty
        : saveStatus === 'saving'
          ? styles.saveStatusSaving
          : styles.saveStatusError

  return (
    <div className={styles.toolbar}>
      {/* ── Left: New File + Save + Status ── */}
      <div className={styles.left}>
        <button
          className={styles.btnIcon}
          onClick={onNewFile}
          title="新建文件"
        >
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <line x1="12" y1="5" x2="12" y2="19" />
            <line x1="5" y1="12" x2="19" y2="12" />
          </svg>
        </button>

        <button
          className={styles.btnIcon}
          onClick={onSave}
          disabled={!activeFilePath || saveStatus === 'saving' || fileTruncated}
          title={fileTruncated ? '文件仅加载了部分内容，当前禁止保存' : '保存'}
        >
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <path d="M19 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11l5 5v11a2 2 0 0 1-2 2z" />
            <polyline points="17,21 17,13 7,13 7,21" />
            <polyline points="7,3 7,8 15,8" />
          </svg>
        </button>

        <span className={statusClass}>{statusLabel}</span>
        {wordCount !== null && (
          <span className={styles.wordCount}>{wordCount.toLocaleString()} 字</span>
        )}
      </div>

      {/* ── Center: Preview Mode | Font Size | Export ── */}
      <div className={styles.center}>
        <WritePreviewModeSelector />
        <WriteFontSizeControl />
        <WriteExportMenu />
      </div>

      {/* ── Right: Toggle AI Assistant ── */}
      <div className={styles.right}>
        <button
          className={assistantOpen ? styles.btnAccent : styles.btnGhost}
          onClick={onToggleAssistant}
          title={assistantOpen ? '收起 AI 助手' : '展开 AI 助手'}
        >
          <svg width="14" height="14" viewBox="0 0 24 24" fill={assistantOpen ? 'none' : 'none'} stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" />
          </svg>
        </button>
      </div>
    </div>
  )
}

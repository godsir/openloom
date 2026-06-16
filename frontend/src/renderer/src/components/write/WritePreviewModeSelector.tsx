import React from 'react'
import { useWriteStore, WritePreviewMode } from '../../stores/write'
import styles from './WritePreviewModeSelector.module.css'

const MODES: { value: WritePreviewMode; label: string }[] = [
  { value: 'rich', label: '富文本' },
  { value: 'source', label: '源码' },
  { value: 'live', label: '实时' },
  { value: 'split', label: '分屏' },
  { value: 'preview', label: '预览' },
]

/**
 * 预览模式选择器 — 切换编辑器的显示模式（富文本/源码/实时/分屏/预览）。
 * 从 useWriteStore 读取 previewMode，通过 setPreviewMode 更新。
 */
export default function WritePreviewModeSelector() {
  const previewMode = useWriteStore((s) => s.previewMode)
  const setPreviewMode = useWriteStore((s) => s.setPreviewMode)

  return (
    <div className={styles.segmentedControl}>
      {MODES.map((m) => (
        <button
          key={m.value}
          className={m.value === previewMode ? styles.segmentBtnActive : styles.segmentBtn}
          onClick={() => setPreviewMode(m.value)}
          title={m.label}
        >
          {m.label}
        </button>
      ))}
    </div>
  )
}

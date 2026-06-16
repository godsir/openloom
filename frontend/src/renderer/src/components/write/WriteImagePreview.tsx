import React from 'react'
import { useWriteStore } from '../../stores/write'
import { IconImage } from '../../utils/icons'

/**
 * Phase 4 placeholder — image file preview.
 * When a user opens an image file in the Write workspace, this component renders
 * a centered placeholder instead of the CodeMirror editor.
 *
 * Reads activeFileKind + activeFilePath directly from useWriteStore;
 * returns null for non-image files so the parent can fall back to the text editor.
 */
const WriteImagePreview: React.FC = () => {
  const activeFileKind = useWriteStore(s => s.activeFileKind)
  const activeFilePath = useWriteStore(s => s.activeFilePath)

  if (activeFileKind !== 'image' || !activeFilePath) return null

  return (
    <div style={containerStyle}>
      <IconImage size={48} style={iconStyle} />
      <p style={textStyle}>图片预览功能将在阶段 4 实现</p>
    </div>
  )
}

const containerStyle: React.CSSProperties = {
  display: 'flex',
  flexDirection: 'column',
  alignItems: 'center',
  justifyContent: 'center',
  height: '100%',
  width: '100%',
  gap: 16,
  color: 'var(--text-muted)',
  userSelect: 'none',
}

const iconStyle: React.CSSProperties = {
  opacity: 0.35,
}

const textStyle: React.CSSProperties = {
  fontSize: 14,
  margin: 0,
  opacity: 0.6,
}

export default WriteImagePreview

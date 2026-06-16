import React, { useCallback } from 'react'
import { useWriteStore, WritePreviewMode } from '../../stores/write'
import { WriteMarkdownEditor } from './WriteMarkdownEditor'
import WriteMarkdownPreview from './WriteMarkdownPreview'
import WriteImagePreview from './WriteImagePreview'
import WriteWorkspaceStart from './WriteWorkspaceStart'

// ============================================================
// 常量
// ============================================================

const LARGE_FILE_THRESHOLD = 300 * 1024 // 300 KB — 大文件强制切回源码模式

// ============================================================
// 辅助函数
// ============================================================

/**
 * 解析实际使用的预览模式：
 * - 大文件 (>300K) 强制降级到 source 模式，避免卡顿
 * - 非文本类型 (image/pdf) 忽略预览模式，由调用方在上层分流
 */
function resolvePreviewMode(
  previewMode: WritePreviewMode,
  fileSize: number,
): WritePreviewMode {
  if (fileSize > LARGE_FILE_THRESHOLD) return 'source'
  return previewMode
}

// ============================================================
// WriteDocumentPane
// ============================================================

export const WriteDocumentPane: React.FC = () => {
  // ── 文件状态 ──
  const activeFilePath = useWriteStore(s => s.activeFilePath)
  const activeFileKind = useWriteStore(s => s.activeFileKind)
  const fileContent = useWriteStore(s => s.fileContent)
  const fileLoading = useWriteStore(s => s.fileLoading)
  const fileError = useWriteStore(s => s.fileError)
  const fileSize = useWriteStore(s => s.fileSize)

  // ── 视图设置 ──
  const previewMode = useWriteStore(s => s.previewMode)

  // ── 操作 ──
  const setFileContent = useWriteStore(s => s.setFileContent)
  const setSaveStatus = useWriteStore(s => s.setSaveStatus)

  // ── 内容变更回调 ──
  const handleChange = useCallback(
    (value: string) => {
      setFileContent(value)
      setSaveStatus('dirty')
    },
    [setFileContent, setSaveStatus],
  )

  // ── 路由: 无活动文件 → 起始页 ──
  if (!activeFilePath) {
    return <WriteWorkspaceStart />
  }

  // ── 路由: 加载中 ──
  if (fileLoading) {
    return (
      <div className="write-doc-pane write-doc-pane--loading">
        <div className="write-doc-pane__spinner" />
        <span className="write-doc-pane__loading-text">Loading...</span>
      </div>
    )
  }

  // ── 路由: 读取错误 ──
  if (fileError) {
    return (
      <div className="write-doc-pane write-doc-pane--error">
        <div className="write-doc-pane__error-icon">!</div>
        <span className="write-doc-pane__error-text">{fileError}</span>
      </div>
    )
  }

  // ── 路由: 图片文件 (Phase 4 占位) ──
  if (activeFileKind === 'image') {
    return <WriteImagePreview filePath={activeFilePath} />
  }

  // ── 路由: PDF 文件 (Phase 4 占位) ──
  if (activeFileKind === 'pdf') {
    return (
      <div className="write-doc-pane write-doc-pane--pdf">
        <div className="write-doc-pane__pdf-icon">PDF</div>
        <span className="write-doc-pane__pdf-name">{activeFilePath.split('/').pop()}</span>
        <span className="write-doc-pane__pdf-hint">PDF preview coming in Phase 4</span>
      </div>
    )
  }

  // ── 路由: 文本文件 ──
  const effectiveMode = resolvePreviewMode(previewMode, fileSize)

  // 纯预览模式 — 全屏 Markdown 渲染
  if (effectiveMode === 'preview') {
    return <WriteMarkdownPreview content={fileContent} />
  }

  // 分屏模式 — 编辑器(左) + 预览(右)
  if (effectiveMode === 'split') {
    return (
      <div className="write-doc-pane write-doc-pane--split">
        <div className="write-doc-pane__editor">
          <WriteMarkdownEditor value={fileContent} onChange={handleChange} />
        </div>
        <div className="write-doc-pane__preview">
          <WriteMarkdownPreview content={fileContent} />
        </div>
      </div>
    )
  }

  // source / live / rich — 仅编辑器
  return <WriteMarkdownEditor value={fileContent} onChange={handleChange} />
}

export default WriteDocumentPane

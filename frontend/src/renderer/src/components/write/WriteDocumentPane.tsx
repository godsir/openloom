import React, { useCallback, useState, useRef, useEffect } from 'react'
import { useWriteStore, WritePreviewMode } from '../../stores/write'
import { WriteMarkdownEditor } from './WriteMarkdownEditor'
import { WriteMarkdownPreview } from './WriteMarkdownPreview'
import { WriteImagePreview } from './WriteImagePreview'
import { WriteWorkspaceStart } from './WriteWorkspaceStart'
import { WriteRichEditor, type WriteRichEditorHandle } from '../../write/tiptap/WriteRichEditor'
import { WritePdfViewer } from './WritePdfViewer'
import { WriteInlineAgent } from './WriteInlineAgent'
import { getRenderSafety } from '../../write/write-render-safety'

const LARGE_FILE_THRESHOLD = 300 * 1024

interface WriteDocumentPaneProps {
  onSelectWorkspace: () => void;
  onSendToAssistant: (text: string) => void;
}

// ── SplitView with draggable divider ──

const SplitView: React.FC<{ left: React.ReactNode; right: React.ReactNode }> = ({ left, right }) => {
  const [ratio, setRatio] = useState(50)
  const dragging = useRef(false)
  const containerRef = useRef<HTMLDivElement>(null)

  const onMouseDown = useCallback((e: React.MouseEvent) => {
    e.preventDefault()
    dragging.current = true
    document.body.style.cursor = 'col-resize'
    document.body.style.userSelect = 'none'
  }, [])

  useEffect(() => {
    const onMouseMove = (e: MouseEvent) => {
      if (!dragging.current || !containerRef.current) return
      const rect = containerRef.current.getBoundingClientRect()
      const x = e.clientX - rect.left
      const pct = Math.max(20, Math.min(80, (x / rect.width) * 100))
      setRatio(pct)
    }
    const onMouseUp = () => {
      dragging.current = false
      document.body.style.cursor = ''
      document.body.style.userSelect = ''
    }
    window.addEventListener('mousemove', onMouseMove)
    window.addEventListener('mouseup', onMouseUp)
    return () => {
      window.removeEventListener('mousemove', onMouseMove)
      window.removeEventListener('mouseup', onMouseUp)
    }
  }, [])

  return (
    <div ref={containerRef} style={{ display: 'flex', flex: 1, overflow: 'hidden' }}>
      <div style={{ width: `${ratio}%`, overflow: 'hidden' }}>{left}</div>
      <div
        onMouseDown={onMouseDown}
        style={{
          width: 6, cursor: 'col-resize', flexShrink: 0,
          background: 'var(--border)', transition: 'background 0.15s',
        }}
        onMouseEnter={(e) => (e.currentTarget.style.background = 'var(--accent)')}
        onMouseLeave={(e) => { if (!dragging.current) e.currentTarget.style.background = 'var(--border)' }}
      />
      <div style={{ flex: 1, overflow: 'auto' }}>{right}</div>
    </div>
  )
}

// ── Main component ──

function resolvePreviewMode(previewMode: WritePreviewMode, fileSize: number): WritePreviewMode {
  if (fileSize > LARGE_FILE_THRESHOLD) return 'source'
  return previewMode
}

export const WriteDocumentPane: React.FC<WriteDocumentPaneProps> = ({ onSelectWorkspace, onSendToAssistant }) => {
  const richEditorRef = useRef<WriteRichEditorHandle>(null)
  const activeFilePath = useWriteStore(s => s.activeFilePath)
  const activeFileKind = useWriteStore(s => s.activeFileKind)
  const fileContent = useWriteStore(s => s.fileContent)
  const fileLoading = useWriteStore(s => s.fileLoading)
  const fileError = useWriteStore(s => s.fileError)

  // Detect HTML files for raw rendering (bypass markdown-it)
  const isHtmlFile = activeFilePath ? /\.html?$/i.test(activeFilePath) : false
  const fileSize = useWriteStore(s => s.fileSize)
  const fileTruncated = useWriteStore(s => s.fileTruncated)
  const previewMode = useWriteStore(s => s.previewMode)
  const fontSize = useWriteStore(s => s.fontSize)
  const workspaceRoot = useWriteStore(s => s.workspaceRoot)
  const setFileContent = useWriteStore(s => s.setFileContent)
  const setSaveStatus = useWriteStore(s => s.setSaveStatus)
  const renderSafety = getRenderSafety({
    isMarkdown: !!activeFilePath && /\.(md|markdown)$/i.test(activeFilePath),
    contentLength: fileContent.length,
    fileSize,
    truncated: fileTruncated,
  })

  const handleChange = useCallback((value: string) => {
    if (renderSafety.readOnly) return
    setFileContent(value)
    setSaveStatus('dirty')
  }, [renderSafety.readOnly, setFileContent, setSaveStatus])

  const handleApplyEdit = useCallback((newContent: string) => {
    if (renderSafety.readOnly) return
    setFileContent(newContent)
    setSaveStatus('dirty')
  }, [renderSafety.readOnly, setFileContent, setSaveStatus])

  if (!activeFilePath) {
    return <WriteWorkspaceStart onSelectWorkspace={onSelectWorkspace} />
  }

  if (fileLoading) {
    return (
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'center', height: '100%', color: 'var(--text-muted)', gap: 8 }}>
        <div style={{ width: 16, height: 16, border: '2px solid var(--border)', borderTopColor: 'var(--accent)', borderRadius: '50%', animation: 'spin 0.8s linear infinite' }} />
        <span style={{ fontSize: 13 }}>Loading...</span>
      </div>
    )
  }

  if (fileError) {
    return (
      <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center', height: '100%', color: 'var(--text-error)', gap: 8 }}>
        <span style={{ fontSize: 24 }}>!</span>
        <span style={{ fontSize: 13 }}>{fileError}</span>
      </div>
    )
  }

  if (activeFileKind === 'image') {
    return <WriteImagePreview />
  }

  if (activeFileKind === 'pdf') {
    return <WritePdfViewer filePath={activeFilePath} workspaceRoot={workspaceRoot || ''} />
  }

  const effectiveMode = renderSafety.readOnly ? 'source' : resolvePreviewMode(previewMode, fileSize)

  if (effectiveMode === 'rich') {
    return (
      <div style={{ flex: 1, display: 'flex', flexDirection: 'column', minHeight: 0 }}>
        <WriteInlineAgent
          editorValue={fileContent}
          onApplyEdit={handleApplyEdit}
          onSendToAssistant={onSendToAssistant}
          onRichBlockType={(type) => { richEditorRef.current?.applyBlock(type) }}
          onRichInlineFormat={(kind) => { richEditorRef.current?.toggleInline(kind) }}
          getRichActiveState={() => richEditorRef.current?.getActiveState()}
        />
        <WriteRichEditor ref={richEditorRef} value={fileContent} onChange={handleChange} fontSize={fontSize} />
      </div>
    )
  }

  if (effectiveMode === 'live') {
    return (
      <div style={{ flex: 1, display: 'flex', flexDirection: 'column', minHeight: 0 }}>
        <WriteInlineAgent editorValue={fileContent} onApplyEdit={handleApplyEdit} onSendToAssistant={onSendToAssistant} />
        <WriteMarkdownEditor value={fileContent} onChange={handleChange} fontSize={fontSize} previewMode="live" />
      </div>
    )
  }

  if (effectiveMode === 'preview') {
    return <WriteMarkdownPreview content={fileContent} rawHtml={isHtmlFile} />
  }

  if (effectiveMode === 'split') {
    return <SplitView
      left={
        <div style={{ display: 'flex', flexDirection: 'column', height: '100%' }}>
          <WriteInlineAgent editorValue={fileContent} onApplyEdit={handleApplyEdit} onSendToAssistant={onSendToAssistant} />
          <div style={{ flex: 1, minHeight: 0 }}>
            <WriteMarkdownEditor value={fileContent} onChange={handleChange} fontSize={fontSize} previewMode={effectiveMode} />
          </div>
        </div>
      }
      right={<WriteMarkdownPreview content={fileContent} rawHtml={isHtmlFile} />}
    />
  }

  return (
    <div style={{ flex: 1, display: 'flex', flexDirection: 'column', minHeight: 0 }}>
      {renderSafety.notice === 'truncated' && (
        <div style={{ padding: '7px 12px', fontSize: 12, color: 'var(--text-warning)', background: 'var(--bg-surface)', borderBottom: '1px solid var(--border)' }}>
          文件仅加载了部分内容，为防止覆盖原文件，当前已设为只读。
        </div>
      )}
      {!renderSafety.readOnly && <WriteInlineAgent editorValue={fileContent} onApplyEdit={handleApplyEdit} onSendToAssistant={onSendToAssistant} />}
      <WriteMarkdownEditor value={fileContent} onChange={handleChange} fontSize={fontSize} previewMode={effectiveMode} readOnly={renderSafety.readOnly} />
    </div>
  )
}

export default WriteDocumentPane

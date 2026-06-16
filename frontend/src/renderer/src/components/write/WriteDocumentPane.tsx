import React, { useCallback, useState, useRef, useEffect } from 'react'
import { useWriteStore, WritePreviewMode } from '../../stores/write'
import { WriteMarkdownEditor } from './WriteMarkdownEditor'
import { WriteMarkdownPreview } from './WriteMarkdownPreview'
import { WriteImagePreview } from './WriteImagePreview'
import { WriteWorkspaceStart } from './WriteWorkspaceStart'
import { WriteRichEditor } from '../../write/tiptap/WriteRichEditor'
import { WritePdfViewer } from './WritePdfViewer'

const LARGE_FILE_THRESHOLD = 300 * 1024

interface WriteDocumentPaneProps {
  onSelectWorkspace: () => void;
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
      <div style={{ flex: 1, overflow: 'hidden' }}>{right}</div>
    </div>
  )
}

// ── Main component ──

function resolvePreviewMode(previewMode: WritePreviewMode, fileSize: number): WritePreviewMode {
  if (fileSize > LARGE_FILE_THRESHOLD) return 'source'
  return previewMode
}

export const WriteDocumentPane: React.FC<WriteDocumentPaneProps> = ({ onSelectWorkspace }) => {
  const activeFilePath = useWriteStore(s => s.activeFilePath)
  const activeFileKind = useWriteStore(s => s.activeFileKind)
  const fileContent = useWriteStore(s => s.fileContent)
  const fileLoading = useWriteStore(s => s.fileLoading)
  const fileError = useWriteStore(s => s.fileError)

  // Detect HTML files for raw rendering (bypass markdown-it)
  const isHtmlFile = activeFilePath ? /\.html?$/i.test(activeFilePath) : false
  const fileSize = useWriteStore(s => s.fileSize)
  const previewMode = useWriteStore(s => s.previewMode)
  const fontSize = useWriteStore(s => s.fontSize)
  const workspaceRoot = useWriteStore(s => s.workspaceRoot)
  const setFileContent = useWriteStore(s => s.setFileContent)
  const setSaveStatus = useWriteStore(s => s.setSaveStatus)

  const handleChange = useCallback((value: string) => {
    setFileContent(value)
    setSaveStatus('dirty')
  }, [setFileContent, setSaveStatus])

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

  const effectiveMode = resolvePreviewMode(previewMode, fileSize)

  if (effectiveMode === 'rich') {
    return <WriteRichEditor value={fileContent} onChange={handleChange} fontSize={fontSize} />
  }

  if (effectiveMode === 'preview') {
    return <WriteMarkdownPreview content={fileContent} rawHtml={isHtmlFile} />
  }

  if (effectiveMode === 'split') {
    return <SplitView left={<WriteMarkdownEditor value={fileContent} onChange={handleChange} />} right={<WriteMarkdownPreview content={fileContent} rawHtml={isHtmlFile} />} />
  }

  return <WriteMarkdownEditor value={fileContent} onChange={handleChange} />
}

export default WriteDocumentPane

import React, { useCallback } from 'react'
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

  // No active file → landing page
  if (!activeFilePath) {
    return <WriteWorkspaceStart onSelectWorkspace={onSelectWorkspace} />
  }

  // Loading
  if (fileLoading) {
    return (
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'center', height: '100%', color: 'var(--text-muted)', gap: 8 }}>
        <div style={{ width: 16, height: 16, border: '2px solid var(--border)', borderTopColor: 'var(--accent)', borderRadius: '50%', animation: 'spin 0.8s linear infinite' }} />
        <span style={{ fontSize: 13 }}>Loading...</span>
      </div>
    )
  }

  // Error
  if (fileError) {
    return (
      <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center', height: '100%', color: 'var(--text-error)', gap: 8 }}>
        <span style={{ fontSize: 24 }}>!</span>
        <span style={{ fontSize: 13 }}>{fileError}</span>
      </div>
    )
  }

  // Image file
  if (activeFileKind === 'image') {
    return <WriteImagePreview />
  }

  // PDF file
  if (activeFileKind === 'pdf') {
    return <WritePdfViewer filePath={activeFilePath} workspaceRoot={workspaceRoot || ''} />
  }

  // Text file — resolve effective mode
  const effectiveMode = resolvePreviewMode(previewMode, fileSize)

  // Rich mode
  if (effectiveMode === 'rich') {
    return <WriteRichEditor value={fileContent} onChange={handleChange} fontSize={fontSize} />
  }

  // Preview only
  if (effectiveMode === 'preview') {
    return <WriteMarkdownPreview content={fileContent} />
  }

  // Split mode
  if (effectiveMode === 'split') {
    return (
      <div style={{ display: 'flex', flex: 1, overflow: 'hidden' }}>
        <div style={{ flex: 1, borderRight: '1px solid var(--border)' }}>
          <WriteMarkdownEditor value={fileContent} onChange={handleChange} />
        </div>
        <div style={{ flex: 1 }}>
          <WriteMarkdownPreview content={fileContent} />
        </div>
      </div>
    )
  }

  // Source / Live
  return <WriteMarkdownEditor value={fileContent} onChange={handleChange} />
}

export default WriteDocumentPane

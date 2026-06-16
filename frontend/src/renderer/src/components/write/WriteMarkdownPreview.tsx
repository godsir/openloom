import React, { useMemo } from 'react'
import { renderMarkdown } from '../../utils/markdown'
import { sanitizeHtml } from '../../utils/markdown-sanitizer'

interface WriteMarkdownPreviewProps {
  content: string
  style?: React.CSSProperties
}

/**
 * Renders markdown content to sanitized HTML for preview in Write mode.
 * Uses useMemo to avoid re-running the render + sanitize pipeline on every render.
 */
export const WriteMarkdownPreview: React.FC<WriteMarkdownPreviewProps> = ({ content, style }) => {
  const html = useMemo(() => {
    if (!content) return ''
    return sanitizeHtml(renderMarkdown(content))
  }, [content])

  return (
    <div
      className="markdown-preview"
      style={style}
      dangerouslySetInnerHTML={{ __html: html }}
    />
  )
}

import React, { useMemo } from 'react'
import { renderMarkdown } from '../../utils/markdown'
import { sanitizeHtml } from '../../utils/markdown-sanitizer'

interface WriteMarkdownPreviewProps {
  content: string
  style?: React.CSSProperties
  /** If true, bypass markdown-it and render raw HTML directly (for .html files) */
  rawHtml?: boolean
}

/**
 * Renders content to sanitized HTML for preview in Write mode.
 * - Markdown files: renderMarkdown → sanitizeHtml
 * - HTML files (rawHtml=true): sanitizeHtml only, no markdown processing
 */
export const WriteMarkdownPreview: React.FC<WriteMarkdownPreviewProps> = ({ content, style, rawHtml }) => {
  const html = useMemo(() => {
    if (!content) return ''
    if (rawHtml) {
      // For HTML files, skip markdown processing — just sanitize
      return sanitizeHtml(content)
    }
    return sanitizeHtml(renderMarkdown(content))
  }, [content, rawHtml])

  return (
    <div
      className="markdown-preview"
      style={style}
      dangerouslySetInnerHTML={{ __html: html }}
    />
  )
}

import { useRef, useEffect, useCallback } from 'react'
import type { ContentBlock } from '../../stores/chat'
import { renderMarkdown } from '../../utils/markdown'
import { sanitizeHtml } from '../../utils/markdown-sanitizer'

export default function TextBlock({ block }: { block: ContentBlock }) {
  const html = (block.html as string) || ''
  const source = (block.source as string) || ''
  const displayHtml = html || sanitizeHtml(renderMarkdown(source))
  const containerRef = useRef<HTMLDivElement>(null)

  const handleClick = useCallback((e: MouseEvent) => {
    const target = e.target as HTMLElement

    // "Open file" button injected by filePathPlugin
    const openBtn = target.closest('.open-file-btn')
    if (openBtn) {
      e.preventDefault()
      const filePath = (openBtn as HTMLElement).getAttribute('data-file-path')
      if (filePath) window.hana.openFile(filePath)
      return
    }

    // <a> links whose href is a local absolute path
    const link = target.closest('a')
    if (link) {
      const href = link.getAttribute('href') || ''
      if (/^[A-Za-z]:\\/.test(href) || /^\/\S+\.\w{1,10}$/.test(href)) {
        e.preventDefault()
        window.hana.openFile(href)
      }
    }
  }, [])

  useEffect(() => {
    const el = containerRef.current
    if (!el) return
    el.addEventListener('click', handleClick)
    return () => el.removeEventListener('click', handleClick)
  }, [handleClick])

  return (
    <div
      ref={containerRef}
      className="prose-chat max-w-none text-[14px] text-[var(--text)]"
      dangerouslySetInnerHTML={{ __html: displayHtml }}
    />
  )
}

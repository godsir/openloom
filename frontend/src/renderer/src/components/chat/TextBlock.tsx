import { useRef, useEffect, useCallback } from 'react'
import type { ContentBlock } from '../../stores/chat'
import { renderMarkdown } from '../../utils/markdown'
import { sanitizeHtml } from '../../utils/markdown-sanitizer'
import { renderMermaidDiagram } from '../../utils/mermaid-renderer'
import { useStore } from '../../stores'

const IMAGE_EXT = /\.(png|jpg|jpeg|gif|webp|svg|bmp|ico)(\?.*)?$/i

export default function TextBlock({ block }: { block: ContentBlock }) {
  const html = (block.html as string) || ''
  const source = (block.source as string) || ''
  const displayHtml = html || sanitizeHtml(renderMarkdown(source))
  const containerRef = useRef<HTMLDivElement>(null)
  const openLightbox = useStore(s => s.openLightbox)

  const handleClick = useCallback((e: MouseEvent) => {
    const target = e.target as HTMLElement

    // "Open file" button injected by filePathPlugin
    const openBtn = target.closest('.open-file-btn')
    if (openBtn) {
      e.preventDefault()
      const filePath = (openBtn as HTMLElement).getAttribute('data-file-path')
      if (filePath) window.loom.openFile(filePath)
      return
    }

    // <a> links
    const link = target.closest('a')
    if (link) {
      const href = link.getAttribute('href') || ''
      // Local absolute path → open in file manager
      if (/^[A-Za-z]:\\/.test(href) || /^\/\S+\.\w{1,10}$/.test(href)) {
        e.preventDefault()
        window.loom.openFile(href)
        return
      }
      // HTTP/HTTPS links
      if (/^https?:\/\//i.test(href)) {
        e.preventDefault()
        // Image links → open in lightbox
        if (IMAGE_EXT.test(href)) {
          openLightbox(href)
        } else {
          // Other links → system browser
          window.loom.openExternal(href)
        }
        return
      }
    }
  }, [openLightbox])

  useEffect(() => {
    const el = containerRef.current
    if (!el) return
    el.addEventListener('click', handleClick)
    return () => el.removeEventListener('click', handleClick)
  }, [handleClick])

  // Render mermaid diagrams after the HTML is in the DOM
  useEffect(() => {
    const el = containerRef.current
    if (!el) return
    const placeholders = el.querySelectorAll<HTMLElement>('.mermaid-placeholder')
    placeholders.forEach((ph) => {
      const source = ph.getAttribute('data-mermaid-source')
      if (source) {
        renderMermaidDiagram(ph, source)
      }
    })
  }, [displayHtml])

  return (
    <div
      ref={containerRef}
      className="prose-chat max-w-none text-[14px] text-[var(--text)]"
      dangerouslySetInnerHTML={{ __html: displayHtml }}
    />
  )
}

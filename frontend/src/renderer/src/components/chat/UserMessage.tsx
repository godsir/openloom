import type { Message } from '../../stores/chat'
import { useStore } from '../../stores'
import FileBlock from './FileBlock'
import QuotedSelectionCard from '../input/QuotedSelectionCard'
import MessageFooterActions from './MessageFooterActions'
import { useCallback, useRef, useEffect, memo } from 'react'
import styles from './UserMessage.module.css'

const IMAGE_EXT = /\.(png|jpg|jpeg|gif|webp|svg|bmp|ico)(\?.*)?$/i

const UserMessage = memo(function UserMessage({ message }: { message: Message }) {
  const textBlock = message.blocks.find((b) => b.type === 'text')
  const imageBlocks = message.blocks.filter((b) => b.type === 'image')
  const fileBlocks = message.blocks.filter((b) => b.type === 'file')
  const quotedSelectionBlocks = message.blocks.filter((b) => b.type === 'quoted_selection')
  const hasVisualBlocks = imageBlocks.length > 0 || fileBlocks.length > 0 || quotedSelectionBlocks.length > 0
  const openLightbox = useStore(s => s.openLightbox)
  const textRef = useRef<HTMLDivElement>(null)

  const handleTextClick = useCallback((e: MouseEvent) => {
    const target = e.target as HTMLElement
    if (target.tagName === 'IMG') {
      const img = target as HTMLImageElement
      if (img.complete && img.naturalWidth > 0 && img.src) {
        e.preventDefault()
        openLightbox(img.src)
        return
      }
    }
    const link = target.closest('a')
    if (link) {
      const href = link.getAttribute('href') || ''
      if (/^https?:\/\//i.test(href)) {
        e.preventDefault()
        if (IMAGE_EXT.test(href)) {
          openLightbox(href)
        } else {
          window.loom.openExternal(href)
        }
        return
      }
      if (/^[A-Za-z]:\\/.test(href) || /^\/\S+\.\w{1,10}$/.test(href)) {
        e.preventDefault()
        window.loom.openFile(href)
      }
    }
  }, [openLightbox])

  useEffect(() => {
    const el = textRef.current
    if (!el) return
    el.addEventListener('click', handleTextClick)
    return () => el.removeEventListener('click', handleTextClick)
  }, [handleTextClick])

  return (
    <div className={styles.wrapper} data-message-id={message.id}>
      <div className={styles.bubble}>
        <div className={styles.content}>
          {hasVisualBlocks && (
            <div className={styles.attachments}>
              {quotedSelectionBlocks.length > 0 && (
                <div className={styles.quotedSelections}>
                  {quotedSelectionBlocks.map((block, i) => (
                    <QuotedSelectionCard
                      key={`qs-${i}`}
                      text={(block.text as string) || ''}
                      filePath={block.filePath as string | undefined}
                    />
                  ))}
                </div>
              )}
              {imageBlocks.map((block, i) => (
                <div key={`img-${i}`} className={styles.imagePreview}>
                  <img
                    src={(block.thumbnail as string) || (block.path as string)}
                    alt={(block.name as string) || 'image'}
                    className={styles.imageThumb}
                    onClick={() => {
                      const src = (block.thumbnail as string) || (block.path as string)
                      if (src) openLightbox(src)
                    }}
                  />
                </div>
              ))}
              {fileBlocks.map((block, i) => (
                <FileBlock key={`file-${i}`} block={block} />
              ))}
            </div>
          )}
          {textBlock ? (
            <div
              ref={textRef}
              className={styles.text}
              dangerouslySetInnerHTML={{
                __html: (textBlock.html as string) || escapeHtml(textBlock.source as string) || '',
              }}
            />
          ) : !hasVisualBlocks ? (
            <span className={styles.empty}>(空)</span>
          ) : null}
        </div>
        <MessageFooterActions messageId={message.id} role="user" timestamp={message.timestamp} usage={message.usage} />
      </div>
    </div>
  )
}, (prev, next) => {
  return prev.message.id === next.message.id &&
    prev.message.blocks === next.message.blocks
})

export default UserMessage

function escapeHtml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
}

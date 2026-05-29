import type { Message } from '../../stores/chat'
import { useStore } from '../../stores'
import FileBlock from './FileBlock'
import MessageFooterActions from './MessageFooterActions'
import styles from './UserMessage.module.css'

export default function UserMessage({ message }: { message: Message }) {
  const textBlock = message.blocks.find((b) => b.type === 'text')
  const imageBlocks = message.blocks.filter((b) => b.type === 'image')
  const fileBlocks = message.blocks.filter((b) => b.type === 'file')
  const hasVisualBlocks = imageBlocks.length > 0 || fileBlocks.length > 0
  const openLightbox = useStore(s => s.openLightbox)

  return (
    <div className={styles.wrapper}>
      <div className={styles.bubble}>
        <div className={styles.content}>
          {hasVisualBlocks && (
            <div className={styles.attachments}>
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
}

function escapeHtml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
}

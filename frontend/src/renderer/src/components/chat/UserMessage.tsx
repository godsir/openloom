import type { Message } from '../../stores/chat'
import MessageFooterActions from './MessageFooterActions'
import styles from './UserMessage.module.css'

export default function UserMessage({ message }: { message: Message }) {
  const textBlock = message.blocks.find((b) => b.type === 'text')

  return (
    <div className={styles.wrapper}>
      <div className={styles.bubble}>
        <div className={styles.content}>
          {textBlock ? (
            <div
              className={styles.text}
              dangerouslySetInnerHTML={{
                __html: (textBlock.html as string) || escapeHtml(textBlock.source as string) || '',
              }}
            />
          ) : (
            <span className={styles.empty}>(空)</span>
          )}
        </div>
        <MessageFooterActions messageId={message.id} role="user" timestamp={message.timestamp} />
      </div>
    </div>
  )
}

function escapeHtml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
}

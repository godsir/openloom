import { useStore } from '../../stores'
import { IconCopy, IconTrash } from '../../utils/icons'
import styles from './MessageFooterActions.module.css'

interface Props { messageId: string; role: 'user' | 'assistant'; timestamp: string; usage?: { prompt: number; completion: number } }

function formatTokens(n: number): string {
  if (n >= 1000) return (n / 1000).toFixed(1) + 'k'
  return String(n)
}

export default function MessageFooterActions({ messageId, role, timestamp, usage }: Props) {
  const deleteMessage = useStore((s) => s.deleteMessage)
  const currentSessionId = useStore((s) => s.currentSessionId)

  const handleCopy = () => {
    const msgs = useStore.getState().messagesBySession.get(currentSessionId || '')
    const msg = msgs?.find((m) => m.id === messageId)
    if (!msg) return
    const text = msg.blocks
      .filter((b) => b.type === 'text')
      .map((b) => (b.source as string) || (b.html as string) || '')
      .join('\n')
    if (text) navigator.clipboard.writeText(text)
  }

  const handleDelete = () => {
    if (!currentSessionId) return
    deleteMessage(currentSessionId, messageId)
  }

  const time = new Date(timestamp).toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' })

  return (
    <div className={`${styles.footer} ${role === 'user' ? styles.footerRight : ''}`}>
      <span className={styles.time}>{time}</span>
      {usage && (usage.prompt > 0 || usage.completion > 0) && (
        <span className={styles.tokens} title={`输入 ${usage.prompt} tokens · 输出 ${usage.completion} tokens`}>
          {formatTokens(usage.prompt)}&nbsp;↑&nbsp;{formatTokens(usage.completion)}&nbsp;↓
        </span>
      )}
      <button onClick={handleCopy} className={styles.btn} title="复制">
        <IconCopy size={13} />
      </button>
      <button onClick={handleDelete} className={`${styles.btn} ${styles.btnDanger}`} title="删除">
        <IconTrash size={13} />
      </button>
    </div>
  )
}

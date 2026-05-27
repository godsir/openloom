import { useStore } from '../../stores'
import { IconCopy, IconTrash } from '../../utils/icons'

interface Props { messageId: string; role: 'user' | 'assistant'; timestamp: string }

export default function MessageFooterActions({ messageId, role, timestamp }: Props) {
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
    <div className={`flex items-center gap-0.5 mt-1 opacity-0 group-hover:opacity-100 transition-opacity ${role === 'user' ? 'justify-end' : ''}`}>
      <span className="text-[10px] text-[var(--text-muted)] mr-1 tabular-nums">{time}</span>
      <button onClick={handleCopy} className="flex items-center gap-0.5 text-[10px] text-[var(--text-muted)] hover:text-[var(--accent)] px-1 py-0.5 rounded-[var(--r-sm)] transition-colors">
        <IconCopy size={9} />
      </button>
      <button onClick={handleDelete} className="flex items-center gap-0.5 text-[10px] text-[var(--text-muted)] hover:text-[var(--red)] px-1 py-0.5 rounded-[var(--r-sm)] transition-colors">
        <IconTrash size={9} />
      </button>
    </div>
  )
}

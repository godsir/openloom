import { useStore } from '../../stores'

interface MessageFooterActionsProps {
  messageId: string
  role: 'user' | 'assistant'
  timestamp: string
}

export default function MessageFooterActions({
  messageId,
  role,
  timestamp,
}: MessageFooterActionsProps) {
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

  const formattedTime = new Date(timestamp).toLocaleTimeString('zh-CN', {
    hour: '2-digit',
    minute: '2-digit',
  })

  return (
    <div className="flex items-center gap-1 mt-1 opacity-0 group-hover:opacity-100 transition-opacity">
      <span className="text-[10px] text-zinc-600 mr-1">{formattedTime}</span>
      <button
        onClick={handleCopy}
        className="text-[10px] text-zinc-500 hover:text-zinc-300 px-1 py-0.5 rounded"
      >
        复制
      </button>
      <button
        onClick={handleDelete}
        className="text-[10px] text-zinc-500 hover:text-red-400 px-1 py-0.5 rounded"
      >
        删除
      </button>
    </div>
  )
}

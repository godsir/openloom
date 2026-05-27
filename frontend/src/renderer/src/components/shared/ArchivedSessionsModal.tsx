import { useState } from 'react'
import { useStore } from '../../stores'
import Overlay from './Overlay'
import Button from './Button'

export default function ArchivedSessionsModal({
  open,
  onClose,
}: {
  open: boolean
  onClose: () => void
}) {
  const sessions = useStore((s) => s.sessions)
  const [deleting, setDeleting] = useState<string | null>(null)

  return (
    <Overlay open={open} onClose={onClose} title="已归档会话">
      {sessions.length === 0 ? (
        <p className="text-sm text-zinc-500 text-center py-8">暂无已归档会话</p>
      ) : (
        <div className="space-y-1 max-h-64 overflow-y-auto">
          {sessions.map((s) => (
            <div
              key={s.path}
              className="flex items-center gap-2 px-3 py-2 bg-zinc-800/50 rounded text-sm"
            >
              <span className="flex-1 truncate text-zinc-300">
                {s.title || s.path.slice(0, 8)}
              </span>
              <span className="text-xs text-zinc-500">
                {s.messageCount ?? 0} 条消息
              </span>
              <span className="text-[10px] text-zinc-600">
                {new Date(s.modified).toLocaleDateString('zh-CN')}
              </span>
            </div>
          ))}
        </div>
      )}
    </Overlay>
  )
}

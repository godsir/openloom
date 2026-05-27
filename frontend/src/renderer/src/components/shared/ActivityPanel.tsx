import { useStore } from '../../stores'
import Overlay from './Overlay'
import Button from './Button'

export default function ActivityPanel({
  open,
  onClose,
}: {
  open: boolean
  onClose: () => void
}) {
  const agents = useStore((s) => s.agents)
  const streamingSessions = useStore((s) => s.streamingSessionIds)

  const activeAgents = agents.filter(
    (a) => a.status !== 'idle' && a.status !== 'completed',
  )
  const streamingCount = streamingSessions.size

  return (
    <Overlay open={open} onClose={onClose} title="活动">
      <div className="space-y-4 text-sm">
        <div className="grid grid-cols-2 gap-3">
          <div className="bg-zinc-800/50 rounded-lg p-3 text-center">
            <p className="text-2xl font-bold text-zinc-200">{agents.length}</p>
            <p className="text-xs text-zinc-500 mt-1">Agent 总数</p>
          </div>
          <div className="bg-zinc-800/50 rounded-lg p-3 text-center">
            <p className="text-2xl font-bold text-green-400">{streamingCount}</p>
            <p className="text-xs text-zinc-500 mt-1">活跃流式</p>
          </div>
        </div>

        {activeAgents.length > 0 && (
          <div>
            <h4 className="text-xs font-semibold text-zinc-400 mb-2 uppercase tracking-wider">
              活跃 Agent
            </h4>
            <div className="space-y-1">
              {activeAgents.map((a) => (
                <div
                  key={a.id}
                  className="flex items-center gap-2 px-3 py-2 bg-zinc-800/50 rounded text-sm"
                >
                  <span className="w-2 h-2 rounded-full bg-yellow-500 animate-pulse" />
                  <span className="text-zinc-300">{a.name}</span>
                  <span className="text-xs text-zinc-500 ml-auto">{a.status}</span>
                </div>
              ))}
            </div>
          </div>
        )}

        {activeAgents.length === 0 && streamingCount === 0 && (
          <p className="text-zinc-500 text-center py-4">暂无活动</p>
        )}
      </div>
    </Overlay>
  )
}

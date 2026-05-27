import { useStore } from '../../stores'
import Overlay from './Overlay'

export default function ActivityPanel({ open, onClose }: { open: boolean; onClose: () => void }) {
  const agents = useStore((s) => s.agents)
  const streamingSessions = useStore((s) => s.streamingSessionIds)

  const activeAgents = agents.filter((a) => a.status !== 'idle' && a.status !== 'completed')
  const streamingCount = streamingSessions.size

  return (
    <Overlay open={open} onClose={onClose} title="活动">
      <div className="space-y-4 text-sm">
        <div className="grid grid-cols-2 gap-3">
          <div className="bg-[var(--bg-card)] rounded-[var(--r-sm)] p-4 text-center border border-[var(--border)]">
            <p className="font-display text-2xl text-[var(--text)]">{agents.length}</p>
            <p className="text-[11px] font-mono text-[var(--text-muted)] mt-1 uppercase tracking-wide">Agent 总数</p>
          </div>
          <div className="bg-[var(--bg-card)] rounded-[var(--r-sm)] p-4 text-center border border-[var(--border)]">
            <p className="font-display text-2xl text-[var(--green)]">{streamingCount}</p>
            <p className="text-[11px] font-mono text-[var(--text-muted)] mt-1 uppercase tracking-wide">活跃流式</p>
          </div>
        </div>

        {activeAgents.length > 0 && (
          <div>
            <h4 className="text-[10px] font-mono text-[var(--text-muted)] mb-2 uppercase tracking-widest">
              活跃 Agent
            </h4>
            <div className="space-y-1">
              {activeAgents.map((a) => (
                <div
                  key={a.id}
                  className="flex items-center gap-2 px-3.5 py-2.5 bg-[var(--bg-card)] rounded-[var(--r-sm)] text-sm border border-[var(--border)]"
                >
                  <span className="w-2 h-2 rounded-full bg-[var(--amber)] animate-pulse-soft" />
                  <span className="text-[var(--text-light)]">{a.name}</span>
                  <span className="text-[11px] font-mono text-[var(--text-muted)] ml-auto">{a.status}</span>
                </div>
              ))}
            </div>
          </div>
        )}

        {activeAgents.length === 0 && streamingCount === 0 && (
          <p className="text-[var(--text-muted)] text-center py-6">暂无活动</p>
        )}
      </div>
    </Overlay>
  )
}

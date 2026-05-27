import type { ContentBlock } from '../../stores/chat'

export default function SubagentCard({ block }: { block: ContentBlock }) {
  const name = (block.name as string) || '子 Agent'
  const status = (block.streamStatus as string) || 'running'
  const summary = (block.summary as string) || ''

  return (
    <div className="border border-purple-700/30 rounded-md overflow-hidden">
      <div className="flex items-center gap-2 px-3 py-1.5 bg-purple-900/10">
        <span className="text-xs text-purple-400">&#9733;</span>
        <span className="text-xs text-purple-300">{name}</span>
        <span className={`ml-auto text-[10px] ${status === 'done' ? 'text-green-400' : 'text-yellow-400 animate-pulse'}`}>
          {status === 'done' ? '完成' : '执行中'}
        </span>
      </div>
      {summary && (
        <div className="px-3 py-1.5 text-xs text-zinc-400 border-t border-zinc-700/30 bg-zinc-900/20">
          {summary}
        </div>
      )}
    </div>
  )
}

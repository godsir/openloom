import { useState } from 'react'
import type { ContentBlock } from '../../stores/chat'

export default function ThinkingBlock({ block }: { block: ContentBlock }) {
  const [expanded, setExpanded] = useState(false)
  const sealed = block.sealed as boolean
  const content = block.content as string

  return (
    <div className="border border-zinc-700/50 rounded-md overflow-hidden">
      <button
        onClick={() => setExpanded(!expanded)}
        className="flex items-center gap-2 w-full px-3 py-1.5 text-xs text-zinc-500 hover:bg-zinc-800/50 transition-colors"
      >
        <span className={`transition-transform ${expanded ? 'rotate-90' : ''}`}>
          ▶
        </span>
        <span>思考过程</span>
        {!sealed && (
          <span className="inline-block w-2 h-2 rounded-full bg-yellow-500 animate-pulse" />
        )}
      </button>
      {expanded && (
        <div className="px-3 py-2 text-xs text-zinc-400 border-t border-zinc-700/50 bg-zinc-900/30 whitespace-pre-wrap max-h-48 overflow-y-auto">
          {content}
        </div>
      )}
    </div>
  )
}

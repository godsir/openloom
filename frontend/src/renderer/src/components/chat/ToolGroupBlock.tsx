import { useState } from 'react'
import type { ContentBlock } from '../../stores/chat'

interface ToolCall {
  id: string
  name: string
  status: 'running' | 'done'
  elapsed: number
  args: Record<string, unknown>
  result?: string
}

export default function ToolGroupBlock({ block }: { block: ContentBlock }) {
  const [expandedId, setExpandedId] = useState<string | null>(null)
  const tools = (block.tools as ToolCall[]) || []
  const collapsed = block.collapsed as boolean

  return (
    <div className="border border-blue-700/30 rounded-md overflow-hidden">
      <div className="px-3 py-1.5 text-xs text-blue-400 bg-blue-900/10">
        工具调用 ({tools.length})
      </div>
      <div className={collapsed ? 'hidden' : ''}>
        {tools.map((tool) => (
          <div
            key={tool.id}
            className="border-t border-zinc-700/30 px-3 py-1.5"
          >
            <button
              onClick={() =>
                setExpandedId(expandedId === tool.id ? null : tool.id)
              }
              className="flex items-center gap-2 w-full text-xs"
            >
              <span className="text-blue-400">{tool.name}</span>
              <span className="text-zinc-500">
                {tool.status === 'running' ? '执行中...' : '完成'}
              </span>
            </button>
            {expandedId === tool.id && (
              <div className="mt-1 text-xs text-zinc-500 space-y-1">
                {Object.keys(tool.args).length > 0 && (
                  <pre className="bg-zinc-900/50 rounded p-1 overflow-x-auto text-zinc-600">
                    {JSON.stringify(tool.args, null, 2)}
                  </pre>
                )}
                {tool.result && (
                  <pre className="bg-zinc-900/50 rounded p-1 overflow-x-auto text-zinc-400 max-h-32 overflow-y-auto">
                    {tool.result}
                  </pre>
                )}
              </div>
            )}
          </div>
        ))}
      </div>
    </div>
  )
}

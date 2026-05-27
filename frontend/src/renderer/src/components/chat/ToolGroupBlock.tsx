import { useState } from 'react'
import type { ContentBlock } from '../../stores/chat'
import { IconZap, IconCheck, IconLoader, IconXCircle, IconChevronRight, IconChevronDown } from '../../utils/icons'

interface ToolCall {
  id: string; name: string; status: 'running' | 'done' | 'error'
  elapsed: number; args: Record<string, unknown>; result?: string
}

const statusIcon = (s: string) => {
  if (s === 'done') return <IconCheck size={10} className="text-[var(--accent)]" />
  if (s === 'running') return <IconLoader size={10} className="text-[var(--amber)] animate-spin" />
  return <IconXCircle size={10} className="text-[var(--red)]" />
}

export default function ToolGroupBlock({ block }: { block: ContentBlock }) {
  const [expandedId, setExpandedId] = useState<string | null>(null)
  const tools = (block.tools as ToolCall[]) || []
  const collapsed = block.collapsed as boolean

  return (
    <div className="bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.04)] rounded-[var(--r-md)] overflow-hidden">
      {!collapsed && tools.map((tool, idx) => (
        <div key={tool.id} className={idx > 0 ? 'border-t border-[rgba(255,255,255,0.03)]' : ''}>
          <button
            onClick={() => setExpandedId(expandedId === tool.id ? null : tool.id)}
            className="flex items-center gap-2.5 w-full px-3 py-2 text-[11px] hover:bg-[rgba(255,255,255,0.02)] transition-colors"
          >
            <IconZap size={10} className="text-[var(--accent)] shrink-0" />
            <span className="font-medium text-[var(--text-light)]">{tool.name}</span>
            <span className="ml-auto">{statusIcon(tool.status)}</span>
            {expandedId !== tool.id ? <IconChevronRight size={9} className="text-[var(--text-muted)]" /> : <IconChevronDown size={9} className="text-[var(--text-muted)]" />}
          </button>
          {expandedId === tool.id && (
            <div className="px-3 pb-2.5 space-y-1.5">
              {Object.keys(tool.args).length > 0 && (
                <pre className="bg-[var(--bg)] rounded-[var(--r-sm)] p-2 overflow-x-auto text-[10px] text-[var(--text-muted)] font-mono">
                  {JSON.stringify(tool.args, null, 2)}
                </pre>
              )}
              {tool.result && (
                <pre className="bg-[var(--bg)] rounded-[var(--r-sm)] p-2 overflow-x-auto text-[11px] text-[var(--text-light)] font-mono max-h-36 overflow-y-auto">
                  {tool.result}
                </pre>
              )}
            </div>
          )}
        </div>
      ))}
    </div>
  )
}

import type { Message } from '../../stores/chat'
import ThinkingBlock from './ThinkingBlock'
import ToolGroupBlock from './ToolGroupBlock'
import TextBlock from './TextBlock'
import FileBlock from './FileBlock'
import SubagentCard from './SubagentCard'
import MessageFooterActions from './MessageFooterActions'
import TypingIndicator from '../shared/TypingIndicator'

export default function AssistantMessage({ message }: { message: Message }) {
  return (
    <div className="flex gap-2.5 max-w-[85%] group animate-fade-in">
      {/* Avatar */}
      <div className="w-7 h-7 rounded-full bg-[rgba(0,227,199,0.08)] border border-[rgba(0,227,199,0.12)] flex items-center justify-center shrink-0 mt-0.5">
        <span className="text-[11px] font-extrabold text-[var(--accent)]">L</span>
      </div>
      {/* Content */}
      <div className="flex-1 min-w-0 space-y-2">
        {message.blocks.map((block, i) => {
          switch (block.type) {
            case 'thinking':
              return <ThinkingBlock key={i} block={block} />
            case 'tool_group':
              return <ToolGroupBlock key={i} block={block} />
            case 'text':
              return (
                <div key={i} className="bg-[rgba(0,227,199,0.03)] backdrop-blur-[12px] border border-[rgba(0,227,199,0.06)] rounded-[4px_var(--r-lg)_var(--r-lg)_var(--r-lg)] px-3 py-2.5">
                  <TextBlock block={block} />
                </div>
              )
            case 'file':
              return <FileBlock key={i} block={block} />
            case 'subagent':
              return <SubagentCard key={i} block={block} />
            default:
              return null
          }
        })}
        {message.blocks.length === 0 && (
          <div className="flex items-center gap-2 text-[13px] text-[var(--text-muted)]">
            <span>思考中</span>
            <TypingIndicator />
          </div>
        )}
        {message.blocks.length > 0 && (
          <MessageFooterActions messageId={message.id} role="assistant" timestamp={message.timestamp} />
        )}
      </div>
    </div>
  )
}

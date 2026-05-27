import type { Message } from '../../stores/chat'
import ThinkingBlock from './ThinkingBlock'
import ToolGroupBlock from './ToolGroupBlock'
import TextBlock from './TextBlock'
import FileBlock from './FileBlock'
import SubagentCard from './SubagentCard'
import MessageFooterActions from './MessageFooterActions'

export default function AssistantMessage({ message }: { message: Message }) {
  return (
    <div className="mb-4 max-w-3xl group">
      <div className="text-xs text-zinc-600 mb-1 ml-1">AI</div>
      <div className="space-y-2">
        {message.blocks.map((block, i) => {
          switch (block.type) {
            case 'thinking':
              return <ThinkingBlock key={i} block={block} />
            case 'tool_group':
              return <ToolGroupBlock key={i} block={block} />
            case 'text':
              return <TextBlock key={i} block={block} />
            case 'file':
              return <FileBlock key={i} block={block} />
            case 'subagent':
              return <SubagentCard key={i} block={block} />
            default:
              return null
          }
        })}
        {message.blocks.length === 0 && (
          <div className="text-zinc-500 text-sm animate-pulse">思考中...</div>
        )}
      </div>
      {message.blocks.length > 0 && (
        <MessageFooterActions
          messageId={message.id}
          role="assistant"
          timestamp={message.timestamp}
        />
      )}
    </div>
  )
}

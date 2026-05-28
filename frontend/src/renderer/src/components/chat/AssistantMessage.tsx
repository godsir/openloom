import type { Message } from '../../stores/chat'
import ThinkingBlock from './ThinkingBlock'
import ToolGroupBlock from './ToolGroupBlock'
import TextBlock from './TextBlock'
import FileBlock from './FileBlock'
import SubagentCard from './SubagentCard'
import MessageFooterActions from './MessageFooterActions'
import TypingIndicator from '../shared/TypingIndicator'
import styles from './AssistantMessage.module.css'

export default function AssistantMessage({ message }: { message: Message }) {
  return (
    <div className={styles.message}>
      <div className={styles.header}>
        <div className={styles.avatar}>
          <span className={styles.avatarText}>L</span>
        </div>
        <span className={styles.name}>Loom</span>
        {message.timestamp && (
          <span className={styles.time}>
            {new Date(message.timestamp).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}
          </span>
        )}
      </div>

      <div className={styles.content}>
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
          <div className={styles.thinkingHint}>
            <span>思考中</span>
            <TypingIndicator />
          </div>
        )}
        {message.blocks.length > 0 && (
          <MessageFooterActions messageId={message.id} role="assistant" timestamp={message.timestamp} usage={message.usage} />
        )}
      </div>
    </div>
  )
}

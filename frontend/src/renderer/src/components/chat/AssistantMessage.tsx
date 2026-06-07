import { useMemo, memo } from 'react'
import type { Message } from '../../stores/chat'
import { useStore } from '../../stores'
import ThinkingBlock from './ThinkingBlock'
import SkillBlock from './SkillBlock'
import ShellBlock from './ShellBlock'
import ToolGroupBlock from './ToolGroupBlock'
import TextBlock from './TextBlock'
import FileBlock from './FileBlock'
import SubagentCard from './SubagentCard'
import VisionProcessingBlock from './VisionProcessingBlock'
import MessageFooterActions from './MessageFooterActions'
import TypingIndicator from '../shared/TypingIndicator'
import styles from './AssistantMessage.module.css'

const AssistantMessage = memo(function AssistantMessage({
  message,
  sessionId,
  isStreaming = false,
  isStreamingActive = false,
}: {
  message: Message
  sessionId: string | null
  isStreaming?: boolean
  isStreamingActive?: boolean
}) {
  const openLightbox = useStore(s => s.openLightbox)
  const agent = useStore(s => sessionId ? s.getSessionAgent(sessionId) : undefined)

  const avatarContent = useMemo(() => {
    if (agent?.avatar) {
      return <img src={agent.avatar} alt={agent.name} className={styles.avatarImg} />
    }
    return <span className={styles.avatarText}>L</span>
  }, [agent?.avatar, agent?.name])

  const displayName = (agent?.name && agent.name !== 'default') ? agent.name : 'Loom'

  return (
    <div className={styles.message} data-message-id={message.id}>
      <div className={styles.header}>
        <div className={styles.avatar}>
          {avatarContent}
        </div>
        <span className={styles.name}>{displayName}</span>
      </div>

      <div className={styles.content}>
        {message.blocks.map((block, i) => {
          switch (block.type) {
            case 'vision_processing':
              return <VisionProcessingBlock key={i} block={block} />
            case 'thinking':
              return <ThinkingBlock key={i} block={block} />
            case 'skill':
              return <SkillBlock key={i} block={block} />
            case 'shell':
              return <ShellBlock key={i} block={block} />
            case 'tool_group':
              return <ToolGroupBlock key={i} block={block} />
            case 'text':
              return <TextBlock key={i} block={block} />
            case 'image': {
              const src = (block.thumbnail as string) || (block.path as string)
              return (
                <div key={i} className={styles.imageBlock}>
                  <img
                    src={src}
                    alt={(block.name as string) || 'image'}
                    className={styles.imageBlockImg}
                    onClick={() => {
                      console.log('[lightbox] assistant img click, src len=', src?.length)
                      if (src) openLightbox(src)
                    }}
                  />
                </div>
              )
            }
            case 'file':
              return <FileBlock key={i} block={block} />
            case 'subagent':
              return <SubagentCard key={i} block={block} />
            default:
              return null
          }
        })}
        {message.blocks.length === 0 && isStreamingActive && (
          <div className={styles.thinkingHint}>
            <span>思考中</span>
            <TypingIndicator />
          </div>
        )}
        {message.blocks.length > 0 && isStreamingActive && (
          <div className={styles.streamingHint}>
            <TypingIndicator />
          </div>
        )}
        {message.blocks.length === 0 && !isStreaming && (
          <div className={styles.thinkingHint}>
            <span>已停止生成</span>
          </div>
        )}
        {message.blocks.length > 0 && (
          <MessageFooterActions messageId={message.id} role="assistant" timestamp={message.timestamp} usage={message.usage} blocks={message.blocks} />
        )}
      </div>
    </div>
  )
}, (prev, next) => {
  return prev.message.id === next.message.id &&
    prev.message.blocks === next.message.blocks &&
    prev.sessionId === next.sessionId &&
    prev.isStreaming === next.isStreaming &&
    prev.isStreamingActive === next.isStreamingActive
})

export default AssistantMessage

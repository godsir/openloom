import { useMemo, memo } from 'react'
import { useLocale } from '../../i18n'
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
import ProcessOutputBlock from './ProcessOutputBlock'
import MessageFooterActions from './MessageFooterActions'
import ContinueButton from './ContinueButton'
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
  const { t } = useLocale()

  const avatarContent = useMemo(() => {
    if (agent?.avatar) {
      return <img src={agent.avatar} alt={agent.name} className={styles.avatarImg} />
    }
    return <span className={styles.avatarText}>L</span>
  }, [agent?.avatar, agent?.name])

  const displayName = (agent?.name && agent.name !== 'default') ? agent.name : 'Loom'

  const isTruncated = message.stop_reason === 'budget_exhausted' || message.stop_reason === 'max_iterations'

  return (
    <div className={`${styles.message} ${isTruncated ? styles.truncated : ''}`} data-message-id={message.id}>
      <div className={styles.header}>
        <div className={styles.avatar}>
          {avatarContent}
        </div>
        <span className={styles.name}>{displayName}</span>
      </div>

      <div className={styles.content}>
        {(() => {
          // Derive a stable key per block so stateful blocks (Skill/Shell/
          // Thinking — each holds local expanded/scroll state) are NOT remounted
          // when block order/count shifts during streaming. Streaming shell/skill
          // blocks carry a tool `id`; otherwise fall back to a type-derived key.
          // A per-type occurrence counter disambiguates blocks that share an
          // identity (e.g. multiple text blocks in hydrated history) so React
          // keys stay unique.
          const seen = new Map<string, number>()
          const keyFor = (block: typeof message.blocks[number], i: number): string => {
            let base: string
            switch (block.type) {
              case 'shell':
                base = `shell:${(block.id as string) ?? i}`
                break
              case 'skill':
                base = `skill:${(block.id as string) ?? i}`
                break
              case 'thinking':
                base = 'thinking'
                break
              case 'text':
                base = 'text'
                break
              case 'vision_processing':
                base = `vision:${i}`
                break
              case 'process_output':
                base = `proc:${(block.pid as string) ?? i}`
                break
              case 'image':
                base = `image:${i}`
                break
              default:
                base = `${block.type}:${i}`
            }
            const n = seen.get(base) ?? 0
            seen.set(base, n + 1)
            return n === 0 ? base : `${base}#${n}`
          }
          return message.blocks.map((block, i) => {
            const key = keyFor(block, i)
            switch (block.type) {
              case 'vision_processing':
                return <VisionProcessingBlock key={key} block={block} />
              case 'process_output':
                return <ProcessOutputBlock key={key} block={block} />
              case 'thinking':
                return <ThinkingBlock key={key} block={block} />
              case 'skill':
                return <SkillBlock key={key} block={block} />
              case 'shell':
                return <ShellBlock key={key} block={block} />
              case 'tool_group':
                return <ToolGroupBlock key={key} block={block} />
              case 'text':
                return <TextBlock key={key} block={block} />
              case 'image': {
                const src = (block.thumbnail as string) || (block.path as string)
                return (
                  <div key={key} className={styles.imageBlock}>
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
                return <FileBlock key={key} block={block} />
              case 'subagent':
                return <SubagentCard key={key} block={block} />
              default:
                return null
            }
          })
        })()}
        {message.blocks.length === 0 && isStreamingActive && (
          <div className={styles.thinkingHint}>
            <span>{t('chat.thinking')}</span>
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
            <span>{t('chat.stopped')}</span>
          </div>
        )}
        {message.blocks.length > 0 && (
          <MessageFooterActions messageId={message.id} role="assistant" timestamp={message.timestamp} usage={message.usage} blocks={message.blocks} />
        )}
        {!isStreamingActive && isTruncated && (
          <ContinueButton sessionId={sessionId ?? ''} disabled={isStreamingActive} />
        )}
      </div>
    </div>
  )
}, (prev, next) => {
  return prev.message.id === next.message.id &&
    prev.message.blocks === next.message.blocks &&
    prev.message.stop_reason === next.message.stop_reason &&
    prev.sessionId === next.sessionId &&
    prev.isStreaming === next.isStreaming &&
    prev.isStreamingActive === next.isStreamingActive
})

export default AssistantMessage

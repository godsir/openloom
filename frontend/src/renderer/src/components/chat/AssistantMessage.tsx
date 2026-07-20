import { useMemo, memo } from 'react'
import { useLocale } from '../../i18n'
import type { Message, ContentBlock } from '../../stores/chat'
import { useStore } from '../../stores'
import ThinkingBlock from './ThinkingBlock'
import SkillBlock from './SkillBlock'
import TextBlock from './TextBlock'
import VisionProcessingBlock from './VisionProcessingBlock'
import ProcessOutputBlock from './ProcessOutputBlock'
import WorkBlockPanel, { WORK_BLOCK_TYPES } from './WorkBlockPanel'
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
  const team = useStore(s => sessionId ? s.getSessionTeam(sessionId) : undefined)
  const { t } = useLocale()

  const avatarContent = useMemo(() => {
    if (agent?.avatar) {
      return <img src={agent.avatar} alt={agent.name} className={styles.avatarImg} />
    }
    if (team) {
      return <span className={styles.avatarText}>T</span>
    }
    return <span className={styles.avatarText}>L</span>
  }, [agent?.avatar, agent?.name, team])

  const displayName = team ? team.name : (agent?.name && agent.name !== 'default') ? agent.name : 'Loom'

  const isTruncated = message.stop_reason === 'budget_exhausted' || message.stop_reason === 'max_iterations' || message.stop_reason === 'length'

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
          // Partition blocks into contiguous segments.
          // Consecutive work blocks (shell/tool_group/subagent/team/file)
          // are grouped into a WorkBlockPanel; non-work blocks render individually.
          // This keeps the original block order while folding hundreds of tool
          // calls into one collapsible drawer per contiguous run.
          const segments: Array<
            | { type: 'work'; blocks: ContentBlock[] }
            | { type: 'single'; block: ContentBlock; index: number }
          > = []
          {
            let workBuffer: ContentBlock[] = []
            for (let i = 0; i < message.blocks.length; i++) {
              const b = message.blocks[i]
              if (WORK_BLOCK_TYPES.has(b.type)) {
                workBuffer.push(b)
              } else {
                if (workBuffer.length > 0) {
                  segments.push({ type: 'work', blocks: workBuffer })
                  workBuffer = []
                }
                segments.push({ type: 'single', block: b, index: i })
              }
            }
            if (workBuffer.length > 0) {
              segments.push({ type: 'work', blocks: workBuffer })
            }
          }

          // Stable keys for non-work blocks (work blocks are handled inside the panel)
          const seen = new Map<string, number>()
          const keyFor = (block: ContentBlock, i: number): string => {
            let base: string
            switch (block.type) {
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

          let wpIdx = 0
          return segments.map((seg) => {
            if (seg.type === 'work') {
              const idx = wpIdx++
              return (
                <WorkBlockPanel
                  key={`wp-${idx}`}
                  blocks={seg.blocks}
                  defaultExpanded={isStreamingActive}
                />
              )
            }
            const block = seg.block
            const key = keyFor(block, seg.index)
            switch (block.type) {
              case 'vision_processing':
                return <VisionProcessingBlock key={key} block={block} />
              case 'process_output':
                return <ProcessOutputBlock key={key} block={block} />
              case 'thinking':
                return <ThinkingBlock key={key} block={block} />
              case 'skill':
                return <SkillBlock key={key} block={block} />
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

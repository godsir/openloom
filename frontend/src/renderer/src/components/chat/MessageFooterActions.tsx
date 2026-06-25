import { useStore } from '../../stores'
import { useLocale } from '../../i18n'
import { loomRpc } from '../../services/jsonrpc'
import { sendMessage } from '../../services/sendMessage'
import { IconCopy, IconTrash, IconRefresh, IconRotateCcw } from '../../utils/icons'
import type { ContentBlock } from '../../stores/chat'
import styles from './MessageFooterActions.module.css'

interface Props {
  messageId: string
  role: 'user' | 'assistant'
  timestamp: string
  usage?: { prompt: number; completion: number }
  blocks?: ContentBlock[]
}

function formatTokens(n: number): string {
  if (n >= 1000) return (n / 1000).toFixed(1) + 'k'
  return String(n)
}

export default function MessageFooterActions({ messageId, role, timestamp, usage, blocks = [] }: Props) {
  const { t } = useLocale()
  const deleteMessage = useStore((s) => s.deleteMessage)
  const currentSessionId = useStore((s) => s.currentSessionId)
  const streaming = useStore((s) => currentSessionId ? s.streamingSessionIds.has(currentSessionId) : false)

  // Count tool calls, skill calls, and thinking blocks
  // tool_group = live streaming; shell = hydrated from history
  const toolCount = blocks.filter(b => b.type === 'tool_group').reduce((sum, b) => {
    const tools = (b as any).tools
    return sum + (Array.isArray(tools) ? tools.length : 1)
  }, 0) + blocks.filter(b => b.type === 'shell').length
  const skillCount = blocks.filter(b => b.type === 'skill').length
  const thinkCount = blocks.filter(b => b.type === 'thinking').length
  const totalTools = toolCount + skillCount

  const handleCopy = () => {
    const msgs = useStore.getState().messagesBySession.get(currentSessionId || '')
    const msg = msgs?.find((m) => m.id === messageId)
    if (!msg) return
    const text = msg.blocks
      .filter((b) => b.type === 'text')
      .map((b) => (b.source as string) || (b.html as string) || '')
      .join('\n')
    if (text) {
      navigator.clipboard.writeText(text)
      useStore.getState().showIslandTransient(t('island.copied'))
    }
  }

  const handleDelete = async () => {
    if (!currentSessionId) return
    const msgs = useStore.getState().messagesBySession.get(currentSessionId)
    const index = msgs?.findIndex(m => m.id === messageId) ?? -1
    if (index >= 0) {
      loomRpc('session.delete_message', { session_id: currentSessionId, index }).catch(() => {})
    }
    deleteMessage(currentSessionId, messageId)
  }

  const handleResend = async () => {
    if (!currentSessionId || streaming) return
    const msgs = useStore.getState().messagesBySession.get(currentSessionId)
    const msg = msgs?.find((m) => m.id === messageId)
    if (!msg || msg.role !== 'user') return

    const textBlock = msg.blocks.find((b) => b.type === 'text')
    const content = (textBlock?.source as string) || ''
    const imageBlocks = msg.blocks.filter((b) => b.type === 'image')
    const fileBlocks = msg.blocks.filter((b) => b.type === 'file')

    const attachedFiles = [
      ...imageBlocks.map((b) => ({
        path: b.path as string,
        name: (b.name as string) || 'image',
        size: 0,
        mimeType: (b.mimeType as string) || 'image/png',
        thumbnail: b.thumbnail as string | undefined,
      })),
      ...fileBlocks.map((b) => ({
        path: b.path as string,
        name: (b.name as string) || 'file',
        size: (b.size as number) || 0,
        mimeType: (b.mimeType as string) || 'application/octet-stream',
      })),
    ]

    await sendMessage({ sessionId: currentSessionId, content, attachedFiles })
  }

  const handleRetry = async () => {
    if (!currentSessionId || streaming) return
    const msgs = useStore.getState().messagesBySession.get(currentSessionId)
    const msgIndex = msgs?.findIndex((m) => m.id === messageId) ?? -1
    if (msgIndex <= 0) return

    const prevMsgs = msgs!.slice(0, msgIndex)
    const prevUserMsg = [...prevMsgs].reverse().find((m) => m.role === 'user')
    if (!prevUserMsg) return

    handleDelete()

    const textBlock = prevUserMsg.blocks.find((b) => b.type === 'text')
    const content = (textBlock?.source as string) || ''
    const imageBlocks = prevUserMsg.blocks.filter((b) => b.type === 'image')
    const fileBlocks = prevUserMsg.blocks.filter((b) => b.type === 'file')

    const attachedFiles = [
      ...imageBlocks.map((b) => ({
        path: b.path as string,
        name: (b.name as string) || 'image',
        size: 0,
        mimeType: (b.mimeType as string) || 'image/png',
        thumbnail: b.thumbnail as string | undefined,
      })),
      ...fileBlocks.map((b) => ({
        path: b.path as string,
        name: (b.name as string) || 'file',
        size: (b.size as number) || 0,
        mimeType: (b.mimeType as string) || 'application/octet-stream',
      })),
    ]

    await sendMessage({ sessionId: currentSessionId, content, attachedFiles, skipUserMessage: true })
  }

  const time = new Date(timestamp).toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' })

  return (
    <div className={`${styles.footer} ${role === 'user' ? styles.footerRight : ''}`}>
      <span className={styles.time}>{time}</span>
      {usage && (usage.prompt > 0 || usage.completion > 0) && (
        <span className={styles.tokens} title={t('chat.tokensDetail', { input: usage.prompt, output: usage.completion })}>
          {formatTokens(usage.prompt)}&nbsp;↑&nbsp;{formatTokens(usage.completion)}&nbsp;↓
        </span>
      )}
      {role === 'assistant' && totalTools > 0 && (
        <span className={styles.tokens} title={skillCount > 0 ? t('chat.toolsWithSkillsDetail', { tools: toolCount, skills: skillCount }) : t('chat.toolsDetail', { tools: toolCount })}>
          · {t('chat.toolCount', { n: totalTools })}
        </span>
      )}
      {role === 'assistant' && thinkCount > 0 && (
        <span className={styles.tokens} title={t('chat.thinkCount', { n: thinkCount })}>
          · {t('chat.thinkCount', { n: thinkCount })}
        </span>
      )}
      {role === 'user' && (
        <button onClick={handleResend} className={styles.btn} title={t('chat.resend')} disabled={streaming}>
          <IconRotateCcw size={13} />
        </button>
      )}
      {role === 'assistant' && (
        <button onClick={handleRetry} className={styles.btn} title={t('chat.reReply')} disabled={streaming}>
          <IconRefresh size={13} />
        </button>
      )}
      <button onClick={handleCopy} className={styles.btn} title={t('common.copy')}>
        <IconCopy size={13} />
      </button>
      <button onClick={handleDelete} className={`${styles.btn} ${styles.btnDanger}`} title={t('common.delete')}>
        <IconTrash size={13} />
      </button>
    </div>
  )
}

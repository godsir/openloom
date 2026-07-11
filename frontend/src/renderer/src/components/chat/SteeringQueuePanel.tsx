import { useCallback } from 'react'
import { useStore } from '../../stores'
import { useLocale } from '../../i18n'
import { loomRpc } from '../../services/jsonrpc'
import { streamBufferManager } from '../../services/stream-buffer'
import { IconX } from '../../utils/icons'
import styles from './SteeringQueuePanel.module.css'

interface Props {
  sessionId: string
}

const EMPTY_ITEMS: any[] = []

export default function SteeringQueuePanel({ sessionId }: Props) {
  const { t } = useLocale()
  const items = useStore(s => s.steeringQueueItems[sessionId] ?? EMPTY_ITEMS)
  const streamingActive = useStore(s => s.streamingSessionIds.has(sessionId))

  const handleForceSend = useCallback(async (itemId: string, text: string) => {
    if (!sessionId) return
    // Stop current streaming
    try { await loomRpc('chat.stop', { session_id: sessionId }) } catch { /* ignore */ }
    useStore.getState().removeStreamingSession(sessionId)
    streamBufferManager.clear(sessionId)
    // Remove this item from queue
    useStore.getState().removeSteeringItems(sessionId, [itemId])
    // Send as a brand-new normal message
    const { sendMessage } = await import('../../services/sendMessage')
    useStore.getState().ensureSession(sessionId)
    await sendMessage({ sessionId, content: text })
  }, [sessionId])

  const handleRemoveOne = useCallback((itemId: string) => {
    useStore.getState().removeSteeringItems(sessionId, [itemId])
  }, [sessionId])

  const handleClearAll = useCallback(() => {
    useStore.getState().clearSteeringItems(sessionId)
  }, [sessionId])

  if (items.length === 0) return null

  return (
    <div className={styles.panel}>
      <div className={styles.header}>
        <span>{t('chat.steeringQueueTitle')} <span className={styles.count}>({items.length})</span></span>
        <button className={styles.clearBtn} onClick={handleClearAll}>
          <IconX size={13} />
        </button>
      </div>
      <div className={styles.list}>
        {items.map((item, i) => (
          <div key={item.id} className={styles.item}>
            <span className={styles.index}>{i + 1}.</span>
            <span className={styles.text}>{item.text}</span>
            <div className={styles.actions}>
              {streamingActive && (
                <button
                  className={styles.forceBtn}
                  onClick={() => handleForceSend(item.id, item.text)}
                >
                  {t('chat.steeringForceSend')}
                </button>
              )}
              <button className={styles.removeBtn} onClick={() => handleRemoveOne(item.id)}>
                <IconX size={10} />
              </button>
            </div>
          </div>
        ))}
      </div>
    </div>
  )
}

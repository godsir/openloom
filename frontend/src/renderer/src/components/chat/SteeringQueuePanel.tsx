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
    // Mark current generation as cancelled so the stale StreamEnd from the
    // killed turn is absorbed instead of terminating the replacement turn.
    streamBufferManager.markCancelled(sessionId)
    try { await loomRpc('chat.stop', { session_id: sessionId }) } catch { /* ignore */ }
    // Collect remaining items + this one, clear queue, send combined
    const items = useStore.getState().steeringQueueItems[sessionId] || []
    const remaining = items.filter(it => it.id !== itemId).map(it => it.text)
    useStore.getState().clearSteeringItems(sessionId)
    useStore.getState().removeStreamingSession(sessionId)
    streamBufferManager.clear(sessionId)
    // Combine into one user message — avoids dual-send race with drainSteeringQueue
    const allTexts = [...remaining, text]
    const combined = allTexts.join('\n')
    const { sendMessage } = await import('../../services/sendMessage')
    await sendMessage({ sessionId, content: combined })
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

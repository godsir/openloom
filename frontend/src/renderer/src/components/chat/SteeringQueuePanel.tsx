import { useState, useCallback } from 'react'
import { useStore } from '../../stores'
import { streamBufferManager } from '../../services/stream-buffer'
import { loomRpc } from '../../services/jsonrpc'
import { sendMessage } from '../../services/sendMessage'
import {
  IconSend,
  IconTrash,
  IconEdit,
  IconX,
  IconGripVertical,
} from '../../utils/icons'
import { useLocale } from '../../i18n'
import styles from './SteeringQueuePanel.module.css'

const EMPTY_STEERING: never[] = []

interface Props {
  sessionId: string
}

export default function SteeringQueuePanel({ sessionId }: Props) {
  const { t } = useLocale()
  const items = useStore((s) => s.steeringQueueItems[sessionId] ?? EMPTY_STEERING)
  const streamingActive = useStore((s) => s.streamingSessionIds.has(sessionId))
  const panelOpen = useStore((s) => s.steeringPanelOpen)

  const [editingId, setEditingId] = useState<string | null>(null)
  const [editText, setEditText] = useState('')
  const [dragIndex, setDragIndex] = useState<number | null>(null)
  const [dragOverIndex, setDragOverIndex] = useState<number | null>(null)

  const startEdit = (id: string, text: string) => {
    setEditingId(id)
    setEditText(text)
  }

  const cancelEdit = () => {
    setEditingId(null)
    setEditText('')
  }

  const saveEdit = (id: string) => {
    if (editText.trim()) {
      useStore.getState().updateSteeringItem(sessionId, id, editText.trim())
    }
    cancelEdit()
  }

  const handleDragStart = (e: React.DragEvent, index: number) => {
    setDragIndex(index)
    e.dataTransfer.effectAllowed = 'move'
  }

  const handleDragOver = (e: React.DragEvent, index: number) => {
    e.preventDefault()
    e.dataTransfer.dropEffect = 'move'
    setDragOverIndex(index)
  }

  const handleDragLeave = () => {
    setDragOverIndex(null)
  }

  const handleDrop = (e: React.DragEvent, toIndex: number) => {
    e.preventDefault()
    if (dragIndex !== null && dragIndex !== toIndex) {
      useStore.getState().reorderSteeringItems(sessionId, dragIndex, toIndex)
    }
    setDragIndex(null)
    setDragOverIndex(null)
  }

  const handleDragEnd = () => {
    setDragIndex(null)
    setDragOverIndex(null)
  }

  const handleSendOne = useCallback(async (id: string, text: string) => {
    if (streamingActive) {
      streamBufferManager.markCancelled(sessionId)
      try {
        await loomRpc('chat.stop', { session_id: sessionId })
      } catch {}
    }

    try {
      await loomRpc('chat.steer_clear', { session_id: sessionId })
    } catch {}

    if (streamingActive) {
      useStore.getState().removeStreamingSession(sessionId)
      streamBufferManager.clear(sessionId)
      // clear() removes the old marker along with the buffer. Restore a
      // generation-0 marker so a late stream_end from the stopped turn cannot
      // terminate the replacement turn started below.
      streamBufferManager.markCancelled(sessionId)
    }

    useStore.getState().removeSteeringItems(sessionId, [id])
    try {
      await sendMessage({ sessionId, content: text })
    } catch (error) {
      useStore.getState().addSteeringItem(sessionId, { id, text })
      throw error
    }
  }, [sessionId, streamingActive])

  const handleSendAll = useCallback(async () => {
    if (items.length === 0) return
    const firstItem = items[0]

    if (streamingActive) {
      streamBufferManager.markCancelled(sessionId)
      try {
        await loomRpc('chat.stop', { session_id: sessionId })
      } catch {}
    }

    try {
      await loomRpc('chat.steer_clear', { session_id: sessionId })
    } catch {}

    useStore.getState().removeSteeringItems(sessionId, [firstItem.id])

    if (streamingActive) {
      useStore.getState().removeStreamingSession(sessionId)
      streamBufferManager.clear(sessionId)
      streamBufferManager.markCancelled(sessionId)
    }

    try {
      await sendMessage({ sessionId, content: firstItem.text })
    } catch (error) {
      useStore.getState().addSteeringItem(sessionId, firstItem)
      throw error
    }
  }, [sessionId, items, streamingActive])

  const handleRemoveOne = useCallback((id: string) => {
    useStore.getState().removeSteeringItems(sessionId, [id])
  }, [sessionId])

  const handleClearAll = useCallback(() => {
    useStore.getState().clearSteeringItems(sessionId)
    useStore.setState({ steeringPanelOpen: false })
  }, [sessionId])

  if (!panelOpen && items.length === 0) return null

  return (
    <div className={styles.panel}>
      <div className={styles.header}>
        <div className={styles.headerLeft}>
          <span className={styles.title}>{t('chat.steeringQueueTitle')}</span>
          <span className={styles.badge}>{items.length}</span>
        </div>
        <div className={styles.headerActions}>
          {items.length > 1 && (
            <button className={styles.sendAllBtn} onClick={handleSendAll}>
              <IconSend size={14} />
              {t('chat.steeringSendAll')}
            </button>
          )}
          <button className={styles.clearBtn} onClick={handleClearAll} title={t('chat.steeringClearAll')}>
            <IconTrash size={14} />
          </button>
        </div>
      </div>

      <div className={styles.list}>
        {items.length === 0 ? (
          <div className={styles.empty}>{t('chat.steeringQueueEmpty')}</div>
        ) : (
          items.map((item, i) => {
            const isEditing = editingId === item.id
            const isDragging = dragIndex === i
            const isDragOver = dragOverIndex === i

            return (
              <div
                key={item.id}
                className={`${styles.item} ${isDragging ? styles.dragging : ''} ${isDragOver ? styles.dragOver : ''}`}
                draggable={!isEditing}
                onDragStart={(e) => handleDragStart(e, i)}
                onDragOver={(e) => handleDragOver(e, i)}
                onDragLeave={handleDragLeave}
                onDrop={(e) => handleDrop(e, i)}
                onDragEnd={handleDragEnd}
              >
                <div className={styles.itemHeader}>
                  {!isEditing && (
                    <button className={styles.dragHandle}>
                      <IconGripVertical size={14} />
                    </button>
                  )}
                  <span className={styles.index}>{i + 1}</span>
                  {isEditing ? (
                    <div className={styles.editContainer}>
                      <input
                        className={styles.editInput}
                        value={editText}
                        onChange={(e) => setEditText(e.target.value)}
                        onKeyDown={(e) => {
                          if (e.key === 'Enter') saveEdit(item.id)
                          if (e.key === 'Escape') cancelEdit()
                        }}
                        autoFocus
                        onClick={(e) => e.stopPropagation()}
                      />
                      <button className={styles.editSaveBtn} onClick={() => saveEdit(item.id)}>
                        <IconEdit size={12} />
                      </button>
                      <button className={styles.editCancelBtn} onClick={cancelEdit}>
                        <IconX size={12} />
                      </button>
                    </div>
                  ) : (
                    <span className={styles.previewText}>
                      {item.text}
                    </span>
                  )}
                </div>

                <div className={styles.actions}>
                  <button
                    className={styles.sendBtn}
                    onClick={() => handleSendOne(item.id, item.text)}
                    title={t('chat.steeringSendOne')}
                  >
                    <IconSend size={14} />
                  </button>
                  {!isEditing && (
                    <button
                      className={styles.editBtn}
                      onClick={() => startEdit(item.id, item.text)}
                      title={t('common.edit')}
                    >
                      <IconEdit size={14} />
                    </button>
                  )}
                  <button
                    className={styles.removeBtn}
                    onClick={() => handleRemoveOne(item.id)}
                    title={t('common.delete')}
                  >
                    <IconX size={14} />
                  </button>
                </div>
              </div>
            )
          })
        )}
      </div>

      {items.length > 0 && (
        <div className={styles.footer}>
          <div className={styles.hint}>
            {streamingActive ? t('chat.steeringStreamingHint') : t('chat.steeringQueuedHint')}
          </div>
        </div>
      )}
    </div>
  )
}

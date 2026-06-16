import React, { useState, useCallback } from 'react'
import { useWriteStore } from '../../stores/write'
import { useLocale } from '../../i18n'
import { IconSparkles, IconSend } from '../../utils/icons'
import WriteChatPanel from './WriteChatPanel'
import styles from './WriteAssistantPanel.module.css'

interface WriteAssistantPanelProps {
  quickSuggestions: { key: string; text: string }[]
  onSend: (text: string) => Promise<void>
  onNewChat: () => void
  onStaleSession: (deadSessionId: string) => void
}

export const WriteAssistantPanel: React.FC<WriteAssistantPanelProps> = ({
  quickSuggestions,
  onSend,
  onNewChat,
  onStaleSession,
}) => {
  const { t } = useLocale()
  const activeFilePath = useWriteStore(s => s.activeFilePath)
  const fileThreads = useWriteStore(s => s.fileThreads)

  const [assistantText, setAssistantText] = useState('')

  const sessionId = activeFilePath ? (fileThreads[activeFilePath] || null) : null
  const activeFileName = activeFilePath ? activeFilePath.split('/').pop() || null : null

  const handleSend = useCallback(async (text?: string) => {
    const msg = (text || assistantText).trim()
    if (!msg) return
    try {
      await onSend(msg)
      if (!text) setAssistantText('') // only clear for manual input, not suggestions
    } catch {
      // onSend threw — keep text for manual input
      if (!text) setAssistantText(msg)
    }
  }, [assistantText, onSend])

  const handleKeyDown = useCallback((e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault()
      handleSend()
    }
  }, [handleSend])

  return (
    <div className={styles.panel}>
      <div className={styles.header}>
        <IconSparkles size={13} />
        <span>{t('write.aiWritingAssistant')}</span>
      </div>

      <WriteChatPanel
        sessionId={sessionId}
        activeFileName={activeFileName}
        quickSuggestions={quickSuggestions}
        onSuggestionClick={(text: string) => handleSend(text)}
        onNewChat={onNewChat}
        onStaleSession={onStaleSession}
      />

      <div className={styles.footer}>
        <div className={styles.inputRow}>
          <input
            className={styles.input}
            value={assistantText}
            onChange={e => setAssistantText(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder={t('write.inputInstruction')}
          />
          <button
            className={styles.sendBtn}
            onClick={() => handleSend()}
            disabled={!assistantText.trim()}
            title={t('chat.send')}
          >
            <IconSend size={13} />
          </button>
        </div>
      </div>
    </div>
  )
}

export default WriteAssistantPanel

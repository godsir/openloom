import React, { useState, useCallback } from 'react'
import { getWriteThreadKey, useWriteStore } from '../../stores/write'
import { useStore } from '../../stores'
import { loomRpc } from '../../services/jsonrpc'
import { streamBufferManager } from '../../services/stream-buffer'
import { resolveAgentPreset } from '../../write/agent-presets'
import { useLocale } from '../../i18n'
import { IconSparkles, IconSend, IconWorkflow, IconPenLine, IconScanSearch, IconClipboardCheck, IconStopCircle, IconQuote, IconX } from '../../utils/icons'
import WriteChatPanel from './WriteChatPanel'
import styles from './WriteAssistantPanel.module.css'

const PERSONA_IDS = ['plot-coordinator', 'line-editor', 'foreshadowing', 'continuity'] as const

const PERSONA_ICON: Record<string, React.ComponentType<{ size?: number }>> = {
  'plot-coordinator': IconWorkflow,
  'line-editor': IconPenLine,
  foreshadowing: IconScanSearch,
  continuity: IconClipboardCheck,
}

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
  const workspaceRoot = useWriteStore(s => s.workspaceRoot)
  const agentPresetId = useWriteStore(s => s.agentPresetId)
  const setAgentPresetId = useWriteStore(s => s.setAgentPresetId)
  const quotedSelections = useWriteStore(s => s.quotedSelections)
  const removeQuotedSelection = useWriteStore(s => s.removeQuotedSelection)

  const [assistantText, setAssistantText] = useState('')

  const sessionId = activeFilePath && workspaceRoot
    ? (fileThreads[getWriteThreadKey(workspaceRoot, activeFilePath)] || null)
    : null
  const activeFileName = activeFilePath ? activeFilePath.split('/').pop() || null : null

  // 当前文件会话是否正在流式生成：是则发送键切换为"停止"（A22）
  const streamingIds = useStore(s => s.streamingSessionIds)
  const isStreaming = sessionId ? streamingIds.has(sessionId) : false

  const handleStop = useCallback(async () => {
    if (!sessionId) return
    // 与聊天模式一致：先标记取消以吸收被 kill turn 的迟到 StreamEnd，再下发停止
    streamBufferManager.markCancelled(sessionId)
    try {
      await loomRpc('chat.stop', { session_id: sessionId })
    } catch {
      /* ignore */
    }
  }, [sessionId])

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
  }, [assistantText, onSend, activeFileName])

  const handleKeyDown = useCallback((e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault()
      handleSend()
    }
  }, [handleSend])

  return (
    <div className={styles.panel}>
      {/* Persona switcher */}
      <div className={styles.personaRow}>
        <button
          className={!agentPresetId ? styles.personaBtnActive : styles.personaBtn}
          onClick={() => setAgentPresetId(null)}
          title="默认风格"
        >
          <IconSparkles size={12} />
          <span>默认</span>
        </button>
        {PERSONA_IDS.map((id) => {
          const preset = resolveAgentPreset(id)
          const PIcon = PERSONA_ICON[id]
          return (
            <button
              key={id}
              className={agentPresetId === id ? styles.personaBtnActive : styles.personaBtn}
              onClick={() => setAgentPresetId(id)}
              title={preset?.persona}
            >
              <PIcon size={12} />
              <span>{preset?.name}</span>
            </button>
          )
        })}
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
        {quotedSelections.length > 0 && (
          <div className={styles.quoteTray}>
            <div className={styles.quoteTrayHeader}>
              <IconQuote size={12} />
              <span>{t('write.quotedCount', { count: quotedSelections.length })}</span>
            </div>
            <div className={styles.quoteList}>
              {quotedSelections.map((quote) => (
                <div className={styles.quoteTip} key={quote.id}>
                  <div className={styles.quoteText}>
                    <span className={styles.quoteSource}>
                      {quote.filePath.split('/').pop()} · L{quote.lineFrom + 1}–{quote.lineTo + 1}
                    </span>
                    <span className={styles.quotePreview}>{quote.text}</span>
                  </div>
                  <button
                    className={styles.quoteRemove}
                    onClick={() => removeQuotedSelection(quote.id)}
                    aria-label={t('common.removeQuote')}
                    title={t('common.removeQuote')}
                  >
                    <IconX size={12} />
                  </button>
                </div>
              ))}
            </div>
          </div>
        )}
        <div className={styles.inputRow}>
          <input
            className={styles.input}
            value={assistantText}
            onChange={e => setAssistantText(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder={t('write.inputInstruction')}
          />
          <button
            className={isStreaming ? styles.stopBtn : styles.sendBtn}
            onClick={() => (isStreaming ? handleStop() : handleSend())}
            disabled={!isStreaming && !assistantText.trim()}
            title={isStreaming ? t('chat.stop') : t('chat.send')}
            aria-label={isStreaming ? t('chat.stop') : t('chat.send')}
          >
            {isStreaming ? <IconStopCircle size={13} /> : <IconSend size={13} />}
          </button>
        </div>
      </div>
    </div>
  )
}

export default WriteAssistantPanel

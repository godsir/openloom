import React, { useState, useCallback, useEffect, useMemo } from 'react'
import { getWriteThreadKey, useWriteStore } from '../../stores/write'
import { useStore } from '../../stores'
import { loomRpc } from '../../services/jsonrpc'
import { streamBufferManager } from '../../services/stream-buffer'
import { useLocale } from '../../i18n'
import { IconSend, IconStopCircle, IconQuote, IconX } from '../../utils/icons'
import Select from '../shared/Select'
import type { ModelListItem } from '../../types/bindings'
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
  const workspaceRoot = useWriteStore(s => s.workspaceRoot)
  const writingAgentName = useWriteStore(s => s.writingAgentName)
  const setWritingAgentName = useWriteStore(s => s.setWritingAgentName)
  const writingModelName = useWriteStore(s => s.writingModelName)
  const setWritingModelName = useWriteStore(s => s.setWritingModelName)
  const agents = useStore(s => s.agents)
  const models = useStore(s => s.models)
  const quotedSelections = useWriteStore(s => s.quotedSelections)
  const removeQuotedSelection = useWriteStore(s => s.removeQuotedSelection)

  const [assistantText, setAssistantText] = useState('')
  const [isSubmitting, setIsSubmitting] = useState(false)

  const sessionId = activeFilePath && workspaceRoot
    ? (fileThreads[getWriteThreadKey(workspaceRoot, activeFilePath)] || null)
    : null
  const activeFileName = activeFilePath ? activeFilePath.split('/').pop() || null : null

  // 当前文件会话是否正在流式生成：是则发送键切换为"停止"（A22）
  const streamingIds = useStore(s => s.streamingSessionIds)
  const isStreaming = sessionId ? streamingIds.has(sessionId) : false
  const agentOptions = useMemo(() => [
    { value: '', label: t('write.settingsAgentDefault') },
    ...agents
      .filter(agent => agent.name && agent.name !== 'default' && !agent.name.startsWith('__team_'))
      .map(agent => ({ value: agent.name, label: agent.name, avatar: agent.avatar })),
  ], [agents, t])
  const modelOptions = useMemo(() => [
    { value: '', label: t('write.modelFollowCurrent') },
    ...models.map(model => ({
      value: model.name,
      label: model.name,
      group: model.backend_label || model.backend,
    })),
  ], [models, t])

  useEffect(() => {
    if (models.length > 0) return
    loomRpc<{ models: ModelListItem[] }>('model.list')
      .then(result => useStore.getState().setModels(result.models || []))
      .catch(() => {})
  }, [models.length])

  const handleAgentChange = useCallback(async (name: string) => {
    const nextName = name || null
    setWritingAgentName(nextName)
    if (!sessionId) return
    const bindingName = nextName || 'default'
    try {
      await loomRpc('session.bind_agent', {
        session_id: sessionId,
        agent_config_name: bindingName,
      })
      useStore.getState().setSessionAgentBinding(sessionId, bindingName)
    } catch {
      // The preference is persisted and will be applied again on the next send.
    }
  }, [sessionId, setWritingAgentName])

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
    if (!msg || isSubmitting) return
    const isManualInput = !text
    if (isManualInput) setAssistantText('')
    setIsSubmitting(true)
    try {
      await onSend(msg)
    } catch {
      // onSend threw — keep text for manual input
      if (isManualInput) setAssistantText(current => current || msg)
    } finally {
      setIsSubmitting(false)
    }
  }, [assistantText, isSubmitting, onSend])

  const handleKeyDown = useCallback((e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault()
      handleSend()
    }
  }, [handleSend])

  return (
    <div className={styles.panel}>
      {/* Persona switcher */}
      <div className={styles.personaRow}>
        <div className={styles.selectorGroup}>
          <span className={styles.agentLabel}>{t('write.settingsAgent')}</span>
          <Select
            value={writingAgentName || ''}
            options={agentOptions}
            onChange={handleAgentChange}
            variant="pill"
            className={styles.selector}
            menuWidth={220}
            ariaLabel={t('write.settingsAgent')}
          />
        </div>
        <div className={styles.selectorGroup}>
          <span className={styles.agentLabel}>{t('write.model')}</span>
          <Select
            value={writingModelName || ''}
            options={modelOptions}
            onChange={value => setWritingModelName(value || null)}
            variant="pill"
            className={styles.selector}
            menuWidth={240}
            ariaLabel={t('write.model')}
            emptyText={t('model.empty')}
          />
        </div>
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
          <textarea
            className={styles.input}
            value={assistantText}
            onChange={e => setAssistantText(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder={t('write.inputInstruction')}
            rows={2}
          />
          <button
            className={isStreaming ? styles.stopBtn : styles.sendBtn}
            onClick={() => (isStreaming ? handleStop() : handleSend())}
            disabled={!isStreaming && (!assistantText.trim() || isSubmitting)}
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

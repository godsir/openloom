import React, { useState, useCallback } from 'react'
import { useWriteStore } from '../../stores/write'
import { composeWritePrompt } from '../../write/quoted-selection'
import { resolveAgentPreset } from '../../write/agent-presets'
import { useLocale } from '../../i18n'
import { IconSparkles, IconSend, IconWorkflow, IconPenLine, IconScanSearch, IconClipboardCheck } from '../../utils/icons'
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
  const agentPresetId = useWriteStore(s => s.agentPresetId)
  const setAgentPresetId = useWriteStore(s => s.setAgentPresetId)

  const [assistantText, setAssistantText] = useState('')

  const sessionId = activeFilePath ? (fileThreads[activeFilePath] || null) : null
  const activeFileName = activeFilePath ? activeFilePath.split('/').pop() || null : null

  const handleSend = useCallback(async (text?: string) => {
    const msg = (text || assistantText).trim()
    if (!msg) return
    const persona = resolveAgentPreset(useWriteStore.getState().agentPresetId)
    const quotedSelections = useWriteStore.getState().quotedSelections
    const fullPrompt = composeWritePrompt(
      msg,
      activeFileName || '',
      '', // fileContent will be added by the parent
      quotedSelections.length > 0 ? quotedSelections : undefined,
      undefined, // retrieval context (Phase 3 RAG)
      persona?.persona,
    )
    try {
      await onSend(fullPrompt)
      if (!text) setAssistantText('') // only clear for manual input, not suggestions
    } catch {
      // onSend threw — keep text for manual input
      if (!text) setAssistantText(msg)
    }
    useWriteStore.getState().clearQuotedSelections()
  }, [assistantText, onSend, activeFileName])

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

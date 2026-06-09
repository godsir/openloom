import React, { useState, useRef, useEffect, useCallback } from 'react'
import { createPortal } from 'react-dom'
import { useStore } from '../../stores'
import { useLocale } from '../../i18n'
import styles from './InlineInput.module.css'

export const InlineInput: React.FC = () => {
  const { t } = useLocale()
  const {
    inlineInputOpen,
    inlineInputText,
    inlineInputRect,
    inlineInputFilePath,
    inlineInputStartLine,
    inlineInputEndLine,
    setInlineInputText,
    closeInlineInput,
    addQuotedSelection,
  } = useStore()

  const [instructionText, setInstructionText] = useState('')
  const inputRef = useRef<HTMLTextAreaElement>(null)

  useEffect(() => {
    if (inlineInputOpen && inputRef.current) {
      setInstructionText('')
      inputRef.current.focus()
    }
  }, [inlineInputOpen])

  const handleConfirm = useCallback(() => {
    const text = instructionText.trim()
    if (!text) return

    // Get selected text from the current selection
    const sel = window.getSelection()
    const quotedText = sel && !sel.isCollapsed ? sel.toString() : ''

    addQuotedSelection({
      text: quotedText,
      filePath: inlineInputFilePath,
      startLine: inlineInputStartLine,
      endLine: inlineInputEndLine,
      charCount: quotedText.length,
    })

    // TODO: Send the instruction to the agent via sendMessage
    closeInlineInput()
  }, [instructionText, inlineInputFilePath, inlineInputStartLine, inlineInputEndLine, addQuotedSelection, closeInlineInput])

  const handleCancel = useCallback(() => {
    closeInlineInput()
  }, [closeInlineInput])

  const handleKeyDown = useCallback((e: React.KeyboardEvent) => {
    if (e.key === 'Escape') {
      e.preventDefault()
      handleCancel()
    } else if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault()
      handleConfirm()
    }
    // Shift+Enter: insert newline (default browser behavior)
  }, [handleConfirm, handleCancel])

  if (!inlineInputOpen || !inlineInputRect) return null

  return createPortal(
    <div className={styles.overlay}>
      <div className={styles.backdrop} onClick={handleCancel} />
      <div className={styles.container} style={{ top: inlineInputRect.top, left: inlineInputRect.left }}>
        <textarea
          ref={inputRef}
          value={instructionText}
          onChange={e => setInstructionText(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder={t('input.explainCode')}
          className={styles.textarea}
        />
        <div className={styles.actions}>
          <span className={styles.hint}>{t('input.inlineEnterEsc')}</span>
          <button onClick={handleCancel} className={styles.cancelBtn}>{t('common.cancel')}</button>
          <button
            onClick={handleConfirm}
            disabled={!instructionText.trim()}
            className={styles.sendBtn}
          >
{t('chat.send')}
          </button>
        </div>
      </div>
    </div>,
    document.body
  )
}

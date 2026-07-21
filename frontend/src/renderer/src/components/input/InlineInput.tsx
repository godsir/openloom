import React, { useState, useRef, useEffect, useLayoutEffect, useCallback } from 'react'
import { createPortal } from 'react-dom'
import { useStore } from '../../stores'
import { useLocale } from '../../i18n'
import { sendMessage } from '../../services/sendMessage'
import styles from './InlineInput.module.css'

export const InlineInput: React.FC = () => {
  const { t } = useLocale()
  const {
    inlineInputOpen,
    inlineInputRect,
    inlineInputFilePath,
    inlineInputStartLine,
    inlineInputEndLine,
    closeInlineInput,
  } = useStore()

  const [instructionText, setInstructionText] = useState('')
  const [pos, setPos] = useState({ top: 0, left: 0 })
  const inputRef = useRef<HTMLTextAreaElement>(null)
  const containerRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (inlineInputOpen && inputRef.current) {
      setInstructionText('')
      inputRef.current.focus()
    }
  }, [inlineInputOpen])

  // 边界处理：选区靠近视口底部/右侧时弹框会整个掉出可视区（backdrop 还阻止
  // 滚动，用户看不到也点不到）。挂载后量取容器尺寸，超出则向上翻转/向左钳制。
  useLayoutEffect(() => {
    if (!inlineInputOpen || !inlineInputRect) return
    const el = containerRef.current
    const h = el?.offsetHeight || 150
    const w = el?.offsetWidth || 400
    let top = inlineInputRect.top
    let left = inlineInputRect.left
    if (top + h > window.innerHeight - 12) {
      top = Math.max(12, inlineInputRect.top - h - 12)
    }
    if (left + w > window.innerWidth - 12) {
      left = Math.max(12, window.innerWidth - w - 12)
    }
    setPos({ top, left })
  }, [inlineInputOpen, inlineInputRect])

  const handleConfirm = useCallback(async () => {
    const text = instructionText.trim()
    if (!text) return

    // 取当前选中文本作为引用（打开弹层时选区仍在）
    const sel = window.getSelection()
    const quotedText = sel && !sel.isCollapsed ? sel.toString() : ''

    // 把指令真正发给 agent（此前指令被直接丢弃，只加了引用卡片）。
    // 引用内容作为 quoted_selection 随消息一并发送。
    let sid = useStore.getState().currentSessionId
    if (!sid) {
      sid = await useStore.getState().createSession()
      if (sid) await useStore.getState().switchSession(sid)
    }
    if (sid) {
      await sendMessage({
        sessionId: sid,
        content: text,
        quotedSelections: quotedText
          ? [{
              id: crypto.randomUUID(),
              text: quotedText,
              filePath: inlineInputFilePath,
              startLine: inlineInputStartLine,
              endLine: inlineInputEndLine,
              charCount: quotedText.length,
            }]
          : undefined,
      })
    }
    closeInlineInput()
  }, [instructionText, inlineInputFilePath, inlineInputStartLine, inlineInputEndLine, closeInlineInput])

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
      <div ref={containerRef} className={styles.container} style={{ top: pos.top, left: pos.left }}>
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

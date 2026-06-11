import { useState, useEffect, useCallback, useRef } from 'react'
import { useLocale, t as _t } from '../../i18n'
import { eventToKeyString, keyStringToDisplay } from '../../services/keybindings'
import styles from './KeyCaptureModal.module.css'

interface KeyCaptureModalProps {
  /** Human-readable command label (already translated) */
  commandLabel: string
  /** Current keybinding display string (e.g. "Ctrl+N") */
  currentKeys: string
  /** Conflict command label if the new binding conflicts, or null */
  conflictLabel: string | null
  /** Called when user confirms the new binding */
  onConfirm: (keys: string) => void
  /** Called when user cancels or presses Escape */
  onCancel: () => void
  /** Called when user wants to clear (disable) the binding */
  onClear: () => void
}

export function KeyCaptureModal({
  commandLabel,
  currentKeys,
  conflictLabel,
  onConfirm,
  onCancel,
  onClear,
}: KeyCaptureModalProps) {
  const { t } = useLocale()
  const [capturing, setCapturing] = useState(true)
  const [capturedKeys, setCapturedKeys] = useState('')
  const capturedRef = useRef('')

  const handleKeyDown = useCallback(
    (e: KeyboardEvent) => {
      if (!capturing) return

      // Escape cancels
      if (e.key === 'Escape') {
        e.preventDefault()
        onCancel()
        return
      }

      // Backspace/Delete while empty clears the binding
      if ((e.key === 'Backspace' || e.key === 'Delete') && !capturedRef.current) {
        e.preventDefault()
        onClear()
        return
      }

      const keyString = eventToKeyString(e)
      if (!keyString) return // Pure modifier press

      e.preventDefault()
      e.stopPropagation()

      capturedRef.current = keyString
      setCapturedKeys(keyString)
      setCapturing(false)

      // Auto-confirm after short delay if no conflict
      if (!conflictLabel) {
        setTimeout(() => onConfirm(keyString), 400)
      }
    },
    [capturing, conflictLabel, onCancel, onClear, onConfirm],
  )

  useEffect(() => {
    window.addEventListener('keydown', handleKeyDown, true)
    return () => window.removeEventListener('keydown', handleKeyDown, true)
  }, [handleKeyDown])

  const displayKeys = capturedKeys
    ? keyStringToDisplay(capturedKeys)
    : currentKeys

  return (
    <div className={styles.overlay} onClick={onCancel}>
      <div className={styles.modal} onClick={(e) => e.stopPropagation()}>
        <h3 className={styles.title}>
          {_t('keybindings.captureTitle', { command: commandLabel })}
        </h3>
        <p className={styles.subtitle}>{t('keybindings.captureSubtitle')}</p>

        <div
          className={`${styles.keyDisplay} ${
            conflictLabel
              ? styles.keyDisplayConflict
              : capturedKeys
                ? styles.keyDisplayListening
                : ''
          }`}
        >
          {capturedKeys
            ? displayKeys
            : currentKeys || _t('keybindings.notSet')}
        </div>

        {conflictLabel && (
          <p className={styles.conflictMessage}>
            {_t('keybindings.conflictMessage', { command: conflictLabel })}
          </p>
        )}

        <p className={styles.hint}>
          {capturedKeys
            ? conflictLabel
              ? t('keybindings.pressConfirmOverride')
              : t('keybindings.captured')
            : t('keybindings.pressKeys')}
        </p>

        <div className={styles.actions}>
          {!capturedKeys && currentKeys && (
            <button className={`${styles.btn} ${styles.btnDanger}`} onClick={onClear}>
              {t('keybindings.clearBinding')}
            </button>
          )}
          <button className={styles.btn} onClick={onCancel}>
            {t('common.cancel')}
          </button>
          {capturedKeys && conflictLabel && (
            <button
              className={`${styles.btn} ${styles.btnPrimary}`}
              onClick={() => onConfirm(capturedKeys)}
            >
              {_t('keybindings.overrideConfirm', { command: conflictLabel })}
            </button>
          )}
        </div>
      </div>
    </div>
  )
}

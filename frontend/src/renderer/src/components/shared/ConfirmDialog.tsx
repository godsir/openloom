import { useEffect } from 'react'
import { IconAlertCircle, IconCheck } from '../../utils/icons'
import { useLocale } from '../../i18n'
import styles from './ConfirmDialog.module.css'

interface ConfirmDialogProps {
  open: boolean
  title: string
  message: string
  confirmLabel?: string
  cancelLabel?: string
  danger?: boolean
  onConfirm: () => void
  onCancel: () => void
}

export default function ConfirmDialog({
  open,
  title,
  message,
  confirmLabel,
  cancelLabel,
  danger,
  onConfirm,
  onCancel,
}: ConfirmDialogProps) {
  const { t } = useLocale()
  const actualConfirmLabel = confirmLabel ?? t('common.confirm')
  const actualCancelLabel = cancelLabel ?? t('common.cancel')

  useEffect(() => {
    if (!open) return
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onCancel()
    }
    document.addEventListener('keydown', onKey)
    // Auto-cancel after 60s to prevent stuck promise
    const t = setTimeout(() => onCancel(), 60_000)
    return () => {
      document.removeEventListener('keydown', onKey)
      clearTimeout(t)
    }
  }, [open, onCancel])

  if (!open) return null

  return (
    <div className={styles.overlay}>
      <div className={styles.backdrop} onClick={onCancel} />
      <div className={styles.dialog}>
        {/* Header */}
        <div className={`${styles.header} ${danger ? styles.headerDanger : styles.headerNormal}`}>
          <div className={`${styles.headerIcon} ${danger ? styles.headerIconDanger : styles.headerIconNormal}`}>
            <IconAlertCircle size={22} />
          </div>
          <h3 className={styles.title}>{title}</h3>
        </div>

        {/* Body */}
        <div className={styles.body}>
          <p className={styles.message}>{message}</p>
        </div>

        {/* Actions */}
        <div className={styles.actions}>
          <button onClick={onCancel} className={styles.btnCancel}>
            {actualCancelLabel}
          </button>
          <button
            onClick={onConfirm}
            className={`${styles.btnConfirm} ${danger ? styles.btnConfirmDanger : styles.btnConfirmNormal}`}
          >
            <IconCheck size={14} />
            {actualConfirmLabel}
          </button>
        </div>
      </div>
    </div>
  )
}

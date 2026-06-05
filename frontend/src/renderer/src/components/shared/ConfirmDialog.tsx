import { useEffect } from 'react'
import { IconAlertCircle } from '../../utils/icons'
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
  confirmLabel = '确定',
  cancelLabel = '取消',
  danger,
  onConfirm,
  onCancel,
}: ConfirmDialogProps) {
  useEffect(() => {
    if (!open) return
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onCancel()
    }
    document.addEventListener('keydown', onKey)
    return () => document.removeEventListener('keydown', onKey)
  }, [open, onCancel])

  if (!open) return null

  return (
    <div className={styles.overlay}>
      <div className={styles.backdrop} onClick={onCancel} />
      <div className={styles.dialog}>
        <div className={styles.body}>
          <div className={`${styles.iconCircle} ${danger ? styles.iconCircleDanger : styles.iconCircleNormal}`}>
            <IconAlertCircle size={20} />
          </div>
          <div>
            <h3 className={styles.title}>{title}</h3>
            <p className={styles.message}>{message}</p>
          </div>
          <div className={styles.actions}>
            <button onClick={onCancel} className={`${styles.btn} ${styles.btnCancel}`}>
              {cancelLabel}
            </button>
            <button
              onClick={onConfirm}
              className={`${styles.btn} ${danger ? styles.btnDanger : styles.btnConfirm}`}
            >
              {confirmLabel}
            </button>
          </div>
        </div>
      </div>
    </div>
  )
}

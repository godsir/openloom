import { useEffect } from 'react'
import { IconAlertCircle, IconShield, IconCheck, IconX } from '../../utils/icons'
import { useLocale } from '../../i18n'
import styles from './PermissionDialog.module.css'

interface PermissionDialogProps {
  open: boolean
  title: string
  message: string
  toolName: string
  danger?: boolean
  onApprove: () => void
  onApproveAlways: () => void
  onDeny: () => void
}

export default function PermissionDialog({
  open,
  title,
  message,
  toolName,
  danger,
  onApprove,
  onApproveAlways,
  onDeny,
}: PermissionDialogProps) {
  const { t } = useLocale()

  useEffect(() => {
    if (!open) return
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onDeny()
    }
    document.addEventListener('keydown', onKey)
    // Auto-deny after 60s to prevent backend hang if user is afk
    const t = setTimeout(() => onDeny(), 60_000)
    return () => {
      document.removeEventListener('keydown', onKey)
      clearTimeout(t)
    }
  }, [open, onDeny])

  if (!open) return null

  return (
    <div className={styles.overlay}>
      <div className={styles.backdrop} onClick={onDeny} />
      <div className={styles.dialog}>
        {/* Header with icon */}
        <div className={`${styles.header} ${danger ? styles.headerDanger : styles.headerNormal}`}>
          <div className={`${styles.headerIcon} ${danger ? styles.headerIconDanger : styles.headerIconNormal}`}>
            {danger ? <IconShield size={22} /> : <IconAlertCircle size={22} />}
          </div>
          <div className={styles.headerText}>
            <h3 className={styles.title}>{title}</h3>
            <span className={`${styles.riskBadge} ${danger ? styles.riskBadgeHigh : styles.riskBadgeMedium}`}>
              {danger ? t('permissions.riskHigh') : t('permissions.riskMedium')}
            </span>
          </div>
        </div>

        {/* Body with tool info */}
        <div className={styles.body}>
          <p className={styles.message}>{message}</p>
          <div className={styles.toolChip}>
            <span className={styles.toolChipLabel}>{toolName}</span>
          </div>
        </div>

        {/* Actions */}
        <div className={styles.actions}>
          <button onClick={onApproveAlways} className={styles.btnApproveAlways}>
            <IconCheck size={14} />
            <span className={styles.btnLabel}>{t('permissions.approveAlways')}</span>
            <span className={styles.btnHint}>{t('permissions.approveAlwaysHint')}</span>
          </button>
          <button onClick={onApprove} className={styles.btnApprove}>
            <IconCheck size={14} />
            <span className={styles.btnLabel}>{t('permissions.approveOnce')}</span>
            <span className={styles.btnHint}>{t('permissions.approveOnceHint')}</span>
          </button>
          <button onClick={onDeny} className={styles.btnDeny}>
            <IconX size={14} />
            <span className={styles.btnLabel}>{t('permissions.deny')}</span>
          </button>
        </div>
      </div>
    </div>
  )
}

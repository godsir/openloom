import { useEffect } from 'react'
import { IconCalendarClock, IconX } from '../../utils/icons'
import { useLocale, t as _t } from '../../i18n'
import styles from './CronDetectedDialog.module.css'

interface CronDetectedDialogProps {
  open: boolean
  name: string
  prompt: string
  cronExpression: string
  kind: string
  confirmation: string
  onCreate: () => void
  onCancel: () => void
  loading?: boolean
}

function describeCron(expr: string): string {
  const p = expr.trim().split(/\s+/)
  if (p.length !== 7) return expr
  if (p[0] === '0' && p[1] === '*' && p.slice(2).every(f => f === '*')) return _t('cron.everyMinute')
  if (p[0] === '0' && p[1].startsWith('*/') && p.slice(2).every(f => f === '*')) return _t('cron.everyNMinutes', { n: p[1].slice(2) })
  if (p[0] === '0' && p[1] === '0' && p.slice(2).every(f => f === '*')) return _t('cron.hourly')
  if (p[0] === '0' && p[1] === '0' && /^\d+$/.test(p[2]) && p.slice(3).every(f => f === '*')) return _t('sidebar.everyDayAt', { hour: p[2] })
  if (p[0] === '0' && p[1] === '0' && /^\d+$/.test(p[2]) && /^\d+$/.test(p[3]) && p.slice(4).every(f => f === '*')) return _t('cron.everyDayAtTime', { hour: p[2], minute: p[3].padStart(2, '0') })
  return expr
}

function kindLabel(kind: string, t: (k: string) => string): string {
  if (kind === 'daily') return t('cron.detectedKindDaily')
  if (kind === 'interval') return t('cron.detectedKindInterval')
  return t('cron.detectedKindAt')
}

export default function CronDetectedDialog({
  open,
  name,
  prompt,
  cronExpression,
  kind,
  confirmation,
  onCreate,
  onCancel,
  loading,
}: CronDetectedDialogProps) {
  const { t } = useLocale()

  useEffect(() => {
    if (!open) return
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onCancel()
    }
    document.addEventListener('keydown', onKey)
    const t = setTimeout(() => onCancel(), 60_000)
    return () => {
      document.removeEventListener('keydown', onKey)
      clearTimeout(t)
    }
  }, [open, onCancel])

  // Cleanup on unmount: resolve pending promise to prevent leaks
  useEffect(() => {
    return () => { onCancel() }
  }, [])

  if (!open) return null

  return (
    <div className={styles.overlay}>
      <div className={styles.backdrop} onClick={onCancel} />
      <div className={styles.dialog}>
        {/* Header */}
        <div className={styles.header}>
          <div className={styles.headerIcon}>
            <IconCalendarClock size={22} />
          </div>
          <div className={styles.headerText}>
            <h3 className={styles.title}>{t('cron.detectedTitle')}</h3>
            <span className={styles.kindBadge}>{kindLabel(kind, t)}</span>
          </div>
        </div>

        {/* Body */}
        <div className={styles.body}>
          {confirmation && (
            <p className={styles.confirmation}>{confirmation}</p>
          )}
          <div className={styles.fieldGroup}>
            <label className={styles.fieldLabel}>{t('cron.detectedName')}</label>
            <span className={styles.fieldValue}>{name}</span>
          </div>
          <div className={styles.fieldGroup}>
            <label className={styles.fieldLabel}>{t('cron.detectedPrompt')}</label>
            <span className={styles.fieldValue}>{prompt}</span>
          </div>
          <div className={styles.fieldGroup}>
            <label className={styles.fieldLabel}>{t('cron.detectedSchedule')}</label>
            <div className={styles.scheduleRow}>
              <code className={styles.cronExpr}>{cronExpression}</code>
              <span className={styles.cronHuman}>{describeCron(cronExpression)}</span>
            </div>
          </div>
        </div>

        {/* Actions */}
        <div className={styles.actions}>
          <button className={styles.btnCreate} onClick={onCreate} disabled={loading}>
            {loading ? t('cron.detectedCreating') : t('cron.detectedCreate')}
          </button>
          <button className={styles.btnCancel} onClick={onCancel} disabled={loading}>
            {t('cron.detectedCancel')}
          </button>
        </div>
      </div>
    </div>
  )
}

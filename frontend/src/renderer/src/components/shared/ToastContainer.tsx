import { useEffect, useRef } from 'react'
import { useStore } from '../../stores'
import type { Toast } from '../../stores/toast'
import { IconCheck, IconAlertCircle, IconXCircle, IconX } from '../../utils/icons'
import styles from './ToastContainer.module.css'

const TYPE_ICONS: Record<string, (size?: number) => JSX.Element> = {
  info: (size) => <svg width={size ?? 16} height={size ?? 16} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><circle cx="12" cy="12" r="10"/><line x1="12" y1="16" x2="12" y2="12"/><line x1="12" y1="8" x2="12.01" y2="8"/></svg>,
  success: (size) => <IconCheck size={size ?? 16} />,
  warning: (size) => <IconAlertCircle size={size ?? 16} />,
  error: (size) => <IconXCircle size={size ?? 16} />,
}

const TYPE_LABELS: Record<string, string> = {
  info: 'Info',
  success: 'Success',
  warning: 'Warning',
  error: 'Error',
}

function ToastItem({ toast, onRemove }: { toast: Toast; onRemove: () => void }) {
  const duration = toast.duration ?? 4000
  const progressRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (duration <= 0) return
    const el = progressRef.current
    if (!el) return
    // Trigger shrink animation
    const raf = requestAnimationFrame(() => {
      el.style.transition = `width ${duration}ms linear`
      el.style.width = '0%'
    })
    return () => cancelAnimationFrame(raf)
  }, [duration])

  const icon = TYPE_ICONS[toast.type]

  return (
    <div className={`${styles.toast} ${styles[toast.type]}`}>
      <div className={styles.iconWrap}>
        {icon(16)}
      </div>
      <div className={styles.content}>
        <span className={styles.label}>{TYPE_LABELS[toast.type]}</span>
        <span className={styles.message}>{toast.message}</span>
      </div>
      <div className={styles.actions}>
        {toast.action && (
          <button
            className={styles.actionBtn}
            onClick={() => {
              toast.action!.onClick()
              onRemove()
            }}
          >
            {toast.action.label}
          </button>
        )}
        <button className={styles.closeBtn} onClick={onRemove}>
          <IconX size={13} />
        </button>
      </div>
      {duration > 0 && (
        <div className={styles.progressTrack}>
          <div ref={progressRef} className={styles.progressBar} style={{ width: '100%' }} />
        </div>
      )}
    </div>
  )
}

export default function ToastContainer() {
  const toasts = useStore((s: any) => s.toasts as Toast[])
  const removeToast = useStore((s: any) => s.removeToast as (id: string) => void)

  if (toasts.length === 0) return null

  return (
    <div className={styles.container}>
      {toasts.map((t) => (
        <ToastItem key={t.id} toast={t} onRemove={() => removeToast(t.id)} />
      ))}
    </div>
  )
}

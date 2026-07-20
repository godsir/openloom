import { useEffect, useRef, useState, useCallback } from 'react'
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

/** 退场动画时长，需与 .toastExiting 的 0.18s 保持一致 */
const EXIT_MS = 180

function ToastItem({ toast, onRemove }: { toast: Toast; onRemove: () => void }) {
  const duration = toast.duration ?? 4000
  const [exiting, setExiting] = useState(false)
  const [paused, setPaused] = useState(false)
  // 用 ref 持有最新 onRemove，避免父组件重渲染使计时器 effect 反复重置
  const onRemoveRef = useRef(onRemove)
  onRemoveRef.current = onRemove
  const remainingRef = useRef(duration)
  const startedAtRef = useRef(0)
  const exitingRef = useRef(false)

  // 触发退场：先播退出动画，EXIT_MS 后再真正从 store 移除（消除硬切与堆叠跳动）
  const dismiss = useCallback(() => {
    if (exitingRef.current) return
    exitingRef.current = true
    setExiting(true)
    setTimeout(() => onRemoveRef.current(), EXIT_MS)
  }, [])

  // 自动消失计时器：悬停暂停（记录剩余时长），离开续上
  useEffect(() => {
    if (duration <= 0 || exiting || paused) return
    startedAtRef.current = Date.now()
    const timer = setTimeout(() => dismiss(), remainingRef.current)
    return () => clearTimeout(timer)
  }, [duration, exiting, paused, dismiss])

  const handleMouseEnter = () => {
    if (exitingRef.current || duration <= 0) return
    remainingRef.current = Math.max(0, remainingRef.current - (Date.now() - startedAtRef.current))
    setPaused(true)
  }
  const handleMouseLeave = () => {
    if (exitingRef.current || duration <= 0) return
    setPaused(false)
  }

  const icon = TYPE_ICONS[toast.type]

  return (
    <div
      className={`${styles.toast} ${styles[toast.type]} ${exiting ? styles.toastExiting : ''}`}
      onMouseEnter={handleMouseEnter}
      onMouseLeave={handleMouseLeave}
    >
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
              dismiss()
            }}
          >
            {toast.action.label}
          </button>
        )}
        <button className={styles.closeBtn} onClick={dismiss}>
          <IconX size={13} />
        </button>
      </div>
      {duration > 0 && (
        <div className={styles.progressTrack}>
          <div
            className={styles.progressBar}
            style={{
              animationDuration: `${duration}ms`,
              animationPlayState: paused || exiting ? 'paused' : 'running',
            }}
          />
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

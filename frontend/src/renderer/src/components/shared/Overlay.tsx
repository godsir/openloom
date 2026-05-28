import { useEffect, useRef, type ReactNode } from 'react'
import { IconX } from '../../utils/icons'
import styles from './Overlay.module.css'

interface OverlayProps {
  open: boolean
  onClose: () => void
  children: ReactNode
  title?: string
  size?: 'md' | 'lg'
}

export default function Overlay({ open, onClose, children, title, size = 'md' }: OverlayProps) {
  const overlayRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (!open) return
    const handleEsc = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose()
    }
    document.addEventListener('keydown', handleEsc)
    return () => document.removeEventListener('keydown', handleEsc)
  }, [open, onClose])

  if (!open) return null

  const modalClass = `${styles.modal} ${size === 'lg' ? styles.modalLg : styles.modalMd}`

  return (
    <div className={styles.overlay}>
      <div ref={overlayRef} className={styles.backdrop} onClick={onClose} />
      <div className={modalClass}>
        {title && (
          <div className={styles.titleBar}>
            <h2 className={styles.titleText}>{title}</h2>
            <button onClick={onClose} className={styles.closeBtn}>
              <IconX size={14} />
            </button>
          </div>
        )}
        <div className={size === 'lg' ? styles.bodyLg : styles.body}>{children}</div>
      </div>
    </div>
  )
}

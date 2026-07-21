import { useEffect, useRef, useState, type ReactNode } from 'react'
import { IconX } from '../../utils/icons'
import styles from './Overlay.module.css'

/** 退场动画时长，需与 Overlay.module.css 的 0.18s 保持一致 */
const EXIT_MS = 180

interface OverlayProps {
  open: boolean
  onClose: () => void
  children: ReactNode
  title?: string
  size?: 'md' | 'lg'
}

export default function Overlay({ open, onClose, children, title, size = 'md' }: OverlayProps) {
  const overlayRef = useRef<HTMLDivElement>(null)
  // render 控制是否挂载；exiting 表示正在播放退场动画。关闭时先播退场再卸载，
  // 避免此前的"啪"一下硬切。
  const [render, setRender] = useState(open)
  const [exiting, setExiting] = useState(false)

  useEffect(() => {
    if (open) {
      setRender(true)
      setExiting(false)
      return
    }
    if (!render) return
    setExiting(true)
    const timer = setTimeout(() => {
      setRender(false)
      setExiting(false)
    }, EXIT_MS)
    return () => clearTimeout(timer)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open])

  useEffect(() => {
    if (!render) return
    const handleEsc = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose()
    }
    document.addEventListener('keydown', handleEsc)
    return () => document.removeEventListener('keydown', handleEsc)
  }, [render, onClose])

  if (!render) return null

  const modalClass = `${styles.modal} ${size === 'lg' ? styles.modalLg : styles.modalMd} ${exiting ? styles.modalExiting : ''}`

  return (
    <div className={`${styles.overlay} ${exiting ? styles.overlayExiting : ''}`}>
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

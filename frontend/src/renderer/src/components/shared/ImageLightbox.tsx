import { useEffect, useCallback } from 'react'
import styles from './ImageLightbox.module.css'

interface Props {
  src: string | null
  onClose: () => void
}

export default function ImageLightbox({ src, onClose }: Props) {
  const handleKey = useCallback((e: KeyboardEvent) => {
    if (e.key === 'Escape') onClose()
  }, [onClose])

  useEffect(() => {
    if (!src) return
    document.addEventListener('keydown', handleKey)
    return () => document.removeEventListener('keydown', handleKey)
  }, [src, handleKey])

  if (!src) return null

  return (
    <div className={styles.overlay} onClick={onClose}>
      <div className={styles.backdrop} />
      <img
        src={src}
        alt="Preview"
        className={styles.image}
        onClick={(e) => e.stopPropagation()}
      />
      <button className={styles.closeBtn} onClick={onClose}>✕</button>
    </div>
  )
}

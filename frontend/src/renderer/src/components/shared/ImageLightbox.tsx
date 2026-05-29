import { useEffect, useCallback, useState } from 'react'
import ContextMenu, { ContextMenuItem } from './ContextMenu'
import styles from './ImageLightbox.module.css'

interface Props {
  src: string | null
  onClose: () => void
}

export default function ImageLightbox({ src, onClose }: Props) {
  const [menuOpen, setMenuOpen] = useState(false)
  const [menuPos, setMenuPos] = useState({ x: 0, y: 0 })

  const handleKey = useCallback((e: KeyboardEvent) => {
    if (e.key === 'Escape') onClose()
  }, [onClose])

  useEffect(() => {
    if (!src) return
    document.addEventListener('keydown', handleKey)
    return () => document.removeEventListener('keydown', handleKey)
  }, [src, handleKey])

  if (!src) return null

  const handleSave = () => {
    setMenuOpen(false)
    const a = document.createElement('a')
    a.href = src
    a.download = 'image.png'
    document.body.appendChild(a)
    a.click()
    document.body.removeChild(a)
  }

  const handleContextMenu = (e: React.MouseEvent) => {
    e.preventDefault()
    setMenuPos({ x: e.clientX, y: e.clientY })
    setMenuOpen(true)
  }

  return (
    <div className={styles.overlay} onClick={onClose}>
      <div className={styles.backdrop} />
      <img
        src={src}
        alt="Preview"
        className={styles.image}
        onClick={(e) => e.stopPropagation()}
        onContextMenu={handleContextMenu}
      />
      <button className={styles.closeBtn} onClick={onClose}>✕</button>
      <ContextMenu open={menuOpen} x={menuPos.x} y={menuPos.y} onClose={() => setMenuOpen(false)}>
        <ContextMenuItem onClick={handleSave}>保存图片</ContextMenuItem>
      </ContextMenu>
    </div>
  )
}

import { useState, useCallback, useEffect, useRef } from 'react'
import ContextMenu, { ContextMenuItem } from './ContextMenu'
import styles from './ImageLightbox.module.css'

interface Props {
  src: string | null
  onClose: () => void
}

export default function ImageLightbox({ src, onClose }: Props) {
  const [menuOpen, setMenuOpen] = useState(false)
  const [menuPos, setMenuPos] = useState({ x: 0, y: 0 })
  const [loadError, setLoadError] = useState(false)
  const [scale, setScale] = useState(1)
  const [offset, setOffset] = useState({ x: 0, y: 0 })
  const dragging = useRef(false)
  const dragStart = useRef({ x: 0, y: 0, ox: 0, oy: 0 })
  const imgRef = useRef<HTMLImageElement>(null)

  const close = useCallback(() => {
    setMenuOpen(false)
    setScale(1)
    setOffset({ x: 0, y: 0 })
    onClose()
  }, [onClose])

  const handleKey = useCallback((e: KeyboardEvent) => {
    if (e.key === 'Escape') close()
  }, [close])

  useEffect(() => {
    if (!src) return
    setLoadError(false)
    setScale(1)
    setOffset({ x: 0, y: 0 })
    document.addEventListener('keydown', handleKey)
    return () => document.removeEventListener('keydown', handleKey)
  }, [src, handleKey])

  // Auto-close on load failure
  useEffect(() => {
    if (!loadError) return
    const t = setTimeout(() => close(), 3000)
    return () => clearTimeout(t)
  }, [loadError, close])

  // Scroll wheel zoom
  const handleWheel = useCallback((e: React.WheelEvent) => {
    e.preventDefault()
    const delta = e.deltaY > 0 ? -0.15 : 0.15
    setScale(prev => Math.max(0.2, Math.min(8, prev + delta)))
  }, [])

  // Drag to pan
  const handleMouseDown = useCallback((e: React.MouseEvent) => {
    if (e.button !== 0) return // left button only
    e.preventDefault()
    dragging.current = true
    dragStart.current = { x: e.clientX, y: e.clientY, ox: offset.x, oy: offset.y }
  }, [offset])

  useEffect(() => {
    if (!src) return
    const handleMouseMove = (e: MouseEvent) => {
      if (!dragging.current) return
      setOffset({
        x: dragStart.current.ox + (e.clientX - dragStart.current.x),
        y: dragStart.current.oy + (e.clientY - dragStart.current.y),
      })
    }
    const handleMouseUp = () => { dragging.current = false }
    document.addEventListener('mousemove', handleMouseMove)
    document.addEventListener('mouseup', handleMouseUp)
    return () => {
      document.removeEventListener('mousemove', handleMouseMove)
      document.removeEventListener('mouseup', handleMouseUp)
    }
  }, [src])

  const handleContextMenu = useCallback((e: React.MouseEvent) => {
    e.preventDefault()
    setMenuPos({ x: e.clientX, y: e.clientY })
    setMenuOpen(true)
  }, [])

  const handleSave = useCallback(() => {
    setMenuOpen(false)
    const a = document.createElement('a')
    a.href = src!
    a.download = 'image.png'
    document.body.appendChild(a)
    a.click()
    document.body.removeChild(a)
  }, [src])

  const handleCopy = useCallback(async () => {
    setMenuOpen(false)
    try {
      const img = imgRef.current
      if (!img) return
      const canvas = document.createElement('canvas')
      canvas.width = img.naturalWidth
      canvas.height = img.naturalHeight
      const ctx = canvas.getContext('2d')
      if (!ctx) return
      ctx.drawImage(img, 0, 0)
      const blob = await new Promise<Blob | null>(resolve => canvas.toBlob(resolve, 'image/png'))
      if (!blob) return
      await navigator.clipboard.write([new ClipboardItem({ 'image/png': blob })])
    } catch {
      // Fallback: copy URL
      try { await navigator.clipboard.writeText(src!) } catch { /* ignore */ }
    }
  }, [src])

  if (!src) return null

  return (
    <div
      className={styles.overlay}
      onClick={close}
      onWheel={handleWheel}
      onContextMenu={e => e.preventDefault()}
    >
      <div className={styles.backdrop} />
      {loadError ? (
        <p className={styles.errorMsg}>图片加载失败，即将关闭...</p>
      ) : (
        <img
          ref={imgRef}
          src={src}
          alt="Preview"
          className={styles.image}
          style={{
            transform: `translate(${offset.x}px, ${offset.y}px) scale(${scale})`,
            cursor: scale > 1 ? (dragging.current ? 'grabbing' : 'grab') : 'default',
          }}
          draggable={false}
          onClick={e => e.stopPropagation()}
          onDoubleClick={() => { setScale(1); setOffset({ x: 0, y: 0 }) }}
          onMouseDown={handleMouseDown}
          onContextMenu={handleContextMenu}
          onError={() => setLoadError(true)}
        />
      )}
      <div className={styles.toolbar} onClick={e => e.stopPropagation()}>
        <button className={styles.toolBtn} onClick={() => setScale(s => Math.min(8, s + 0.3))} title="放大">+</button>
        <button className={styles.toolBtn} onClick={() => setScale(s => Math.max(0.2, s - 0.3))} title="缩小">−</button>
        <button className={styles.toolBtn} onClick={() => { setScale(1); setOffset({ x: 0, y: 0 }) }} title="重置">1:1</button>
        <button className={styles.closeBtn} onClick={close} title="关闭 (Esc)">✕</button>
      </div>
      <span className={styles.hint}>滚轮缩放 · 拖拽移动 · 双击重置 · Esc 关闭</span>
      <ContextMenu open={menuOpen} x={menuPos.x} y={menuPos.y} onClose={() => setMenuOpen(false)}>
        <ContextMenuItem onClick={handleCopy}>复制图片</ContextMenuItem>
        <ContextMenuItem onClick={handleSave}>保存图片</ContextMenuItem>
      </ContextMenu>
    </div>
  )
}

import { useState, useCallback, useEffect, useRef } from 'react'
import ContextMenu, { ContextMenuItem } from './ContextMenu'
import { useStore } from '../../stores'
import { useLocale } from '../../i18n'
import styles from './ImageLightbox.module.css'

/** 退场动画时长，需与 ImageLightbox.module.css 的 0.15s 一致 */
const EXIT_MS = 150
const MIN_SCALE = 0.2
const MAX_SCALE = 8

interface Props {
  onClose: () => void
}

export default function ImageLightbox({ onClose }: Props) {
  const { t } = useLocale()
  const src = useStore(s => s.lightbox.lightboxSrc)
  const list = useStore(s => s.lightbox.lightboxList)
  const index = useStore(s => s.lightbox.lightboxIndex)
  const nextImage = useStore(s => s.nextLightboxImage)
  const prevImage = useStore(s => s.prevLightboxImage)
  const showTransient = useStore(s => s.showIslandTransient)

  const [menuOpen, setMenuOpen] = useState(false)
  const [menuPos, setMenuPos] = useState({ x: 0, y: 0 })
  const [loadError, setLoadError] = useState(false)
  const [loaded, setLoaded] = useState(false)
  const [exiting, setExiting] = useState(false)
  const [scale, setScale] = useState(1)
  const [offset, setOffset] = useState({ x: 0, y: 0 })
  const dragging = useRef(false)
  const dragStart = useRef({ x: 0, y: 0, ox: 0, oy: 0 })
  const imgRef = useRef<HTMLImageElement>(null)
  const overlayRef = useRef<HTMLDivElement>(null)

  const canNav = list.length > 1

  // 边界约束：图片最多拖出视口 90%，避免整个拖丢找不回
  const clampOffset = useCallback((o: { x: number; y: number }) => {
    const maxX = window.innerWidth * 0.9
    const maxY = window.innerHeight * 0.9
    return {
      x: Math.max(-maxX, Math.min(maxX, o.x)),
      y: Math.max(-maxY, Math.min(maxY, o.y)),
    }
  }, [])

  const close = useCallback(() => {
    setMenuOpen(false)
    setExiting(true)
    setTimeout(() => {
      setScale(1)
      setOffset({ x: 0, y: 0 })
      setExiting(false)
      onClose()
    }, EXIT_MS)
  }, [onClose])

  const navigate = useCallback((dir: 1 | -1) => {
    // 切换图片时复位缩放/平移与加载态
    setScale(1)
    setOffset({ x: 0, y: 0 })
    setLoaded(false)
    setLoadError(false)
    if (dir === 1) nextImage()
    else prevImage()
  }, [nextImage, prevImage])

  const handleKey = useCallback((e: KeyboardEvent) => {
    if (e.key === 'Escape') close()
    else if (e.key === 'ArrowRight' && canNav) navigate(1)
    else if (e.key === 'ArrowLeft' && canNav) navigate(-1)
  }, [close, navigate, canNav])

  useEffect(() => {
    if (!src) return
    setLoadError(false)
    setLoaded(false)
    setScale(1)
    setOffset({ x: 0, y: 0 })
    document.addEventListener('keydown', handleKey)
    return () => document.removeEventListener('keydown', handleKey)
  }, [src, handleKey])

  // Auto-close on load failure
  useEffect(() => {
    if (!loadError) return
    const t0 = setTimeout(() => close(), 3000)
    return () => clearTimeout(t0)
  }, [loadError, close])

  // 滚轮缩放：用原生 passive:false 监听（React onWheel 是 passive，preventDefault
  // 不生效）。乘法缩放并以光标为锚点——鼠标下的区域缩放后仍留在鼠标下。
  useEffect(() => {
    const el = overlayRef.current
    if (!el || !src) return
    const onWheel = (e: WheelEvent) => {
      e.preventDefault()
      const rect = imgRef.current?.getBoundingClientRect()
      const factor = e.deltaY > 0 ? 0.85 : 1.18
      setScale(prev => {
        const next = Math.max(MIN_SCALE, Math.min(MAX_SCALE, prev * factor))
        const ratio = next / prev
        if (rect && ratio !== 1) {
          const cx = rect.left + rect.width / 2
          const cy = rect.top + rect.height / 2
          const rx = e.clientX - cx
          const ry = e.clientY - cy
          setOffset(o => clampOffset({ x: o.x + rx * (1 - ratio), y: o.y + ry * (1 - ratio) }))
        }
        return next
      })
    }
    el.addEventListener('wheel', onWheel, { passive: false })
    return () => el.removeEventListener('wheel', onWheel)
  }, [src, clampOffset])

  // Drag to pan
  const handleMouseDown = useCallback((e: React.MouseEvent) => {
    if (e.button !== 0) return
    e.preventDefault()
    dragging.current = true
    dragStart.current = { x: e.clientX, y: e.clientY, ox: offset.x, oy: offset.y }
  }, [offset])

  useEffect(() => {
    if (!src) return
    const handleMouseMove = (e: MouseEvent) => {
      if (!dragging.current) return
      setOffset(clampOffset({
        x: dragStart.current.ox + (e.clientX - dragStart.current.x),
        y: dragStart.current.oy + (e.clientY - dragStart.current.y),
      }))
    }
    const handleMouseUp = () => { dragging.current = false }
    document.addEventListener('mousemove', handleMouseMove)
    document.addEventListener('mouseup', handleMouseUp)
    return () => {
      document.removeEventListener('mousemove', handleMouseMove)
      document.removeEventListener('mouseup', handleMouseUp)
    }
  }, [src, clampOffset])

  const handleContextMenu = useCallback((e: React.MouseEvent) => {
    e.preventDefault()
    setMenuPos({ x: e.clientX, y: e.clientY })
    setMenuOpen(true)
  }, [])

  const handleSave = useCallback(() => {
    setMenuOpen(false)
    if (!src) return
    const a = document.createElement('a')
    a.href = src
    // 推断扩展名（data URL 取 MIME 子类型；普通 URL 取路径后缀），避免
    // webp/gif 一律存成 .png
    let ext = 'png'
    if (src.startsWith('data:')) {
      ext = src.match(/^data:image\/([a-zA-Z0-9]+)/)?.[1] ?? 'png'
    } else {
      const tail = src.split('?')[0].split('#')[0].split('.').pop() ?? ''
      if (/^[a-zA-Z0-9]{2,5}$/.test(tail)) ext = tail.toLowerCase()
    }
    a.download = `image.${ext}`
    document.body.appendChild(a)
    a.click()
    document.body.removeChild(a)
  }, [src])

  const handleCopy = useCallback(async () => {
    setMenuOpen(false)
    if (!src) return
    try {
      const img = imgRef.current
      if (!img) throw new Error('no img')
      const canvas = document.createElement('canvas')
      canvas.width = img.naturalWidth
      canvas.height = img.naturalHeight
      const ctx = canvas.getContext('2d')
      if (!ctx) throw new Error('no ctx')
      ctx.drawImage(img, 0, 0)
      const blob = await new Promise<Blob | null>(resolve => canvas.toBlob(resolve, 'image/png'))
      if (!blob) throw new Error('no blob')
      await navigator.clipboard.write([new ClipboardItem({ 'image/png': blob })])
      showTransient(t('common.imageCopied'), 1800)
    } catch {
      // 降级：复制图片链接，并明确提示用户（此前是静默降级）
      try {
        await navigator.clipboard.writeText(src)
        showTransient(t('common.imageUrlCopied'), 1800)
      } catch { /* ignore */ }
    }
  }, [src, showTransient, t])

  if (!src) return null

  return (
    <div
      ref={overlayRef}
      className={`${styles.overlay} ${exiting ? styles.overlayExiting : ''}`}
      onClick={close}
      onContextMenu={e => e.preventDefault()}
    >
      <div className={styles.backdrop} />
      {loadError ? (
        <p className={styles.errorMsg}>{t('common.imageLoadError')}</p>
      ) : (
        <>
          {!loaded && <div className={styles.loading}>{t('common.imageLoading')}</div>}
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
            onLoad={() => setLoaded(true)}
            onError={() => setLoadError(true)}
          />
        </>
      )}
      {canNav && !loadError && (
        <>
          <button className={`${styles.navBtn} ${styles.navPrev}`} onClick={(e) => { e.stopPropagation(); navigate(-1) }} title={t('common.prevImage')}>‹</button>
          <button className={`${styles.navBtn} ${styles.navNext}`} onClick={(e) => { e.stopPropagation(); navigate(1) }} title={t('common.nextImage')}>›</button>
          <span className={styles.counter}>{index + 1} / {list.length}</span>
        </>
      )}
      <div className={styles.toolbar} onClick={e => e.stopPropagation()}>
        <button className={styles.toolBtn} onClick={() => setScale(s => Math.min(MAX_SCALE, s * 1.3))} disabled={scale >= MAX_SCALE} title={t('common.zoomIn')}>+</button>
        <button className={styles.toolBtn} onClick={() => setScale(s => Math.max(MIN_SCALE, s / 1.3))} disabled={scale <= MIN_SCALE} title={t('common.zoomOut')}>−</button>
        <button className={styles.toolBtn} onClick={() => { setScale(1); setOffset({ x: 0, y: 0 }) }} title={t('common.resetZoom')}>1:1</button>
        <button className={styles.closeBtn} onClick={close} title={t('common.closeWithKey', { key: 'Esc' })}>✕</button>
      </div>
      <span className={styles.hint}>{t('common.imageHint')}</span>
      <ContextMenu open={menuOpen} x={menuPos.x} y={menuPos.y} onClose={() => setMenuOpen(false)}>
        <ContextMenuItem onClick={handleCopy}>{t('common.copyImage')}</ContextMenuItem>
        <ContextMenuItem onClick={handleSave}>{t('common.saveImage')}</ContextMenuItem>
      </ContextMenu>
    </div>
  )
}

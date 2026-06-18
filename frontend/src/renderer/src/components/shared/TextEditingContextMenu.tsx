import { useEffect, useState, useCallback, useRef, useLayoutEffect } from 'react'
import { useLocale } from '../../i18n'
import styles from './TextEditingContextMenu.module.css'

interface ContextMenuParams {
  isEditable: boolean
  canCut: boolean
  canCopy: boolean
  canPaste: boolean
  canSelectAll: boolean
  hasSelection: boolean
  x: number
  y: number
}

export default function TextEditingContextMenu() {
  const { t } = useLocale()
  const [params, setParams] = useState<ContextMenuParams | null>(null)
  const menuRef = useRef<HTMLDivElement>(null)
  const [menuStyle, setMenuStyle] = useState<{ left: number; top: number }>({ left: 0, top: 0 })

  useEffect(() => {
    const unsub = window.loom.onContextMenu((p) => { setParams(p) })
    return unsub
  }, [])

  const close = useCallback(() => setParams(null), [])

  // Clamp position so the menu never overflows the viewport
  useLayoutEffect(() => {
    if (!params) return
    const el = menuRef.current
    if (!el) return
    const r = el.getBoundingClientRect()
    const vw = window.innerWidth
    const vh = window.innerHeight
    let left = params.x
    let top = params.y
    if (left + r.width > vw) left = Math.max(0, params.x - r.width)
    if (top + r.height > vh) top = Math.max(0, params.y - r.height)
    setMenuStyle({ left, top })
  }, [params])

  useEffect(() => {
    if (!params) return
    const handleClick = () => close()
    const handleKey = (e: KeyboardEvent) => { if (e.key === 'Escape') close() }
    window.addEventListener('click', handleClick, true)
    window.addEventListener('keydown', handleKey, true)
    return () => {
      window.removeEventListener('click', handleClick, true)
      window.removeEventListener('keydown', handleKey, true)
    }
  }, [params, close])

  const execute = useCallback((action: 'cut' | 'copy' | 'paste' | 'selectAll') => {
    window.loom.executeContextMenuAction(action)
    close()
  }, [close])

  if (!params) return null

  const items: { label: string; action: 'cut' | 'copy' | 'paste' | 'selectAll'; shortcut: string; enabled: boolean }[] = []

  if (params.isEditable) {
    items.push(
      { label: t('menu.cut'),       action: 'cut',       shortcut: 'Ctrl+X', enabled: params.canCut },
      { label: t('menu.copy'),      action: 'copy',      shortcut: 'Ctrl+C', enabled: params.canCopy },
      { label: t('menu.paste'),     action: 'paste',     shortcut: 'Ctrl+V', enabled: params.canPaste },
      { label: t('menu.selectAll'), action: 'selectAll', shortcut: 'Ctrl+A', enabled: params.canSelectAll },
    )
  } else if (params.hasSelection) {
    items.push(
      { label: t('menu.copy'), action: 'copy', shortcut: 'Ctrl+C', enabled: params.canCopy },
    )
  }

  if (items.length === 0) return null

  return (
    <div className={styles.overlay} onContextMenu={(e) => e.preventDefault()}>
      <div ref={menuRef} className={styles.menu} style={{ left: menuStyle.left, top: menuStyle.top }}>
        {items.map((item, i) => (
          <button
            key={i}
            className={styles.item}
            disabled={!item.enabled}
            onClick={() => item.enabled && execute(item.action)}
          >
            <span className={styles.label}>{item.label}</span>
            <span className={styles.shortcut}>{item.shortcut}</span>
          </button>
        ))}
      </div>
    </div>
  )
}

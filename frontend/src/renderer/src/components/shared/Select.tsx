import { useState, useRef, useEffect, useCallback, useMemo } from 'react'
import { createPortal } from 'react-dom'
import { IconChevronDown, IconCheck } from '../../utils/icons'
import styles from './Select.module.css'

export interface SelectOption<T extends string = string> {
  value: T
  label: string
  group?: string
  avatar?: string | null
}

interface SelectProps<T extends string> {
  value: T
  options: SelectOption<T>[]
  onChange: (value: T) => void
  className?: string
  placeholder?: string
  variant?: 'form' | 'pill'
  disabled?: boolean
  menuWidth?: number
}

const ITEM_HEIGHT = 34
const HEADER_HEIGHT = 26
const MENU_MAX_HEIGHT = 300
const MENU_GAP = 4
const VIEWPORT_MARGIN = 12

export default function Select<T extends string = string>({
  value,
  options,
  onChange,
  className = '',
  placeholder,
  variant = 'form',
  disabled,
  menuWidth,
}: SelectProps<T>) {
  const [open, setOpen] = useState(false)
  const triggerRef = useRef<HTMLButtonElement>(null)
  const menuRef = useRef<HTMLDivElement>(null)
  const [menuPos, setMenuPos] = useState<React.CSSProperties>({})
  const selected = options.find((o) => o.value === value)
  const label = selected?.label ?? placeholder ?? ''
  const selectedAvatar = selected?.avatar ?? null

  const groupHeaders = useMemo(() => {
    const seen = new Set<string>()
    const headers: { group: string; index: number }[] = []
    options.forEach((o, i) => {
      if (o.group && !seen.has(o.group)) {
        seen.add(o.group)
        headers.push({ group: o.group, index: i + headers.length })
      }
    })
    return headers
  }, [options])

  const updatePosition = useCallback(() => {
    if (!triggerRef.current) return
    const rect = triggerRef.current.getBoundingClientRect()
    const estimatedHeight = Math.min(
      options.length * ITEM_HEIGHT + groupHeaders.length * HEADER_HEIGHT,
      MENU_MAX_HEIGHT,
    )
    const spaceBelow = window.innerHeight - rect.bottom - VIEWPORT_MARGIN
    const spaceAbove = rect.top - VIEWPORT_MARGIN
    const shouldFlip = spaceBelow < estimatedHeight && spaceAbove > spaceBelow

    const width = menuWidth ?? Math.max(rect.width, 160)
    const pos: React.CSSProperties = {
      position: 'fixed',
      left: rect.left,
      width,
      maxHeight: MENU_MAX_HEIGHT,
      overflowY: 'auto',
      zIndex: 9999,
    }
    if (shouldFlip) {
      pos.bottom = window.innerHeight - rect.top + MENU_GAP
    } else {
      pos.top = rect.bottom + MENU_GAP
    }
    setMenuPos(pos)
  }, [options.length, groupHeaders.length, menuWidth])

  useEffect(() => {
    if (open) {
      updatePosition()
      const handler = (e: MouseEvent) => {
        const target = e.target as Node
        if (triggerRef.current?.contains(target)) return
        if (menuRef.current?.contains(target)) return
        setOpen(false)
      }
      const timer = setTimeout(() => document.addEventListener('mousedown', handler), 0)
      return () => {
        clearTimeout(timer)
        document.removeEventListener('mousedown', handler)
      }
    }
  }, [open, updatePosition])

  const isPill = variant === 'pill'
  const triggerClass = [
    isPill ? styles.triggerPill : styles.trigger,
    className,
  ].filter(Boolean).join(' ')

  let lastGroup = ''

  return (
    <>
      <button
        ref={triggerRef}
        type="button"
        onClick={() => !disabled && setOpen(!open)}
        className={triggerClass}
        disabled={disabled}
      >
        {isPill && selectedAvatar ? (
          <span className={styles.triggerAvatar}>
            {selectedAvatar.startsWith('data:') ? (
              <img src={selectedAvatar} alt="" className={styles.triggerAvatarImg} />
            ) : (
              <span className={styles.triggerAvatarLetter}>{label[0]?.toUpperCase() || '?'}</span>
            )}
          </span>
        ) : null}
        <span className={`${styles.label} ${value ? '' : styles.placeholder}`}>{label}</span>
        <IconChevronDown size={isPill ? 8 : 12} />
      </button>
      {open &&
        createPortal(
          <div ref={menuRef} style={menuPos} className={styles.menu}>
            {options.map((opt) => {
              const showHeader = opt.group && opt.group !== lastGroup
              if (showHeader) lastGroup = opt.group!
              const headerEl = showHeader ? (
                <div key={`h-${opt.group}`} className={styles.groupHeader}>
                  {opt.group}
                </div>
              ) : null
              const itemEl = (
                <div
                  key={opt.value}
                  onClick={() => {
                    onChange(opt.value)
                    setOpen(false)
                  }}
                  className={`${styles.item} ${opt.value === value ? styles.itemActive : ''}`}
                >
                  {opt.avatar ? (
                    opt.avatar.startsWith('data:') ? (
                      <img src={opt.avatar} alt="" className={styles.itemAvatar} />
                    ) : (
                      <span className={styles.itemAvatarLetter}>{opt.label[0]?.toUpperCase() || '?'}</span>
                    )
                  ) : null}
                  <span className={styles.itemLabel} title={opt.label}>{opt.label}</span>
                  {opt.value === value && <IconCheck size={12} className={styles.check} />}
                </div>
              )
              return showHeader ? [headerEl, itemEl] : itemEl
            })}
          </div>,
          document.body,
        )}
    </>
  )
}

import { useState, useRef, useEffect, useCallback, useMemo } from 'react'
import { createPortal } from 'react-dom'
import { IconChevronDown, IconCheck } from '../../utils/icons'
import { useMenuKeyboard, useClickOutside } from './menu-hooks'
import styles from './Select.module.css'

export interface SelectOption<T extends string = string> {
  value: T
  label: string
  group?: string
  avatar?: string | null
  /** Font family applied to the option item for live preview */
  fontFamily?: string
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
  /** 无选项时展示的空态文案（不传则不渲染空态） */
  emptyText?: string
  /** 选项加载中（展示加载态，优先于空态） */
  loading?: boolean
  loadingText?: string
  /** 无障碍标签（触发器 aria-label） */
  ariaLabel?: string
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
  emptyText,
  loading,
  loadingText,
  ariaLabel,
}: SelectProps<T>) {
  const [open, setOpen] = useState(false)
  const triggerRef = useRef<HTMLButtonElement>(null)
  const menuRef = useRef<HTMLDivElement>(null)
  const itemRefs = useRef<(HTMLDivElement | null)[]>([])
  const [menuPos, setMenuPos] = useState<React.CSSProperties>({})
  // 键盘高亮项（指向 options 数组下标，与分组头无关）
  const [activeIndex, setActiveIndex] = useState(0)
  const menuIdRef = useRef(`select-menu-${Math.random().toString(36).slice(2, 8)}`)
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
    let left = rect.left
    // Keep the menu inside the viewport when the trigger sits near the right
    // edge (e.g. the channel pill under page zoom): right-align to the
    // trigger rather than overflowing the window.
    if (left + width > window.innerWidth - VIEWPORT_MARGIN) {
      left = Math.max(rect.right - width, VIEWPORT_MARGIN)
    }
    // Cap height to the space available in the chosen direction so the menu
    // scrolls internally instead of overflowing the viewport.
    const maxHeight = Math.min(
      MENU_MAX_HEIGHT,
      Math.max(shouldFlip ? spaceAbove : spaceBelow, 0),
    )
    const pos: React.CSSProperties = {
      position: 'fixed',
      left,
      width,
      maxHeight,
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

  // 打开时定位，并把键盘高亮复位到当前选中项
  useEffect(() => {
    if (open) {
      updatePosition()
      const idx = options.findIndex((o) => o.value === value)
      setActiveIndex(idx >= 0 ? idx : 0)
    }
  }, [open, updatePosition])

  // 点击外部关闭（统一 hook，取代原先各弹层各写一套的 mousedown + setTimeout）
  useClickOutside(
    (target) =>
      !!triggerRef.current?.contains(target) || !!menuRef.current?.contains(target),
    () => setOpen(false),
    open,
  )

  const selectIndex = useCallback(
    (i: number) => {
      const opt = options[i]
      if (!opt) return
      onChange(opt.value)
      setOpen(false)
    },
    [options, onChange],
  )

  // 键盘导航：↑/↓ 循环、Enter 选中、Esc 关闭
  useMenuKeyboard({
    open: open && options.length > 0,
    itemCount: options.length,
    activeIndex,
    setActiveIndex,
    onSelect: selectIndex,
    onClose: () => setOpen(false),
  })

  // 键盘移动高亮时，把对应项滚入可视区
  useEffect(() => {
    if (!open) return
    itemRefs.current[activeIndex]?.scrollIntoView({ block: 'nearest' })
  }, [open, activeIndex])

  const isPill = variant === 'pill'
  const triggerClass = [
    isPill ? styles.triggerPill : styles.trigger,
    open ? styles.triggerOpen : '',
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
        aria-label={ariaLabel}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-controls={open ? menuIdRef.current : undefined}
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
        <span className={`${styles.label} ${value ? '' : styles.placeholder}`} style={selected?.fontFamily ? { fontFamily: selected.fontFamily } : undefined}>{label}</span>
        <span className={`${styles.chevron} ${open ? styles.chevronOpen : ''}`}>
          <IconChevronDown size={isPill ? 8 : 12} />
        </span>
      </button>
      {open &&
        createPortal(
          <div
            ref={menuRef}
            id={menuIdRef.current}
            role="listbox"
            aria-label={ariaLabel}
            style={menuPos}
            className={styles.menu}
          >
            {loading ? (
              <div className={styles.menuEmpty}>
                <span className={styles.menuSpinner} aria-hidden="true" />
                {loadingText ?? '…'}
              </div>
            ) : options.length === 0 ? (
              emptyText ? (
                <div className={styles.menuEmpty}>{emptyText}</div>
              ) : null
            ) : (
              options.map((opt, i) => {
                const showHeader = opt.group && opt.group !== lastGroup
                if (showHeader) lastGroup = opt.group!
                const headerEl = showHeader ? (
                  <div key={`h-${opt.group}`} className={styles.groupHeader}>
                    {opt.group}
                  </div>
                ) : null
                const itemStyle = opt.fontFamily ? { fontFamily: opt.fontFamily } : undefined
                const isActive = i === activeIndex
                const itemEl = (
                  <div
                    key={opt.value}
                    ref={(el) => { itemRefs.current[i] = el }}
                    role="option"
                    aria-selected={opt.value === value}
                    onMouseEnter={() => setActiveIndex(i)}
                    onClick={() => selectIndex(i)}
                    className={[
                      styles.item,
                      opt.value === value ? styles.itemActive : '',
                      isActive && opt.value !== value ? styles.itemHighlight : '',
                    ].filter(Boolean).join(' ')}
                    style={itemStyle}
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
              })
            )}
          </div>,
          document.body,
        )}
    </>
  )
}

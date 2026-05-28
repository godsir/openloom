import { useState, useRef, useEffect, useCallback } from 'react'
import { createPortal } from 'react-dom'
import { IconChevronDown, IconCheck } from '../../utils/icons'

export interface SelectOption<T extends string = string> {
  value: T
  label: string
}

interface SelectProps<T extends string> {
  value: T
  options: SelectOption<T>[]
  onChange: (value: T) => void
  className?: string
  placeholder?: string
  variant?: 'form' | 'pill'
  disabled?: boolean
}

export default function Select<T extends string = string>({
  value,
  options,
  onChange,
  className = '',
  placeholder,
  variant = 'form',
  disabled,
}: SelectProps<T>) {
  const [open, setOpen] = useState(false)
  const triggerRef = useRef<HTMLButtonElement>(null)
  const menuRef = useRef<HTMLDivElement>(null)
  const [menuPos, setMenuPos] = useState<React.CSSProperties>({})
  const label = options.find((o) => o.value === value)?.label ?? placeholder ?? ''

  const updatePosition = useCallback(() => {
    if (!triggerRef.current) return
    const rect = triggerRef.current.getBoundingClientRect()
    setMenuPos({
      position: 'fixed',
      left: rect.left,
      top: rect.bottom + 4,
      minWidth: rect.width,
      zIndex: 9999,
    })
  }, [])

  useEffect(() => {
    if (open) {
      updatePosition()
      const handler = (e: MouseEvent) => {
        const target = e.target as Node
        if (triggerRef.current?.contains(target)) return
        if (menuRef.current?.contains(target)) return
        setOpen(false)
      }
      // Delay listener to avoid the mousedown that opened the menu closing it immediately
      const timer = setTimeout(() => document.addEventListener('mousedown', handler), 0)
      return () => {
        clearTimeout(timer)
        document.removeEventListener('mousedown', handler)
      }
    }
  }, [open, updatePosition])

  const triggerClass =
    variant === 'pill'
      ? `pill-neutral pr-5 cursor-pointer flex items-center gap-1 ${className} ${disabled ? 'opacity-50 pointer-events-none' : ''}`
      : `bg-[var(--bg-card)] text-[var(--text)] text-sm rounded-[var(--r-input)] px-3 py-1.5 outline-none border border-[var(--border)] cursor-pointer transition-colors flex items-center justify-between min-w-[120px] ${className} ${disabled ? 'opacity-50 pointer-events-none' : ''}`

  return (
    <>
      <button
        ref={triggerRef}
        type="button"
        onClick={() => !disabled && setOpen(!open)}
        className={triggerClass}
        disabled={disabled}
      >
        <span className={value ? '' : 'text-[var(--text-muted)]'}>{label}</span>
        <IconChevronDown size={variant === 'pill' ? 8 : 12} />
      </button>
      {open &&
        createPortal(
          <div ref={menuRef} style={menuPos} className="bg-[var(--bg-card)] border border-[var(--border)] rounded-[var(--r-md)] shadow-[var(--shadow-lg)] overflow-hidden">
            {options.map((opt) => (
              <div
                key={opt.value}
                onClick={() => {
                  onChange(opt.value)
                  setOpen(false)
                }}
                className={`flex items-center gap-2 px-3 py-2 text-sm cursor-pointer transition-colors
                  ${opt.value === value ? 'text-[var(--accent)] bg-[var(--accent-subtle)]' : 'text-[var(--text)] hover:bg-[rgba(255,255,255,0.04)]'}
                `}
              >
                <span className="flex-1">{opt.label}</span>
                {opt.value === value && <IconCheck size={12} />}
              </div>
            ))}
          </div>,
          document.body,
        )}
    </>
  )
}

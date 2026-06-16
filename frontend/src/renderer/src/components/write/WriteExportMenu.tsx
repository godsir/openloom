import React, { useState, useRef, useEffect, useCallback } from 'react'
import styles from './WriteExportMenu.module.css'

interface ExportOption {
  key: string
  label: string
}

const EXPORT_OPTIONS: ExportOption[] = [
  { key: 'markdown', label: '导出 Markdown' },
  { key: 'html', label: '导出 HTML' },
  { key: 'pdf', label: '导出 PDF' },
]

/**
 * 导出菜单 — 提供 Markdown / HTML / PDF 导出选项的下拉菜单。
 * 当前实现为 UI 占位，点击选项仅关闭菜单，具体导出逻辑由上层实现。
 */
export default function WriteExportMenu() {
  const [open, setOpen] = useState(false)
  const triggerRef = useRef<HTMLButtonElement>(null)
  const menuRef = useRef<HTMLDivElement>(null)

  const handleClickOutside = useCallback((e: MouseEvent) => {
    const target = e.target as Node
    if (triggerRef.current?.contains(target)) return
    if (menuRef.current?.contains(target)) return
    setOpen(false)
  }, [])

  useEffect(() => {
    if (!open) return
    const timer = setTimeout(() => document.addEventListener('mousedown', handleClickOutside), 0)
    return () => {
      clearTimeout(timer)
      document.removeEventListener('mousedown', handleClickOutside)
    }
  }, [open, handleClickOutside])

  const handleSelect = (_key: string) => {
    // TODO: wire up actual export logic
    setOpen(false)
  }

  return (
    <div className={styles.wrapper}>
      <button
        ref={triggerRef}
        className={styles.trigger}
        onClick={() => setOpen((o) => !o)}
        title="导出"
      >
        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
          <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />
          <polyline points="7,10 12,15 17,10" />
          <line x1="12" y1="15" x2="12" y2="3" />
        </svg>
        <span>导出</span>
      </button>

      {open && (
        <div ref={menuRef} className={styles.menu}>
          {EXPORT_OPTIONS.map((opt) => (
            <button
              key={opt.key}
              className={styles.menuItem}
              onClick={() => handleSelect(opt.key)}
            >
              {opt.label}
            </button>
          ))}
        </div>
      )}
    </div>
  )
}

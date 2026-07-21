import { useState, useRef, useEffect, useCallback } from 'react'
import { useWriteStore } from '../../stores/write'
import { renderMarkdown } from '../../utils/markdown'
import { useLocale } from '../../i18n'
import styles from './WriteExportMenu.module.css'

const EXPORT_OPTIONS = [
  { key: 'markdown', label: 'Markdown' },
  { key: 'html', label: 'HTML' },
  { key: 'pdf', label: 'PDF' },
  { key: 'docx', label: 'DOCX' },
]

export default function WriteExportMenu() {
  const [open, setOpen] = useState(false)
  const triggerRef = useRef<HTMLButtonElement>(null)
  const menuRef = useRef<HTMLDivElement>(null)
  const fileContent = useWriteStore(s => s.fileContent)
  const activeFilePath = useWriteStore(s => s.activeFilePath)
  const showToast = useWriteStore(s => s.showToast)
  const { t } = useLocale()

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

  const handleSelect = async (key: string) => {
    setOpen(false)
    if (!fileContent || !activeFilePath) return
    const title = activeFilePath.split('/').pop()?.replace(/\.[^.]+$/, '') || 'document'

    const html = renderMarkdown(fileContent)
    const loom = (window as any).loom
    const formatLabel = EXPORT_OPTIONS.find(o => o.key === key)?.label || key

    try {
      let handled = false
      switch (key) {
        case 'markdown': {
          // Copy markdown source to clipboard
          if (loom?.copyWriteDocumentAsRichText) {
            await loom.copyWriteDocumentAsRichText(activeFilePath, '', fileContent)
            handled = true
          }
          break
        }
        case 'html': {
          if (loom?.exportWriteHtml) {
            await loom.exportWriteHtml(html, title)
            handled = true
          }
          break
        }
        case 'pdf': {
          if (loom?.exportWritePdf) {
            await loom.exportWritePdf(html, title)
            handled = true
          }
          break
        }
        case 'docx': {
          if (loom?.exportWriteDocx) {
            await loom.exportWriteDocx(html, title)
            handled = true
          }
          break
        }
      }
      // 导出成功/不可用/失败都给出可见反馈，而非静默（A21）
      if (handled) {
        showToast('success', t('write.exportedOk', { format: formatLabel }))
      } else {
        showToast('error', t('write.exportFailed'))
      }
    } catch (err) {
      console.error('Export failed:', err)
      showToast('error', t('write.exportFailed'))
    }
  }

  return (
    <div className={styles.wrapper}>
      <button
        ref={triggerRef}
        className={styles.trigger}
        onClick={() => setOpen(o => !o)}
        title={t('write.export', '导出')}
      >
        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
          <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />
          <polyline points="7,10 12,15 17,10" />
          <line x1="12" y1="15" x2="12" y2="3" />
        </svg>
        {t('write.export', '导出')}
      </button>

      {open && (
        <div ref={menuRef} className={styles.menu}>
          {EXPORT_OPTIONS.map(opt => (
            <button
              key={opt.key}
              className={styles.menuItem}
              onClick={() => handleSelect(opt.key)}
              disabled={!fileContent}
            >
              {t(`write.export${opt.key.toUpperCase()}`, opt.label)}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
